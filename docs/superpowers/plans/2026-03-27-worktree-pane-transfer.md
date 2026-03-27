# Worktree Pane Transfer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** When Claude Code creates a git worktree from main, automatically transfer the active terminal pane to the new worktree and replace it with a fresh terminal in main.

**Architecture:** Extend the existing WorktreeCreate/CwdChanged webhook flow with a "pending transfer" tracker. When WorktreeCreate arrives (with `cwd` = source worktree, `worktree_name` = new branch), record the transfer intent. When the new worktree is subsequently discovered via CwdChanged, transfer the source worktree's primary surface to the new worktree instead of creating a new one, and backfill the source worktree with a fresh terminal.

**Tech Stack:** Swift 5.10, AppKit, existing WebhookServer + SplitTree + TerminalSurfaceManager infrastructure

---

## Background: Hook Event Flow

When Claude Code creates a worktree:

1. **WorktreeCreate** hook fires: `{ cwd: "/repo" (source), worktree_name: "feature-x", session_id: "abc" }`
2. **CwdChanged** hook fires: `{ cwd: "/repo/.worktrees/feature-x" (new path), session_id: "abc" }`
3. `WebhookStatusProvider` detects unknown path → `onNewWorktreeDetected` callback
4. `TabCoordinator.handleNewWorktreeFromHook` discovers + integrates the new worktree

Currently step 4 creates a **new** terminal for the new worktree. This plan changes it to **transfer** the existing terminal from the source worktree.

## File Structure

| Action | File | Responsibility |
|--------|------|----------------|
| Create | `Sources/Core/PendingWorktreeTransfer.swift` | Data struct + tracker for pending transfers |
| Modify | `Sources/Status/WebhookStatusProvider.swift` | Extract worktree_name from WorktreeCreate, pass source info to callback |
| Modify | `Sources/App/TabCoordinator.swift` | Orchestrate transfer in `handleNewWorktreeFromHook` |
| Modify | `Sources/Core/TerminalSurfaceManager.swift` | Add `transferTree` method to re-key a tree |
| Create | `tests/PaneTransferTests.swift` | Unit tests for PendingWorktreeTransfer and TerminalSurfaceManager.transferTree |

---

### Task 1: PendingWorktreeTransfer Data Model

**Files:**
- Create: `Sources/Core/PendingWorktreeTransfer.swift`
- Create: `tests/PaneTransferTests.swift`

- [ ] **Step 1: Write the test for PendingWorktreeTransfer**

```swift
// tests/PaneTransferTests.swift
import XCTest
@testable import amux

final class PaneTransferTests: XCTestCase {

    // MARK: - PendingWorktreeTransfer Tests

    func testRecordAndMatch() {
        let tracker = PendingTransferTracker()
        tracker.record(sourceWorktreePath: "/repo", worktreeName: "feature-x", sessionId: "s1")

        let result = tracker.consume(newWorktreePath: "/repo/.worktrees/feature-x")
        XCTAssertNotNil(result)
        XCTAssertEqual(result?.sourceWorktreePath, "/repo")
        XCTAssertEqual(result?.worktreeName, "feature-x")
        XCTAssertEqual(result?.sessionId, "s1")
    }

    func testConsumeRemovesEntry() {
        let tracker = PendingTransferTracker()
        tracker.record(sourceWorktreePath: "/repo", worktreeName: "feature-x", sessionId: "s1")

        _ = tracker.consume(newWorktreePath: "/repo/.worktrees/feature-x")
        let second = tracker.consume(newWorktreePath: "/repo/.worktrees/feature-x")
        XCTAssertNil(second)
    }

    func testNoMatchForUnrelatedPath() {
        let tracker = PendingTransferTracker()
        tracker.record(sourceWorktreePath: "/repo", worktreeName: "feature-x", sessionId: "s1")

        let result = tracker.consume(newWorktreePath: "/other-repo/feature-y")
        XCTAssertNil(result)
    }

    func testMatchByWorktreeNameSuffix() {
        let tracker = PendingTransferTracker()
        tracker.record(sourceWorktreePath: "/repo", worktreeName: "feature-x", sessionId: "s1")

        // Worktree might be created at a sibling path, not nested
        let result = tracker.consume(newWorktreePath: "/worktrees/feature-x")
        XCTAssertNotNil(result)
        XCTAssertEqual(result?.worktreeName, "feature-x")
    }

    func testStaleEntriesExpire() {
        let tracker = PendingTransferTracker()
        tracker.record(sourceWorktreePath: "/repo", worktreeName: "old", sessionId: "s1")
        // Manually expire by setting timestamp in the past
        tracker.expireAll()

        let result = tracker.consume(newWorktreePath: "/repo/.worktrees/old")
        XCTAssertNil(result)
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/PaneTransferTests 2>&1 | tail -20`
Expected: Compilation errors — `PendingTransferTracker` does not exist yet.

- [ ] **Step 3: Implement PendingWorktreeTransfer**

```swift
// Sources/Core/PendingWorktreeTransfer.swift
import Foundation

/// Records a pending worktree transfer intent from a WorktreeCreate hook event.
struct PendingWorktreeTransfer {
    let sourceWorktreePath: String
    let worktreeName: String
    let sessionId: String
    let recordedAt: Date
}

/// Tracks pending transfers between WorktreeCreate and the subsequent CwdChanged/discovery.
/// Thread-safe — guarded by NSLock.
class PendingTransferTracker {
    private var pending: [PendingWorktreeTransfer] = []
    private let lock = NSLock()
    /// Transfers older than this are discarded (seconds).
    private let ttl: TimeInterval = 30

    /// Record that a worktree creation is in progress.
    func record(sourceWorktreePath: String, worktreeName: String, sessionId: String) {
        lock.lock()
        defer { lock.unlock() }
        pruneStale()
        pending.append(PendingWorktreeTransfer(
            sourceWorktreePath: sourceWorktreePath,
            worktreeName: worktreeName,
            sessionId: sessionId,
            recordedAt: Date()
        ))
    }

    /// Try to match a newly discovered worktree path to a pending transfer.
    /// Matching strategy: the new path's last component equals the recorded worktreeName.
    /// Consumes (removes) the match if found.
    func consume(newWorktreePath: String) -> PendingWorktreeTransfer? {
        lock.lock()
        defer { lock.unlock() }
        pruneStale()
        let newName = URL(fileURLWithPath: newWorktreePath).lastPathComponent
        guard let index = pending.firstIndex(where: { $0.worktreeName == newName }) else {
            return nil
        }
        return pending.remove(at: index)
    }

    /// For testing: expire all entries immediately.
    func expireAll() {
        lock.lock()
        defer { lock.unlock() }
        pending.removeAll()
    }

    private func pruneStale() {
        let cutoff = Date().addingTimeInterval(-ttl)
        pending.removeAll { $0.recordedAt < cutoff }
    }
}
```

- [ ] **Step 4: Regenerate Xcode project and run tests**

Run: `xcodegen generate && xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/PaneTransferTests 2>&1 | tail -20`
Expected: All 5 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add Sources/Core/PendingWorktreeTransfer.swift tests/PaneTransferTests.swift project.yml
git commit -m "feat: add PendingTransferTracker for worktree pane transfers"
```

---

### Task 2: TerminalSurfaceManager.transferTree

**Files:**
- Modify: `Sources/Core/TerminalSurfaceManager.swift`
- Modify: `tests/PaneTransferTests.swift`

- [ ] **Step 1: Write the test for transferTree**

Append to `tests/PaneTransferTests.swift`:

```swift
// MARK: - TerminalSurfaceManager Transfer Tests

func testTransferTreeRekeys() {
    let manager = TerminalSurfaceManager()
    let info = WorktreeInfo(path: "/repo", branch: "main", commitHash: "abc", isMainWorktree: true)
    let tree = manager.tree(for: info, backend: "local")

    let transferred = manager.transferTree(fromPath: "/repo", toPath: "/worktrees/feature-x")
    XCTAssertNotNil(transferred)
    XCTAssertNil(manager.tree(forPath: "/repo"))
    XCTAssertNotNil(manager.tree(forPath: "/worktrees/feature-x"))
    XCTAssertTrue(transferred === tree)
}

func testTransferTreeReturnsNilForUnknownPath() {
    let manager = TerminalSurfaceManager()
    let result = manager.transferTree(fromPath: "/nonexistent", toPath: "/dest")
    XCTAssertNil(result)
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/PaneTransferTests/testTransferTreeRekeys 2>&1 | tail -20`
Expected: FAIL — `transferTree` does not exist.

- [ ] **Step 3: Add transferTree to TerminalSurfaceManager**

Add this method to `Sources/Core/TerminalSurfaceManager.swift` after the `registerTree` method (around line 41):

```swift
/// Transfer a tree from one worktree path to another.
/// Removes the tree from the old key and registers it under the new key.
/// Returns the transferred tree, or nil if no tree exists at fromPath.
@discardableResult
func transferTree(fromPath: String, toPath: String) -> SplitTree? {
    guard let tree = trees.removeValue(forKey: fromPath) else { return nil }
    trees[toPath] = tree
    return tree
}
```

- [ ] **Step 4: Run tests**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/PaneTransferTests 2>&1 | tail -20`
Expected: All 7 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add Sources/Core/TerminalSurfaceManager.swift tests/PaneTransferTests.swift
git commit -m "feat: add TerminalSurfaceManager.transferTree for re-keying trees"
```

---

### Task 3: WebhookStatusProvider — Pass WorktreeCreate Source Info

Currently `onNewWorktreeDetected` only passes the new worktree path. We need to also capture the WorktreeCreate event data (source path, worktree name, session ID) and pass it through.

**Files:**
- Modify: `Sources/Status/WebhookStatusProvider.swift`

- [ ] **Step 1: Add onWorktreeCreateReceived callback**

In `Sources/Status/WebhookStatusProvider.swift`, add a new callback alongside the existing `onNewWorktreeDetected`:

```swift
/// Called when a WorktreeCreate event arrives, with source worktree path and worktree name.
/// Fires before the new worktree is discoverable (the git operation may still be in progress).
var onWorktreeCreateReceived: ((_ sourceWorktreePath: String, _ worktreeName: String, _ sessionId: String) -> Void)?
```

- [ ] **Step 2: Extract worktree_name from WorktreeCreate events and fire callback**

In the `handleEvent` method, replace the early-return block for `.worktreeCreate` (lines 37-44):

Old code:
```swift
// WorktreeCreate / CwdChanged with unknown path → notify upstream to discover it
if event.event == .worktreeCreate || event.event == .cwdChanged {
    if matchWorktree(canonCwd) == nil {
        NSLog("[WebhookStatusProvider] New worktree detected via hook (\(event.event.rawValue)): \(event.cwd)")
        DispatchQueue.main.async { [weak self] in
            self?.onNewWorktreeDetected?(canonCwd)
        }
    }
    if event.event == .worktreeCreate { return }
    // CwdChanged falls through to update session status
}
```

New code:
```swift
// WorktreeCreate: record transfer intent before new worktree is discoverable
if event.event == .worktreeCreate {
    let worktreeName = event.data?["worktree_name"] as? String ?? ""
    if !worktreeName.isEmpty {
        let sourcePath = canonCwd
        NSLog("[WebhookStatusProvider] WorktreeCreate from \(sourcePath): \(worktreeName)")
        DispatchQueue.main.async { [weak self] in
            self?.onWorktreeCreateReceived?(sourcePath, worktreeName, event.sessionId)
        }
    }
    return
}

// CwdChanged with unknown path → notify upstream to discover it
if event.event == .cwdChanged {
    if matchWorktree(canonCwd) == nil {
        NSLog("[WebhookStatusProvider] New worktree detected via CwdChanged: \(event.cwd)")
        DispatchQueue.main.async { [weak self] in
            self?.onNewWorktreeDetected?(canonCwd)
        }
    }
    // CwdChanged falls through to update session status
}
```

- [ ] **Step 3: Verify build compiles**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -5`
Expected: BUILD SUCCEEDED

- [ ] **Step 4: Commit**

```bash
git add Sources/Status/WebhookStatusProvider.swift
git commit -m "feat: extract worktree_name from WorktreeCreate hook, add onWorktreeCreateReceived callback"
```

---

### Task 4: TabCoordinator — Wire Up Transfer Logic

This is the core orchestration task. Modify `TabCoordinator` to:
1. Listen for `onWorktreeCreateReceived` → record in `PendingTransferTracker`
2. In `handleNewWorktreeFromHook` → check tracker, transfer instead of creating new

**Files:**
- Modify: `Sources/App/TabCoordinator.swift`

- [ ] **Step 1: Add PendingTransferTracker property**

In `TabCoordinator`, add after the `runtimeBackend` property (around line 28):

```swift
private let pendingTransfers = PendingTransferTracker()
```

- [ ] **Step 2: Wire onWorktreeCreateReceived in loadWorkspaces**

In the webhook setup block inside `loadWorkspaces()` (around line 363-373), add the new callback after the existing `onNewWorktreeDetected` wiring:

```swift
self.statusPublisher.webhookProvider.onWorktreeCreateReceived = { [weak self] sourcePath, worktreeName, sessionId in
    guard let self else { return }
    NSLog("[TabCoordinator] WorktreeCreate: recording pending transfer from \(sourcePath) for \(worktreeName)")
    self.pendingTransfers.record(sourceWorktreePath: sourcePath, worktreeName: worktreeName, sessionId: sessionId)
}
```

- [ ] **Step 3: Modify handleNewWorktreeFromHook to attempt transfer**

Replace the inner loop in `handleNewWorktreeFromHook` (lines 403-413) that creates new trees. The full replacement of the `for info in newWorktrees` loop:

```swift
for info in newWorktrees {
    let proj = self.workspaceManager.tabs.first(where: { $0.repoPath == repoRoot })?.displayName
        ?? URL(fileURLWithPath: repoRoot).lastPathComponent

    // Check if this worktree has a pending transfer (created via hook from an existing pane)
    if let transfer = self.pendingTransfers.consume(newWorktreePath: info.path) {
        NSLog("[TabCoordinator] Transferring pane from \(transfer.sourceWorktreePath) to \(info.path)")
        self.performPaneTransfer(transfer: transfer, newInfo: info, repoRoot: repoRoot, project: proj)
    } else {
        // No pending transfer — create a fresh tree as before
        let tree = self.terminalCoordinator.resolveTree(for: info)
        self.allWorktrees.append((info: info, tree: tree))
        self.worktreeRepoCache[info.path] = repoRoot

        let sessionName = self.runtimeBackend == "local" ? nil : SessionManager.persistentSessionName(for: info.path)
        if let surface = self.terminalCoordinator.surfaceManager.primarySurface(forPath: info.path) {
            AgentHead.shared.register(surface: surface, worktreePath: info.path, branch: info.branch, project: proj, startedAt: Date(), tmuxSessionName: sessionName, backend: self.runtimeBackend)
        }
    }
}
```

- [ ] **Step 4: Implement performPaneTransfer**

Add this private method to `TabCoordinator`, after `handleNewWorktreeFromHook`:

```swift
private func performPaneTransfer(transfer: PendingWorktreeTransfer, newInfo: WorktreeInfo, repoRoot: String, project: String) {
    let sourcePath = transfer.sourceWorktreePath

    // 1. Transfer the SplitTree from source → new worktree path
    guard let transferredTree = terminalCoordinator.surfaceManager.transferTree(fromPath: sourcePath, toPath: newInfo.path) else {
        NSLog("[TabCoordinator] Transfer failed: no tree at \(sourcePath), falling back to fresh tree")
        let tree = terminalCoordinator.resolveTree(for: newInfo)
        allWorktrees.append((info: newInfo, tree: tree))
        worktreeRepoCache[newInfo.path] = repoRoot
        return
    }

    // 2. Update allWorktrees: remove old entry for source, add new entry
    allWorktrees.removeAll { $0.info.path == sourcePath }
    allWorktrees.append((info: newInfo, tree: transferredTree))
    worktreeRepoCache[newInfo.path] = repoRoot

    // 3. Re-register transferred surfaces in AgentHead under new worktree
    for leaf in transferredTree.allLeaves {
        if let surface = SurfaceRegistry.shared.surface(forId: leaf.surfaceId) {
            if let oldAgent = AgentHead.shared.agent(forWorktree: sourcePath) {
                AgentHead.shared.unregister(terminalID: oldAgent.id)
            }
            let sessionName = runtimeBackend == "local" ? nil : SessionManager.persistentSessionName(for: newInfo.path)
            AgentHead.shared.register(surface: surface, worktreePath: newInfo.path, branch: newInfo.branch, project: project, startedAt: Date(), tmuxSessionName: sessionName, backend: runtimeBackend)
        }
    }

    // 4. Save the transferred tree's layout under the new path, remove old
    terminalCoordinator.config.splitLayouts.removeValue(forKey: sourcePath)
    terminalCoordinator.saveSplitLayout(transferredTree)

    // 5. Invalidate the old split container so the UI rebuilds it
    if let repoVC = repoVCs[repoRoot] {
        repoVC.invalidateSplitContainer(forPath: sourcePath)
    }

    // 6. Create a fresh tree for the source worktree (e.g., main)
    if let sourceInfo = allWorktrees.first(where: { $0.info.path == sourcePath })?.info
        ?? worktrees(forRepo: repoRoot)?.first(where: { $0.path == sourcePath }) {
        let freshTree = terminalCoordinator.surfaceManager.tree(for: sourceInfo, backend: runtimeBackend)
        // Update allWorktrees with the fresh tree for source
        if let idx = allWorktrees.firstIndex(where: { $0.info.path == sourcePath }) {
            allWorktrees[idx] = (info: sourceInfo, tree: freshTree)
        } else {
            allWorktrees.append((info: sourceInfo, tree: freshTree))
        }
        let sessionName = runtimeBackend == "local" ? nil : SessionManager.persistentSessionName(for: sourceInfo.path)
        if let surface = terminalCoordinator.surfaceManager.primarySurface(forPath: sourcePath) {
            AgentHead.shared.register(surface: surface, worktreePath: sourcePath, branch: sourceInfo.branch, project: project, startedAt: Date(), tmuxSessionName: sessionName, backend: runtimeBackend)
        }
        terminalCoordinator.saveSplitLayout(freshTree)
    }
}
```

- [ ] **Step 5: Add helper method worktrees(forRepo:)**

Add this private helper to `TabCoordinator` (it's used in `performPaneTransfer` to find the source worktree info when it's no longer in `allWorktrees` because it was already removed — the full worktree list from discovery still has it):

```swift
/// Look up discovered worktrees for a repo from the workspace manager.
private func worktrees(forRepo repoPath: String) -> [WorktreeInfo]? {
    guard let tabIndex = workspaceManager.tabs.firstIndex(where: { $0.repoPath == repoPath }) else { return nil }
    return workspaceManager.tab(at: tabIndex)?.worktrees
}
```

Note: This depends on how `WorkspaceManager` stores worktrees. If it doesn't store them directly, the worktrees are available from the `worktrees` parameter in the closure where `performPaneTransfer` is called. In that case, pass `worktrees` as a parameter to `performPaneTransfer` instead.

- [ ] **Step 6: Verify build compiles**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -10`
Expected: BUILD SUCCEEDED (or adjust for missing methods — see Task 5).

- [ ] **Step 7: Commit**

```bash
git add Sources/App/TabCoordinator.swift
git commit -m "feat: wire up pane transfer in handleNewWorktreeFromHook"
```

---

### Task 5: RepoViewController — invalidateSplitContainer

The transferred surface's old `SplitContainerView` is stale (it still references the old tree/path). We need a method to clear it so the next `showTerminal` call rebuilds it.

**Files:**
- Modify: `Sources/UI/Repo/RepoViewController.swift`

- [ ] **Step 1: Add invalidateSplitContainer method**

Add after the existing `reconfigure()` method (around line 202):

```swift
/// Remove the cached SplitContainerView for a worktree path.
/// Called when a surface has been transferred to a different worktree
/// and the old container is stale.
func invalidateSplitContainer(forPath path: String) {
    if let container = splitContainers.removeValue(forKey: path) {
        if container === activeSplitContainer {
            container.removeFromSuperview()
            activeSplitContainer = nil
        }
    }
}
```

- [ ] **Step 2: Verify build compiles**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -5`
Expected: BUILD SUCCEEDED

- [ ] **Step 3: Commit**

```bash
git add Sources/UI/Repo/RepoViewController.swift
git commit -m "feat: add invalidateSplitContainer for pane transfer cleanup"
```

---

### Task 6: Update handleNewWorktreeFromHook — Pass worktrees to performPaneTransfer

The `performPaneTransfer` method needs access to the full worktree list from `WorktreeDiscovery` (to find the source worktree's `WorktreeInfo` when creating its replacement). The discovery results are available in the enclosing closure but not passed through.

**Files:**
- Modify: `Sources/App/TabCoordinator.swift`

- [ ] **Step 1: Update performPaneTransfer signature to accept worktrees**

Change the method signature:

```swift
private func performPaneTransfer(transfer: PendingWorktreeTransfer, newInfo: WorktreeInfo, repoRoot: String, project: String, allDiscoveredWorktrees: [WorktreeInfo]) {
```

And replace the source worktree lookup (step 6 in the method body) to use `allDiscoveredWorktrees`:

```swift
// 6. Create a fresh tree for the source worktree (e.g., main)
if let sourceInfo = allDiscoveredWorktrees.first(where: { $0.path == transfer.sourceWorktreePath }) {
    let freshTree = terminalCoordinator.surfaceManager.tree(for: sourceInfo, backend: runtimeBackend)
    if let idx = allWorktrees.firstIndex(where: { $0.info.path == sourceInfo.path }) {
        allWorktrees[idx] = (info: sourceInfo, tree: freshTree)
    } else {
        allWorktrees.append((info: sourceInfo, tree: freshTree))
    }
    worktreeRepoCache[sourceInfo.path] = repoRoot
    let sessionName = runtimeBackend == "local" ? nil : SessionManager.persistentSessionName(for: sourceInfo.path)
    if let surface = terminalCoordinator.surfaceManager.primarySurface(forPath: sourceInfo.path) {
        AgentHead.shared.register(surface: surface, worktreePath: sourceInfo.path, branch: sourceInfo.branch, project: project, startedAt: Date(), tmuxSessionName: sessionName, backend: runtimeBackend)
    }
    terminalCoordinator.saveSplitLayout(freshTree)
}
```

- [ ] **Step 2: Update call site in handleNewWorktreeFromHook**

In the `for info in newWorktrees` loop, pass `worktrees` (from the discovery closure):

```swift
self.performPaneTransfer(transfer: transfer, newInfo: info, repoRoot: repoRoot, project: proj, allDiscoveredWorktrees: worktrees)
```

- [ ] **Step 3: Remove the worktrees(forRepo:) helper if added in Task 4**

It's no longer needed — delete it.

- [ ] **Step 4: Verify full build**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -5`
Expected: BUILD SUCCEEDED

- [ ] **Step 5: Commit**

```bash
git add Sources/App/TabCoordinator.swift
git commit -m "refactor: pass discovered worktrees to performPaneTransfer"
```

---

### Task 7: Auto-Navigate Sidebar to New Worktree

After a transfer, the sidebar should auto-select the new worktree so the user sees the transferred terminal immediately.

**Files:**
- Modify: `Sources/App/TabCoordinator.swift`

- [ ] **Step 1: Add navigation after transfer in handleNewWorktreeFromHook**

At the end of `handleNewWorktreeFromHook`, after the existing sidebar/dashboard update block (after line 422), add navigation to the new worktree:

```swift
// Auto-navigate to the transferred worktree in the repo VC
if let repoVC = self.repoVCs[repoRoot] {
    repoVC.configure(worktrees: worktrees, trees: self.terminalCoordinator.surfaceManager.all)
    // Find index of newly added worktree and switch to it
    if let newIndex = worktrees.firstIndex(where: { newWorktrees.contains(where: { nw in nw.path == $0.path }) }) {
        repoVC.showTerminal(at: newIndex)
    }
}
```

This replaces the existing:
```swift
// Update repo VC sidebar if it's open
if let repoVC = self.repoVCs[repoRoot] {
    repoVC.configure(worktrees: worktrees, trees: self.terminalCoordinator.surfaceManager.all)
}
```

- [ ] **Step 2: Verify build**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -5`
Expected: BUILD SUCCEEDED

- [ ] **Step 3: Commit**

```bash
git add Sources/App/TabCoordinator.swift
git commit -m "feat: auto-navigate sidebar to newly transferred worktree"
```

---

### Task 8: Run Full Test Suite & Manual Verification

- [ ] **Step 1: Run all unit tests**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test 2>&1 | tail -30`
Expected: All tests PASS.

- [ ] **Step 2: Manual test plan**

1. Launch amux with a repo that has a main worktree
2. Open a terminal in main, start Claude Code
3. Ask Claude Code to create a worktree (e.g., `git worktree add ../feature-test feature-test`)
4. Verify:
   - Sidebar refreshes showing the new worktree
   - The terminal pane (with Claude Code running) moves to the new worktree entry
   - Main gets a fresh, empty terminal
   - Clicking on main in sidebar shows the fresh terminal
   - Clicking on the new worktree shows the transferred terminal with Claude Code still running

- [ ] **Step 3: Final commit (if any fixups needed)**

```bash
git add -A
git commit -m "fix: address issues found during manual testing"
```
