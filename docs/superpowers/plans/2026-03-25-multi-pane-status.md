# Multi-Pane Status, LastMessage & Notification Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Support per-pane status tracking, multi-dot dashboard display, and per-pane notifications when a worktree has multiple split panes.

**Architecture:** A new `WorktreeStatusAggregator` layer sits between `AgentHead` (per-terminal storage) and UI/notification consumers. It queries `SplitTree` for pane ordering, builds `WorktreeStatus` snapshots, diffs changes, and fires delegate callbacks. Dashboard views consume `WorktreeStatus` instead of reading `AgentHead` directly.

**Tech Stack:** Swift 5.10, AppKit, XCTest

**Spec:** `docs/superpowers/specs/2026-03-25-multi-pane-status-design.md`

---

## File Structure

| File | Action | Responsibility |
|------|--------|---------------|
| `Sources/Status/PaneStatus.swift` | Create | `PaneStatus` and `WorktreeStatus` value types |
| `Sources/Status/WorktreeStatusAggregator.swift` | Create | Aggregation layer: listens to AgentHead updates, builds WorktreeStatus, fires delegate callbacks |
| `Sources/Core/AgentHead.swift` | Modify | Change `worktreeIndex` from `[String: String]` to `[String: [String]]` |
| `Sources/Status/StatusPublisher.swift` | Modify | Call `aggregator.agentDidUpdate(terminalID:)` after updating AgentHead |
| `Sources/Status/NotificationManager.swift` | Modify | Accept per-pane callbacks, use terminalID for cooldown key, add `[Pane N]` to titles |
| `Sources/Status/NotificationHistory.swift` | Modify | Add `paneIndex: Int?` to `NotificationEntry` |
| `Sources/UI/Dashboard/DashboardViewController.swift` | Modify | Update `AgentDisplayInfo` to use `paneStatuses: [AgentStatus]` instead of `status: String` |
| `Sources/UI/Dashboard/AgentCardView.swift` | Modify | Render multi-dot array in bottom bar |
| `Sources/UI/Dashboard/MiniCardView.swift` | Modify | Render multi-dot array |
| `Sources/UI/Dashboard/FocusPanelView.swift` | Modify | Render dot array with active highlight, click to switch pane |
| `Sources/App/MainWindowController.swift` | Modify | Own aggregator, implement `WorktreeStatusDelegate`, update `buildAgentDisplayInfos()` |
| `Tests/PaneStatusTests.swift` | Create | Unit tests for PaneStatus, WorktreeStatus |
| `Tests/WorktreeStatusAggregatorTests.swift` | Create | Unit tests for aggregation logic |
| `Tests/NotificationManagerTests.swift` | Create | Tests for per-pane cooldown and title formatting |

---

### Task 1: Data Model — PaneStatus and WorktreeStatus

**Files:**
- Create: `Sources/Status/PaneStatus.swift`
- Create: `Tests/PaneStatusTests.swift`

- [ ] **Step 1: Write failing tests for PaneStatus and WorktreeStatus**

```swift
// Tests/PaneStatusTests.swift
import XCTest
@testable import amux

final class PaneStatusTests: XCTestCase {

    func testWorktreeStatusStatuses() {
        let ws = WorktreeStatus(
            worktreePath: "/repo/main",
            panes: [
                PaneStatus(paneIndex: 1, terminalID: "t1", status: .running, lastMessage: "building", lastUpdated: Date()),
                PaneStatus(paneIndex: 2, terminalID: "t2", status: .idle, lastMessage: "done", lastUpdated: Date()),
            ],
            mostRecentPaneIndex: 1,
            mostRecentMessage: "building"
        )
        XCTAssertEqual(ws.statuses, [.running, .idle])
    }

    func testWorktreeStatusHasUrgent() {
        let ws = WorktreeStatus(
            worktreePath: "/repo/main",
            panes: [
                PaneStatus(paneIndex: 1, terminalID: "t1", status: .running, lastMessage: "", lastUpdated: Date()),
                PaneStatus(paneIndex: 2, terminalID: "t2", status: .error, lastMessage: "failed", lastUpdated: Date()),
            ],
            mostRecentPaneIndex: 2,
            mostRecentMessage: "failed"
        )
        XCTAssertTrue(ws.hasUrgent)
    }

    func testWorktreeStatusNotUrgent() {
        let ws = WorktreeStatus(
            worktreePath: "/repo/main",
            panes: [
                PaneStatus(paneIndex: 1, terminalID: "t1", status: .running, lastMessage: "", lastUpdated: Date()),
            ],
            mostRecentPaneIndex: 1,
            mostRecentMessage: ""
        )
        XCTAssertFalse(ws.hasUrgent)
    }

    func testWorktreeStatusHighestPriority() {
        let ws = WorktreeStatus(
            worktreePath: "/repo/main",
            panes: [
                PaneStatus(paneIndex: 1, terminalID: "t1", status: .idle, lastMessage: "", lastUpdated: Date()),
                PaneStatus(paneIndex: 2, terminalID: "t2", status: .waiting, lastMessage: "?", lastUpdated: Date()),
                PaneStatus(paneIndex: 3, terminalID: "t3", status: .running, lastMessage: "", lastUpdated: Date()),
            ],
            mostRecentPaneIndex: 2,
            mostRecentMessage: "?"
        )
        XCTAssertEqual(ws.highestPriority, .waiting)
    }

    func testSinglePaneWorktreeStatus() {
        let ws = WorktreeStatus(
            worktreePath: "/repo/main",
            panes: [
                PaneStatus(paneIndex: 1, terminalID: "t1", status: .running, lastMessage: "working", lastUpdated: Date()),
            ],
            mostRecentPaneIndex: 1,
            mostRecentMessage: "working"
        )
        XCTAssertEqual(ws.statuses.count, 1)
        XCTAssertEqual(ws.highestPriority, .running)
        XCTAssertFalse(ws.hasUrgent)
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/PaneStatusTests 2>&1 | tail -20`
Expected: FAIL — `PaneStatus` and `WorktreeStatus` not defined

- [ ] **Step 3: Implement PaneStatus and WorktreeStatus**

```swift
// Sources/Status/PaneStatus.swift
import Foundation

struct PaneStatus {
    let paneIndex: Int        // 1-based, follows SplitTree leaf order
    let terminalID: String    // TerminalSurface.id
    var status: AgentStatus
    var lastMessage: String
    var lastUpdated: Date     // When status or message last changed
}

struct WorktreeStatus {
    let worktreePath: String
    var panes: [PaneStatus]           // Ordered by SplitTree leaf position
    var mostRecentPaneIndex: Int      // Pane whose lastMessage is displayed
    var mostRecentMessage: String     // That pane's lastMessage

    var statuses: [AgentStatus] {
        panes.map(\.status)
    }

    var hasUrgent: Bool {
        panes.contains { $0.status.isUrgent }
    }

    var highestPriority: AgentStatus {
        AgentStatus.highestPriority(panes.map(\.status))
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/PaneStatusTests 2>&1 | tail -20`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add Sources/Status/PaneStatus.swift Tests/PaneStatusTests.swift
git commit -m "feat: add PaneStatus and WorktreeStatus data models for multi-pane status"
```

---

### Task 2: AgentHead — Change worktreeIndex to 1:N

**Files:**
- Modify: `Sources/Core/AgentHead.swift:19` (worktreeIndex), `:29-69` (register), `:71-82` (unregister), `:286-292` (agent(forWorktree:))

- [ ] **Step 1: Write failing test for multi-terminal worktreeIndex**

Add to an existing or new test file:

```swift
// Tests/AgentHeadTests.swift
import XCTest
@testable import amux

final class AgentHeadTests: XCTestCase {

    func testWorktreeIndexStoresMultipleTerminals() {
        // Use AgentHead.shared — reset state in setUp/tearDown
        let head = AgentHead.shared
        head.registerTerminalID("test-t1", forWorktree: "/test/repo/main")
        head.registerTerminalID("test-t2", forWorktree: "/test/repo/main")

        let ids = head.terminalIDs(forWorktree: "/test/repo/main")
        XCTAssertEqual(ids, ["test-t1", "test-t2"])

        // Cleanup
        head.unregisterTerminalID("test-t1", forWorktree: "/test/repo/main")
        head.unregisterTerminalID("test-t2", forWorktree: "/test/repo/main")
    }

    func testUnregisterRemovesFromWorktreeIndex() {
        let head = AgentHead.shared
        head.registerTerminalID("test-t1", forWorktree: "/test/repo/main")
        head.registerTerminalID("test-t2", forWorktree: "/test/repo/main")
        head.unregisterTerminalID("test-t1", forWorktree: "/test/repo/main")

        let ids = head.terminalIDs(forWorktree: "/test/repo/main")
        XCTAssertEqual(ids, ["test-t2"])

        // Cleanup
        head.unregisterTerminalID("test-t2", forWorktree: "/test/repo/main")
    }

    func testUnregisterLastTerminalRemovesWorktreeEntry() {
        let head = AgentHead.shared
        head.registerTerminalID("test-t1", forWorktree: "/test/repo/main")
        head.unregisterTerminalID("test-t1", forWorktree: "/test/repo/main")

        let ids = head.terminalIDs(forWorktree: "/test/repo/main")
        XCTAssertEqual(ids, [])
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/AgentHeadTests 2>&1 | tail -20`
Expected: FAIL — methods not defined

- [ ] **Step 3: Modify AgentHead**

In `Sources/Core/AgentHead.swift`:

Change line 19:
```swift
// Before:
var worktreeIndex: [String: String] = [:]

// After:
var worktreeIndex: [String: [String]] = [:]
```

Add helper methods:
```swift
func registerTerminalID(_ terminalID: String, forWorktree worktreePath: String) {
    lock.lock()
    defer { lock.unlock() }
    var ids = worktreeIndex[worktreePath] ?? []
    if !ids.contains(terminalID) {
        ids.append(terminalID)
    }
    worktreeIndex[worktreePath] = ids
}

func unregisterTerminalID(_ terminalID: String, forWorktree worktreePath: String) {
    lock.lock()
    defer { lock.unlock() }
    worktreeIndex[worktreePath]?.removeAll { $0 == terminalID }
    if worktreeIndex[worktreePath]?.isEmpty == true {
        worktreeIndex.removeValue(forKey: worktreePath)
    }
}

func terminalIDs(forWorktree worktreePath: String) -> [String] {
    lock.lock()
    defer { lock.unlock() }
    return worktreeIndex[worktreePath] ?? []
}
```

Update `register(surface:worktreePath:...)` (line ~65):
```swift
// Before:
worktreeIndex[worktreePath] = surface.id

// After:
registerTerminalID(surface.id, forWorktree: worktreePath)
```

Update `unregister(terminalID:)` (line ~71-82):
```swift
// Before: remove worktreeIndex entry by value
// After:
if let info = agents[terminalID] {
    unregisterTerminalID(terminalID, forWorktree: info.worktreePath)
}
```

Update `agent(forWorktree:)` (line ~286-292):
```swift
// Before:
func agent(forWorktree path: String) -> AgentInfo? {
    lock.lock()
    defer { lock.unlock() }
    guard let terminalID = worktreeIndex[path] else { return nil }
    return agents[terminalID]
}

// After: return first terminal's agent (backward compat for callers that expect single)
func agent(forWorktree path: String) -> AgentInfo? {
    lock.lock()
    defer { lock.unlock() }
    guard let terminalID = worktreeIndex[path]?.first else { return nil }
    return agents[terminalID]
}
```

- [ ] **Step 4: Fix any compilation errors from callers of worktreeIndex**

Search for all direct accesses to `worktreeIndex` and update them to work with `[String]` values.

- [ ] **Step 5: Run tests to verify they pass**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/AgentHeadTests 2>&1 | tail -20`
Expected: PASS

- [ ] **Step 6: Run full test suite to check for regressions**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test 2>&1 | tail -30`
Expected: All tests PASS

- [ ] **Step 7: Commit**

```bash
git add Sources/Core/AgentHead.swift Tests/AgentHeadTests.swift
git commit -m "refactor: change AgentHead.worktreeIndex from 1:1 to 1:N mapping"
```

---

### Task 3: WorktreeStatusAggregator

**Files:**
- Create: `Sources/Status/WorktreeStatusAggregator.swift`
- Create: `Tests/WorktreeStatusAggregatorTests.swift`

- [ ] **Step 1: Write failing tests for aggregator**

```swift
// Tests/WorktreeStatusAggregatorTests.swift
import XCTest
@testable import amux

final class WorktreeStatusAggregatorTests: XCTestCase {

    // Mock delegate to capture callbacks
    class MockDelegate: WorktreeStatusDelegate {
        var lastUpdatedStatus: WorktreeStatus?
        var paneChanges: [(worktreePath: String, paneIndex: Int, oldStatus: AgentStatus, newStatus: AgentStatus, lastMessage: String)] = []

        func worktreeStatusDidUpdate(_ status: WorktreeStatus) {
            lastUpdatedStatus = status
        }

        func paneStatusDidChange(worktreePath: String, paneIndex: Int, oldStatus: AgentStatus, newStatus: AgentStatus, lastMessage: String) {
            paneChanges.append((worktreePath, paneIndex, oldStatus, newStatus, lastMessage))
        }
    }

    func testSinglePaneUpdate() {
        let aggregator = WorktreeStatusAggregator()
        let mockDelegate = MockDelegate()
        aggregator.delegate = mockDelegate

        // Register a mapping
        aggregator.registerTerminal("t1", worktreePath: "/repo/main", leafIndex: 0)

        // Simulate status update
        aggregator.agentDidUpdate(
            terminalID: "t1",
            status: .running,
            lastMessage: "building..."
        )

        XCTAssertNotNil(mockDelegate.lastUpdatedStatus)
        XCTAssertEqual(mockDelegate.lastUpdatedStatus?.panes.count, 1)
        XCTAssertEqual(mockDelegate.lastUpdatedStatus?.panes[0].status, .running)
        XCTAssertEqual(mockDelegate.lastUpdatedStatus?.panes[0].paneIndex, 1)
        XCTAssertEqual(mockDelegate.lastUpdatedStatus?.mostRecentMessage, "building...")
    }

    func testMultiPaneUpdate() {
        let aggregator = WorktreeStatusAggregator()
        let mockDelegate = MockDelegate()
        aggregator.delegate = mockDelegate

        aggregator.registerTerminal("t1", worktreePath: "/repo/main", leafIndex: 0)
        aggregator.registerTerminal("t2", worktreePath: "/repo/main", leafIndex: 1)

        aggregator.agentDidUpdate(terminalID: "t1", status: .running, lastMessage: "building")
        aggregator.agentDidUpdate(terminalID: "t2", status: .idle, lastMessage: "done")

        let ws = mockDelegate.lastUpdatedStatus!
        XCTAssertEqual(ws.panes.count, 2)
        XCTAssertEqual(ws.statuses, [.running, .idle])
        // t2 updated last, so its message is mostRecent
        XCTAssertEqual(ws.mostRecentMessage, "done")
        XCTAssertEqual(ws.mostRecentPaneIndex, 2)
    }

    func testStatusChangeFiresPaneCallback() {
        let aggregator = WorktreeStatusAggregator()
        let mockDelegate = MockDelegate()
        aggregator.delegate = mockDelegate

        aggregator.registerTerminal("t1", worktreePath: "/repo/main", leafIndex: 0)
        aggregator.agentDidUpdate(terminalID: "t1", status: .running, lastMessage: "")

        mockDelegate.paneChanges.removeAll()
        aggregator.agentDidUpdate(terminalID: "t1", status: .waiting, lastMessage: "need input")

        XCTAssertEqual(mockDelegate.paneChanges.count, 1)
        XCTAssertEqual(mockDelegate.paneChanges[0].paneIndex, 1)
        XCTAssertEqual(mockDelegate.paneChanges[0].oldStatus, .running)
        XCTAssertEqual(mockDelegate.paneChanges[0].newStatus, .waiting)
    }

    func testNoChangeDoesNotFireCallbacks() {
        let aggregator = WorktreeStatusAggregator()
        let mockDelegate = MockDelegate()
        aggregator.delegate = mockDelegate

        aggregator.registerTerminal("t1", worktreePath: "/repo/main", leafIndex: 0)
        aggregator.agentDidUpdate(terminalID: "t1", status: .running, lastMessage: "building")

        mockDelegate.lastUpdatedStatus = nil
        mockDelegate.paneChanges.removeAll()

        // Same status and message — no change
        aggregator.agentDidUpdate(terminalID: "t1", status: .running, lastMessage: "building")

        XCTAssertNil(mockDelegate.lastUpdatedStatus)
        XCTAssertTrue(mockDelegate.paneChanges.isEmpty)
    }

    func testReindexOnPaneRemoval() {
        let aggregator = WorktreeStatusAggregator()
        let mockDelegate = MockDelegate()
        aggregator.delegate = mockDelegate

        aggregator.registerTerminal("t1", worktreePath: "/repo/main", leafIndex: 0)
        aggregator.registerTerminal("t2", worktreePath: "/repo/main", leafIndex: 1)
        aggregator.registerTerminal("t3", worktreePath: "/repo/main", leafIndex: 2)

        aggregator.agentDidUpdate(terminalID: "t1", status: .running, lastMessage: "a")
        aggregator.agentDidUpdate(terminalID: "t2", status: .idle, lastMessage: "b")
        aggregator.agentDidUpdate(terminalID: "t3", status: .waiting, lastMessage: "c")

        // Remove middle pane
        aggregator.unregisterTerminal("t2", worktreePath: "/repo/main")
        // Reindex: t1 at index 0, t3 at index 1
        aggregator.updateLeafOrder(worktreePath: "/repo/main", terminalIDs: ["t1", "t3"])

        let ws = aggregator.status(for: "/repo/main")!
        XCTAssertEqual(ws.panes.count, 2)
        XCTAssertEqual(ws.panes[0].paneIndex, 1) // t1
        XCTAssertEqual(ws.panes[1].paneIndex, 2) // t3 (was pane 3, now pane 2)
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/WorktreeStatusAggregatorTests 2>&1 | tail -20`
Expected: FAIL

- [ ] **Step 3: Implement WorktreeStatusAggregator**

```swift
// Sources/Status/WorktreeStatusAggregator.swift
import Foundation

protocol WorktreeStatusDelegate: AnyObject {
    func worktreeStatusDidUpdate(_ status: WorktreeStatus)
    func paneStatusDidChange(worktreePath: String, paneIndex: Int,
                             oldStatus: AgentStatus, newStatus: AgentStatus,
                             lastMessage: String)
}

/// Thread safety: All methods must be called on the main queue.
/// StatusPublisher dispatches to main before calling agentDidUpdate.
class WorktreeStatusAggregator {
    weak var delegate: WorktreeStatusDelegate?

    // Current state snapshots, keyed by worktreePath
    private var worktreeStatuses: [String: WorktreeStatus] = [:]

    // Per-terminal state for diffing
    private var paneStates: [String: PaneStatus] = [:]  // keyed by terminalID

    // Mappings
    private var terminalToWorktree: [String: String] = [:]  // terminalID → worktreePath
    private var worktreeTerminals: [String: [String]] = [:]  // worktreePath → [terminalID] in leaf order

    // MARK: - Registration

    func registerTerminal(_ terminalID: String, worktreePath: String, leafIndex: Int) {
        terminalToWorktree[terminalID] = worktreePath
        var ids = worktreeTerminals[worktreePath] ?? []
        if !ids.contains(terminalID) {
            // Insert at correct position or append
            if leafIndex < ids.count {
                ids.insert(terminalID, at: leafIndex)
            } else {
                ids.append(terminalID)
            }
        }
        worktreeTerminals[worktreePath] = ids
    }

    func unregisterTerminal(_ terminalID: String, worktreePath: String) {
        terminalToWorktree.removeValue(forKey: terminalID)
        worktreeTerminals[worktreePath]?.removeAll { $0 == terminalID }
        paneStates.removeValue(forKey: terminalID)
        if worktreeTerminals[worktreePath]?.isEmpty == true {
            worktreeTerminals.removeValue(forKey: worktreePath)
            worktreeStatuses.removeValue(forKey: worktreePath)
        }
    }

    func updateLeafOrder(worktreePath: String, terminalIDs: [String]) {
        worktreeTerminals[worktreePath] = terminalIDs
        rebuildWorktreeStatus(worktreePath: worktreePath)
    }

    // MARK: - Status Updates

    func agentDidUpdate(terminalID: String, status: AgentStatus, lastMessage: String) {
        guard let worktreePath = terminalToWorktree[terminalID] else { return }

        let now = Date()
        let oldPaneState = paneStates[terminalID]
        let statusChanged = oldPaneState?.status != status
        let messageChanged = oldPaneState?.lastMessage != lastMessage

        guard statusChanged || messageChanged else { return }  // No-op if nothing changed

        let paneIndex = paneIndexForTerminal(terminalID, worktreePath: worktreePath)
        let newPaneState = PaneStatus(
            paneIndex: paneIndex,
            terminalID: terminalID,
            status: status,
            lastMessage: lastMessage,
            lastUpdated: now
        )
        paneStates[terminalID] = newPaneState

        // Fire pane status change if status actually changed
        if statusChanged, let oldStatus = oldPaneState?.status {
            delegate?.paneStatusDidChange(
                worktreePath: worktreePath,
                paneIndex: paneIndex,
                oldStatus: oldStatus,
                newStatus: status,
                lastMessage: lastMessage
            )
        }

        // Rebuild and fire worktree update
        rebuildWorktreeStatus(worktreePath: worktreePath)
    }

    // MARK: - Queries

    func status(for worktreePath: String) -> WorktreeStatus? {
        worktreeStatuses[worktreePath]
    }

    // MARK: - Private

    private func paneIndexForTerminal(_ terminalID: String, worktreePath: String) -> Int {
        let ids = worktreeTerminals[worktreePath] ?? []
        let index = ids.firstIndex(of: terminalID) ?? 0
        return index + 1  // 1-based
    }

    private func rebuildWorktreeStatus(worktreePath: String) {
        guard let terminalIDs = worktreeTerminals[worktreePath], !terminalIDs.isEmpty else { return }

        var panes: [PaneStatus] = []
        for (index, tid) in terminalIDs.enumerated() {
            if var pane = paneStates[tid] {
                pane = PaneStatus(
                    paneIndex: index + 1,
                    terminalID: pane.terminalID,
                    status: pane.status,
                    lastMessage: pane.lastMessage,
                    lastUpdated: pane.lastUpdated
                )
                paneStates[tid] = pane
                panes.append(pane)
            }
        }

        guard !panes.isEmpty else { return }

        let mostRecent = panes.max(by: { $0.lastUpdated < $1.lastUpdated }) ?? panes[0]

        let ws = WorktreeStatus(
            worktreePath: worktreePath,
            panes: panes,
            mostRecentPaneIndex: mostRecent.paneIndex,
            mostRecentMessage: mostRecent.lastMessage
        )
        worktreeStatuses[worktreePath] = ws
        delegate?.worktreeStatusDidUpdate(ws)
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/WorktreeStatusAggregatorTests 2>&1 | tail -20`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add Sources/Status/WorktreeStatusAggregator.swift Tests/WorktreeStatusAggregatorTests.swift
git commit -m "feat: add WorktreeStatusAggregator for per-pane status aggregation"
```

---

### Task 4: Wire StatusPublisher → Aggregator

**Files:**
- Modify: `Sources/Status/StatusPublisher.swift:11,123-198`
- Modify: `Sources/App/MainWindowController.swift:1649-1674`

- [ ] **Step 1: Add aggregator property to StatusPublisher**

In `Sources/Status/StatusPublisher.swift`, add after line 22:

```swift
var aggregator: WorktreeStatusAggregator?
```

- [ ] **Step 2: Update pollAll to call aggregator after AgentHead update**

**Webhook scoping note:** `StatusPublisher.pollAll` already merges webhook status before calling `AgentHead.updateStatus`. Since webhook status is per-worktreePath, it will be applied identically to all panes in that worktree. No additional webhook handling needed in the aggregator — it just consumes whatever status `StatusPublisher` provides per surface.

In `StatusPublisher.pollAll()` (around lines 170-190), after each call to `AgentHead.shared.updateStatus(terminalID:status:lastMessage:roundDuration:)`, add:

```swift
DispatchQueue.main.async { [weak self] in
    self?.aggregator?.agentDidUpdate(
        terminalID: terminalID,
        status: newStatus,
        lastMessage: message
    )
}
```

- [ ] **Step 3: Update StatusPublisher.start(trees:) to register terminals with aggregator**

In `start(trees:)` (lines 45-74), after populating `surfaces` and `worktreePaths`, add registration calls:

```swift
for (worktreePath, tree) in trees {
    let leaves = tree.allLeaves
    for (index, leaf) in leaves.enumerated() {
        aggregator?.registerTerminal(leaf.surfaceId, worktreePath: worktreePath, leafIndex: index)
    }
}
```

- [ ] **Step 4: Update MainWindowController to create and own aggregator**

In `MainWindowController`, add property:

```swift
private let statusAggregator = WorktreeStatusAggregator()
```

In the setup method where `statusPublisher` is configured, wire it:

```swift
statusPublisher.aggregator = statusAggregator
statusAggregator.delegate = self
```

- [ ] **Step 5: Build to verify compilation**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -20`
Expected: BUILD SUCCEEDED (MainWindowController does not yet conform to WorktreeStatusDelegate — add stub conformance)

- [ ] **Step 6: Add stub WorktreeStatusDelegate conformance to MainWindowController**

```swift
extension MainWindowController: WorktreeStatusDelegate {
    func worktreeStatusDidUpdate(_ status: WorktreeStatus) {
        // Will be fully implemented in Task 7
        dashboardVC?.updateAgents(buildAgentDisplayInfos())
    }

    func paneStatusDidChange(worktreePath: String, paneIndex: Int, oldStatus: AgentStatus, newStatus: AgentStatus, lastMessage: String) {
        // Will be fully implemented in Task 5
    }
}
```

- [ ] **Step 7: Build and run tests**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test 2>&1 | tail -30`
Expected: All tests PASS

- [ ] **Step 8: Commit**

```bash
git add Sources/Status/StatusPublisher.swift Sources/App/MainWindowController.swift
git commit -m "feat: wire StatusPublisher to WorktreeStatusAggregator"
```

---

### Task 5: NotificationManager — Per-Pane Notifications

**Files:**
- Modify: `Sources/Status/NotificationManager.swift:12-13,46-112`
- Modify: `Sources/Status/NotificationHistory.swift:3-21`
- Create: `Tests/NotificationManagerTests.swift`

- [ ] **Step 1: Write failing tests for per-pane notification logic**

```swift
// Tests/NotificationManagerTests.swift
import XCTest
@testable import amux

final class NotificationManagerTests: XCTestCase {

    func testCooldownKeyUsesTerminalID() {
        // Test the static formatTitle and shouldNotify logic via NotificationManager.shared
        // NotificationManager has private init(), so use shared instance
        let manager = NotificationManager.shared
        // First notification should go through
        let sent1 = manager.shouldNotify(
            terminalID: "test-notif-t1",
            oldStatus: .running,
            newStatus: .idle
        )
        XCTAssertTrue(sent1)

        // Same terminalID within cooldown should be blocked
        let sent2 = manager.shouldNotify(
            terminalID: "test-notif-t1",
            oldStatus: .running,
            newStatus: .idle
        )
        XCTAssertFalse(sent2)

        // Different terminalID should go through
        let sent3 = manager.shouldNotify(
            terminalID: "test-notif-t2",
            oldStatus: .running,
            newStatus: .idle
        )
        XCTAssertTrue(sent3)
    }

    func testNotificationTitleWithPaneIndex() {
        let title = NotificationManager.formatTitle(
            status: .idle,
            branch: "main",
            paneIndex: 2,
            paneCount: 3
        )
        XCTAssertEqual(title, "Agent finished — main [Pane 2]")
    }

    func testNotificationTitleSinglePane() {
        let title = NotificationManager.formatTitle(
            status: .idle,
            branch: "main",
            paneIndex: 1,
            paneCount: 1
        )
        XCTAssertEqual(title, "Agent finished — main")
    }

    func testNotificationTitleWaiting() {
        let title = NotificationManager.formatTitle(
            status: .waiting,
            branch: "feat/x",
            paneIndex: 1,
            paneCount: 2
        )
        XCTAssertEqual(title, "Agent needs input — feat/x [Pane 1]")
    }

    func testNotificationTitleError() {
        let title = NotificationManager.formatTitle(
            status: .error,
            branch: "main",
            paneIndex: 3,
            paneCount: 3
        )
        XCTAssertEqual(title, "Agent error — main [Pane 3]")
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/NotificationManagerTests 2>&1 | tail -20`
Expected: FAIL

- [ ] **Step 3: Update NotificationManager**

In `Sources/Status/NotificationManager.swift`:

Change cooldown key from worktreePath to terminalID (line 12):
```swift
// Before:
var lastNotified: [String: Date] = [:]  // keyed by worktree path

// After:
var lastNotified: [String: Date] = [:]  // keyed by terminalID
```

Add `shouldNotify` method (extracting logic from `notify`):
```swift
func shouldNotify(terminalID: String, oldStatus: AgentStatus, newStatus: AgentStatus) -> Bool {
    guard oldStatus == .running else { return false }
    guard newStatus == .waiting || newStatus == .error || newStatus == .idle else { return false }
    if let last = lastNotified[terminalID], Date().timeIntervalSince(last) < cooldown {
        return false
    }
    lastNotified[terminalID] = Date()
    return true
}
```

Add static `formatTitle`:
```swift
static func formatTitle(status: AgentStatus, branch: String, paneIndex: Int, paneCount: Int) -> String {
    let base: String
    switch status {
    case .idle: base = "Agent finished — \(branch)"
    case .waiting: base = "Agent needs input — \(branch)"
    case .error: base = "Agent error — \(branch)"
    default: base = "Agent status — \(branch)"
    }
    if paneCount > 1 {
        return "\(base) [Pane \(paneIndex)]"
    }
    return base
}
```

Update `notify(...)` method signature to accept terminalID and paneIndex:
```swift
func notify(terminalID: String, worktreePath: String, branch: String,
            paneIndex: Int, paneCount: Int,
            oldStatus: AgentStatus, newStatus: AgentStatus, lastMessage: String)
```

- [ ] **Step 4: Update NotificationEntry to include paneIndex**

In `Sources/Status/NotificationHistory.swift`, add to `NotificationEntry` struct (after line 9):
```swift
let paneIndex: Int?  // nil for single-pane worktrees
```

Update init to accept `paneIndex`:
```swift
init(branch: String, worktreePath: String, status: AgentStatus, message: String, paneIndex: Int? = nil) {
    self.id = UUID()
    self.timestamp = Date()
    self.branch = branch
    self.worktreePath = worktreePath
    self.status = status
    self.message = message
    self.isRead = false
    self.paneIndex = paneIndex
}
```

- [ ] **Step 5: Wire MainWindowController.paneStatusDidChange to NotificationManager**

In `MainWindowController`'s `paneStatusDidChange` stub (from Task 4):

```swift
func paneStatusDidChange(worktreePath: String, paneIndex: Int, oldStatus: AgentStatus, newStatus: AgentStatus, lastMessage: String) {
    guard let branch = allWorktrees.first(where: { $0.path == worktreePath })?.branch else { return }
    let paneCount = statusAggregator.status(for: worktreePath)?.panes.count ?? 1
    // Find terminalID for cooldown
    let terminalID = statusAggregator.status(for: worktreePath)?.panes.first(where: { $0.paneIndex == paneIndex })?.terminalID ?? ""
    NotificationManager.shared.notify(
        terminalID: terminalID,
        worktreePath: worktreePath,
        branch: branch,
        paneIndex: paneIndex,
        paneCount: paneCount,
        oldStatus: oldStatus,
        newStatus: newStatus,
        lastMessage: lastMessage
    )
}
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/NotificationManagerTests 2>&1 | tail -20`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add Sources/Status/NotificationManager.swift Sources/Status/NotificationHistory.swift Tests/NotificationManagerTests.swift Sources/App/MainWindowController.swift
git commit -m "feat: per-pane notifications with terminalID cooldown and pane-aware titles"
```

---

### Task 6: Update AgentDisplayInfo

**Files:**
- Modify: `Sources/UI/Dashboard/DashboardViewController.swift:16-29`

- [ ] **Step 1: Update AgentDisplayInfo struct**

In `Sources/UI/Dashboard/DashboardViewController.swift`, replace the `status: String` field:

```swift
struct AgentDisplayInfo {
    let id: String
    let name: String
    let project: String
    let thread: String
    let paneStatuses: [AgentStatus]     // Replaces status: String
    let mostRecentMessage: String       // Replaces lastMessage: String
    let mostRecentPaneIndex: Int
    let totalDuration: String
    let roundDuration: String
    let surface: TerminalSurface
    let worktreePath: String
    let paneCount: Int
    let paneSurfaces: [TerminalSurface]
}
```

- [ ] **Step 2: Update buildAgentDisplayInfos() in MainWindowController**

In `Sources/App/MainWindowController.swift:609-632`, update to use `WorktreeStatus`.

Note: The existing code uses `agent.branch` for `name`, `agent.status.rawValue.lowercased()` for `status`, and `agent.lastMessage` for `lastMessage`. The new version replaces `status` with `paneStatuses` and `lastMessage` with `mostRecentMessage`:

```swift
private func buildAgentDisplayInfos() -> [AgentDisplayInfo] {
    let agents = AgentHead.shared.allAgents()
    // Group by worktreePath — only build one display info per worktree
    var seen = Set<String>()
    var result: [AgentDisplayInfo] = []

    for agent in agents {
        guard let surface = agent.surface else { continue }
        guard !seen.contains(agent.worktreePath) else { continue }
        seen.insert(agent.worktreePath)

        let tree = surfaceManager.tree(forPath: agent.worktreePath)
        let paneCount = tree?.leafCount ?? 1
        let paneSurfaces: [TerminalSurface] = tree?.allLeaves.compactMap {
            SurfaceRegistry.shared.surface(forId: $0.surfaceId)
        } ?? [surface]

        let ws = statusAggregator.status(for: agent.worktreePath)
        let paneStatuses = ws?.statuses ?? [agent.status]
        let mostRecentMessage = ws?.mostRecentMessage ?? (agent.lastMessage.isEmpty ? "No active task." : agent.lastMessage)
        let mostRecentPaneIndex = ws?.mostRecentPaneIndex ?? 1

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
            paneSurfaces: paneSurfaces
        ))
    }
    return result
}
```

- [ ] **Step 3: Fix all compilation errors from callers of AgentDisplayInfo.status and .lastMessage**

Search for all uses of `.status` and `.lastMessage` on `AgentDisplayInfo` and update to use `.paneStatuses` and `.mostRecentMessage`. Common patterns:

```swift
// Before:
agent.status
// After (for display as string):
agent.paneStatuses.first?.rawValue ?? "unknown"

// Before:
agent.lastMessage
// After:
agent.mostRecentMessage
```

- [ ] **Step 4: Build to verify compilation**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -20`
Expected: BUILD SUCCEEDED

- [ ] **Step 5: Run full test suite**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test 2>&1 | tail -30`
Expected: All tests PASS

- [ ] **Step 6: Commit**

```bash
git add Sources/UI/Dashboard/DashboardViewController.swift Sources/App/MainWindowController.swift
git commit -m "refactor: update AgentDisplayInfo to use paneStatuses array"
```

---

### Task 7: AgentCardView — Multi-Dot Rendering

**Files:**
- Modify: `Sources/UI/Dashboard/AgentCardView.swift:30,88-91,135-159`

- [ ] **Step 1: Replace single statusDot with dot array**

In `AgentCardView`, replace the single `statusDot` property (line 30):

```swift
// Before:
let statusDot: NSView

// After:
var statusDots: [NSView] = []
```

- [ ] **Step 2: Update configure method to accept paneStatuses**

Update `configure(...)` (line 47) to accept `paneStatuses: [AgentStatus]` instead of `status: String`:

```swift
func configure(id: String, project: String, thread: String, paneStatuses: [AgentStatus],
               lastMessage: String, totalDuration: String, roundDuration: String, paneCount: Int) {
    // Remove old dots
    statusDots.forEach { $0.removeFromSuperview() }
    statusDots.removeAll()

    // Create dots for each pane status
    for (index, status) in paneStatuses.enumerated() {
        let dot = NSView()
        dot.wantsLayer = true
        dot.layer?.backgroundColor = status.color.cgColor
        dot.layer?.cornerRadius = 3.5
        dot.translatesAutoresizingMaskIntoConstraints = false
        bottomBar.addSubview(dot)

        NSLayoutConstraint.activate([
            dot.widthAnchor.constraint(equalToConstant: 7),
            dot.heightAnchor.constraint(equalToConstant: 7),
            dot.centerYAnchor.constraint(equalTo: bottomBar.centerYAnchor),
            dot.leadingAnchor.constraint(equalTo: index == 0
                ? bottomBar.leadingAnchor
                : statusDots[index - 1].trailingAnchor,
                constant: index == 0 ? 8 : 4),
        ])
        statusDots.append(dot)
    }

    // Update project label leading to be after last dot
    // ... rest of configure
}
```

- [ ] **Step 3: Update callers to pass paneStatuses**

Update all callers of `AgentCardView.configure(...)` to pass `paneStatuses` from `AgentDisplayInfo.paneStatuses`.

- [ ] **Step 4: Build and verify visually**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -20`
Expected: BUILD SUCCEEDED

- [ ] **Step 5: Commit**

```bash
git add Sources/UI/Dashboard/AgentCardView.swift
git commit -m "feat: render multi-dot status array in AgentCardView"
```

---

### Task 8: MiniCardView — Multi-Dot Rendering

**Files:**
- Modify: `Sources/UI/Dashboard/MiniCardView.swift:15,40-61,72-76,131-134`

- [ ] **Step 1: Replace single statusDot with dot array**

Same pattern as AgentCardView — replace `statusDot: NSView` with `statusDots: [NSView]`.

- [ ] **Step 2: Update configure method to accept paneStatuses**

Update `configure(...)` (line 40) to accept `paneStatuses: [AgentStatus]`:

```swift
func configure(id: String, project: String, thread: String, paneStatuses: [AgentStatus],
               lastMessage: String, totalDuration: String, roundDuration: String) {
    // Remove old dots, create new ones (6x6pt, 3pt radius, 4pt spacing)
    statusDots.forEach { $0.removeFromSuperview() }
    statusDots.removeAll()

    for (index, status) in paneStatuses.enumerated() {
        let dot = NSView()
        dot.wantsLayer = true
        dot.layer?.backgroundColor = status.color.cgColor
        dot.layer?.cornerRadius = 3
        dot.translatesAutoresizingMaskIntoConstraints = false
        addSubview(dot)

        NSLayoutConstraint.activate([
            dot.widthAnchor.constraint(equalToConstant: 6),
            dot.heightAnchor.constraint(equalToConstant: 6),
            dot.topAnchor.constraint(equalTo: topAnchor, constant: 10),
            dot.leadingAnchor.constraint(equalTo: index == 0
                ? leadingAnchor
                : statusDots[index - 1].trailingAnchor,
                constant: index == 0 ? 8 : 3),
        ])
        statusDots.append(dot)
    }
}
```

- [ ] **Step 3: Update callers**

Update all callers of `MiniCardView.configure(...)` to pass `paneStatuses`.

- [ ] **Step 4: Build**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -20`
Expected: BUILD SUCCEEDED

- [ ] **Step 5: Commit**

```bash
git add Sources/UI/Dashboard/MiniCardView.swift
git commit -m "feat: render multi-dot status array in MiniCardView"
```

---

### Task 9: FocusPanelView — Dot Array with Active Highlight

**Files:**
- Modify: `Sources/UI/Dashboard/FocusPanelView.swift:35,55-66,100-103,148-151`

- [ ] **Step 1: Replace single statusDot with clickable dot array**

Replace `statusDot: NSView` (line 35) with `statusDots: [NSButton]` — buttons for click handling.

- [ ] **Step 2: Update configure to accept paneStatuses and activePaneIndex**

```swift
func configure(name: String, project: String, thread: String,
               paneStatuses: [AgentStatus], activePaneIndex: Int,
               total: String, round: String) {
    // Remove old dots
    statusDots.forEach { $0.removeFromSuperview() }
    statusDots.removeAll()

    for (index, status) in paneStatuses.enumerated() {
        let dot = NSButton()
        dot.wantsLayer = true
        dot.isBordered = false
        dot.title = ""
        dot.layer?.backgroundColor = status.color.cgColor
        let isActive = (index + 1) == activePaneIndex
        let size: CGFloat = isActive ? 10 : 8
        dot.layer?.cornerRadius = size / 2
        dot.translatesAutoresizingMaskIntoConstraints = false
        dot.tag = index + 1  // paneIndex (1-based)
        dot.target = self
        dot.action = #selector(dotClicked(_:))
        headerView.addSubview(dot)

        NSLayoutConstraint.activate([
            dot.widthAnchor.constraint(equalToConstant: size),
            dot.heightAnchor.constraint(equalToConstant: size),
            dot.centerYAnchor.constraint(equalTo: headerView.centerYAnchor),
            dot.leadingAnchor.constraint(equalTo: index == 0
                ? headerView.leadingAnchor
                : statusDots[index - 1].trailingAnchor,
                constant: index == 0 ? 10 : 5),
        ])
        statusDots.append(dot)
    }
}

@objc private func dotClicked(_ sender: NSButton) {
    delegate?.focusPanelDidSelectPane(sender.tag)
}
```

- [ ] **Step 3: Add delegate method for pane selection**

Add to `FocusPanelDelegate` (or whatever delegate protocol FocusPanelView uses):

```swift
func focusPanelDidSelectPane(_ paneIndex: Int)
```

- [ ] **Step 4: Update callers and delegate implementation**

Update `MainWindowController` or `DashboardViewController` to handle `focusPanelDidSelectPane` by switching the displayed terminal surface.

- [ ] **Step 5: Build**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -20`
Expected: BUILD SUCCEEDED

- [ ] **Step 6: Commit**

```bash
git add Sources/UI/Dashboard/FocusPanelView.swift
git commit -m "feat: clickable multi-dot status in FocusPanelView with pane switching"
```

---

### Task 10: Integration — Remove Old StatusPublisherDelegate Usage

**Files:**
- Modify: `Sources/App/MainWindowController.swift:1649-1674`
- Modify: `Sources/Status/StatusPublisher.swift:3-5`

- [ ] **Step 1: Remove MainWindowController's StatusPublisherDelegate conformance**

The old `statusDidChange(worktreePath:...)` method in MainWindowController (lines 1649-1674) is now replaced by `WorktreeStatusDelegate` methods. Remove the old conformance and its method body.

- [ ] **Step 2: Verify StatusPublisherDelegate has no other consumers**

Search for other implementors of `StatusPublisherDelegate`. If none remain, the protocol and `StatusPublisher.delegate` property can be removed. If other consumers exist, leave it in place.

- [ ] **Step 3: Build and run full test suite**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test 2>&1 | tail -30`
Expected: All tests PASS

- [ ] **Step 4: Commit**

```bash
git add Sources/App/MainWindowController.swift Sources/Status/StatusPublisher.swift
git commit -m "refactor: remove old StatusPublisherDelegate in favor of WorktreeStatusDelegate"
```

---

### Task 11: Notification History — Navigate to Specific Pane

**Files:**
- Modify: `Sources/Status/NotificationManager.swift:126-151` (didReceive response handler)

- [ ] **Step 1: Update notification userInfo to include paneIndex**

In `NotificationManager.notify(...)`, when creating `UNMutableNotificationContent`, add paneIndex to userInfo:

```swift
content.userInfo = [
    "worktreePath": worktreePath,
    "paneIndex": paneIndex  // New
]
```

- [ ] **Step 2: Update didReceive to post paneIndex in notification**

In `userNotificationCenter(_:didReceive:withCompletionHandler:)`, extract paneIndex and include it in the posted notification:

```swift
let paneIndex = response.notification.request.content.userInfo["paneIndex"] as? Int
NotificationCenter.default.post(
    name: .navigateToWorktree,
    object: nil,
    userInfo: [
        "worktreePath": worktreePath,
        "paneIndex": paneIndex as Any
    ]
)
```

- [ ] **Step 3: Update MainWindowController's notification handler to focus specific pane**

In the handler for `.navigateToWorktree`, extract `paneIndex` and focus that pane after navigating to the worktree tab.

- [ ] **Step 4: Build**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -20`
Expected: BUILD SUCCEEDED

- [ ] **Step 5: Commit**

```bash
git add Sources/Status/NotificationManager.swift Sources/App/MainWindowController.swift
git commit -m "feat: notification click navigates to specific pane in multi-pane worktree"
```

---

### Task 12: Final Integration Test and Cleanup

**Files:**
- All modified files

- [ ] **Step 1: Run full test suite**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test 2>&1 | tail -30`
Expected: All tests PASS

- [ ] **Step 2: Build release configuration**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Release build 2>&1 | tail -20`
Expected: BUILD SUCCEEDED

- [ ] **Step 3: Verify single-pane backward compatibility**

Manual check: Launch app with single-pane worktrees. Verify:
- One status dot per card
- lastMessage displays correctly
- Notifications fire as before without `[Pane N]` suffix

- [ ] **Step 4: Verify multi-pane behavior**

Manual check: Split a pane, run agents in both. Verify:
- Multiple dots appear on card
- lastMessage shows most recently updated pane's message
- Notifications include `[Pane N]` suffix
- Clicking dots in FocusPanelView switches terminal

- [ ] **Step 5: Final commit**

```bash
git add -A
git commit -m "feat: complete multi-pane status, lastMessage, and notification support"
```
