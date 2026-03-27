# Webhook Task Progress Display Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show a rich task progress list (with per-item status icons) in the dashboard card's message area when Claude Code has active tasks, instead of generic "Using TaskUpdate" messages.

**Architecture:** Intercept `PostToolUse` webhook events for `TaskCreate`/`TaskUpdate` tools in `WebhookStatusProvider`, build a per-session task list, flow it through `StatusPublisher` → `AgentHead` → `AgentDisplayInfo` → `AgentCardView`, and render as an `NSAttributedString` with colored icons and styled text.

**Tech Stack:** Swift 5.10, AppKit (NSAttributedString), XCTest

---

## File Map

| File | Action | Responsibility |
|------|--------|---------------|
| `Sources/Core/AgentInfo.swift` | Modify | Add `TaskItem`, `TaskItemStatus` types |
| `Sources/Status/WebhookStatusProvider.swift` | Modify | Parse task events, store per-session task list, expose `tasks(for:)` |
| `Sources/Status/StatusPublisher.swift:201` | Modify | Pass webhook tasks to `AgentHead.updateStatus()` |
| `Sources/Core/AgentHead.swift:120-139` | Modify | Accept and store `[TaskItem]` in `updateStatus()` |
| `Sources/UI/Dashboard/DashboardViewController.swift:16-40` | Modify | Add `tasks` field to `AgentDisplayInfo` |
| `Sources/App/TabCoordinator.swift:227-265` | Modify | Pass `agent.tasks` into `AgentDisplayInfo` |
| `Sources/UI/Dashboard/AgentCardView.swift:52-59` | Modify | Accept `tasks`, render attributed string |
| `Tests/WebhookTaskParsingTests.swift` | Create | Test task event parsing and list building |
| `Tests/TaskListRenderingTests.swift` | Create | Test attributed string generation |

---

### Task 1: Data Model — TaskItem and TaskItemStatus

**Files:**
- Modify: `Sources/Core/AgentInfo.swift`
- Create: `Tests/WebhookTaskParsingTests.swift`

- [ ] **Step 1: Add TaskItemStatus and TaskItem to AgentInfo.swift**

Add below the existing `TaskProgress` struct (after line 42):

```swift
enum TaskItemStatus: String {
    case pending
    case inProgress = "in_progress"
    case completed
}

struct TaskItem {
    let id: String
    var subject: String
    var status: TaskItemStatus
}
```

- [ ] **Step 2: Add tasks field to AgentInfo**

In the `AgentInfo` struct, add after line 16 (`var taskProgress: TaskProgress`):

```swift
    var tasks: [TaskItem] = []          // webhook-tracked task items
```

- [ ] **Step 3: Build to verify compilation**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -5`
Expected: `BUILD SUCCEEDED`

- [ ] **Step 4: Commit**

```bash
git add Sources/Core/AgentInfo.swift
git commit -m "feat: add TaskItem and TaskItemStatus data model"
```

---

### Task 2: WebhookStatusProvider — Parse Task Events

**Files:**
- Modify: `Sources/Status/WebhookStatusProvider.swift`
- Create: `Tests/WebhookTaskParsingTests.swift`

- [ ] **Step 1: Write failing tests for task event parsing**

Create `Tests/WebhookTaskParsingTests.swift`:

```swift
import XCTest
@testable import amux

final class WebhookTaskParsingTests: XCTestCase {

    private func makeProvider() -> WebhookStatusProvider {
        let provider = WebhookStatusProvider()
        provider.updateWorktrees(["/tmp/project"])
        return provider
    }

    private func taskCreateEvent(sessionId: String = "sess1", subject: String, taskId: String? = nil) -> WebhookEvent {
        var toolInput: [String: Any] = ["subject": subject, "description": "test"]
        if let taskId { toolInput["taskId"] = taskId }
        return WebhookEvent(
            source: "claude-code",
            sessionId: sessionId,
            event: .toolUseEnd,
            cwd: "/tmp/project",
            timestamp: nil,
            data: ["tool_name": "TaskCreate", "tool_input": toolInput]
        )
    }

    private func taskUpdateEvent(sessionId: String = "sess1", taskId: String, status: String) -> WebhookEvent {
        WebhookEvent(
            source: "claude-code",
            sessionId: sessionId,
            event: .toolUseEnd,
            cwd: "/tmp/project",
            timestamp: nil,
            data: ["tool_name": "TaskUpdate", "tool_input": ["taskId": taskId, "status": status]]
        )
    }

    private func agentStopEvent(sessionId: String = "sess1") -> WebhookEvent {
        WebhookEvent(
            source: "claude-code",
            sessionId: sessionId,
            event: .agentStop,
            cwd: "/tmp/project",
            timestamp: nil,
            data: ["stop_reason": "end_turn"]
        )
    }

    func testTaskCreateAddsItem() {
        let provider = makeProvider()
        provider.handleEvent(taskCreateEvent(subject: "Add tests"))
        let tasks = provider.tasks(for: "/tmp/project")
        XCTAssertEqual(tasks.count, 1)
        XCTAssertEqual(tasks[0].subject, "Add tests")
        XCTAssertEqual(tasks[0].status, .pending)
        XCTAssertEqual(tasks[0].id, "1")
    }

    func testMultipleTasksGetIncrementingIds() {
        let provider = makeProvider()
        provider.handleEvent(taskCreateEvent(subject: "Task A"))
        provider.handleEvent(taskCreateEvent(subject: "Task B"))
        provider.handleEvent(taskCreateEvent(subject: "Task C"))
        let tasks = provider.tasks(for: "/tmp/project")
        XCTAssertEqual(tasks.count, 3)
        XCTAssertEqual(tasks.map(\.id), ["1", "2", "3"])
    }

    func testTaskUpdateChangesStatus() {
        let provider = makeProvider()
        provider.handleEvent(taskCreateEvent(subject: "Write code"))
        provider.handleEvent(taskUpdateEvent(taskId: "1", status: "in_progress"))
        let tasks = provider.tasks(for: "/tmp/project")
        XCTAssertEqual(tasks[0].status, .inProgress)
    }

    func testTaskUpdateToCompleted() {
        let provider = makeProvider()
        provider.handleEvent(taskCreateEvent(subject: "Write code"))
        provider.handleEvent(taskUpdateEvent(taskId: "1", status: "completed"))
        let tasks = provider.tasks(for: "/tmp/project")
        XCTAssertEqual(tasks[0].status, .completed)
    }

    func testAgentStopClearsTasks() {
        let provider = makeProvider()
        provider.handleEvent(taskCreateEvent(subject: "Write code"))
        provider.handleEvent(agentStopEvent())
        let tasks = provider.tasks(for: "/tmp/project")
        XCTAssertTrue(tasks.isEmpty)
    }

    func testNoTasksForUnknownWorktree() {
        let provider = makeProvider()
        provider.handleEvent(taskCreateEvent(subject: "Write code"))
        let tasks = provider.tasks(for: "/tmp/other")
        XCTAssertTrue(tasks.isEmpty)
    }

    func testNonTaskToolUseIgnored() {
        let provider = makeProvider()
        let event = WebhookEvent(
            source: "claude-code",
            sessionId: "sess1",
            event: .toolUseEnd,
            cwd: "/tmp/project",
            timestamp: nil,
            data: ["tool_name": "Bash", "tool_input": ["command": "ls"]]
        )
        provider.handleEvent(event)
        let tasks = provider.tasks(for: "/tmp/project")
        XCTAssertTrue(tasks.isEmpty)
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/WebhookTaskParsingTests 2>&1 | tail -20`
Expected: FAIL — `tasks(for:)` method does not exist yet

- [ ] **Step 3: Add tasks field to SessionState and nextTaskId counter**

In `WebhookStatusProvider.swift`, modify the `SessionState` struct (lines 15-21):

```swift
    struct SessionState {
        let sessionId: String
        let worktreePath: String
        var status: AgentStatus
        var lastEvent: Date
        var lastMessage: String?
        var tasks: [TaskItem] = []
        var nextTaskId: Int = 1
    }
```

- [ ] **Step 4: Add task event interception in handleEvent()**

In `handleEvent()`, after the existing session update block (after line 108, before the closing `}`), add task parsing logic. Replace the session update block (lines 95-108) with:

```swift
            let status = event.event.agentStatus(data: event.data)
            let message = Self.extractMessage(from: event)

            if var existing = sessions[event.sessionId] {
                existing.status = status
                existing.lastEvent = Date()
                if let message { existing.lastMessage = message }
                Self.applyTaskEvent(event, to: &existing)
                sessions[event.sessionId] = existing
            } else {
                var newSession = SessionState(
                    sessionId: event.sessionId,
                    worktreePath: worktreePath,
                    status: status,
                    lastEvent: Date(),
                    lastMessage: message
                )
                Self.applyTaskEvent(event, to: &newSession)
                sessions[event.sessionId] = newSession
            }
```

- [ ] **Step 5: Implement applyTaskEvent() and tasks(for:)**

Add these methods to `WebhookStatusProvider`:

```swift
    /// Parse TaskCreate/TaskUpdate from PostToolUse events and update session task list
    private static func applyTaskEvent(_ event: WebhookEvent, to session: inout SessionState) {
        guard let toolName = event.data?["tool_name"] as? String,
              let toolInput = event.data?["tool_input"] as? [String: Any] else { return }

        switch toolName {
        case "TaskCreate":
            guard let subject = toolInput["subject"] as? String else { return }
            let id = String(session.nextTaskId)
            session.nextTaskId += 1
            session.tasks.append(TaskItem(id: id, subject: subject, status: .pending))

        case "TaskUpdate":
            guard let taskId = toolInput["taskId"] as? String else { return }
            if let statusStr = toolInput["status"] as? String,
               let newStatus = TaskItemStatus(rawValue: statusStr) {
                if let idx = session.tasks.firstIndex(where: { $0.id == taskId }) {
                    session.tasks[idx].status = newStatus
                }
            }

        default:
            break
        }

        // agentStop clears task list
        if event.event == .agentStop {
            session.tasks.removeAll()
            session.nextTaskId = 1
        }
    }

    /// Returns tasks from the most recent session for a worktree
    func tasks(for worktreePath: String) -> [TaskItem] {
        queue.sync {
            let canon = canonicalize(worktreePath)
            return sessions.values
                .filter { $0.worktreePath == canon }
                .max(by: { $0.lastEvent < $1.lastEvent })?
                .tasks ?? []
        }
    }
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/WebhookTaskParsingTests 2>&1 | tail -20`
Expected: All 7 tests PASS

- [ ] **Step 7: Commit**

```bash
git add Sources/Status/WebhookStatusProvider.swift Tests/WebhookTaskParsingTests.swift
git commit -m "feat: parse TaskCreate/TaskUpdate from webhook events into per-session task list"
```

---

### Task 3: Pipeline — Flow Tasks Through StatusPublisher → AgentHead → AgentDisplayInfo

**Files:**
- Modify: `Sources/Status/StatusPublisher.swift:201`
- Modify: `Sources/Core/AgentHead.swift:120-139`
- Modify: `Sources/UI/Dashboard/DashboardViewController.swift:16-40`
- Modify: `Sources/App/TabCoordinator.swift:248-261`

- [ ] **Step 1: Update AgentHead.updateStatus() to accept tasks**

In `Sources/Core/AgentHead.swift`, modify `updateStatus()` (line 120):

```swift
    func updateStatus(terminalID: String, status: AgentStatus,
                      lastMessage: String, roundDuration: TimeInterval,
                      tasks: [TaskItem] = []) {
        lock.lock()
        guard var info = agents[terminalID] else {
            lock.unlock()
            return
        }
        let changed = info.status != status || info.lastMessage != lastMessage
            || info.tasks.count != tasks.count
        info.status = status
        info.lastMessage = lastMessage
        info.roundDuration = roundDuration
        info.tasks = tasks
        agents[terminalID] = info
        lock.unlock()

        if changed {
            DispatchQueue.main.async { [weak self] in
                self?.delegate?.agentDidUpdate(info)
            }
        }
    }
```

- [ ] **Step 2: Pass webhook tasks from StatusPublisher to AgentHead**

In `Sources/Status/StatusPublisher.swift`, after line 180 (`let lastMessage = ...`), add:

```swift
            let webhookTasks = webhookProvider.tasks(for: worktreePath)
```

Then modify the `AgentHead.shared.updateStatus()` call on line 201:

```swift
            AgentHead.shared.updateStatus(terminalID: terminalID, status: detected, lastMessage: lastMessage, roundDuration: roundDur, tasks: webhookTasks)
```

- [ ] **Step 3: Add tasks to AgentDisplayInfo**

In `Sources/UI/Dashboard/DashboardViewController.swift`, add to the `AgentDisplayInfo` struct after `paneSurfaces` (line 29):

```swift
    let tasks: [TaskItem]              // webhook-tracked task items
```

- [ ] **Step 4: Pass tasks in buildAgentDisplayInfos()**

In `Sources/App/TabCoordinator.swift`, in `buildAgentDisplayInfos()`, add `tasks: agent.tasks` to the `AgentDisplayInfo` init call. After `paneSurfaces: paneSurfaces` (line 261):

```swift
                paneSurfaces: paneSurfaces,
                tasks: agent.tasks
```

- [ ] **Step 5: Build to verify compilation**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -5`
Expected: `BUILD SUCCEEDED`

- [ ] **Step 6: Commit**

```bash
git add Sources/Status/StatusPublisher.swift Sources/Core/AgentHead.swift Sources/UI/Dashboard/DashboardViewController.swift Sources/App/TabCoordinator.swift
git commit -m "feat: flow webhook task list through StatusPublisher → AgentHead → AgentDisplayInfo"
```

---

### Task 4: UI Rendering — Attributed Task List in AgentCardView

**Files:**
- Modify: `Sources/UI/Dashboard/AgentCardView.swift`
- Create: `Tests/TaskListRenderingTests.swift`

- [ ] **Step 1: Write failing test for task list attributed string generation**

Create `Tests/TaskListRenderingTests.swift`:

```swift
import XCTest
@testable import amux

final class TaskListRenderingTests: XCTestCase {

    func testEmptyTasksReturnsNil() {
        let result = TaskListRenderer.attributedString(for: [])
        XCTAssertNil(result)
    }

    func testSinglePendingTask() {
        let tasks = [TaskItem(id: "1", subject: "Add tests", status: .pending)]
        let result = TaskListRenderer.attributedString(for: tasks)
        XCTAssertNotNil(result)
        XCTAssertTrue(result!.string.contains("□"))
        XCTAssertTrue(result!.string.contains("Add tests"))
    }

    func testMixedStatusTasks() {
        let tasks = [
            TaskItem(id: "1", subject: "Done task", status: .completed),
            TaskItem(id: "2", subject: "Current task", status: .inProgress),
            TaskItem(id: "3", subject: "Future task", status: .pending),
        ]
        let result = TaskListRenderer.attributedString(for: tasks)
        XCTAssertNotNil(result)
        let str = result!.string
        XCTAssertTrue(str.contains("✓"))
        XCTAssertTrue(str.contains("■"))
        XCTAssertTrue(str.contains("□"))
    }

    func testCompletedTaskHasStrikethrough() {
        let tasks = [TaskItem(id: "1", subject: "Done", status: .completed)]
        let result = TaskListRenderer.attributedString(for: tasks)!
        var range = NSRange()
        let attrs = result.attributes(at: result.string.distance(from: result.string.startIndex, to: result.string.firstIndex(of: "D")!), effectiveRange: &range)
        let strike = attrs[.strikethroughStyle] as? Int ?? 0
        XCTAssertEqual(strike, NSUnderlineStyle.single.rawValue)
    }

    func testInProgressTaskIsBold() {
        let tasks = [TaskItem(id: "1", subject: "Working", status: .inProgress)]
        let result = TaskListRenderer.attributedString(for: tasks)!
        var range = NSRange()
        let attrs = result.attributes(at: result.string.distance(from: result.string.startIndex, to: result.string.firstIndex(of: "W")!), effectiveRange: &range)
        let font = attrs[.font] as? NSFont
        XCTAssertNotNil(font)
        // Bold monospaced font has "Bold" in its name or weight >= .bold
        let fontDesc = font!.fontDescriptor
        let traits = fontDesc.object(forKey: .traits) as? [NSFontDescriptor.TraitKey: Any]
        let weight = traits?[.weight] as? CGFloat ?? 0
        XCTAssertGreaterThanOrEqual(weight, NSFont.Weight.bold.rawValue)
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/TaskListRenderingTests 2>&1 | tail -20`
Expected: FAIL — `TaskListRenderer` does not exist

- [ ] **Step 3: Implement TaskListRenderer**

Add to the bottom of `Sources/UI/Dashboard/AgentCardView.swift`:

```swift
enum TaskListRenderer {
    static func attributedString(for tasks: [TaskItem]) -> NSAttributedString? {
        guard !tasks.isEmpty else { return nil }

        let result = NSMutableAttributedString()
        let font = NSFont.monospacedSystemFont(ofSize: AgentCardView.Typography.secondaryPointSize, weight: .regular)
        let boldFont = NSFont.monospacedSystemFont(ofSize: AgentCardView.Typography.secondaryPointSize, weight: .bold)
        let mutedColor = SemanticColors.muted
        let textColor = SemanticColors.text
        let successColor = SemanticColors.success

        for (index, task) in tasks.enumerated() {
            if index > 0 {
                result.append(NSAttributedString(string: "\n"))
            }

            let icon: String
            let iconColor: NSColor
            let labelFont: NSFont
            let labelColor: NSColor
            var extraAttrs: [NSAttributedString.Key: Any] = [:]

            switch task.status {
            case .completed:
                icon = " ✓ "
                iconColor = successColor
                labelFont = font
                labelColor = mutedColor
                extraAttrs[.strikethroughStyle] = NSUnderlineStyle.single.rawValue
                extraAttrs[.strikethroughColor] = mutedColor
            case .inProgress:
                icon = " ■ "
                iconColor = textColor
                labelFont = boldFont
                labelColor = textColor
            case .pending:
                icon = " □ "
                iconColor = mutedColor
                labelFont = font
                labelColor = mutedColor
            }

            result.append(NSAttributedString(string: icon, attributes: [
                .font: font,
                .foregroundColor: iconColor,
            ]))

            var labelAttrs: [NSAttributedString.Key: Any] = [
                .font: labelFont,
                .foregroundColor: labelColor,
            ]
            labelAttrs.merge(extraAttrs) { _, new in new }
            result.append(NSAttributedString(string: task.subject, attributes: labelAttrs))
        }

        return result
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/TaskListRenderingTests 2>&1 | tail -20`
Expected: All 5 tests PASS

- [ ] **Step 5: Wire TaskListRenderer into AgentCardView.configure()**

Modify `configure()` in `AgentCardView.swift`. Add `tasks: [TaskItem] = []` parameter and use it:

```swift
    func configure(id: String, project: String, thread: String, status: String, lastMessage: String, totalDuration: String, roundDuration: String, paneCount: Int = 1, paneStatuses: [AgentStatus] = [], tasks: [TaskItem] = []) {
        agentId = id
        currentStatus = status
        setAccessibilityIdentifier("dashboard.card.\(id)")

        projectLabel.stringValue = project
        statusLabel.stringValue = status.capitalized

        // Show task list when available, otherwise plain message
        if let taskAttr = TaskListRenderer.attributedString(for: tasks) {
            messageLabel.attributedStringValue = taskAttr
        } else {
            messageLabel.attributedStringValue = NSAttributedString(string: lastMessage, attributes: [
                .font: NSFont.monospacedSystemFont(ofSize: Typography.secondaryPointSize, weight: .regular),
                .foregroundColor: SemanticColors.muted,
            ])
        }
```

The rest of `configure()` (status dots, pane count) stays unchanged.

- [ ] **Step 6: Pass tasks at both call sites in DashboardViewController**

In `Sources/UI/Dashboard/DashboardViewController.swift`, update both `cardView.configure(` calls.

At line ~203 (updateAgents):
```swift
            gridCards[index].cardView.configure(
                id: agent.id,
                project: agent.project,
                thread: agent.thread,
                status: agent.status,
                lastMessage: agent.lastMessage,
                totalDuration: agent.totalDuration,
                roundDuration: agent.roundDuration,
                paneCount: agent.paneCount,
                paneStatuses: agent.paneStatuses,
                tasks: agent.tasks
            )
```

At line ~654 (rebuildGrid):
```swift
            container.cardView.configure(
                id: agent.id,
                project: agent.project,
                thread: agent.thread,
                status: agent.status,
                lastMessage: agent.lastMessage,
                totalDuration: agent.totalDuration,
                roundDuration: agent.roundDuration,
                paneCount: agent.paneCount,
                tasks: agent.tasks
            )
```

- [ ] **Step 7: Build to verify compilation**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -5`
Expected: `BUILD SUCCEEDED`

- [ ] **Step 8: Run all tests**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test 2>&1 | tail -20`
Expected: All tests PASS

- [ ] **Step 9: Commit**

```bash
git add Sources/UI/Dashboard/AgentCardView.swift Sources/UI/Dashboard/DashboardViewController.swift Tests/TaskListRenderingTests.swift
git commit -m "feat: render webhook task progress list in dashboard cards"
```
