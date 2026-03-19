# Webhook-Based Agent Status Detection Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace text-pattern-based agent status detection with a webhook-driven system that receives real-time events from Claude Code (and other agents), while keeping text matching as a parallel fallback.

**Architecture:** A lightweight HTTP server (`WebhookServer`) on localhost:7070 receives hook events, a `WebhookStatusProvider` tracks per-session status and aggregates by worktree, and `StatusPublisher` merges webhook status with existing text-pattern detection using `highestPriority`. Claude Code's native hook format is normalized to a generic protocol via an adapter layer.

**Tech Stack:** Swift 5.10, Network.framework (NWListener + CFHTTPMessage), XCTest

**Spec:** `docs/superpowers/specs/2026-03-20-webhook-status-detection-design.md`

---

## File Structure

| File | Responsibility |
|---|---|
| `Sources/Status/WebhookEvent.swift` | `WebhookEvent` struct, `WebhookEventType` enum, Claude Code adapter (normalize native payload → generic format) |
| `Sources/Status/WebhookStatusProvider.swift` | Session tracking (`SessionState`), cwd→worktree matching, multi-session aggregation via `highestPriority` |
| `Sources/Status/WebhookServer.swift` | NWListener HTTP server, CFHTTPMessage parsing, delegates to provider |
| `Sources/Status/StatusPublisher.swift` | (modify) Add `webhookProvider`, merge hook + text status in `pollAll()`, sync worktrees in `updateSurfaces()` |
| `Sources/App/MainWindowController.swift` | (modify) Start/stop `WebhookServer` lifecycle |
| `project.yml` | (modify) No changes needed — `Sources` is already included as a group |
| `Tests/WebhookEventTests.swift` | Unit tests for event parsing and Claude Code adapter |
| `Tests/WebhookStatusProviderTests.swift` | Unit tests for session tracking, cwd matching, aggregation, cleanup |
| `Tests/WebhookServerTests.swift` | Integration tests for HTTP server |

---

### Task 1: WebhookEvent struct and Claude Code adapter

**Files:**
- Create: `Sources/Status/WebhookEvent.swift`
- Create: `Tests/WebhookEventTests.swift`

- [ ] **Step 1: Write failing tests for WebhookEvent parsing**

```swift
// Tests/WebhookEventTests.swift
import XCTest
@testable import pmux

final class WebhookEventTests: XCTestCase {

    // MARK: - Generic protocol parsing

    func testParseGenericEvent() throws {
        let json = """
        {"source":"claude-code","session_id":"sess_1","event":"tool_use_start","cwd":"/tmp/project","timestamp":"2026-03-20T12:00:00Z","data":{"tool":"Bash"}}
        """.data(using: .utf8)!
        let event = try WebhookEvent.parse(from: json)
        XCTAssertEqual(event.source, "claude-code")
        XCTAssertEqual(event.sessionId, "sess_1")
        XCTAssertEqual(event.event, .toolUseStart)
        XCTAssertEqual(event.cwd, "/tmp/project")
    }

    func testParseGenericEventAllTypes() throws {
        let types: [(String, WebhookEventType)] = [
            ("session_start", .sessionStart),
            ("tool_use_start", .toolUseStart),
            ("tool_use_end", .toolUseEnd),
            ("agent_stop", .agentStop),
            ("notification", .notification),
            ("error", .error),
            ("prompt", .prompt),
        ]
        for (raw, expected) in types {
            let json = """
            {"source":"test","session_id":"s","event":"\(raw)","cwd":"/tmp"}
            """.data(using: .utf8)!
            let event = try WebhookEvent.parse(from: json)
            XCTAssertEqual(event.event, expected, "Failed for \(raw)")
        }
    }

    func testParseMissingRequiredFieldThrows() {
        let json = """
        {"source":"test","event":"agent_stop","cwd":"/tmp"}
        """.data(using: .utf8)!  // missing session_id
        XCTAssertThrowsError(try WebhookEvent.parse(from: json))
    }

    // MARK: - Claude Code native payload adapter

    func testParseClaudeCodePreToolUse() throws {
        let json = """
        {"hook_event_name":"PreToolUse","session_id":"sess_abc","cwd":"/tmp/project","tool_name":"Bash","tool_input":{"command":"ls"}}
        """.data(using: .utf8)!
        let event = try WebhookEvent.parse(from: json)
        XCTAssertEqual(event.source, "claude-code")
        XCTAssertEqual(event.sessionId, "sess_abc")
        XCTAssertEqual(event.event, .toolUseStart)
        XCTAssertEqual(event.cwd, "/tmp/project")
    }

    func testParseClaudeCodeStop() throws {
        let json = """
        {"hook_event_name":"Stop","session_id":"sess_abc","cwd":"/tmp/project","stop_reason":"end_turn"}
        """.data(using: .utf8)!
        let event = try WebhookEvent.parse(from: json)
        XCTAssertEqual(event.event, .agentStop)
    }

    func testParseClaudeCodeNotification() throws {
        let json = """
        {"hook_event_name":"Notification","session_id":"sess_abc","cwd":"/tmp/project","title":"Done","message":"All tests pass","level":"info"}
        """.data(using: .utf8)!
        let event = try WebhookEvent.parse(from: json)
        XCTAssertEqual(event.event, .notification)
        XCTAssertEqual(event.data?["level"] as? String, "info")
        XCTAssertEqual(event.data?["message"] as? String, "All tests pass")
    }

    func testParseClaudeCodeSessionStart() throws {
        let json = """
        {"hook_event_name":"SessionStart","session_id":"sess_new","cwd":"/tmp/project"}
        """.data(using: .utf8)!
        let event = try WebhookEvent.parse(from: json)
        XCTAssertEqual(event.event, .sessionStart)
        XCTAssertEqual(event.source, "claude-code")
    }

    func testParseClaudeCodePostToolUse() throws {
        let json = """
        {"hook_event_name":"PostToolUse","session_id":"sess_abc","cwd":"/tmp/project","tool_name":"Read"}
        """.data(using: .utf8)!
        let event = try WebhookEvent.parse(from: json)
        XCTAssertEqual(event.event, .toolUseEnd)
    }

    // MARK: - Event → AgentStatus mapping

    func testEventToAgentStatus() {
        XCTAssertEqual(WebhookEventType.sessionStart.agentStatus(data: nil), .running)
        XCTAssertEqual(WebhookEventType.toolUseStart.agentStatus(data: nil), .running)
        XCTAssertEqual(WebhookEventType.toolUseEnd.agentStatus(data: nil), .running)
        XCTAssertEqual(WebhookEventType.agentStop.agentStatus(data: nil), .idle)
        XCTAssertEqual(WebhookEventType.error.agentStatus(data: nil), .error)
        XCTAssertEqual(WebhookEventType.prompt.agentStatus(data: nil), .waiting)
    }

    func testNotificationLevelMapping() {
        XCTAssertEqual(WebhookEventType.notification.agentStatus(data: ["level": "error"]), .error)
        XCTAssertEqual(WebhookEventType.notification.agentStatus(data: ["level": "warning"]), .waiting)
        XCTAssertEqual(WebhookEventType.notification.agentStatus(data: ["level": "info"]), .idle)
        XCTAssertEqual(WebhookEventType.notification.agentStatus(data: nil), .idle)
    }

    // MARK: - Invalid JSON

    func testParseInvalidJSON() {
        let json = "not json".data(using: .utf8)!
        XCTAssertThrowsError(try WebhookEvent.parse(from: json))
    }

    func testParseUnknownEventType() throws {
        let json = """
        {"source":"test","session_id":"s","event":"unknown_event","cwd":"/tmp"}
        """.data(using: .utf8)!
        XCTAssertThrowsError(try WebhookEvent.parse(from: json))
    }

    func testParseUnknownClaudeHookType() throws {
        let json = """
        {"hook_event_name":"UnknownHook","session_id":"s","cwd":"/tmp"}
        """.data(using: .utf8)!
        XCTAssertThrowsError(try WebhookEvent.parse(from: json))
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmuxTests -configuration Debug test -only-testing:pmuxTests/WebhookEventTests 2>&1 | tail -5`
Expected: Compilation error — `WebhookEvent` not defined

- [ ] **Step 3: Implement WebhookEvent and Claude Code adapter**

```swift
// Sources/Status/WebhookEvent.swift
import Foundation

enum WebhookEventType: String {
    case sessionStart = "session_start"
    case toolUseStart = "tool_use_start"
    case toolUseEnd = "tool_use_end"
    case agentStop = "agent_stop"
    case notification = "notification"
    case error = "error"
    case prompt = "prompt"

    func agentStatus(data: [String: Any]?) -> AgentStatus {
        switch self {
        case .sessionStart, .toolUseStart, .toolUseEnd:
            return .running
        case .agentStop:
            return .idle
        case .error:
            return .error
        case .prompt:
            return .waiting
        case .notification:
            let level = data?["level"] as? String
            switch level {
            case "error": return .error
            case "warning": return .waiting
            default: return .idle
            }
        }
    }

    /// Map Claude Code hook_event_name to generic event type
    static func fromClaudeCode(_ hookEventName: String) -> WebhookEventType? {
        switch hookEventName {
        case "SessionStart": return .sessionStart
        case "PreToolUse": return .toolUseStart
        case "PostToolUse": return .toolUseEnd
        case "Stop", "SubagentStop": return .agentStop
        case "Notification": return .notification
        default: return nil
        }
    }
}

struct WebhookEvent {
    let source: String
    let sessionId: String
    let event: WebhookEventType
    let cwd: String
    let timestamp: String?
    let data: [String: Any]?

    /// Parse from JSON data. Supports both generic protocol and Claude Code native format.
    static func parse(from jsonData: Data) throws -> WebhookEvent {
        guard let json = try JSONSerialization.jsonObject(with: jsonData) as? [String: Any] else {
            throw WebhookEventError.invalidJSON
        }

        // Detect format: Claude Code native has "hook_event_name", generic has "event"
        if let hookEventName = json["hook_event_name"] as? String {
            return try parseClaudeCode(json: json, hookEventName: hookEventName)
        } else {
            return try parseGeneric(json: json)
        }
    }

    private static func parseGeneric(json: [String: Any]) throws -> WebhookEvent {
        guard let source = json["source"] as? String,
              let sessionId = json["session_id"] as? String,
              let eventRaw = json["event"] as? String,
              let cwd = json["cwd"] as? String else {
            throw WebhookEventError.missingRequiredField
        }
        guard let event = WebhookEventType(rawValue: eventRaw) else {
            throw WebhookEventError.unknownEventType(eventRaw)
        }
        return WebhookEvent(
            source: source,
            sessionId: sessionId,
            event: event,
            cwd: cwd,
            timestamp: json["timestamp"] as? String,
            data: json["data"] as? [String: Any]
        )
    }

    private static func parseClaudeCode(json: [String: Any], hookEventName: String) throws -> WebhookEvent {
        guard let sessionId = json["session_id"] as? String,
              let cwd = json["cwd"] as? String else {
            throw WebhookEventError.missingRequiredField
        }
        guard let event = WebhookEventType.fromClaudeCode(hookEventName) else {
            throw WebhookEventError.unknownEventType(hookEventName)
        }

        // Collect remaining fields as data
        var data: [String: Any] = [:]
        let reservedKeys: Set<String> = ["hook_event_name", "session_id", "cwd", "transcript_path", "permission_mode"]
        for (key, value) in json where !reservedKeys.contains(key) {
            data[key] = value
        }

        return WebhookEvent(
            source: "claude-code",
            sessionId: sessionId,
            event: event,
            cwd: cwd,
            timestamp: nil,
            data: data.isEmpty ? nil : data
        )
    }
}

enum WebhookEventError: Error {
    case invalidJSON
    case missingRequiredField
    case unknownEventType(String)
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmuxTests -configuration Debug test -only-testing:pmuxTests/WebhookEventTests 2>&1 | tail -5`
Expected: All tests PASS

- [ ] **Step 5: Commit**

```bash
git add Sources/Status/WebhookEvent.swift Tests/WebhookEventTests.swift
git commit -m "feat: add WebhookEvent struct with Claude Code adapter"
```

---

### Task 2: WebhookStatusProvider

**Files:**
- Create: `Sources/Status/WebhookStatusProvider.swift`
- Create: `Tests/WebhookStatusProviderTests.swift`

- [ ] **Step 1: Write failing tests for WebhookStatusProvider**

```swift
// Tests/WebhookStatusProviderTests.swift
import XCTest
@testable import pmux

final class WebhookStatusProviderTests: XCTestCase {

    var provider: WebhookStatusProvider!

    override func setUp() {
        super.setUp()
        provider = WebhookStatusProvider()
        provider.updateWorktrees(["/projects/repo/main", "/projects/repo/feature"])
    }

    // MARK: - Basic event handling

    func testNoEventsReturnsUnknown() {
        XCTAssertEqual(provider.status(for: "/projects/repo/main"), .unknown)
    }

    func testSessionStartSetsRunning() {
        let event = makeEvent(sessionId: "s1", event: .sessionStart, cwd: "/projects/repo/main")
        provider.handleEvent(event)
        XCTAssertEqual(provider.status(for: "/projects/repo/main"), .running)
    }

    func testAgentStopSetsIdle() {
        provider.handleEvent(makeEvent(sessionId: "s1", event: .sessionStart, cwd: "/projects/repo/main"))
        provider.handleEvent(makeEvent(sessionId: "s1", event: .agentStop, cwd: "/projects/repo/main"))
        XCTAssertEqual(provider.status(for: "/projects/repo/main"), .idle)
    }

    func testToolUseKeepsRunning() {
        provider.handleEvent(makeEvent(sessionId: "s1", event: .sessionStart, cwd: "/projects/repo/main"))
        provider.handleEvent(makeEvent(sessionId: "s1", event: .toolUseStart, cwd: "/projects/repo/main"))
        XCTAssertEqual(provider.status(for: "/projects/repo/main"), .running)
    }

    func testErrorEvent() {
        provider.handleEvent(makeEvent(sessionId: "s1", event: .error, cwd: "/projects/repo/main"))
        XCTAssertEqual(provider.status(for: "/projects/repo/main"), .error)
    }

    func testPromptEvent() {
        provider.handleEvent(makeEvent(sessionId: "s1", event: .prompt, cwd: "/projects/repo/main"))
        XCTAssertEqual(provider.status(for: "/projects/repo/main"), .waiting)
    }

    // MARK: - cwd matching

    func testExactCwdMatch() {
        provider.handleEvent(makeEvent(sessionId: "s1", event: .sessionStart, cwd: "/projects/repo/feature"))
        XCTAssertEqual(provider.status(for: "/projects/repo/feature"), .running)
        XCTAssertEqual(provider.status(for: "/projects/repo/main"), .unknown)
    }

    func testPrefixCwdMatch() {
        // Agent running in a subdirectory of the worktree
        provider.handleEvent(makeEvent(sessionId: "s1", event: .sessionStart, cwd: "/projects/repo/main/src/lib"))
        XCTAssertEqual(provider.status(for: "/projects/repo/main"), .running)
    }

    func testUnknownCwdDiscarded() {
        provider.handleEvent(makeEvent(sessionId: "s1", event: .sessionStart, cwd: "/unknown/path"))
        XCTAssertEqual(provider.status(for: "/projects/repo/main"), .unknown)
        XCTAssertEqual(provider.status(for: "/projects/repo/feature"), .unknown)
    }

    // MARK: - Multi-session aggregation

    func testMultipleSessionsSameWorktreeAggregates() {
        provider.handleEvent(makeEvent(sessionId: "s1", event: .agentStop, cwd: "/projects/repo/main"))
        provider.handleEvent(makeEvent(sessionId: "s2", event: .sessionStart, cwd: "/projects/repo/main"))
        // s1=idle, s2=running → aggregated = running (higher priority)
        XCTAssertEqual(provider.status(for: "/projects/repo/main"), .running)
    }

    func testMultipleSessionsBothIdle() {
        provider.handleEvent(makeEvent(sessionId: "s1", event: .agentStop, cwd: "/projects/repo/main"))
        provider.handleEvent(makeEvent(sessionId: "s2", event: .agentStop, cwd: "/projects/repo/main"))
        XCTAssertEqual(provider.status(for: "/projects/repo/main"), .idle)
    }

    func testSessionsIsolatedBetweenWorktrees() {
        provider.handleEvent(makeEvent(sessionId: "s1", event: .sessionStart, cwd: "/projects/repo/main"))
        provider.handleEvent(makeEvent(sessionId: "s2", event: .error, cwd: "/projects/repo/feature"))
        XCTAssertEqual(provider.status(for: "/projects/repo/main"), .running)
        XCTAssertEqual(provider.status(for: "/projects/repo/feature"), .error)
    }

    // MARK: - Notification level mapping

    func testNotificationErrorLevel() {
        let event = makeEvent(sessionId: "s1", event: .notification, cwd: "/projects/repo/main", data: ["level": "error"])
        provider.handleEvent(event)
        XCTAssertEqual(provider.status(for: "/projects/repo/main"), .error)
    }

    func testNotificationWarningLevel() {
        let event = makeEvent(sessionId: "s1", event: .notification, cwd: "/projects/repo/main", data: ["level": "warning"])
        provider.handleEvent(event)
        XCTAssertEqual(provider.status(for: "/projects/repo/main"), .waiting)
    }

    func testNotificationInfoLevel() {
        let event = makeEvent(sessionId: "s1", event: .notification, cwd: "/projects/repo/main", data: ["level": "info"])
        provider.handleEvent(event)
        XCTAssertEqual(provider.status(for: "/projects/repo/main"), .idle)
    }

    // MARK: - Path normalization

    func testTrailingSlashNormalized() {
        provider.updateWorktrees(["/projects/repo/main/"])
        provider.handleEvent(makeEvent(sessionId: "s1", event: .sessionStart, cwd: "/projects/repo/main"))
        XCTAssertEqual(provider.status(for: "/projects/repo/main"), .running)
        XCTAssertEqual(provider.status(for: "/projects/repo/main/"), .running)
    }

    // MARK: - Worktree cleanup

    func testUpdateWorktreesRemovesStaleSessions() {
        provider.handleEvent(makeEvent(sessionId: "s1", event: .sessionStart, cwd: "/projects/repo/main"))
        provider.handleEvent(makeEvent(sessionId: "s2", event: .sessionStart, cwd: "/projects/repo/feature"))
        // Remove "feature" worktree
        provider.updateWorktrees(["/projects/repo/main"])
        XCTAssertEqual(provider.status(for: "/projects/repo/main"), .running)
        XCTAssertEqual(provider.status(for: "/projects/repo/feature"), .unknown)
    }

    // MARK: - Helpers

    private func makeEvent(
        sessionId: String,
        event: WebhookEventType,
        cwd: String,
        data: [String: Any]? = nil
    ) -> WebhookEvent {
        WebhookEvent(
            source: "claude-code",
            sessionId: sessionId,
            event: event,
            cwd: cwd,
            timestamp: nil,
            data: data
        )
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmuxTests -configuration Debug test -only-testing:pmuxTests/WebhookStatusProviderTests 2>&1 | tail -5`
Expected: Compilation error — `WebhookStatusProvider` not defined

- [ ] **Step 3: Implement WebhookStatusProvider**

```swift
// Sources/Status/WebhookStatusProvider.swift
import Foundation

class WebhookStatusProvider {
    private let queue = DispatchQueue(label: "pmux.webhook-status")
    private var sessions: [String: SessionState] = [:]
    private var knownWorktrees: [String] = []

    struct SessionState {
        let sessionId: String
        let worktreePath: String
        var status: AgentStatus
        var lastEvent: Date
    }

    func updateWorktrees(_ paths: [String]) {
        queue.sync {
            knownWorktrees = paths.map { canonicalize($0) }
            // Remove sessions for worktrees no longer tracked
            sessions = sessions.filter { (_, state) in
                knownWorktrees.contains(state.worktreePath)
            }
            // Prune stale sessions (no events for >1 hour)
            let cutoff = Date().addingTimeInterval(-3600)
            sessions = sessions.filter { $0.value.lastEvent > cutoff }
        }
    }

    func handleEvent(_ event: WebhookEvent) {
        queue.sync {
            let canonCwd = canonicalize(event.cwd)
            guard let worktreePath = matchWorktree(canonCwd) else {
                NSLog("[WebhookStatusProvider] No worktree match for cwd: \(event.cwd)")
                return
            }

            let status = event.event.agentStatus(data: event.data)
            if var existing = sessions[event.sessionId] {
                existing.status = status
                existing.lastEvent = Date()
                sessions[event.sessionId] = existing
            } else {
                sessions[event.sessionId] = SessionState(
                    sessionId: event.sessionId,
                    worktreePath: worktreePath,
                    status: status,
                    lastEvent: Date()
                )
            }
        }
    }

    func status(for worktreePath: String) -> AgentStatus {
        queue.sync {
            let canon = canonicalize(worktreePath)
            let sessionStatuses = sessions.values
                .filter { $0.worktreePath == canon }
                .map { $0.status }
            return AgentStatus.highestPriority(sessionStatuses)
        }
    }

    private func matchWorktree(_ canonCwd: String) -> String? {
        // Exact match first
        if knownWorktrees.contains(canonCwd) {
            return canonCwd
        }
        // Prefix match (agent in subdirectory)
        for worktree in knownWorktrees {
            if canonCwd.hasPrefix(worktree + "/") {
                return worktree
            }
        }
        return nil
    }

    private func canonicalize(_ path: String) -> String {
        URL(fileURLWithPath: path).standardized.resolvingSymlinksInPath().path
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmuxTests -configuration Debug test -only-testing:pmuxTests/WebhookStatusProviderTests 2>&1 | tail -5`
Expected: All tests PASS

- [ ] **Step 5: Commit**

```bash
git add Sources/Status/WebhookStatusProvider.swift Tests/WebhookStatusProviderTests.swift
git commit -m "feat: add WebhookStatusProvider with session tracking and cwd matching"
```

---

### Task 3: WebhookServer (HTTP listener)

**Files:**
- Create: `Sources/Status/WebhookServer.swift`
- Create: `Tests/WebhookServerTests.swift`

- [ ] **Step 1: Write failing integration test for WebhookServer**

```swift
// Tests/WebhookServerTests.swift
import XCTest
@testable import pmux

final class WebhookServerTests: XCTestCase {

    var server: WebhookServer!
    let lock = NSLock()
    var _receivedEvents: [WebhookEvent] = []
    var receivedEvents: [WebhookEvent] {
        lock.lock()
        defer { lock.unlock() }
        return _receivedEvents
    }
    let testPort: UInt16 = 17070  // avoid conflict with running pmux

    override func setUp() {
        super.setUp()
        _receivedEvents = []
        server = WebhookServer(port: testPort) { [weak self] event in
            guard let self = self else { return }
            self.lock.lock()
            self._receivedEvents.append(event)
            self.lock.unlock()
        }
    }

    override func tearDown() {
        server.stop()
        super.tearDown()
    }

    func testServerReceivesValidEvent() throws {
        server.start()
        let expectation = expectation(description: "event received")

        let json = """
        {"source":"test","session_id":"s1","event":"session_start","cwd":"/tmp"}
        """.data(using: .utf8)!

        postToWebhook(body: json) { statusCode in
            XCTAssertEqual(statusCode, 200)
            expectation.fulfill()
        }

        wait(for: [expectation], timeout: 5.0)
        XCTAssertEqual(receivedEvents.count, 1)
        XCTAssertEqual(receivedEvents.first?.event, .sessionStart)
    }

    func testServerRejects404ForWrongPath() throws {
        server.start()
        let expectation = expectation(description: "response received")

        let json = "{}".data(using: .utf8)!
        postToURL("http://localhost:\(testPort)/wrong", body: json) { statusCode in
            XCTAssertEqual(statusCode, 404)
            expectation.fulfill()
        }

        wait(for: [expectation], timeout: 5.0)
        XCTAssertEqual(receivedEvents.count, 0)
    }

    func testServerRejects400ForMalformedJSON() throws {
        server.start()
        let expectation = expectation(description: "response received")

        let json = "not json".data(using: .utf8)!
        postToWebhook(body: json) { statusCode in
            XCTAssertEqual(statusCode, 400)
            expectation.fulfill()
        }

        wait(for: [expectation], timeout: 5.0)
        XCTAssertEqual(receivedEvents.count, 0)
    }

    func testServerRejects404ForGetRequest() throws {
        server.start()
        let expectation = expectation(description: "response received")

        var request = URLRequest(url: URL(string: "http://localhost:\(testPort)/webhook")!)
        request.httpMethod = "GET"
        URLSession.shared.dataTask(with: request) { _, response, _ in
            let statusCode = (response as? HTTPURLResponse)?.statusCode ?? 0
            XCTAssertEqual(statusCode, 404)
            expectation.fulfill()
        }.resume()

        wait(for: [expectation], timeout: 5.0)
        XCTAssertEqual(receivedEvents.count, 0)
    }

    func testServerHandlesClaudeCodeNativeFormat() throws {
        server.start()
        let expectation = expectation(description: "event received")

        let json = """
        {"hook_event_name":"PreToolUse","session_id":"sess_abc","cwd":"/tmp","tool_name":"Bash"}
        """.data(using: .utf8)!

        postToWebhook(body: json) { statusCode in
            XCTAssertEqual(statusCode, 200)
            expectation.fulfill()
        }

        wait(for: [expectation], timeout: 5.0)
        XCTAssertEqual(receivedEvents.count, 1)
        XCTAssertEqual(receivedEvents.first?.source, "claude-code")
        XCTAssertEqual(receivedEvents.first?.event, .toolUseStart)
    }

    // MARK: - Helpers

    private func postToWebhook(body: Data, completion: @escaping (Int) -> Void) {
        postToURL("http://localhost:\(testPort)/webhook", body: body, completion: completion)
    }

    private func postToURL(_ urlString: String, body: Data, completion: @escaping (Int) -> Void) {
        var request = URLRequest(url: URL(string: urlString)!)
        request.httpMethod = "POST"
        request.httpBody = body
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        URLSession.shared.dataTask(with: request) { _, response, _ in
            let statusCode = (response as? HTTPURLResponse)?.statusCode ?? 0
            completion(statusCode)
        }.resume()
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmuxTests -configuration Debug test -only-testing:pmuxTests/WebhookServerTests 2>&1 | tail -5`
Expected: Compilation error — `WebhookServer` not defined

- [ ] **Step 3: Implement WebhookServer**

```swift
// Sources/Status/WebhookServer.swift
import Foundation
import Network
import CFNetwork

class WebhookServer {
    private var listener: NWListener?
    private let port: UInt16
    private let onEvent: (WebhookEvent) -> Void
    private let queue = DispatchQueue(label: "pmux.webhook-server")

    init(port: UInt16, onEvent: @escaping (WebhookEvent) -> Void) {
        self.port = port
        self.onEvent = onEvent
    }

    func start() {
        do {
            let params = NWParameters.tcp
            listener = try NWListener(using: params, on: NWEndpoint.Port(rawValue: port)!)
        } catch {
            NSLog("[WebhookServer] Failed to create listener: \(error)")
            return
        }

        listener?.newConnectionHandler = { [weak self] connection in
            self?.handleConnection(connection)
        }

        listener?.stateUpdateHandler = { [weak self] state in
            guard let self = self else { return }
            switch state {
            case .ready:
                NSLog("[WebhookServer] Listening on port \(self.port)")
            case .failed(let error):
                NSLog("[WebhookServer] Failed: \(error)")
            default:
                break
            }
        }

        listener?.start(queue: queue)
    }

    func stop() {
        listener?.cancel()
        listener = nil
    }

    private func handleConnection(_ connection: NWConnection) {
        connection.start(queue: queue)
        receiveData(connection: connection, buffer: Data())
    }

    private func receiveData(connection: NWConnection, buffer: Data) {
        connection.receive(minimumIncompleteLength: 1, maximumLength: 65536) { [weak self] data, _, isComplete, error in
            guard let self = self else { return }

            var accumulated = buffer
            if let data = data {
                accumulated.append(data)
            }

            if isComplete || error != nil {
                self.processHTTPRequest(data: accumulated, connection: connection)
            } else {
                // Check if we have a complete HTTP request
                if self.hasCompleteHTTPRequest(accumulated) {
                    self.processHTTPRequest(data: accumulated, connection: connection)
                } else {
                    self.receiveData(connection: connection, buffer: accumulated)
                }
            }
        }
    }

    private func hasCompleteHTTPRequest(_ data: Data) -> Bool {
        let message = CFHTTPMessageCreateEmpty(kCFAllocatorDefault, true).takeRetainedValue()
        CFHTTPMessageAppendBytes(message, [UInt8](data), data.count)
        guard CFHTTPMessageIsHeaderComplete(message) else { return false }
        // Check Content-Length to determine if full body has arrived
        guard let contentLengthStr = CFHTTPMessageCopyHeaderFieldValue(message, "Content-Length" as CFString)?.takeRetainedValue() as String?,
              let contentLength = Int(contentLengthStr) else {
            return true  // No Content-Length means no body expected
        }
        let body = CFHTTPMessageCopyBody(message)?.takeRetainedValue() as Data?
        return (body?.count ?? 0) >= contentLength
    }

    private func processHTTPRequest(data: Data, connection: NWConnection) {
        let message = CFHTTPMessageCreateEmpty(kCFAllocatorDefault, true).takeRetainedValue()
        CFHTTPMessageAppendBytes(message, [UInt8](data), data.count)

        guard CFHTTPMessageIsHeaderComplete(message) else {
            sendResponse(connection: connection, statusCode: 400, body: "Bad Request")
            return
        }

        let method = CFHTTPMessageCopyRequestMethod(message)?.takeRetainedValue() as String? ?? ""
        let url = CFHTTPMessageCopyRequestURL(message)?.takeRetainedValue() as URL?
        let path = url?.path ?? ""
        let body = CFHTTPMessageCopyBody(message)?.takeRetainedValue() as Data?

        guard method == "POST", path == "/webhook" else {
            sendResponse(connection: connection, statusCode: 404, body: "Not Found")
            return
        }

        guard let body = body else {
            sendResponse(connection: connection, statusCode: 400, body: "Missing body")
            return
        }

        do {
            let event = try WebhookEvent.parse(from: body)
            onEvent(event)
            sendResponse(connection: connection, statusCode: 200, body: "")
        } catch {
            NSLog("[WebhookServer] Parse error: \(error)")
            sendResponse(connection: connection, statusCode: 400, body: "Bad Request")
        }
    }

    private func sendResponse(connection: NWConnection, statusCode: Int, body: String) {
        let statusText: String
        switch statusCode {
        case 200: statusText = "OK"
        case 400: statusText = "Bad Request"
        case 404: statusText = "Not Found"
        default: statusText = "Error"
        }

        let response = "HTTP/1.1 \(statusCode) \(statusText)\r\nContent-Length: \(body.utf8.count)\r\nConnection: close\r\n\r\n\(body)"
        let responseData = response.data(using: .utf8)!

        connection.send(content: responseData, completion: .contentProcessed { _ in
            connection.cancel()
        })
    }

    deinit {
        stop()
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmuxTests -configuration Debug test -only-testing:pmuxTests/WebhookServerTests 2>&1 | tail -5`
Expected: All tests PASS

- [ ] **Step 5: Commit**

```bash
git add Sources/Status/WebhookServer.swift Tests/WebhookServerTests.swift
git commit -m "feat: add WebhookServer HTTP listener with CFHTTPMessage parsing"
```

---

### Task 4: Integrate into StatusPublisher

**Files:**
- Modify: `Sources/Status/StatusPublisher.swift`

- [ ] **Step 1: Add webhookProvider property to StatusPublisher**

In `Sources/Status/StatusPublisher.swift`, add a `webhookProvider` property and update `updateSurfaces` to sync worktrees:

```swift
// Add after line 16 (after lastMessages declaration):
private(set) var webhookProvider = WebhookStatusProvider()

// In updateSurfaces(), add after the tracker loop (after line 55):
webhookProvider.updateWorktrees(Array(surfaces.keys))

// In start(), add after the tracker loop (after line 34):
webhookProvider.updateWorktrees(Array(surfaces.keys))
```

- [ ] **Step 2: Modify pollAll() to merge hook + text status**

Replace the detection logic in `pollAll()` (lines 72-77 of `StatusPublisher.swift`):

Current:
```swift
let detected = detector.detect(
    processStatus: processStatus,
    shellInfo: nil,
    content: content,
    agentDef: agentDef
)
```

New:
```swift
let textStatus = detector.detect(
    processStatus: processStatus,
    shellInfo: nil,
    content: content,
    agentDef: agentDef
)
let hookStatus = webhookProvider.status(for: path)
let detected = AgentStatus.highestPriority([textStatus, hookStatus])
```

- [ ] **Step 3: Run existing tests to verify nothing breaks**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmuxTests -configuration Debug test 2>&1 | tail -10`
Expected: All existing tests PASS (webhook provider returns `.unknown` when no events, so `highestPriority` falls through to text status as before)

- [ ] **Step 4: Commit**

```bash
git add Sources/Status/StatusPublisher.swift
git commit -m "feat: integrate WebhookStatusProvider into StatusPublisher polling"
```

---

### Task 5: Wire up WebhookServer in MainWindowController

**Files:**
- Modify: `Sources/App/MainWindowController.swift`

- [ ] **Step 1: Add WebhookServer property**

In `MainWindowController`, add after the `statusPublisher` lazy var (around line 29):

```swift
private var webhookServer: WebhookServer?
```

- [ ] **Step 2: Start server in loadWorkspaces()**

In `loadWorkspaces()`, after `statusPublisher.start(surfaces: surfaces)` (around line 597), add:

```swift
if config.webhook.enabled {
    let server = WebhookServer(port: config.webhook.port) { [weak self] event in
        self?.statusPublisher.webhookProvider.handleEvent(event)
    }
    server.start()
    webhookServer = server
}
```

- [ ] **Step 3: Stop server in windowWillClose()**

In `windowWillClose(_:)` (around line 762), after `statusPublisher.stop()`, add:

```swift
webhookServer?.stop()
webhookServer = nil
```

- [ ] **Step 4: Build to verify compilation**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmux -configuration Debug build 2>&1 | tail -5`
Expected: BUILD SUCCEEDED

- [ ] **Step 5: Commit**

```bash
git add Sources/App/MainWindowController.swift
git commit -m "feat: wire WebhookServer lifecycle into MainWindowController"
```

---

### Task 6: Regenerate Xcode project and verify full build

- [ ] **Step 1: Regenerate Xcode project**

Run: `cd /Users/matt.chow/workspace/pmux-swift && xcodegen generate`
Expected: `Generated project at pmux.xcodeproj`

Note: `project.yml` uses `Sources` as a group source, so new files under `Sources/Status/` are automatically included. No `project.yml` changes needed.

- [ ] **Step 2: Full build**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmux -configuration Debug build 2>&1 | tail -5`
Expected: BUILD SUCCEEDED

- [ ] **Step 3: Run all tests**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmuxTests -configuration Debug test 2>&1 | tail -10`
Expected: All tests PASS

- [ ] **Step 4: Commit if any changes needed**

```bash
git add -A && git commit -m "chore: regenerate Xcode project with webhook files"
```
