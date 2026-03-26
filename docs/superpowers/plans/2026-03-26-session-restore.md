# Session Restore Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remember the last active tab, worktree, and focused pane so the app restores to that state on relaunch instead of showing the dashboard.

**Architecture:** Add three fields to Config (`activeTabRepoPath`, `activeWorktreePaths`, `focusedPaneIds`) that are saved on every navigation change (debounced). On launch, after `loadWorkspaces()` completes, restore the saved tab/worktree/pane with fallback to dashboard if the saved state is stale.

**Tech Stack:** Swift, AppKit, JSON (existing Config system)

---

### Task 1: Add session state fields to Config

**Files:**
- Modify: `Sources/Core/Config.swift:3-69`
- Test: `Tests/ConfigTests.swift` (if exists, otherwise new test in existing test target)

- [ ] **Step 1: Write the failing test**

```swift
// In Tests/ConfigTests.swift (or a new file Tests/SessionRestoreConfigTests.swift)
import XCTest
@testable import pmux

final class SessionRestoreConfigTests: XCTestCase {
    func testSessionFieldsDecodeFromJSON() throws {
        let json = """
        {
            "active_tab_repo_path": "/repos/myproject",
            "active_worktree_paths": {"/repos/myproject": "/repos/myproject/wt-feat"},
            "focused_pane_ids": {"/repos/myproject/wt-feat": "leaf-abc"}
        }
        """.data(using: .utf8)!
        let config = try JSONDecoder().decode(Config.self, from: json)
        XCTAssertEqual(config.activeTabRepoPath, "/repos/myproject")
        XCTAssertEqual(config.activeWorktreePaths["/repos/myproject"], "/repos/myproject/wt-feat")
        XCTAssertEqual(config.focusedPaneIds["/repos/myproject/wt-feat"], "leaf-abc")
    }

    func testSessionFieldsDefaultToEmpty() throws {
        let json = "{}".data(using: .utf8)!
        let config = try JSONDecoder().decode(Config.self, from: json)
        XCTAssertNil(config.activeTabRepoPath)
        XCTAssertTrue(config.activeWorktreePaths.isEmpty)
        XCTAssertTrue(config.focusedPaneIds.isEmpty)
    }

    func testSessionFieldsRoundTrip() throws {
        var config = Config()
        config.activeTabRepoPath = "/repos/proj"
        config.activeWorktreePaths = ["/repos/proj": "/repos/proj/wt-1"]
        config.focusedPaneIds = ["/repos/proj/wt-1": "leaf-xyz"]

        let data = try JSONEncoder().encode(config)
        let decoded = try JSONDecoder().decode(Config.self, from: data)
        XCTAssertEqual(decoded.activeTabRepoPath, "/repos/proj")
        XCTAssertEqual(decoded.activeWorktreePaths, config.activeWorktreePaths)
        XCTAssertEqual(decoded.focusedPaneIds, config.focusedPaneIds)
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmuxTests -configuration Debug test -only-testing:pmuxTests/SessionRestoreConfigTests 2>&1 | tail -20`
Expected: Compile error — `activeTabRepoPath`, `activeWorktreePaths`, `focusedPaneIds` don't exist on Config

- [ ] **Step 3: Add the three fields to Config**

In `Sources/Core/Config.swift`, add three new fields to the `Config` struct:

```swift
// After line 16 (splitLayouts)
var activeTabRepoPath: String?
var activeWorktreePaths: [String: String]
var focusedPaneIds: [String: String]
```

Add CodingKeys:

```swift
// In CodingKeys enum, after splitLayouts
case activeTabRepoPath = "active_tab_repo_path"
case activeWorktreePaths = "active_worktree_paths"
case focusedPaneIds = "focused_pane_ids"
```

Update `init()` defaults:

```swift
// After splitLayouts = [:]
activeTabRepoPath = nil
activeWorktreePaths = [:]
focusedPaneIds = [:]
```

Update `init(from decoder:)`:

```swift
// After the splitLayouts line
activeTabRepoPath = try container.decodeIfPresent(String.self, forKey: .activeTabRepoPath)
activeWorktreePaths = try container.decodeIfPresent([String: String].self, forKey: .activeWorktreePaths) ?? [:]
focusedPaneIds = try container.decodeIfPresent([String: String].self, forKey: .focusedPaneIds) ?? [:]
```

- [ ] **Step 4: Run test to verify it passes**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmuxTests -configuration Debug test -only-testing:pmuxTests/SessionRestoreConfigTests 2>&1 | tail -20`
Expected: All 3 tests PASS

- [ ] **Step 5: Commit**

```bash
git add Sources/Core/Config.swift Tests/SessionRestoreConfigTests.swift
git commit -m "feat: add session restore fields to Config"
```

---

### Task 2: Save session state on tab switch

**Files:**
- Modify: `Sources/App/TabCoordinator.swift:51-82` (switchToTab method)

- [ ] **Step 1: Add saveSessionState helper to TabCoordinator**

Add this method at the end of the `TabCoordinator` class (before the closing `}`):

```swift
// MARK: - Session State Persistence

func saveSessionState() {
    if activeTabIndex == 0 {
        config.activeTabRepoPath = nil
    } else {
        let repoIndex = activeTabIndex - 1
        if let tab = workspaceManager.tab(at: repoIndex) {
            config.activeTabRepoPath = tab.repoPath
        }
    }
    config.save()
}
```

- [ ] **Step 2: Call saveSessionState at end of switchToTab**

In `switchToTab(_:)`, after line 81 (`delegate?.tabCoordinatorDidSwitchTab(self)`), add:

```swift
saveSessionState()
```

- [ ] **Step 3: Build to verify compilation**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmux -configuration Debug build 2>&1 | tail -5`
Expected: BUILD SUCCEEDED

- [ ] **Step 4: Commit**

```bash
git add Sources/App/TabCoordinator.swift
git commit -m "feat: save active tab repo path on tab switch"
```

---

### Task 3: Save active worktree on worktree switch

**Files:**
- Modify: `Sources/UI/Repo/RepoViewController.swift:206-264` (showTerminal method)
- Modify: `Sources/App/TabCoordinator.swift` (saveSessionState)

- [ ] **Step 1: Add notification for worktree selection change**

In `Sources/UI/Repo/RepoViewController.swift`, at the end of `showTerminal(at:)` (after line 263, the makeFirstResponder block), add:

```swift
NotificationCenter.default.post(
    name: .repoViewDidChangeWorktree,
    object: self,
    userInfo: ["worktreePath": info.path]
)
```

Add the notification name as an extension at the bottom of the file (after the `Collection` extension):

```swift
extension Notification.Name {
    static let repoViewDidChangeWorktree = Notification.Name("repoViewDidChangeWorktree")
}
```

- [ ] **Step 2: Observe the notification in TabCoordinator**

In `TabCoordinator.init(config:)`, add observer:

```swift
NotificationCenter.default.addObserver(self, selector: #selector(handleWorktreeSelectionChanged(_:)), name: .repoViewDidChangeWorktree, object: nil)
```

Add the handler in the Session State Persistence section:

```swift
@objc private func handleWorktreeSelectionChanged(_ notification: Notification) {
    guard let worktreePath = notification.userInfo?["worktreePath"] as? String,
          let repoPath = worktreeRepoCache[worktreePath] else { return }
    config.activeWorktreePaths[repoPath] = worktreePath
    config.save()
}
```

- [ ] **Step 3: Build to verify compilation**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmux -configuration Debug build 2>&1 | tail -5`
Expected: BUILD SUCCEEDED

- [ ] **Step 4: Commit**

```bash
git add Sources/UI/Repo/RepoViewController.swift Sources/App/TabCoordinator.swift
git commit -m "feat: save active worktree path on worktree switch"
```

---

### Task 4: Save focused pane on pane focus change

**Files:**
- Modify: `Sources/UI/Repo/RepoViewController.swift:362-364` (splitContainer didChangeFocus delegate)
- Modify: `Sources/App/TabCoordinator.swift` (saveSessionState)

- [ ] **Step 1: Post notification on pane focus change**

In `RepoViewController`, update the `splitContainer(_:didChangeFocus:)` delegate method:

```swift
func splitContainer(_ view: SplitContainerView, didChangeFocus leafId: String) {
    guard activeWorktreeIndex >= 0, activeWorktreeIndex < worktrees.count else { return }
    let worktreePath = worktrees[activeWorktreeIndex].path
    NotificationCenter.default.post(
        name: .repoViewDidChangeFocusedPane,
        object: self,
        userInfo: ["worktreePath": worktreePath, "focusedLeafId": leafId]
    )
}
```

Add notification name:

```swift
// In the Notification.Name extension
static let repoViewDidChangeFocusedPane = Notification.Name("repoViewDidChangeFocusedPane")
```

- [ ] **Step 2: Observe in TabCoordinator**

In `TabCoordinator.init(config:)`, add:

```swift
NotificationCenter.default.addObserver(self, selector: #selector(handlePaneFocusChanged(_:)), name: .repoViewDidChangeFocusedPane, object: nil)
```

Add handler:

```swift
@objc private func handlePaneFocusChanged(_ notification: Notification) {
    guard let worktreePath = notification.userInfo?["worktreePath"] as? String,
          let leafId = notification.userInfo?["focusedLeafId"] as? String else { return }
    config.focusedPaneIds[worktreePath] = leafId
    config.save()
}
```

- [ ] **Step 3: Build to verify compilation**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmux -configuration Debug build 2>&1 | tail -5`
Expected: BUILD SUCCEEDED

- [ ] **Step 4: Commit**

```bash
git add Sources/UI/Repo/RepoViewController.swift Sources/App/TabCoordinator.swift
git commit -m "feat: save focused pane ID on pane focus change"
```

---

### Task 5: Restore session state on launch

**Files:**
- Modify: `Sources/App/TabCoordinator.swift:249-354` (loadWorkspaces method)
- Modify: `Sources/UI/Repo/RepoViewController.swift:153-166` (configure method)

- [ ] **Step 1: Add restoreSessionState method to TabCoordinator**

Add after `saveSessionState()`:

```swift
func restoreSessionState() {
    // Determine which tab to show
    guard let savedRepoPath = config.activeTabRepoPath,
          let tabIndex = workspaceManager.tabs.firstIndex(where: { $0.repoPath == savedRepoPath }) else {
        // No saved state or repo no longer exists — stay on dashboard
        switchToTab(0)
        return
    }

    let uiTabIndex = tabIndex + 1
    switchToTab(uiTabIndex)

    // Restore worktree selection within the repo
    let tab = workspaceManager.tabs[tabIndex]
    if let savedWorktreePath = config.activeWorktreePaths[savedRepoPath],
       let repoVC = repoVCs[savedRepoPath] {
        repoVC.selectWorktree(byPath: savedWorktreePath)

        // Restore focused pane within the worktree
        if let savedLeafId = config.focusedPaneIds[savedWorktreePath],
           let container = repoVC.activeSplitContainer,
           let tree = container.tree,
           tree.allLeaves.contains(where: { $0.id == savedLeafId }) {
            tree.focusedId = savedLeafId
            container.updateDimOverlays()
            if let leaf = tree.allLeaves.first(where: { $0.id == savedLeafId }),
               let surface = SurfaceRegistry.shared.surface(forId: leaf.surfaceId),
               let termView = surface.view {
                repoVC.view.window?.makeFirstResponder(termView)
            }
        }
    } else if let firstWorktree = tab.worktrees.first,
              let repoVC = repoVCs[savedRepoPath] {
        // Saved worktree gone, fall back to first
        repoVC.selectWorktree(byPath: firstWorktree.path)
    }
}
```

- [ ] **Step 2: Call restoreSessionState instead of switchToTab(0) in loadWorkspaces**

In `loadWorkspaces()`, find the section after `self.updateStatusPollPreferences()` (around line 335). The current code does NOT have an explicit `switchToTab(0)` call — the dashboard is just already embedded by `setupLayout()`. Add the restore call after `self.updateStatusPollPreferences()`:

```swift
// After self.updateStatusPollPreferences() (line 335)
// Restore last session state (tab, worktree, pane)
self.restoreSessionState()
```

Note: `restoreSessionState()` calls `switchToTab(0)` as its fallback, which embeds the dashboard — so the existing behavior is preserved when there's no saved state.

- [ ] **Step 3: Build to verify compilation**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmux -configuration Debug build 2>&1 | tail -5`
Expected: BUILD SUCCEEDED

- [ ] **Step 4: Commit**

```bash
git add Sources/App/TabCoordinator.swift
git commit -m "feat: restore last tab, worktree, and pane on app launch"
```

---

### Task 6: Handle stale pane IDs gracefully

Pane leaf IDs are generated at runtime (UUIDs). When split layouts are restored from `config.splitLayouts`, new leaf IDs are assigned. The saved `focusedPaneIds` will reference old IDs that no longer exist. We need to save/restore by session name (stable across launches) instead of leaf ID.

**Files:**
- Modify: `Sources/App/TabCoordinator.swift` (handlePaneFocusChanged, restoreSessionState)

- [ ] **Step 1: Write test for session-name-based pane lookup**

```swift
// In Tests/SessionRestoreConfigTests.swift
func testFocusedPaneIdsBySessionName() throws {
    let json = """
    {
        "focused_pane_ids": {"/repos/proj/wt": "pmux-proj-feat-1"}
    }
    """.data(using: .utf8)!
    let config = try JSONDecoder().decode(Config.self, from: json)
    XCTAssertEqual(config.focusedPaneIds["/repos/proj/wt"], "pmux-proj-feat-1")
}
```

- [ ] **Step 2: Run test to verify it passes (field already exists)**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmuxTests -configuration Debug test -only-testing:pmuxTests/SessionRestoreConfigTests/testFocusedPaneIdsBySessionName 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 3: Update save to store session name instead of leaf ID**

In `handlePaneFocusChanged`, change to resolve the session name from the leaf:

```swift
@objc private func handlePaneFocusChanged(_ notification: Notification) {
    guard let worktreePath = notification.userInfo?["worktreePath"] as? String,
          let leafId = notification.userInfo?["focusedLeafId"] as? String else { return }
    // Find the session name for this leaf (stable across app launches)
    if let tree = terminalCoordinator.surfaceManager.tree(forPath: worktreePath),
       let leaf = tree.allLeaves.first(where: { $0.id == leafId }) {
        config.focusedPaneIds[worktreePath] = leaf.sessionName
    }
    config.save()
}
```

- [ ] **Step 4: Update restoreSessionState to match by session name**

In `restoreSessionState`, replace the pane restoration block:

```swift
// Restore focused pane within the worktree
if let savedSessionName = config.focusedPaneIds[savedWorktreePath],
   let container = repoVC.activeSplitContainer,
   let tree = container.tree,
   let targetLeaf = tree.allLeaves.first(where: { $0.sessionName == savedSessionName }) {
    tree.focusedId = targetLeaf.id
    container.updateDimOverlays()
    if let surface = SurfaceRegistry.shared.surface(forId: targetLeaf.surfaceId),
       let termView = surface.view {
        repoVC.view.window?.makeFirstResponder(termView)
    }
}
```

- [ ] **Step 5: Build to verify compilation**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmux -configuration Debug build 2>&1 | tail -5`
Expected: BUILD SUCCEEDED

- [ ] **Step 6: Commit**

```bash
git add Sources/App/TabCoordinator.swift Tests/SessionRestoreConfigTests.swift
git commit -m "fix: use session name for pane focus persistence (stable across launches)"
```

---

### Task 7: Also save worktree from dashboard spotlight selection

When the user selects a card on the dashboard (spotlight mode), that's also a "last viewed worktree" for the project. Save it so if the user then opens that project tab, it restores to the right worktree.

**Files:**
- Modify: `Sources/App/TabCoordinator.swift:569-576` (dashboardDidSelectProject)

- [ ] **Step 1: Save worktree path in dashboardDidSelectProject**

In `dashboardDidSelectProject`, after `switchToTab(tabIndex + 1)`, add:

```swift
// Save the selected worktree for this project
if let worktreePath = tab.worktrees.first(where: { $0.branch == thread })?.path {
    config.activeWorktreePaths[tab.repoPath] = worktreePath
    config.save()
}
```

- [ ] **Step 2: Build to verify compilation**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmux -configuration Debug build 2>&1 | tail -5`
Expected: BUILD SUCCEEDED

- [ ] **Step 3: Commit**

```bash
git add Sources/App/TabCoordinator.swift
git commit -m "feat: save worktree selection from dashboard spotlight navigation"
```

---

### Task 8: End-to-end manual test

- [ ] **Step 1: Build and run the app**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmux -configuration Debug build 2>&1 | tail -5`

- [ ] **Step 2: Manual verification**

1. Launch pmux
2. Click into a project tab (not dashboard)
3. Select a non-default worktree in the sidebar
4. If split panes exist, click a non-first pane
5. Quit the app (Cmd+Q)
6. Relaunch — verify it restores to the same tab, worktree, and pane
7. Verify: if you delete `active_tab_repo_path` from config.json and relaunch, it shows dashboard (fallback works)

- [ ] **Step 3: Run all tests**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmuxTests -configuration Debug test 2>&1 | tail -20`
Expected: All tests PASS

- [ ] **Step 4: Final commit if any cleanup needed**
