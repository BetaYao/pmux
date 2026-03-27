# Notification Click Navigation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Clicking a macOS notification navigates to the corresponding Repo tab and selects the worktree that triggered it.

**Architecture:** NotificationManager posts an NSNotification (`navigateToWorktree`) with the worktree path. MainWindowController listens, finds/creates the Repo tab, switches to it, and selects the worktree via `RepoViewController.selectWorktree(byPath:)`.

**Tech Stack:** Swift 5.10, AppKit, UserNotifications, XCTest

**Spec:** `docs/superpowers/specs/2026-03-19-notification-navigation-design.md`

---

## File Map

| File | Action | Responsibility |
|------|--------|----------------|
| `Sources/Status/NotificationManager.swift` | Modify | Add `Notification.Name`, update `didReceive` to post navigation notification |
| `Sources/App/MainWindowController.swift` | Modify | Register observer, add `handleNavigateToWorktree` method |
| `Sources/UI/Repo/RepoViewController.swift` | Modify | Add `selectWorktree(byPath:)` public method |
| `Tests/NotificationNavigationTests.swift` | Create | Unit tests for navigation logic |

---

### Task 1: Add `Notification.Name.navigateToWorktree` constant

**Files:**
- Modify: `Sources/Status/NotificationManager.swift:1-5`

- [ ] **Step 1: Add the Notification.Name extension**

Add before the `NotificationManager` class definition:

```swift
extension Notification.Name {
    static let navigateToWorktree = Notification.Name("amux.navigateToWorktree")
}
```

- [ ] **Step 2: Build to verify it compiles**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -5`
Expected: BUILD SUCCEEDED

- [ ] **Step 3: Commit**

```bash
git add Sources/Status/NotificationManager.swift
git commit -m "feat: add Notification.Name.navigateToWorktree constant"
```

---

### Task 2: Update `didReceive` to post navigation notification

**Files:**
- Modify: `Sources/Status/NotificationManager.swift:88-93`

- [ ] **Step 1: Replace the existing `didReceive` implementation**

Replace:
```swift
func userNotificationCenter(_ center: UNUserNotificationCenter,
                            didReceive response: UNNotificationResponse,
                            withCompletionHandler completionHandler: @escaping () -> Void) {
    NSApp.activate(ignoringOtherApps: true)
    completionHandler()
}
```

With:
```swift
func userNotificationCenter(_ center: UNUserNotificationCenter,
                            didReceive response: UNNotificationResponse,
                            withCompletionHandler completionHandler: @escaping () -> Void) {
    let userInfo = response.notification.request.content.userInfo

    // didReceive may be called off main thread; UI ops must be on main
    DispatchQueue.main.async {
        NSApp.activate(ignoringOtherApps: true)
        NSApp.mainWindow?.deminiaturize(nil)

        if let path = userInfo["worktreePath"] as? String {
            NotificationCenter.default.post(
                name: .navigateToWorktree,
                object: nil,
                userInfo: ["worktreePath": path]
            )
        }
    }

    completionHandler()
}
```

- [ ] **Step 2: Build to verify it compiles**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -5`
Expected: BUILD SUCCEEDED

- [ ] **Step 3: Commit**

```bash
git add Sources/Status/NotificationManager.swift
git commit -m "feat: post navigateToWorktree notification on click"
```

---

### Task 3: Add `selectWorktree(byPath:)` to RepoViewController

**Files:**
- Modify: `Sources/UI/Repo/RepoViewController.swift:91-106`
- Create: `Tests/NotificationNavigationTests.swift`

- [ ] **Step 1: Write the test for selectWorktree(byPath:)**

Create `Tests/NotificationNavigationTests.swift`:

```swift
import XCTest
@testable import amux

class NotificationNavigationTests: XCTestCase {

    // MARK: - RepoViewController.selectWorktree(byPath:)

    // Note: RepoViewController has private state, so we test via the public
    // interface indirectly. For a focused unit test, we verify the lookup logic
    // that will be used (matching path in worktrees array).

    func testWorktreePathLookup() {
        let worktrees = [
            WorktreeInfo(path: "/repos/main", branch: "main", commitHash: "abc12345", isMainWorktree: true),
            WorktreeInfo(path: "/repos/feature-a", branch: "feature-a", commitHash: "def67890", isMainWorktree: false),
            WorktreeInfo(path: "/repos/feature-b", branch: "feature-b", commitHash: "ghi11111", isMainWorktree: false),
        ]

        // Should find existing path
        let index = worktrees.firstIndex(where: { $0.path == "/repos/feature-a" })
        XCTAssertEqual(index, 1)

        // Should return nil for missing path
        let missing = worktrees.firstIndex(where: { $0.path == "/repos/nonexistent" })
        XCTAssertNil(missing)
    }
}
```

- [ ] **Step 2: Run test to verify it passes**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/NotificationNavigationTests/testWorktreePathLookup 2>&1 | tail -10`
Expected: Test Suite 'NotificationNavigationTests' passed

- [ ] **Step 3: Add selectWorktree(byPath:) to RepoViewController**

In `Sources/UI/Repo/RepoViewController.swift`, add after `showTerminal(at:)` (after line 106):

```swift
func selectWorktree(byPath path: String) {
    guard let index = worktrees.firstIndex(where: { $0.path == path }) else { return }
    showTerminal(at: index)
}
```

- [ ] **Step 4: Build to verify it compiles**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -5`
Expected: BUILD SUCCEEDED

- [ ] **Step 5: Commit**

```bash
git add Sources/UI/Repo/RepoViewController.swift Tests/NotificationNavigationTests.swift
git commit -m "feat: add RepoViewController.selectWorktree(byPath:)"
```

---

### Task 4: Add navigation handler to MainWindowController

**Files:**
- Modify: `Sources/App/MainWindowController.swift`
- Modify: `Tests/NotificationNavigationTests.swift`

- [ ] **Step 1: Write test for tab lookup by worktree path**

Add to `Tests/NotificationNavigationTests.swift`:

```swift
func testTabLookupByWorktreePath() {
    let manager = WorkspaceManager()
    let worktrees1 = [
        WorktreeInfo(path: "/repos/alpha/main", branch: "main", commitHash: "aaa", isMainWorktree: true),
        WorktreeInfo(path: "/repos/alpha-worktrees/feat", branch: "feat", commitHash: "bbb", isMainWorktree: false),
    ]
    let worktrees2 = [
        WorktreeInfo(path: "/repos/beta/main", branch: "main", commitHash: "ccc", isMainWorktree: true),
    ]
    _ = manager.addTab(repoPath: "/repos/alpha", worktrees: worktrees1)
    _ = manager.addTab(repoPath: "/repos/beta", worktrees: worktrees2)

    // Find tab containing a worktree path
    let tabIndex = manager.tabs.firstIndex(where: { tab in
        tab.worktrees.contains(where: { $0.path == "/repos/alpha-worktrees/feat" })
    })
    XCTAssertEqual(tabIndex, 0)
    XCTAssertEqual(manager.tabs[tabIndex!].repoPath, "/repos/alpha")

    // Find tab for second repo
    let tabIndex2 = manager.tabs.firstIndex(where: { tab in
        tab.worktrees.contains(where: { $0.path == "/repos/beta/main" })
    })
    XCTAssertEqual(tabIndex2, 1)

    // Missing worktree returns nil
    let missing = manager.tabs.firstIndex(where: { tab in
        tab.worktrees.contains(where: { $0.path == "/nonexistent" })
    })
    XCTAssertNil(missing)
}
```

- [ ] **Step 2: Run test to verify it passes**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/NotificationNavigationTests/testTabLookupByWorktreePath 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 3: Register observer in MainWindowController init**

In `Sources/App/MainWindowController.swift`, find the end of `windowDidLoad()` or the `init` setup section. Add:

```swift
NotificationCenter.default.addObserver(
    self, selector: #selector(handleNavigateToWorktree(_:)),
    name: .navigateToWorktree, object: nil
)
```

- [ ] **Step 4: Add handleNavigateToWorktree method**

Add a new `// MARK: - Notification Navigation` section after the `StatusPublisherDelegate` section (after line 890):

```swift
// MARK: - Notification Navigation

@objc private func handleNavigateToWorktree(_ notification: Notification) {
    guard let worktreePath = notification.userInfo?["worktreePath"] as? String else { return }

    // 1. Find existing tab containing this worktree
    var repoPath: String?
    if let tabIndex = workspaceManager.tabs.firstIndex(where: { tab in
        tab.worktrees.contains(where: { $0.path == worktreePath })
    }) {
        repoPath = workspaceManager.tabs[tabIndex].repoPath
        switchToTab(tabIndex + 1)  // +1 because Dashboard is index 0
    } else {
        // Tab not open — find repo from config and open it
        guard let foundRepoPath = config.workspacePaths.first(where: { wsPath in
            WorktreeDiscovery.discover(repoPath: wsPath).contains(where: { $0.path == worktreePath })
        }) else { return }
        repoPath = foundRepoPath
        openRepoTab(repoPath: foundRepoPath)
    }

    // 2. Select the worktree in the repo view
    if let rp = repoPath, let repoVC = repoVCs[rp] {
        repoVC.selectWorktree(byPath: worktreePath)
    }
}
```

- [ ] **Step 5: Add observer cleanup in deinit**

In the `MainWindowController` class body, add:

```swift
deinit {
    NotificationCenter.default.removeObserver(self, name: .navigateToWorktree, object: nil)
}
```

- [ ] **Step 6: Build to verify it compiles**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -5`
Expected: BUILD SUCCEEDED

- [ ] **Step 7: Run all tests**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test 2>&1 | tail -10`
Expected: All tests passed

- [ ] **Step 8: Commit**

```bash
git add Sources/App/MainWindowController.swift Tests/NotificationNavigationTests.swift
git commit -m "feat: navigate to worktree on notification click"
```

---

### Task 5: Verify end-to-end and final commit

- [ ] **Step 1: Run full build**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -5`
Expected: BUILD SUCCEEDED

- [ ] **Step 2: Run all tests**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test 2>&1 | tail -10`
Expected: All tests passed

- [ ] **Step 3: Verify no unintended changes**

Run: `git diff --stat`
Expected: Only the 4 files listed in the file map are modified/created.
