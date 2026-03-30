# Activity Event Pipeline — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Populate `AgentInfo.activityEvents` from webhook tool events (Claude Code) and terminal text parsing (other agents), so the activity feed on grid cards shows real data.

**Architecture:** `ActivityEventExtractor` is a pure function that extracts `ActivityEvent` from `WebhookEvent` data. `AgentHead.handleWebhookEvent()` calls it on `toolUseEnd`/`toolUseFailed` and appends to a ring buffer. For non-webhook agents, `StatusDetector` extracts activity events from viewport text patterns.

**Tech Stack:** Swift 5.10, AppKit, XCTest

---

### Task 1: Add ActivityEventExtractor for webhook events

**Files:**
- Create: `Sources/Core/ActivityEventExtractor.swift`
- Test: `Tests/ActivityEventExtractorTests.swift`

- [ ] **Step 1: Write the test file**

```swift
// Tests/ActivityEventExtractorTests.swift
import XCTest
@testable import amux

final class ActivityEventExtractorTests: XCTestCase {

    // MARK: - Detail extraction per tool type

    func testReadToolExtractsBasename() {
        let event = makeWebhookEvent(tool: "Read", input: ["file_path": "/Users/dev/project/src/auth/login.swift"])
        let activity = ActivityEventExtractor.extract(from: event)
        XCTAssertEqual(activity.tool, "Read")
        XCTAssertEqual(activity.detail, "auth/login.swift")
        XCTAssertFalse(activity.isError)
    }

    func testEditToolExtractsPathAndLine() {
        let event = makeWebhookEvent(tool: "Edit", input: [
            "file_path": "/Users/dev/project/src/main.swift",
            "old_string": "let x = 1",
            "new_string": "let x = 2",
        ])
        let activity = ActivityEventExtractor.extract(from: event)
        XCTAssertEqual(activity.tool, "Edit")
        XCTAssertTrue(activity.detail.contains("main.swift"))
    }

    func testBashToolExtractsCommand() {
        let event = makeWebhookEvent(tool: "Bash", input: ["command": "swift test --filter AuthTests"])
        let activity = ActivityEventExtractor.extract(from: event)
        XCTAssertEqual(activity.tool, "Bash")
        XCTAssertEqual(activity.detail, "swift test --filter AuthTests")
    }

    func testBashToolTruncatesLongCommand() {
        let longCmd = String(repeating: "a", count: 100)
        let event = makeWebhookEvent(tool: "Bash", input: ["command": longCmd])
        let activity = ActivityEventExtractor.extract(from: event)
        XCTAssertTrue(activity.detail.count <= 63) // 60 + "..."
    }

    func testGrepToolExtractsPattern() {
        let event = makeWebhookEvent(tool: "Grep", input: ["pattern": "validateToken"])
        let activity = ActivityEventExtractor.extract(from: event)
        XCTAssertEqual(activity.detail, "\"validateToken\"")
    }

    func testGlobToolExtractsPattern() {
        let event = makeWebhookEvent(tool: "Glob", input: ["pattern": "**/*.swift"])
        let activity = ActivityEventExtractor.extract(from: event)
        XCTAssertEqual(activity.detail, "**/*.swift")
    }

    func testWriteToolExtractsPath() {
        let event = makeWebhookEvent(tool: "Write", input: ["file_path": "/tmp/project/config.json"])
        let activity = ActivityEventExtractor.extract(from: event)
        XCTAssertEqual(activity.detail, "project/config.json")
    }

    func testAgentToolExtractsPrompt() {
        let event = makeWebhookEvent(tool: "Agent", input: ["prompt": "Explore the grid card UI code and find all rendering logic"])
        let activity = ActivityEventExtractor.extract(from: event)
        XCTAssertTrue(activity.detail.count <= 43) // 40 + "..."
    }

    func testUnknownToolUsesToolName() {
        let event = makeWebhookEvent(tool: "TaskCreate", input: ["subject": "Do something"])
        let activity = ActivityEventExtractor.extract(from: event)
        XCTAssertEqual(activity.tool, "TaskCreate")
        XCTAssertEqual(activity.detail, "TaskCreate")
    }

    // MARK: - Error detection

    func testToolUseFailedIsError() {
        let event = makeWebhookEvent(tool: "Bash", input: [:], eventType: .toolUseFailed)
        let activity = ActivityEventExtractor.extract(from: event)
        XCTAssertTrue(activity.isError)
    }

    func testToolUseEndIsNotError() {
        let event = makeWebhookEvent(tool: "Read", input: ["file_path": "test.swift"])
        let activity = ActivityEventExtractor.extract(from: event)
        XCTAssertFalse(activity.isError)
    }

    func testBashWithNonZeroExitIsError() {
        let event = makeWebhookEvent(tool: "Bash", input: ["command": "swift test"], result: "Exit code: 1\nTest failed")
        let activity = ActivityEventExtractor.extract(from: event)
        XCTAssertTrue(activity.isError)
    }

    func testBashWithZeroExitIsNotError() {
        let event = makeWebhookEvent(tool: "Bash", input: ["command": "swift test"], result: "Exit code: 0\nAll tests passed")
        let activity = ActivityEventExtractor.extract(from: event)
        XCTAssertFalse(activity.isError)
    }

    // MARK: - Short file path

    func testShortPathLastTwoComponents() {
        XCTAssertEqual(ActivityEventExtractor.shortPath("/a/b/c/d/e.swift"), "d/e.swift")
    }

    func testShortPathSingleComponent() {
        XCTAssertEqual(ActivityEventExtractor.shortPath("file.swift"), "file.swift")
    }

    func testShortPathTwoComponents() {
        XCTAssertEqual(ActivityEventExtractor.shortPath("/a/b.swift"), "a/b.swift")
    }

    // MARK: - Helpers

    private func makeWebhookEvent(tool: String, input: [String: Any], eventType: WebhookEventType = .toolUseEnd, result: String? = nil) -> WebhookEvent {
        var data: [String: Any] = ["tool_name": tool, "tool_input": input]
        if let result { data["tool_result"] = result }
        return WebhookEvent(
            source: "claude-code",
            sessionId: "test-session",
            event: eventType,
            cwd: "/tmp/test",
            timestamp: nil,
            data: data
        )
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `xcodegen generate && xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/ActivityEventExtractorTests 2>&1 | tail -20`
Expected: FAIL — `ActivityEventExtractor` not found

- [ ] **Step 3: Implement ActivityEventExtractor**

```swift
// Sources/Core/ActivityEventExtractor.swift
import Foundation

enum ActivityEventExtractor {

    /// Extract an ActivityEvent from a webhook toolUseEnd or toolUseFailed event.
    static func extract(from event: WebhookEvent) -> ActivityEvent {
        let toolName = event.data?["tool_name"] as? String ?? "Unknown"
        let toolInput = event.data?["tool_input"] as? [String: Any] ?? [:]
        let toolResult = event.data?["tool_result"] as? String

        let detail = extractDetail(toolName: toolName, toolInput: toolInput)
        let isError = detectError(event: event, toolName: toolName, toolResult: toolResult)

        return ActivityEvent(
            tool: toolName,
            detail: detail,
            isError: isError,
            timestamp: Date()
        )
    }

    /// Extract last 2 path components for compact display.
    static func shortPath(_ path: String) -> String {
        let url = URL(fileURLWithPath: path)
        let components = url.pathComponents.filter { $0 != "/" }
        if components.count <= 2 {
            return components.joined(separator: "/")
        }
        return components.suffix(2).joined(separator: "/")
    }

    private static func extractDetail(toolName: String, toolInput: [String: Any]) -> String {
        switch toolName {
        case "Read", "Write":
            if let path = toolInput["file_path"] as? String {
                return shortPath(path)
            }
            return toolName

        case "Edit":
            if let path = toolInput["file_path"] as? String {
                return shortPath(path)
            }
            return toolName

        case "Bash":
            if let cmd = toolInput["command"] as? String {
                return truncate(cmd, maxLen: 60)
            }
            return toolName

        case "Grep":
            if let pattern = toolInput["pattern"] as? String {
                return "\"\(pattern)\""
            }
            return toolName

        case "Glob":
            if let pattern = toolInput["pattern"] as? String {
                return pattern
            }
            return toolName

        case "Agent":
            if let prompt = toolInput["prompt"] as? String {
                return truncate(prompt, maxLen: 40)
            }
            return toolName

        case "WebSearch":
            if let query = toolInput["query"] as? String {
                return "\"\(truncate(query, maxLen: 40))\""
            }
            return toolName

        case "WebFetch":
            if let url = toolInput["url"] as? String {
                return truncate(url, maxLen: 60)
            }
            return toolName

        default:
            return toolName
        }
    }

    private static func detectError(event: WebhookEvent, toolName: String, toolResult: String?) -> Bool {
        if event.event == .toolUseFailed {
            return true
        }
        if toolName == "Bash", let result = toolResult {
            // Check for non-zero exit code
            if let range = result.range(of: "Exit code: ", options: .caseInsensitive) {
                let afterPrefix = result[range.upperBound...]
                let codeStr = afterPrefix.prefix(while: { $0.isNumber })
                if let code = Int(codeStr), code != 0 {
                    return true
                }
            }
        }
        return false
    }

    private static func truncate(_ str: String, maxLen: Int) -> String {
        if str.count <= maxLen { return str }
        return String(str.prefix(maxLen)) + "..."
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `xcodegen generate && xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/ActivityEventExtractorTests 2>&1 | tail -20`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add Sources/Core/ActivityEventExtractor.swift Tests/ActivityEventExtractorTests.swift
git commit -m "feat: add ActivityEventExtractor for webhook tool events"
```

---

### Task 2: Add activity event methods to AgentHead

**Files:**
- Modify: `Sources/Core/AgentHead.swift`
- Test: `Tests/AgentHeadActivityEventTests.swift`

- [ ] **Step 1: Write the test file**

```swift
// Tests/AgentHeadActivityEventTests.swift
import XCTest
@testable import amux

final class AgentHeadActivityEventTests: XCTestCase {

    func testAppendActivityEventAddsToFront() {
        let head = AgentHead.shared
        // Register a test agent (we need a surface stub)
        // Since we can't easily create a TerminalSurface in tests,
        // test the ring buffer logic directly via the static helper
        var events: [ActivityEvent] = []
        let event1 = ActivityEvent(tool: "Read", detail: "a.swift", isError: false, timestamp: Date())
        let event2 = ActivityEvent(tool: "Edit", detail: "b.swift", isError: false, timestamp: Date())

        AgentHead.appendToRingBuffer(&events, event: event1, maxSize: 20)
        AgentHead.appendToRingBuffer(&events, event: event2, maxSize: 20)

        XCTAssertEqual(events.count, 2)
        XCTAssertEqual(events[0].tool, "Edit") // newest first
        XCTAssertEqual(events[1].tool, "Read")
    }

    func testRingBufferCapsAtMaxSize() {
        var events: [ActivityEvent] = []
        for i in 0..<25 {
            let event = ActivityEvent(tool: "Read", detail: "file\(i).swift", isError: false, timestamp: Date())
            AgentHead.appendToRingBuffer(&events, event: event, maxSize: 20)
        }
        XCTAssertEqual(events.count, 20)
        XCTAssertEqual(events[0].detail, "file24.swift") // newest
    }

    func testClearActivityEventsEmptiesBuffer() {
        var events: [ActivityEvent] = []
        let event = ActivityEvent(tool: "Read", detail: "a.swift", isError: false, timestamp: Date())
        AgentHead.appendToRingBuffer(&events, event: event, maxSize: 20)
        XCTAssertEqual(events.count, 1)

        events.removeAll()
        XCTAssertTrue(events.isEmpty)
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `xcodegen generate && xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/AgentHeadActivityEventTests 2>&1 | tail -20`
Expected: FAIL — `appendToRingBuffer` not found

- [ ] **Step 3: Add ring buffer helper and activity event methods to AgentHead**

In `Sources/Core/AgentHead.swift`, add after the existing `handleWebhookEvent()` method (around line 297):

```swift
    // MARK: - Activity Events

    /// Ring buffer helper: insert at front, cap at maxSize.
    /// Exposed as static for testability.
    static func appendToRingBuffer(_ buffer: inout [ActivityEvent], event: ActivityEvent, maxSize: Int) {
        buffer.insert(event, at: 0)
        if buffer.count > maxSize {
            buffer.removeLast()
        }
    }

    /// Append an activity event for a terminal's agent.
    func appendActivityEvent(_ event: ActivityEvent, forTerminalID tid: String) {
        lock.lock()
        guard agents[tid] != nil else {
            lock.unlock()
            return
        }
        Self.appendToRingBuffer(&agents[tid]!.activityEvents, event: event, maxSize: 20)
        lock.unlock()
    }

    /// Replace activity events for a terminal (used by text-based extraction).
    func updateActivityEvents(_ events: [ActivityEvent], forTerminalID tid: String) {
        lock.lock()
        guard agents[tid] != nil else {
            lock.unlock()
            return
        }
        agents[tid]!.activityEvents = events
        lock.unlock()
    }

    /// Clear activity events for a terminal (on agent stop).
    func clearActivityEvents(forTerminalID tid: String) {
        lock.lock()
        agents[tid]?.activityEvents.removeAll()
        lock.unlock()
    }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/AgentHeadActivityEventTests 2>&1 | tail -20`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add Sources/Core/AgentHead.swift Tests/AgentHeadActivityEventTests.swift
git commit -m "feat: add activity event ring buffer methods to AgentHead"
```

---

### Task 3: Wire webhook events to activity events in AgentHead

**Files:**
- Modify: `Sources/Core/AgentHead.swift` (the `handleWebhookEvent` method)

- [ ] **Step 1: Update handleWebhookEvent to extract activity events**

In `Sources/Core/AgentHead.swift`, modify `handleWebhookEvent()` to create activity events from tool use events. Replace the existing method:

```swift
    /// Route a webhook event to the appropriate HooksChannel based on cwd matching
    func handleWebhookEvent(_ event: WebhookEvent) {
        lock.lock()
        // Find the agent whose worktree path matches the event's cwd
        let matchingTIDs = worktreeIndex.first { (worktreePath, _) in
            event.cwd == worktreePath || event.cwd.hasPrefix(worktreePath + "/")
        }?.value
        guard let tid = matchingTIDs?.first else {
            lock.unlock()
            return
        }
        let hooks = channels[tid] as? HooksChannel
        lock.unlock()

        hooks?.handleWebhookEvent(event)

        // Extract activity events from tool use events
        switch event.event {
        case .toolUseEnd, .toolUseFailed:
            let activityEvent = ActivityEventExtractor.extract(from: event)
            appendActivityEvent(activityEvent, forTerminalID: tid)
        case .agentStop:
            clearActivityEvents(forTerminalID: tid)
        default:
            break
        }
    }
```

- [ ] **Step 2: Build and run tests**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test 2>&1 | tail -20`
Expected: BUILD SUCCEEDED, tests pass

- [ ] **Step 3: Commit**

```bash
git add Sources/Core/AgentHead.swift
git commit -m "feat: create activity events from webhook tool use events"
```

---

### Task 4: Add terminal text activity event extraction to StatusDetector

**Files:**
- Modify: `Sources/Status/StatusDetector.swift`
- Test: `Tests/StatusDetectorActivityTests.swift`

- [ ] **Step 1: Write the test file**

```swift
// Tests/StatusDetectorActivityTests.swift
import XCTest
@testable import amux

final class StatusDetectorActivityTests: XCTestCase {
    let detector = StatusDetector()

    func testExtractClaudeCodeToolLines() {
        let text = """
        ⏺ Read(src/main.swift)
        ⏺ Edit(src/auth/login.swift)
        ⏺ Bash(swift test --filter Auth)
        """
        let events = detector.extractActivityEvents(from: text)
        XCTAssertEqual(events.count, 3)
        XCTAssertEqual(events[0].tool, "Bash")  // newest first (bottom of terminal = most recent)
        XCTAssertEqual(events[0].detail, "swift test --filter Auth")
        XCTAssertEqual(events[1].tool, "Edit")
        XCTAssertEqual(events[2].tool, "Read")
    }

    func testExtractWithErrorMarker() {
        let text = """
        ⏺ Read(config.json)
        ✗ Bash(swift build) — error
        """
        let events = detector.extractActivityEvents(from: text)
        XCTAssertEqual(events.count, 2)
        XCTAssertTrue(events[0].isError)  // ✗ line
        XCTAssertFalse(events[1].isError) // ⏺ line
    }

    func testExtractEmptyTextReturnsEmpty() {
        let events = detector.extractActivityEvents(from: "")
        XCTAssertTrue(events.isEmpty)
    }

    func testExtractNoToolLinesReturnsEmpty() {
        let text = "$ echo hello\nhello\n$ "
        let events = detector.extractActivityEvents(from: text)
        XCTAssertTrue(events.isEmpty)
    }

    func testExtractTriangleMarkerLines() {
        let text = """
        ▸ Read   src/main.swift
        ▸ Grep   "pattern"
        """
        let events = detector.extractActivityEvents(from: text)
        XCTAssertEqual(events.count, 2)
    }

    func testMaxEventsFromText() {
        var lines = ""
        for i in 0..<30 {
            lines += "⏺ Read(file\(i).swift)\n"
        }
        let events = detector.extractActivityEvents(from: lines)
        XCTAssertLessThanOrEqual(events.count, 20)
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `xcodegen generate && xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/StatusDetectorActivityTests 2>&1 | tail -20`
Expected: FAIL — `extractActivityEvents` not found

- [ ] **Step 3: Add extractActivityEvents to StatusDetector**

In `Sources/Status/StatusDetector.swift`, add the following method:

```swift
    /// Extract activity events from terminal viewport text.
    /// Looks for tool-call-like patterns (⏺ Tool(detail), ▸ Tool detail, ✗ Tool detail).
    /// Returns newest-first (bottom of terminal = most recent).
    func extractActivityEvents(from text: String) -> [ActivityEvent] {
        guard !text.isEmpty else { return [] }

        var events: [ActivityEvent] = []
        let lines = text.components(separatedBy: .newlines)

        // Regex patterns for Claude Code terminal output
        // ⏺ ToolName(args) or ✗ ToolName(args)
        let circlePattern = try! NSRegularExpression(pattern: #"^[[:space:]]*([⏺✗▸])\s+(\w+)\((.+?)\)"#)
        // ▸ ToolName   detail or ✗ ToolName   detail
        let arrowPattern = try! NSRegularExpression(pattern: #"^[[:space:]]*([▸✗])\s+(\w+)\s{2,}(.+)$"#)

        for line in lines {
            let nsLine = line as NSString
            let range = NSRange(location: 0, length: nsLine.length)

            var marker: String?
            var tool: String?
            var detail: String?

            if let match = circlePattern.firstMatch(in: line, range: range) {
                marker = nsLine.substring(with: match.range(at: 1))
                tool = nsLine.substring(with: match.range(at: 2))
                detail = nsLine.substring(with: match.range(at: 3))
            } else if let match = arrowPattern.firstMatch(in: line, range: range) {
                marker = nsLine.substring(with: match.range(at: 1))
                tool = nsLine.substring(with: match.range(at: 2))
                detail = nsLine.substring(with: match.range(at: 3))
            }

            if let marker, let tool, let detail {
                let isError = marker == "✗"
                events.append(ActivityEvent(
                    tool: tool,
                    detail: detail.trimmingCharacters(in: .whitespaces),
                    isError: isError,
                    timestamp: Date()
                ))
            }
        }

        // Reverse so newest (bottom of terminal) is first, cap at 20
        events.reverse()
        if events.count > 20 {
            events = Array(events.prefix(20))
        }
        return events
    }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/StatusDetectorActivityTests 2>&1 | tail -20`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add Sources/Status/StatusDetector.swift Tests/StatusDetectorActivityTests.swift
git commit -m "feat: extract activity events from terminal text patterns"
```

---

### Task 5: Wire StatusPublisher to pass text-extracted events to AgentHead

**Files:**
- Modify: `Sources/Status/StatusPublisher.swift`

- [ ] **Step 1: Add text-based activity event extraction to pollAll()**

In `Sources/Status/StatusPublisher.swift`, inside the `pollAll()` method, after the existing `AgentHead.shared.updateStatus(...)` call (around line 202), add:

```swift
            // Extract activity events from terminal text (for non-webhook agents)
            // Only if no webhook events exist (webhook takes priority)
            let webhookEvents = AgentHead.shared.agent(for: terminalID)?.activityEvents ?? []
            if webhookEvents.isEmpty {
                let textEvents = detector.extractActivityEvents(from: content)
                if !textEvents.isEmpty {
                    AgentHead.shared.updateActivityEvents(textEvents, forTerminalID: terminalID)
                }
            }
```

- [ ] **Step 2: Build and run tests**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test 2>&1 | tail -20`
Expected: BUILD SUCCEEDED, tests pass

- [ ] **Step 3: Commit**

```bash
git add Sources/Status/StatusPublisher.swift
git commit -m "feat: extract activity events from terminal text in StatusPublisher"
```

---

### Task 6: End-to-end integration test

**Files:**
- Create: `Tests/ActivityEventPipelineTests.swift`

- [ ] **Step 1: Write integration test**

```swift
// Tests/ActivityEventPipelineTests.swift
import XCTest
@testable import amux

final class ActivityEventPipelineTests: XCTestCase {

    func testExtractorProducesValidEvents() {
        // Simulate a full webhook event with tool_input data
        let data: [String: Any] = [
            "tool_name": "Read",
            "tool_input": ["file_path": "/Users/dev/project/Sources/Core/AgentHead.swift"],
        ]
        let event = WebhookEvent(
            source: "claude-code",
            sessionId: "s1",
            event: .toolUseEnd,
            cwd: "/Users/dev/project",
            timestamp: nil,
            data: data
        )

        let activity = ActivityEventExtractor.extract(from: event)
        XCTAssertEqual(activity.tool, "Read")
        XCTAssertEqual(activity.detail, "Core/AgentHead.swift")
        XCTAssertFalse(activity.isError)
    }

    func testExtractorHandlesBashError() {
        let data: [String: Any] = [
            "tool_name": "Bash",
            "tool_input": ["command": "swift test"],
            "tool_result": "Test Suite 'All tests' failed.\nExit code: 1",
        ]
        let event = WebhookEvent(
            source: "claude-code",
            sessionId: "s1",
            event: .toolUseEnd,
            cwd: "/tmp",
            timestamp: nil,
            data: data
        )

        let activity = ActivityEventExtractor.extract(from: event)
        XCTAssertEqual(activity.tool, "Bash")
        XCTAssertTrue(activity.isError)
    }

    func testRingBufferMaintainsOrder() {
        var buffer: [ActivityEvent] = []
        let tools = ["Read", "Edit", "Bash", "Grep", "Write"]
        for tool in tools {
            let event = ActivityEvent(tool: tool, detail: "test", isError: false, timestamp: Date())
            AgentHead.appendToRingBuffer(&buffer, event: event, maxSize: 20)
        }
        XCTAssertEqual(buffer[0].tool, "Write")  // newest
        XCTAssertEqual(buffer[4].tool, "Read")    // oldest
    }

    func testTextExtractionNewsetFirst() {
        let detector = StatusDetector()
        let text = """
        ⏺ Read(first.swift)
        ⏺ Edit(second.swift)
        ⏺ Bash(third command)
        """
        let events = detector.extractActivityEvents(from: text)
        XCTAssertEqual(events[0].tool, "Bash")   // bottom = newest
        XCTAssertEqual(events[2].tool, "Read")    // top = oldest
    }
}
```

- [ ] **Step 2: Run test to verify it passes**

Run: `xcodegen generate && xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/ActivityEventPipelineTests 2>&1 | tail -20`
Expected: PASS

- [ ] **Step 3: Run full test suite**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test 2>&1 | tail -20`
Expected: All tests pass

- [ ] **Step 4: Commit**

```bash
git add Tests/ActivityEventPipelineTests.swift
git commit -m "test: add activity event pipeline integration tests"
```
