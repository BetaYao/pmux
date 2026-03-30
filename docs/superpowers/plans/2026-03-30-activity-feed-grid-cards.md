# Activity Feed on Grid Cards — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the empty terminal container area on dashboard grid cards with a live, terminal-log-style activity feed showing recent agent actions.

**Architecture:** Add an `ActivityEvent` model, thread it through `AgentDisplayInfo`, and render it in `AgentCardView` as a stack of monospaced labels with progressive opacity fade. The data source is left pluggable — any provider can populate the events array.

**Tech Stack:** Swift 5.10, AppKit, XCTest

---

### Task 1: Add ActivityEvent model

**Files:**
- Create: `Sources/Core/ActivityEvent.swift`
- Test: `Tests/ActivityEventTests.swift`

- [ ] **Step 1: Write the test file**

```swift
// Tests/ActivityEventTests.swift
import XCTest
@testable import amux

final class ActivityEventTests: XCTestCase {
    func testActivityEventProperties() {
        let date = Date()
        let event = ActivityEvent(tool: "Read", detail: "src/main.swift", isError: false, timestamp: date)
        XCTAssertEqual(event.tool, "Read")
        XCTAssertEqual(event.detail, "src/main.swift")
        XCTAssertFalse(event.isError)
        XCTAssertEqual(event.timestamp, date)
    }

    func testErrorEvent() {
        let event = ActivityEvent(tool: "Bash", detail: "swift test — 2 failures", isError: true, timestamp: Date())
        XCTAssertTrue(event.isError)
        XCTAssertEqual(event.tool, "Bash")
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/ActivityEventTests 2>&1 | tail -20`
Expected: FAIL — `ActivityEvent` not found

- [ ] **Step 3: Create the model**

```swift
// Sources/Core/ActivityEvent.swift
import Foundation

struct ActivityEvent {
    let tool: String
    let detail: String
    let isError: Bool
    let timestamp: Date
}
```

- [ ] **Step 4: Add file to Xcode project, run test to verify it passes**

Run `xcodegen generate` to pick up the new file, then:
Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/ActivityEventTests 2>&1 | tail -20`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add Sources/Core/ActivityEvent.swift Tests/ActivityEventTests.swift
git commit -m "feat: add ActivityEvent model"
```

---

### Task 2: Add activityEvents to AgentDisplayInfo

**Files:**
- Modify: `Sources/UI/Dashboard/DashboardViewController.swift` (the `AgentDisplayInfo` struct, around line 16)
- Modify: `Sources/App/TabCoordinator.swift` (where `AgentDisplayInfo` is constructed, around line 248)

- [ ] **Step 1: Add `activityEvents` property to `AgentDisplayInfo`**

In `Sources/UI/Dashboard/DashboardViewController.swift`, add the new property to the struct:

```swift
struct AgentDisplayInfo {
    let id: String
    let name: String
    let project: String
    let thread: String
    let paneStatuses: [AgentStatus]
    let mostRecentMessage: String
    let mostRecentPaneIndex: Int
    let totalDuration: String
    let roundDuration: String
    let surface: TerminalSurface
    let worktreePath: String
    let paneCount: Int
    let paneSurfaces: [TerminalSurface]
    let tasks: [TaskItem]
    let activityEvents: [ActivityEvent]  // <-- ADD THIS LINE

    // ... existing computed properties unchanged ...
}
```

- [ ] **Step 2: Update TabCoordinator construction site**

In `Sources/App/TabCoordinator.swift`, at the `AgentDisplayInfo(` call (around line 248), add the new parameter with an empty default for now:

```swift
result.append(AgentDisplayInfo(
    id: agent.id,
    name: agent.branch,
    project: agent.project,
    thread: agent.branch,
    paneStatuses: paneStatuses,
    mostRecentMessage: mostRecentMessage,
    mostRecentPaneIndex: mostRecentPaneIndex,
    totalDuration: AgentDisplayHelpers.formatDuration(agent.totalDuration),
    roundDuration: AgentDisplayHelpers.formatDuration(agent.roundDuration),
    surface: surface,
    worktreePath: agent.worktreePath,
    paneCount: paneCount,
    paneSurfaces: paneSurfaces,
    tasks: agent.tasks,
    activityEvents: []  // <-- ADD THIS LINE
))
```

- [ ] **Step 3: Build to verify no compilation errors**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -10`
Expected: BUILD SUCCEEDED

- [ ] **Step 4: Commit**

```bash
git add Sources/UI/Dashboard/DashboardViewController.swift Sources/App/TabCoordinator.swift
git commit -m "feat: add activityEvents to AgentDisplayInfo"
```

---

### Task 3: Add ActivityFeedRenderer

**Files:**
- Create: `Sources/UI/Dashboard/ActivityFeedRenderer.swift`
- Test: `Tests/ActivityFeedRendererTests.swift`

This is a pure function that converts `[ActivityEvent]` into `[NSAttributedString]` with the correct formatting and opacity. Separating rendering logic from the view makes it testable.

- [ ] **Step 1: Write the test file**

```swift
// Tests/ActivityFeedRendererTests.swift
import XCTest
@testable import amux

final class ActivityFeedRendererTests: XCTestCase {
    func testEmptyEventsReturnsEmpty() {
        let lines = ActivityFeedRenderer.render(events: [], maxLines: 10)
        XCTAssertTrue(lines.isEmpty)
    }

    func testNormalEventFormat() {
        let event = ActivityEvent(tool: "Read", detail: "main.swift", isError: false, timestamp: Date())
        let lines = ActivityFeedRenderer.render(events: [event], maxLines: 10)
        XCTAssertEqual(lines.count, 1)
        let text = lines[0].string
        XCTAssertTrue(text.contains("▸"), "Normal marker expected")
        XCTAssertTrue(text.contains("Read"), "Tool name expected")
        XCTAssertTrue(text.contains("main.swift"), "Detail expected")
    }

    func testErrorEventFormat() {
        let event = ActivityEvent(tool: "Bash", detail: "test failed", isError: true, timestamp: Date())
        let lines = ActivityFeedRenderer.render(events: [event], maxLines: 10)
        XCTAssertEqual(lines.count, 1)
        let text = lines[0].string
        XCTAssertTrue(text.contains("✗"), "Error marker expected")
        XCTAssertTrue(text.contains("Bash"), "Tool name expected")
    }

    func testNewestFirstOrdering() {
        let old = ActivityEvent(tool: "Read", detail: "old.swift", isError: false, timestamp: Date(timeIntervalSinceNow: -10))
        let new = ActivityEvent(tool: "Edit", detail: "new.swift", isError: false, timestamp: Date())
        // Events passed in newest-first order (caller is responsible for ordering)
        let lines = ActivityFeedRenderer.render(events: [new, old], maxLines: 10)
        XCTAssertEqual(lines.count, 2)
        XCTAssertTrue(lines[0].string.contains("Edit"), "Newest should be first")
        XCTAssertTrue(lines[1].string.contains("Read"), "Oldest should be second")
    }

    func testMaxLinesTruncation() {
        let events = (0..<5).map { i in
            ActivityEvent(tool: "Read", detail: "file\(i).swift", isError: false, timestamp: Date())
        }
        let lines = ActivityFeedRenderer.render(events: events, maxLines: 3)
        XCTAssertEqual(lines.count, 3)
    }

    func testOpacityDecreases() {
        let events = (0..<4).map { i in
            ActivityEvent(tool: "Read", detail: "file\(i).swift", isError: false, timestamp: Date())
        }
        let lines = ActivityFeedRenderer.render(events: events, maxLines: 10)
        // Check that opacity metadata decreases for later entries
        for i in 1..<lines.count {
            let prevOpacity = ActivityFeedRenderer.opacity(forIndex: i - 1, total: lines.count)
            let curOpacity = ActivityFeedRenderer.opacity(forIndex: i, total: lines.count)
            XCTAssertLessThanOrEqual(curOpacity, prevOpacity, "Opacity should decrease for older entries")
        }
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `xcodegen generate && xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/ActivityFeedRendererTests 2>&1 | tail -20`
Expected: FAIL — `ActivityFeedRenderer` not found

- [ ] **Step 3: Implement ActivityFeedRenderer**

```swift
// Sources/UI/Dashboard/ActivityFeedRenderer.swift
import AppKit

enum ActivityFeedRenderer {
    /// Render activity events into attributed strings for display.
    /// Events should be passed in newest-first order.
    /// Returns at most `maxLines` attributed strings.
    static func render(events: [ActivityEvent], maxLines: Int) -> [NSAttributedString] {
        let visible = Array(events.prefix(maxLines))
        return visible.enumerated().map { index, event in
            attributedString(for: event, index: index, total: visible.count)
        }
    }

    /// Compute opacity for a given index (0 = newest = full opacity).
    static func opacity(forIndex index: Int, total: Int) -> CGFloat {
        guard total > 1 else { return 1.0 }
        let progress = CGFloat(index) / CGFloat(total - 1)
        // Fade from 1.0 down to 0.15
        return max(0.15, 1.0 - progress * 0.85)
    }

    private static func attributedString(for event: ActivityEvent, index: Int, total: Int) -> NSAttributedString {
        let alpha = opacity(forIndex: index, total: total)
        let fontSize: CGFloat = 11

        let marker: String
        let markerColor: NSColor
        let toolColor: NSColor
        let detailColor: NSColor

        if event.isError {
            marker = "✗ "
            markerColor = SemanticColors.danger.withAlphaComponent(alpha)
            toolColor = SemanticColors.danger.withAlphaComponent(alpha)
            detailColor = SemanticColors.danger.withAlphaComponent(alpha * 0.7)
        } else {
            marker = "▸ "
            markerColor = SemanticColors.accent.withAlphaComponent(alpha)
            toolColor = SemanticColors.text.withAlphaComponent(alpha)
            detailColor = SemanticColors.muted.withAlphaComponent(alpha * 0.8)
        }

        let result = NSMutableAttributedString()
        let monoFont = NSFont.monospacedSystemFont(ofSize: fontSize, weight: .regular)
        let monoMedium = NSFont.monospacedSystemFont(ofSize: fontSize, weight: .medium)

        result.append(NSAttributedString(string: marker, attributes: [
            .font: monoFont,
            .foregroundColor: markerColor,
        ]))

        // Pad tool name to 6 chars for alignment
        let paddedTool = event.tool.padding(toLength: 6, withPad: " ", startingAt: 0)
        result.append(NSAttributedString(string: paddedTool + " ", attributes: [
            .font: monoMedium,
            .foregroundColor: toolColor,
        ]))

        result.append(NSAttributedString(string: event.detail, attributes: [
            .font: monoFont,
            .foregroundColor: detailColor,
        ]))

        return result
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `xcodegen generate && xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/ActivityFeedRendererTests 2>&1 | tail -20`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add Sources/UI/Dashboard/ActivityFeedRenderer.swift Tests/ActivityFeedRendererTests.swift
git commit -m "feat: add ActivityFeedRenderer with opacity fade"
```

---

### Task 4: Update AgentCardView to display activity feed

**Files:**
- Modify: `Sources/UI/Dashboard/AgentCardView.swift`

This replaces the single `messageLabel` with a stack of feed entry labels when `activityEvents` is non-empty. The priority order is: tasks > activity feed > last message.

- [ ] **Step 1: Add feed label storage and update configure()**

In `Sources/UI/Dashboard/AgentCardView.swift`, add a `feedLabels` array alongside the existing `messageLabel`:

After the `private let messageLabel` declaration (line 30), add:

```swift
private var feedLabels: [NSTextField] = []
```

- [ ] **Step 2: Add `activityEvents` parameter to `configure()` and implement content priority**

Update the `configure()` method signature to accept `activityEvents`:

```swift
func configure(id: String, project: String, thread: String, status: String, lastMessage: String, totalDuration: String, roundDuration: String, paneCount: Int = 1, paneStatuses: [AgentStatus] = [], tasks: [TaskItem] = [], activityEvents: [ActivityEvent] = []) {
```

Replace the existing message/task display block (lines 59-67, the `if let taskAttr` block) with:

```swift
        // Content priority: tasks > activity feed > last message
        if let taskAttr = TaskListRenderer.attributedString(for: tasks) {
            clearFeedLabels()
            messageLabel.isHidden = false
            messageLabel.attributedStringValue = taskAttr
        } else if !activityEvents.isEmpty {
            messageLabel.isHidden = true
            updateFeedLabels(events: activityEvents)
        } else {
            clearFeedLabels()
            messageLabel.isHidden = false
            messageLabel.attributedStringValue = NSAttributedString(string: lastMessage, attributes: [
                .font: NSFont.monospacedSystemFont(ofSize: Typography.secondaryPointSize, weight: .regular),
                .foregroundColor: SemanticColors.muted,
            ])
        }
```

- [ ] **Step 3: Add the feed label management methods**

Add these methods to `AgentCardView`:

```swift
    private func clearFeedLabels() {
        feedLabels.forEach { $0.removeFromSuperview() }
        feedLabels.removeAll()
    }

    private func updateFeedLabels(events: [ActivityEvent]) {
        clearFeedLabels()

        // Estimate how many lines fit: container height / line height
        // Use a reasonable max (20) as upper bound
        let maxLines = 20
        let rendered = ActivityFeedRenderer.render(events: events, maxLines: maxLines)

        var previousLabel: NSTextField? = nil
        for attrString in rendered {
            let label = NSTextField(labelWithString: "")
            label.attributedStringValue = attrString
            label.lineBreakMode = .byTruncatingTail
            label.maximumNumberOfLines = 1
            label.translatesAutoresizingMaskIntoConstraints = false
            terminalContainer.addSubview(label)

            NSLayoutConstraint.activate([
                label.leadingAnchor.constraint(equalTo: terminalContainer.leadingAnchor, constant: 10),
                label.trailingAnchor.constraint(equalTo: terminalContainer.trailingAnchor, constant: -10),
            ])

            if let prev = previousLabel {
                label.topAnchor.constraint(equalTo: prev.bottomAnchor, constant: 2).isActive = true
            } else {
                label.topAnchor.constraint(equalTo: terminalContainer.topAnchor, constant: 10).isActive = true
            }

            feedLabels.append(label)
            previousLabel = label
        }
    }
```

- [ ] **Step 4: Build to verify no compilation errors**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -10`
Expected: BUILD SUCCEEDED

- [ ] **Step 5: Commit**

```bash
git add Sources/UI/Dashboard/AgentCardView.swift
git commit -m "feat: render activity feed in grid cards"
```

---

### Task 5: Wire activityEvents through DashboardViewController

**Files:**
- Modify: `Sources/UI/Dashboard/DashboardViewController.swift` (the `configure()` call site)

- [ ] **Step 1: Find and update the configure() call**

Search for where `AgentCardView.configure()` is called in `DashboardViewController.swift` and add the `activityEvents` parameter:

```swift
cardView.configure(
    id: agent.id,
    project: agent.project,
    thread: agent.thread,
    status: agent.status,
    lastMessage: agent.lastMessage,
    totalDuration: agent.totalDuration,
    roundDuration: agent.roundDuration,
    paneCount: agent.paneCount,
    paneStatuses: agent.paneStatuses,
    tasks: agent.tasks,
    activityEvents: agent.activityEvents  // <-- ADD THIS
)
```

There may be multiple call sites (initial setup and update path). Update all of them.

- [ ] **Step 2: Build and run existing tests**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test 2>&1 | tail -20`
Expected: All tests PASS

- [ ] **Step 3: Commit**

```bash
git add Sources/UI/Dashboard/DashboardViewController.swift
git commit -m "feat: wire activityEvents through dashboard to card views"
```

---

### Task 6: Add activityEvents to AgentInfo and wire from TabCoordinator

**Files:**
- Modify: `Sources/Core/AgentInfo.swift` (add `activityEvents` property)
- Modify: `Sources/App/TabCoordinator.swift` (pass events to `AgentDisplayInfo`)

- [ ] **Step 1: Add activityEvents to AgentInfo**

In `Sources/Core/AgentInfo.swift`, add to the `AgentInfo` struct after the `tasks` property (line 17):

```swift
    var activityEvents: [ActivityEvent] = []  // recent tool calls for dashboard feed
```

- [ ] **Step 2: Update TabCoordinator to pass events through**

In `Sources/App/TabCoordinator.swift`, update the `AgentDisplayInfo(` construction (around line 248) to pass the events:

```swift
    activityEvents: agent.activityEvents
```

This goes after the `tasks: agent.tasks,` line.

- [ ] **Step 3: Build and run all tests**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test 2>&1 | tail -20`
Expected: All tests PASS

- [ ] **Step 4: Commit**

```bash
git add Sources/Core/AgentInfo.swift Sources/App/TabCoordinator.swift
git commit -m "feat: thread activityEvents from AgentInfo through TabCoordinator"
```

---

### Task 7: Integration test — end-to-end feed rendering

**Files:**
- Create: `Tests/ActivityFeedIntegrationTests.swift`

- [ ] **Step 1: Write integration test**

```swift
// Tests/ActivityFeedIntegrationTests.swift
import XCTest
@testable import amux

final class ActivityFeedIntegrationTests: XCTestCase {
    func testCardConfigureWithActivityEvents() {
        let card = AgentCardView(frame: NSRect(x: 0, y: 0, width: 400, height: 300))
        let events = [
            ActivityEvent(tool: "Read", detail: "main.swift", isError: false, timestamp: Date()),
            ActivityEvent(tool: "Bash", detail: "test failed", isError: true, timestamp: Date(timeIntervalSinceNow: -5)),
        ]

        card.configure(
            id: "test-1",
            project: "myproject",
            thread: "main",
            status: "running",
            lastMessage: "some message",
            totalDuration: "00:05:00",
            roundDuration: "00:01:00",
            activityEvents: events
        )

        // Feed labels should be rendered (messageLabel hidden)
        let feedLabels = card.terminalContainer.subviews.compactMap { $0 as? NSTextField }
            .filter { $0 != card.subviews.first } // exclude messageLabel
        XCTAssertGreaterThanOrEqual(feedLabels.count, 2, "Should have feed labels for each event")
    }

    func testTasksTakePriorityOverFeed() {
        let card = AgentCardView(frame: NSRect(x: 0, y: 0, width: 400, height: 300))
        let events = [
            ActivityEvent(tool: "Read", detail: "main.swift", isError: false, timestamp: Date()),
        ]
        let tasks = [
            TaskItem(id: "1", subject: "Do something", status: .inProgress),
        ]

        card.configure(
            id: "test-1",
            project: "myproject",
            thread: "main",
            status: "running",
            lastMessage: "",
            totalDuration: "00:05:00",
            roundDuration: "00:01:00",
            tasks: tasks,
            activityEvents: events
        )

        // When tasks exist, feed should NOT be shown (messageLabel shows tasks instead)
        // The feed labels array inside the card should be empty
        // We verify by checking that the terminal container doesn't have feed-style labels
        let allLabels = card.terminalContainer.subviews.compactMap { $0 as? NSTextField }
        // Should only have the messageLabel showing tasks, not separate feed labels
        XCTAssertEqual(allLabels.count, 1, "Only messageLabel should be present when tasks exist")
    }
}
```

- [ ] **Step 2: Run test to verify it passes**

Run: `xcodegen generate && xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/ActivityFeedIntegrationTests 2>&1 | tail -20`
Expected: PASS

- [ ] **Step 3: Run full test suite**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test 2>&1 | tail -20`
Expected: All tests PASS

- [ ] **Step 4: Commit**

```bash
git add Tests/ActivityFeedIntegrationTests.swift
git commit -m "test: add activity feed integration tests"
```
