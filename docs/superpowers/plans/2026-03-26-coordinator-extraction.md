# Phase 1: Coordinator Extraction Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Decompose MainWindowController (1,882 lines, 15 protocols) into 4 focused Coordinator objects, reducing it to ~700 lines while preserving all existing behavior.

**Architecture:** Extract responsibilities into UpdateCoordinator, PanelCoordinator, TerminalCoordinator, and TabCoordinator — in order of isolation. Each Coordinator owns its domain state, defines a delegate protocol, and is strongly held by MainWindowController. Cross-coordinator communication routes through MainWindowController's delegate implementations.

**Tech Stack:** Swift 5.10, AppKit, XCTest

**Spec:** `docs/superpowers/specs/2026-03-25-architecture-optimization-design.md`

---

## File Structure

| Action | File | Responsibility |
|--------|------|---------------|
| Create | `Sources/App/UpdateCoordinator.swift` | Auto-update checking, download, install, banner management |
| Create | `Sources/App/PanelCoordinator.swift` | Notification/AI panel popover lifecycle |
| Create | `Sources/App/TerminalCoordinator.swift` | Surface management, split pane ops, worktree deletion, webhook server |
| Create | `Sources/App/TabCoordinator.swift` | Tab switching, workspace loading, repo navigation, branch refresh |
| Modify | `Sources/App/MainWindowController.swift` | Remove extracted code, add coordinator properties + delegate impls + forwarding methods |
| Create | `Tests/UpdateCoordinatorTests.swift` | Tests for update state transitions |
| Create | `Tests/PanelCoordinatorTests.swift` | Tests for panel toggle logic |
| Create | `Tests/TerminalCoordinatorTests.swift` | Tests for split coordination, layout persistence |
| Create | `Tests/TabCoordinatorTests.swift` | Tests for tab switching, workspace ops |

---

## Task 1: Extract UpdateCoordinator

The most isolated coordinator — no cross-coordinator dependencies. Owns update checking, download, install, and banner display.

**Files:**
- Create: `Sources/App/UpdateCoordinator.swift`
- Modify: `Sources/App/MainWindowController.swift:47-50,1762-1846`
- Create: `Tests/UpdateCoordinatorTests.swift`

- [ ] **Step 1: Write UpdateCoordinator tests**

Create `Tests/UpdateCoordinatorTests.swift`:

```swift
import XCTest
@testable import pmux

// Mock delegates and dependencies for testing UpdateCoordinator in isolation
private class MockUpdateCoordinatorDelegate: UpdateCoordinatorDelegate {
    var showBannerCalled = false
    var lastBanner: UpdateBanner?

    func updateCoordinator(_ coordinator: UpdateCoordinator, showBanner banner: UpdateBanner) {
        showBannerCalled = true
        lastBanner = banner
    }
}

final class UpdateCoordinatorTests: XCTestCase {

    func testInitCreatesComponents() {
        let coordinator = UpdateCoordinator()
        XCTAssertNotNil(coordinator.banner)
    }

    func testSetupAutoUpdateWhenDisabled() {
        var config = Config.makeDefault()
        config.autoUpdate.enabled = false
        let coordinator = UpdateCoordinator()
        // Should not crash when autoUpdate is disabled
        coordinator.setup(config: config)
    }

    func testUpdateBannerSkipSavesVersion() {
        let coordinator = UpdateCoordinator()
        var config = Config.makeDefault()
        coordinator.setup(config: config)
        coordinator.config = config

        // Simulate skip
        coordinator.handleSkip(version: "2.0.0")
        XCTAssertEqual(coordinator.config.autoUpdate.skippedVersion, "2.0.0")
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmuxTests -configuration Debug test -only-testing:pmuxTests/UpdateCoordinatorTests 2>&1 | tail -5`
Expected: FAIL — `UpdateCoordinator` type not found

- [ ] **Step 3: Create UpdateCoordinator with delegate protocol**

Create `Sources/App/UpdateCoordinator.swift`:

```swift
import AppKit

protocol UpdateCoordinatorDelegate: AnyObject {
    func updateCoordinator(_ coordinator: UpdateCoordinator, showBanner banner: UpdateBanner)
}

class UpdateCoordinator {
    weak var delegate: UpdateCoordinatorDelegate?
    var config: Config

    let updateChecker = UpdateChecker()
    let updateManager = UpdateManager()
    let banner = UpdateBanner()
    var pendingRelease: ReleaseInfo?

    init(config: Config = Config.makeDefault()) {
        self.config = config
    }

    func setup(config: Config) {
        self.config = config
        guard config.autoUpdate.enabled else { return }
        updateChecker.delegate = self
        updateChecker.skippedVersion = config.autoUpdate.skippedVersion
        updateManager.delegate = self
        updateChecker.startPolling(intervalHours: config.autoUpdate.checkIntervalHours)
    }

    func checkForUpdates() {
        Task {
            do {
                if let release = try await updateChecker.checkNow() {
                    pendingRelease = release
                    banner.showNewVersion(release.version)
                } else {
                    let alert = NSAlert()
                    alert.messageText = "Already up to date"
                    alert.informativeText = "Current version v\(updateChecker.currentVersion) is the latest."
                    alert.alertStyle = .informational
                    alert.runModal()
                }
            } catch {
                NSLog("Update check failed: \(error)")
            }
        }
    }

    func handleSkip(version: String) {
        config.autoUpdate.skippedVersion = version
        config.save()
        updateChecker.skippedVersion = version
        banner.dismiss()
        pendingRelease = nil
    }
}

// MARK: - UpdateCheckerDelegate

extension UpdateCoordinator: UpdateCheckerDelegate {
    func updateChecker(_ checker: UpdateChecker, didFindRelease release: ReleaseInfo) {
        pendingRelease = release
        banner.showNewVersion(release.version)
    }
}

// MARK: - UpdateManagerDelegate

extension UpdateCoordinator: UpdateManagerDelegate {
    func updateManager(_ manager: UpdateManager, didChangeState state: UpdateManager.State) {
        banner.update(state: state)
    }
}

// MARK: - UpdateBannerDelegate

extension UpdateCoordinator: UpdateBannerDelegate {
    func updateBannerDidClickInstall(_ banner: UpdateBanner) {
        guard let release = pendingRelease else { return }
        updateManager.download(release: release)
    }

    func updateBannerDidClickSkip(_ banner: UpdateBanner) {
        handleSkip(version: banner.version)
    }

    func updateBannerDidClickRestart(_ banner: UpdateBanner) {
        updateManager.installAndRestart()
    }

    func updateBannerDidClickRetry(_ banner: UpdateBanner) {
        guard let release = pendingRelease else { return }
        updateManager.download(release: release)
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmuxTests -configuration Debug test -only-testing:pmuxTests/UpdateCoordinatorTests 2>&1 | tail -20`
Expected: PASS (3 tests)

- [ ] **Step 5: Wire UpdateCoordinator into MainWindowController**

In `Sources/App/MainWindowController.swift`:

1. Replace properties (lines 47-50):
   ```swift
   // Remove:
   private let updateChecker = UpdateChecker()
   private let updateManager = UpdateManager()
   private let updateBanner = UpdateBanner()
   private var pendingRelease: ReleaseInfo?

   // Add:
   private lazy var updateCoordinator: UpdateCoordinator = {
       let uc = UpdateCoordinator(config: config)
       uc.delegate = self
       uc.banner.delegate = uc
       return uc
   }()
   ```

2. In `init()`, replace `setupAutoUpdate()` call with:
   ```swift
   updateCoordinator.setup(config: config)
   ```

3. Replace `@objc func checkForUpdates()` body with:
   ```swift
   @objc func checkForUpdates() {
       updateCoordinator.checkForUpdates()
   }
   ```

4. Remove entire extensions (lines 1762-1846):
   - `extension MainWindowController { setupAutoUpdate(), checkForUpdates() }`
   - `extension MainWindowController: UpdateCheckerDelegate`
   - `extension MainWindowController: UpdateManagerDelegate`
   - `extension MainWindowController: UpdateBannerDelegate`

5. Add UpdateCoordinatorDelegate conformance:
   ```swift
   extension MainWindowController: UpdateCoordinatorDelegate {
       func updateCoordinator(_ coordinator: UpdateCoordinator, showBanner banner: UpdateBanner) {
           // Banner display handled by coordinator's banner property
       }
   }
   ```

6. Update references to `updateBanner` → `updateCoordinator.banner` in `setupLayout()` or wherever the banner view is added to the window.

7. Update `windowWillClose` and `cleanupBeforeTermination` — these still reference `webhookServer` and `branchRefreshTimer` which haven't moved yet, leave those for now.

- [ ] **Step 6: Regenerate Xcode project**

Run: `cd /Users/matt.chow/workspace/pmux-swift && xcodegen generate`
Expected: `Generated project pmux.xcodeproj`

- [ ] **Step 7: Build and run full test suite**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmuxTests -configuration Debug test 2>&1 | tail -20`
Expected: All existing tests pass + 3 new UpdateCoordinator tests pass

- [ ] **Step 8: Commit**

```bash
git add Sources/App/UpdateCoordinator.swift Tests/UpdateCoordinatorTests.swift Sources/App/MainWindowController.swift
git commit -m "refactor: extract UpdateCoordinator from MainWindowController"
```

---

## Task 2: Extract PanelCoordinator

Minimal dependencies — only needs a reference to TitleBarView for popover anchoring.

**Files:**
- Create: `Sources/App/PanelCoordinator.swift`
- Modify: `Sources/App/MainWindowController.swift:21-24,454-471,780-823,1537-1560,1810-1856`
- Create: `Tests/PanelCoordinatorTests.swift`

- [ ] **Step 1: Write PanelCoordinator tests**

Create `Tests/PanelCoordinatorTests.swift`:

```swift
import XCTest
@testable import pmux

private class MockPanelCoordinatorDelegate: PanelCoordinatorDelegate {
    var navigateCalled = false
    var lastWorktreePath: String?

    func panelCoordinator(_ coordinator: PanelCoordinator, navigateToWorktreePath path: String) {
        navigateCalled = true
        lastWorktreePath = path
    }
}

final class PanelCoordinatorTests: XCTestCase {

    func testCloseBothPanelsSetsOpenFalse() {
        let coordinator = PanelCoordinator()
        coordinator.closeBothPanels()
        // Should not crash — panels are not shown
        XCTAssertFalse(coordinator.notificationPopover.isShown)
        XCTAssertFalse(coordinator.aiPopover.isShown)
    }

    func testNotificationPanelDelegateNavigates() {
        let coordinator = PanelCoordinator()
        let mockDelegate = MockPanelCoordinatorDelegate()
        coordinator.delegate = mockDelegate

        // Simulate notification history selection
        coordinator.handleNotificationHistorySelect(worktreePath: "/test/path")
        XCTAssertTrue(mockDelegate.navigateCalled)
        XCTAssertEqual(mockDelegate.lastWorktreePath, "/test/path")
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmuxTests -configuration Debug test -only-testing:pmuxTests/PanelCoordinatorTests 2>&1 | tail -5`
Expected: FAIL — `PanelCoordinator` type not found

- [ ] **Step 3: Create PanelCoordinator with delegate protocol**

Create `Sources/App/PanelCoordinator.swift`:

```swift
import AppKit

protocol PanelCoordinatorDelegate: AnyObject {
    func panelCoordinator(_ coordinator: PanelCoordinator, navigateToWorktreePath path: String)
}

class PanelCoordinator: NSObject {
    weak var delegate: PanelCoordinatorDelegate?
    weak var titleBar: TitleBarView?

    let notificationPanel = NotificationPanelView()
    let aiPanel = AIPanelView()
    let notificationPopover = NSPopover()
    let aiPopover = NSPopover()

    func setupPopovers() {
        notificationPopover.behavior = .transient
        notificationPopover.contentViewController = ViewHostController(hostedView: notificationPanel)
        notificationPopover.delegate = self

        aiPopover.behavior = .transient
        aiPopover.contentViewController = ViewHostController(hostedView: aiPanel)
        aiPopover.delegate = self

        notificationPanel.delegate = self
        aiPanel.delegate = self
    }

    func closeBothPanels() {
        notificationPopover.performClose(nil)
        aiPopover.performClose(nil)
        notificationPanel.setOpen(false, animated: false)
        aiPanel.setOpen(false, animated: false)
    }

    func toggleNotificationPanel() {
        if notificationPopover.isShown {
            notificationPopover.performClose(nil)
            notificationPanel.setOpen(false, animated: false)
            return
        }

        aiPopover.performClose(nil)
        aiPanel.setOpen(false, animated: false)

        notificationPanel.updateNotifications(NotificationHistory.shared.entries.map {
            (
                title: "\($0.branch)  \($0.status.rawValue)",
                meta: $0.message,
                worktreePath: $0.worktreePath
            )
        })
        notificationPanel.setOpen(true, animated: false)

        guard let titleBar else { return }
        let anchor = titleBar.notificationsAnchorView()
        notificationPopover.show(relativeTo: anchor.bounds, of: anchor, preferredEdge: .maxY)
    }

    func toggleAIPanel() {
        if aiPopover.isShown {
            aiPopover.performClose(nil)
            aiPanel.setOpen(false, animated: false)
            return
        }

        notificationPopover.performClose(nil)
        notificationPanel.setOpen(false, animated: false)

        aiPanel.setOpen(true, animated: false)
        guard let titleBar else { return }
        let anchor = titleBar.aiAnchorView()
        aiPopover.show(relativeTo: anchor.bounds, of: anchor, preferredEdge: .maxY)
    }

    func handleNotificationHistorySelect(worktreePath: String) {
        NotificationCenter.default.post(
            name: .navigateToWorktree,
            object: nil,
            userInfo: ["worktreePath": worktreePath]
        )
    }
}

// MARK: - NotificationPanelDelegate

extension PanelCoordinator: NotificationPanelDelegate {
    func notificationPanelDidRequestClose() {
        notificationPopover.performClose(nil)
        notificationPanel.setOpen(false, animated: false)
    }

    func notificationPanelDidSelectItem(worktreePath: String) {
        closeBothPanels()
        NotificationCenter.default.post(
            name: .navigateToWorktree,
            object: nil,
            userInfo: ["worktreePath": worktreePath]
        )
    }
}

// MARK: - AIPanelDelegate

extension PanelCoordinator: AIPanelDelegate {
    func aiPanelDidRequestClose() {
        aiPopover.performClose(nil)
        aiPanel.setOpen(false, animated: false)
    }
}

// MARK: - NotificationHistoryDelegate

extension PanelCoordinator: NotificationHistoryDelegate {
    func notificationHistory(_ vc: NotificationHistoryViewController, didSelectWorktreePath path: String) {
        handleNotificationHistorySelect(worktreePath: path)
    }
}

// MARK: - NSPopoverDelegate

extension PanelCoordinator: NSPopoverDelegate {
    func popoverDidClose(_ notification: Notification) {
        guard let popover = notification.object as? NSPopover else { return }
        if popover === notificationPopover {
            notificationPanel.setOpen(false, animated: false)
        } else if popover === aiPopover {
            aiPanel.setOpen(false, animated: false)
        }
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmuxTests -configuration Debug test -only-testing:pmuxTests/PanelCoordinatorTests 2>&1 | tail -20`
Expected: PASS (2 tests)

- [ ] **Step 5: Wire PanelCoordinator into MainWindowController**

In `Sources/App/MainWindowController.swift`:

1. Remove panel properties (lines 21-24):
   ```swift
   // Remove:
   private let notificationPanel = NotificationPanelView()
   private let aiPanel = AIPanelView()
   private let notificationPopover = NSPopover()
   private let aiPopover = NSPopover()
   ```

2. Add:
   ```swift
   private lazy var panelCoordinator: PanelCoordinator = {
       let pc = PanelCoordinator()
       pc.delegate = self
       pc.titleBar = titleBar
       return pc
   }()
   ```

3. In `init()` or `setupLayout()`, replace `setupPanelPopovers()` with:
   ```swift
   panelCoordinator.setupPopovers()
   ```

4. Replace all `toggleNotificationPanel()` calls with `panelCoordinator.toggleNotificationPanel()`
5. Replace all `toggleAIPanel()` calls with `panelCoordinator.toggleAIPanel()`
6. Replace all `closeBothPanels()` calls with `panelCoordinator.closeBothPanels()`

7. Remove these extensions entirely:
   - `extension MainWindowController: NotificationPanelDelegate`
   - `extension MainWindowController: AIPanelDelegate`
   - `extension MainWindowController: NotificationHistoryDelegate`
   - `extension MainWindowController: NSPopoverDelegate`
   - The `setupPanelPopovers()` method
   - The `toggleNotificationPanel()`, `toggleAIPanel()`, `closeBothPanels()` methods

8. Add PanelCoordinatorDelegate:
   ```swift
   extension MainWindowController: PanelCoordinatorDelegate {
       func panelCoordinator(_ coordinator: PanelCoordinator, navigateToWorktreePath path: String) {
           // Navigation handled by NotificationCenter .navigateToWorktree
       }
   }
   ```

- [ ] **Step 6: Regenerate Xcode project and run full test suite**

Run: `cd /Users/matt.chow/workspace/pmux-swift && xcodegen generate && xcodebuild -project pmux.xcodeproj -scheme pmuxTests -configuration Debug test 2>&1 | tail -20`
Expected: All tests pass

- [ ] **Step 7: Commit**

```bash
git add Sources/App/PanelCoordinator.swift Tests/PanelCoordinatorTests.swift Sources/App/MainWindowController.swift
git commit -m "refactor: extract PanelCoordinator from MainWindowController"
```

---

## Task 3: Extract TerminalCoordinator

Owns surface management, split pane operations, worktree deletion, and webhook server. Receives a `currentRepoVC` closure to access the active split container.

**Files:**
- Create: `Sources/App/TerminalCoordinator.swift`
- Modify: `Sources/App/MainWindowController.swift:32-34,870-878,989-1082,1160-1297`
- Create: `Tests/TerminalCoordinatorTests.swift`

- [ ] **Step 1: Write TerminalCoordinator tests**

Create `Tests/TerminalCoordinatorTests.swift`:

```swift
import XCTest
@testable import pmux

private class MockTerminalCoordinatorDelegate: TerminalCoordinatorDelegate {
    var surfacesUpdated = false
    var deletedWorktree: WorktreeInfo?

    func terminalCoordinatorDidUpdateSurfaces(_ coordinator: TerminalCoordinator) {
        surfacesUpdated = true
    }

    func terminalCoordinator(_ coordinator: TerminalCoordinator, didDeleteWorktree info: WorktreeInfo) {
        deletedWorktree = info
    }
}

final class TerminalCoordinatorTests: XCTestCase {

    func testSurfaceManagerAccess() {
        let config = Config.makeDefault()
        let coordinator = TerminalCoordinator(config: config, currentRepoVC: { nil })
        XCTAssertNotNil(coordinator.surfaceManager)
    }

    func testResolveTreeCreatesNewTree() {
        var config = Config.makeDefault()
        config.backend = "local"
        let coordinator = TerminalCoordinator(config: config, currentRepoVC: { nil })
        let info = WorktreeInfo(path: "/tmp/test-wt", branch: "main", isMainWorktree: true)
        let tree = coordinator.resolveTree(for: info)
        XCTAssertEqual(tree.worktreePath, "/tmp/test-wt")
    }

    func testSaveSplitLayoutPersistsToConfig() {
        var config = Config.makeDefault()
        let coordinator = TerminalCoordinator(config: config, currentRepoVC: { nil })
        let tree = SplitTree(worktreePath: "/tmp/test")
        coordinator.saveSplitLayout(tree)
        XCTAssertNotNil(coordinator.config.splitLayouts["/tmp/test"])
    }

    func testSplitFocusedPaneWithNilRepoVCIsNoop() {
        let config = Config.makeDefault()
        let coordinator = TerminalCoordinator(config: config, currentRepoVC: { nil })
        // Should not crash when no repoVC
        coordinator.splitFocusedPane(axis: .horizontal)
    }

    func testCloseFocusedPaneWithNilRepoVCIsNoop() {
        let config = Config.makeDefault()
        let coordinator = TerminalCoordinator(config: config, currentRepoVC: { nil })
        coordinator.closeFocusedPane()
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmuxTests -configuration Debug test -only-testing:pmuxTests/TerminalCoordinatorTests 2>&1 | tail -5`
Expected: FAIL — `TerminalCoordinator` type not found

- [ ] **Step 3: Create TerminalCoordinator with delegate protocol**

Create `Sources/App/TerminalCoordinator.swift`. Extract the following from MainWindowController:

- Properties: `surfaceManager`, `webhookServer`
- Methods: `resolveTree(for:)`, `saveSplitLayout(_:)`, `splitFocusedPane(axis:)`, `closeFocusedPane()`, `moveFocus(_:positive:)`, `resizeSplit(_:delta:)`, `resetSplitRatio()`, `confirmAndDeleteWorktree(_:)`, `performDeleteWorktree(...)`, webhook setup from `loadWorkspaces`

```swift
import AppKit

protocol TerminalCoordinatorDelegate: AnyObject {
    func terminalCoordinatorDidUpdateSurfaces(_ coordinator: TerminalCoordinator)
    func terminalCoordinator(_ coordinator: TerminalCoordinator, didDeleteWorktree info: WorktreeInfo)
}

class TerminalCoordinator {
    weak var delegate: TerminalCoordinatorDelegate?
    var config: Config
    let surfaceManager = TerminalSurfaceManager()
    var webhookServer: WebhookServer?

    /// Closure to access the current RepoViewController for split pane operations.
    /// Provided by MainWindowController, avoids direct TabCoordinator dependency.
    var currentRepoVC: () -> RepoViewController?

    private static let iso8601: ISO8601DateFormatter = {
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withInternetDateTime]
        return f
    }()

    init(config: Config, currentRepoVC: @escaping () -> RepoViewController?) {
        self.config = config
        self.currentRepoVC = currentRepoVC
    }

    // MARK: - Tree Resolution

    func resolveTree(for info: WorktreeInfo) -> SplitTree {
        let backend = config.backend
        if backend != "local",
           let savedLayout = config.splitLayouts[info.path],
           let restored = SplitTree.restore(from: savedLayout, worktreePath: info.path, backend: backend) {
            surfaceManager.registerTree(restored, forPath: info.path)
            return restored
        }
        return surfaceManager.tree(for: info, backend: backend)
    }

    func saveSplitLayout(_ tree: SplitTree) {
        config.splitLayouts[tree.worktreePath] = tree.toCodable()
        config.save()
    }

    // MARK: - Split Pane Operations

    func splitFocusedPane(axis: SplitAxis) {
        guard let repoVC = currentRepoVC(),
              let container = repoVC.activeSplitContainer,
              let tree = container.tree else { return }

        let sessionName = tree.nextSessionName()
        let surface = TerminalSurface()
        surface.sessionName = sessionName
        surface.backend = config.backend
        SurfaceRegistry.shared.register(surface)

        let leafId = UUID().uuidString
        tree.splitFocusedLeaf(axis: axis, newLeafId: leafId, newSurfaceId: surface.id, newSessionName: sessionName)

        _ = surface.create(in: container, workingDirectory: tree.worktreePath, sessionName: sessionName)

        container.surfaceViews[surface.id] = surface.view
        container.layoutTree()

        let capturedLeafId = leafId
        DispatchQueue.main.async { [weak container] in
            guard let container,
                  let tree = container.tree,
                  let newLeaf = tree.allLeaves.first(where: { $0.id == capturedLeafId }),
                  let newSurface = SurfaceRegistry.shared.surface(forId: newLeaf.surfaceId),
                  let termView = newSurface.view else { return }
            container.window?.makeFirstResponder(termView)
        }

        delegate?.terminalCoordinatorDidUpdateSurfaces(self)
        saveSplitLayout(tree)
    }

    func closeFocusedPane() {
        guard let repoVC = currentRepoVC(),
              let container = repoVC.activeSplitContainer,
              let tree = container.tree else { return }

        guard let closed = tree.closeFocusedLeaf() else { return }

        SessionManager.killSession(closed.sessionName, backend: config.backend)

        if let surface = SurfaceRegistry.shared.surface(forId: closed.surfaceId) {
            surface.view?.removeFromSuperview()
            surface.destroy()
        }
        SurfaceRegistry.shared.unregister(closed.surfaceId)
        container.surfaceViews.removeValue(forKey: closed.surfaceId)
        container.layoutTree()

        if let focusedLeaf = tree.allLeaves.first(where: { $0.id == tree.focusedId }),
           let focusSurface = SurfaceRegistry.shared.surface(forId: focusedLeaf.surfaceId),
           let terminalView = focusSurface.view {
            container.window?.makeFirstResponder(terminalView)
        }

        if let focusedLeaf = tree.allLeaves.first(where: { $0.id == tree.focusedId }),
           let focusSurface = SurfaceRegistry.shared.surface(forId: focusedLeaf.surfaceId) {
            DispatchQueue.main.async {
                focusSurface.syncSize()
                DispatchQueue.main.async {
                    focusSurface.refreshSessionLayout()
                }
            }
        }

        delegate?.terminalCoordinatorDidUpdateSurfaces(self)
        saveSplitLayout(tree)
    }

    func moveFocus(_ axis: SplitAxis, positive: Bool) {
        guard let repoVC = currentRepoVC(),
              let container = repoVC.activeSplitContainer else { return }
        if let newFocusId = container.focusLeaf(direction: axis, positive: positive) {
            if let tree = container.tree,
               let leaf = tree.root.findLeaf(id: newFocusId),
               let surface = SurfaceRegistry.shared.surface(forId: leaf.surfaceId),
               let view = surface.view {
                container.window?.makeFirstResponder(view)
            }
        }
    }

    func resizeSplit(_ axis: SplitAxis, delta: CGFloat) {
        guard let repoVC = currentRepoVC(),
              let container = repoVC.activeSplitContainer,
              let tree = container.tree else { return }
        guard let splitId = tree.nearestAncestorSplit(axis: axis) else { return }
        func findRatio(in node: SplitNode) -> CGFloat? {
            if node.id == splitId, case .split(_, _, let ratio, _, _) = node { return ratio }
            if case .split(_, _, _, let first, let second) = node {
                return findRatio(in: first) ?? findRatio(in: second)
            }
            return nil
        }
        if let currentRatio = findRatio(in: tree.root) {
            tree.updateRatio(splitId: splitId, newRatio: currentRatio + delta)
            container.layoutTree()
            saveSplitLayout(tree)
        }
    }

    func resetSplitRatio() {
        guard let repoVC = currentRepoVC(),
              let container = repoVC.activeSplitContainer,
              let tree = container.tree else { return }
        for axis in [SplitAxis.horizontal, .vertical] {
            if let splitId = tree.nearestAncestorSplit(axis: axis) {
                tree.updateRatio(splitId: splitId, newRatio: 0.5)
            }
        }
        container.layoutTree()
        saveSplitLayout(tree)
    }

    // MARK: - Worktree Deletion

    func confirmAndDeleteWorktree(_ info: WorktreeInfo, window: NSWindow?) {
        guard !info.isMainWorktree else { return }

        let hasChanges = WorktreeDeleter.hasUncommittedChanges(worktreePath: info.path)
        let repoPath = WorktreeDiscovery.findRepoRoot(from: info.path) ?? info.path

        let alert = NSAlert()
        alert.alertStyle = hasChanges ? .critical : .warning
        alert.messageText = "Delete worktree \"\(info.branch)\"?"
        if hasChanges {
            alert.informativeText = "This worktree has uncommitted changes that will be lost."
        } else {
            alert.informativeText = "The worktree directory will be removed."
        }
        alert.addButton(withTitle: "Delete")
        alert.addButton(withTitle: "Delete + Branch")
        alert.addButton(withTitle: "Cancel")

        alert.buttons[0].hasDestructiveAction = true
        alert.buttons[1].hasDestructiveAction = true

        guard let window else { return }
        alert.beginSheetModal(for: window) { [weak self] response in
            guard let self else { return }
            switch response {
            case .alertFirstButtonReturn:
                self.performDeleteWorktree(info, repoPath: repoPath, deleteBranch: false, force: hasChanges)
            case .alertSecondButtonReturn:
                self.performDeleteWorktree(info, repoPath: repoPath, deleteBranch: true, force: hasChanges)
            default:
                break
            }
        }
    }

    private func performDeleteWorktree(_ info: WorktreeInfo, repoPath: String, deleteBranch: Bool, force: Bool) {
        surfaceManager.removeTree(forPath: info.path)

        DispatchQueue.global().async { [weak self] in
            do {
                try WorktreeDeleter.deleteWorktree(
                    worktreePath: info.path,
                    repoPath: repoPath,
                    branchName: info.branch,
                    deleteBranch: deleteBranch,
                    force: force
                )
                DispatchQueue.main.async {
                    self?.delegate?.terminalCoordinator(self!, didDeleteWorktree: info)
                }
            } catch {
                DispatchQueue.main.async { [weak self] in
                    let errAlert = NSAlert()
                    errAlert.alertStyle = .critical
                    errAlert.messageText = "Failed to delete worktree"
                    errAlert.informativeText = error.localizedDescription
                    // Show error as app-modal since we don't own the window
                    errAlert.runModal()
                }
            }
        }
    }

    // MARK: - Cleanup

    func cleanup() {
        webhookServer?.stop()
        webhookServer = nil
        surfaceManager.removeAll()
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmuxTests -configuration Debug test -only-testing:pmuxTests/TerminalCoordinatorTests 2>&1 | tail -20`
Expected: PASS (5 tests)

- [ ] **Step 5: Wire TerminalCoordinator into MainWindowController**

In `Sources/App/MainWindowController.swift`:

1. Remove properties: `surfaceManager`, `allWorktrees` (allWorktrees stays for now — moves in Task 4), `webhookServer`

2. Add:
   ```swift
   private lazy var terminalCoordinator: TerminalCoordinator = {
       let tc = TerminalCoordinator(config: config, currentRepoVC: { [weak self] in
           self?.currentRepoVC
       })
       tc.delegate = self
       return tc
   }()
   ```

3. Add forwarding methods for PmuxWindow:
   ```swift
   func splitFocusedPane(axis: SplitAxis) {
       terminalCoordinator.splitFocusedPane(axis: axis)
   }
   func closeFocusedPane() {
       terminalCoordinator.closeFocusedPane()
   }
   func moveFocus(_ axis: SplitAxis, positive: Bool) {
       terminalCoordinator.moveFocus(axis, positive: positive)
   }
   func resizeSplit(_ axis: SplitAxis, delta: CGFloat) {
       terminalCoordinator.resizeSplit(axis, delta: delta)
   }
   func resetSplitRatio() {
       terminalCoordinator.resetSplitRatio()
   }
   ```

4. Replace all `surfaceManager` references with `terminalCoordinator.surfaceManager`

5. Replace `confirmAndDeleteWorktree(info)` with `terminalCoordinator.confirmAndDeleteWorktree(info, window: window)`

6. Replace `resolveTree(for:)` with `terminalCoordinator.resolveTree(for:)`

7. Remove original methods: `splitFocusedPane`, `closeFocusedPane`, `moveFocus`, `resizeSplit`, `resetSplitRatio`, `saveSplitLayout`, `resolveTree`, `confirmAndDeleteWorktree`, `performDeleteWorktree`

8. Update cleanup methods:
   ```swift
   func windowWillClose(_ notification: Notification) {
       statusPublisher.stop()
       branchRefreshTimer?.invalidate()
       branchRefreshTimer = nil
       terminalCoordinator.cleanup()
   }
   ```

9. Add TerminalCoordinatorDelegate:
   ```swift
   extension MainWindowController: TerminalCoordinatorDelegate {
       func terminalCoordinatorDidUpdateSurfaces(_ coordinator: TerminalCoordinator) {
           statusPublisher.updateSurfaces(coordinator.surfaceManager.all)
       }
       func terminalCoordinator(_ coordinator: TerminalCoordinator, didDeleteWorktree info: WorktreeInfo) {
           worktreeDidDelete(info)
       }
   }
   ```

- [ ] **Step 6: Regenerate Xcode project and run full test suite**

Run: `cd /Users/matt.chow/workspace/pmux-swift && xcodegen generate && xcodebuild -project pmux.xcodeproj -scheme pmuxTests -configuration Debug test 2>&1 | tail -20`
Expected: All tests pass

- [ ] **Step 7: Commit**

```bash
git add Sources/App/TerminalCoordinator.swift Tests/TerminalCoordinatorTests.swift Sources/App/MainWindowController.swift
git commit -m "refactor: extract TerminalCoordinator from MainWindowController"
```

---

## Task 4: Extract TabCoordinator

The largest extraction. Owns tab switching, workspace loading, repo management, branch refresh, navigation, and worktree lifecycle cleanup.

**Files:**
- Create: `Sources/App/TabCoordinator.swift`
- Modify: `Sources/App/MainWindowController.swift:27-44,573-608,635-777,880-988,1053-1158,1411-1517,1521-1533,1564-1643,1648-1760`
- Create: `Tests/TabCoordinatorTests.swift`

- [ ] **Step 1: Write TabCoordinator tests**

Create `Tests/TabCoordinatorTests.swift`:

```swift
import XCTest
@testable import pmux

private class MockTabCoordinatorDelegate: TabCoordinatorDelegate {
    var embeddedVC: NSViewController?
    var switchTabCalled = false
    var updateTitleBarCalled = false
    var showNewBranchCalled = false
    var showDiffWorktreePath: String?

    func tabCoordinator(_ coordinator: TabCoordinator, embedViewController vc: NSViewController) {
        embeddedVC = vc
    }
    func tabCoordinatorDidSwitchTab(_ coordinator: TabCoordinator) {
        switchTabCalled = true
    }
    func tabCoordinatorRequestUpdateTitleBar(_ coordinator: TabCoordinator) {
        updateTitleBarCalled = true
    }
    func tabCoordinatorRequestShowNewBranchDialog(_ coordinator: TabCoordinator) {
        showNewBranchCalled = true
    }
    func tabCoordinatorRequestShowDiff(_ coordinator: TabCoordinator, worktreePath: String) {
        showDiffWorktreePath = worktreePath
    }
}

final class TabCoordinatorTests: XCTestCase {

    func testInitialActiveTabIsZero() {
        let coordinator = TabCoordinator(config: Config.makeDefault())
        XCTAssertEqual(coordinator.activeTabIndex, 0)
    }

    func testSwitchToSameTabIsNoop() {
        let coordinator = TabCoordinator(config: Config.makeDefault())
        let mockDelegate = MockTabCoordinatorDelegate()
        coordinator.delegate = mockDelegate

        coordinator.switchToTab(0)  // already at 0
        XCTAssertFalse(mockDelegate.switchTabCalled)
    }

    func testBuildAgentDisplayInfosEmptyByDefault() {
        let coordinator = TabCoordinator(config: Config.makeDefault())
        let infos = coordinator.buildAgentDisplayInfos()
        XCTAssertTrue(infos.isEmpty)
    }

    func testWorktreeDidDeleteRemovesFromList() {
        let coordinator = TabCoordinator(config: Config.makeDefault())
        let info = WorktreeInfo(path: "/tmp/test-wt", branch: "feature", isMainWorktree: false)
        let tree = SplitTree(worktreePath: info.path)
        coordinator.allWorktrees.append((info: info, tree: tree))

        coordinator.worktreeDidDelete(info)
        XCTAssertTrue(coordinator.allWorktrees.isEmpty)
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmuxTests -configuration Debug test -only-testing:pmuxTests/TabCoordinatorTests 2>&1 | tail -5`
Expected: FAIL — `TabCoordinator` type not found

- [ ] **Step 3: Create TabCoordinator with delegate protocol**

Create `Sources/App/TabCoordinator.swift`. This is the largest coordinator. Extract:

- Properties: `repoVCs`, `activeTabIndex`, `allWorktrees`, `worktreeRepoCache`, `branchRefreshTimer`, `dashboardVC` reference, `workspaceManager` reference
- Methods: `switchToTab`, `openRepoTab`, `getOrCreateRepoVC`, `addRepo`, `addRepoViaOpenPanel`, `integrateDiscoveredRepoForTesting`, `performCloseRepo`, `updateStatusPollPreferences`, `showCloseProjectModal`, `showAddProjectModal`, `showNewThreadModal`, `buildAgentDisplayInfos`, `loadWorkspaces`, `startBranchRefreshTimer`, `refreshBranches`, `handleNavigateToWorktree`, `worktreeDidDelete`
- Protocol conformances: `DashboardDelegate`, `QuickSwitcherDelegate`, `RepoViewDelegate`, `NewBranchDialogDelegate`, TitleBar tab callbacks

The file should be structured as:

```swift
import AppKit

protocol TabCoordinatorDelegate: AnyObject {
    func tabCoordinator(_ coordinator: TabCoordinator, embedViewController vc: NSViewController)
    func tabCoordinatorDidSwitchTab(_ coordinator: TabCoordinator)
    func tabCoordinatorRequestUpdateTitleBar(_ coordinator: TabCoordinator)
    func tabCoordinatorRequestShowNewBranchDialog(_ coordinator: TabCoordinator)
    func tabCoordinatorRequestShowDiff(_ coordinator: TabCoordinator, worktreePath: String)
}

class TabCoordinator {
    weak var delegate: TabCoordinatorDelegate?
    var config: Config
    let workspaceManager = WorkspaceManager()

    var repoVCs: [String: RepoViewController] = [:]
    var activeTabIndex: Int = 0
    var allWorktrees: [(info: WorktreeInfo, tree: SplitTree)] = []
    var worktreeRepoCache: [String: String] = [:]
    var branchRefreshTimer: Timer?
    weak var dashboardVC: DashboardViewController?

    // References to other coordinators (accessed via MainWindowController delegation)
    var terminalCoordinator: TerminalCoordinator?
    var statusPublisher: StatusPublisher?
    var runtimeBackend: String = "local"

    private static let iso8601: ISO8601DateFormatter = {
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withInternetDateTime]
        return f
    }()

    init(config: Config) {
        self.config = config
    }

    // ... (all extracted methods from MainWindowController, adapted to use
    //      delegate callbacks instead of direct view manipulation)
}
```

Copy each method from MainWindowController, replacing:
- `self.embedViewController(vc)` → `delegate?.tabCoordinator(self, embedViewController: vc)`
- `self.updateTitleBar()` → `delegate?.tabCoordinatorRequestUpdateTitleBar(self)`
- `self.showNewBranchDialog()` → `delegate?.tabCoordinatorRequestShowNewBranchDialog(self)`
- `self.presentDiffOverlay(for: path)` → `delegate?.tabCoordinatorRequestShowDiff(self, worktreePath: path)`
- `self.surfaceManager` → `terminalCoordinator?.surfaceManager`
- `self.confirmAndDeleteWorktree(info)` → `terminalCoordinator?.confirmAndDeleteWorktree(info, window: ...)` (pass window via delegate or stored reference)
- `self.statusPublisher` → `statusPublisher`

Add protocol conformances as extensions on TabCoordinator:
- `DashboardDelegate`
- `QuickSwitcherDelegate`
- `RepoViewDelegate`
- `NewBranchDialogDelegate`

Note: `TitleBarDelegate` stays split — tab-related callbacks route through TabCoordinator, but the protocol itself stays on MainWindowController since it also handles theme/layout/window callbacks. MainWindowController's TitleBar delegate methods forward tab calls to TabCoordinator.

- [ ] **Step 4: Run tests to verify they pass**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmuxTests -configuration Debug test -only-testing:pmuxTests/TabCoordinatorTests 2>&1 | tail -20`
Expected: PASS (4 tests)

- [ ] **Step 5: Wire TabCoordinator into MainWindowController**

In `Sources/App/MainWindowController.swift`:

1. Remove properties: `repoVCs`, `activeTabIndex`, `allWorktrees`, `worktreeRepoCache`, `branchRefreshTimer`, `workspaceManager`

2. Add:
   ```swift
   private lazy var tabCoordinator: TabCoordinator = {
       let tc = TabCoordinator(config: config)
       tc.delegate = self
       tc.terminalCoordinator = terminalCoordinator
       tc.statusPublisher = statusPublisher
       tc.runtimeBackend = runtimeBackend
       return tc
   }()
   ```

3. Update `init()`:
   - Replace `loadWorkspaces()` with `tabCoordinator.loadWorkspaces()`
   - Move `NotificationCenter.default.addObserver` for `.navigateToWorktree` to TabCoordinator's `init` or `loadWorkspaces`

4. Update `dashboardVC` setup to set `tabCoordinator.dashboardVC = dashboardVC`

5. Replace `switchToTab(0)` and similar calls with `tabCoordinator.switchToTab(0)`

6. Update `TitleBarDelegate` to forward tab calls:
   ```swift
   func titleBarDidSelectDashboard() {
       tabCoordinator.switchToTab(0)
   }
   func titleBarDidSelectProject(_ projectName: String) {
       tabCoordinator.titleBarDidSelectProject(projectName)
   }
   // ... etc
   ```

7. Update `StatusPublisherDelegate` to use tabCoordinator:
   ```swift
   func statusDidChange(worktreePath: String, oldStatus: AgentStatus, newStatus: AgentStatus, lastMessage: String) {
       // Keep notification logic here, delegate UI update to TabCoordinator
       let branch = tabCoordinator.allWorktrees.first(where: { $0.info.path == worktreePath })?.info.branch ?? ""
       NotificationManager.shared.notify(...)
       DispatchQueue.main.async { [weak self] in
           guard let self else { return }
           self.tabCoordinator.dashboardVC?.updateAgents(self.tabCoordinator.buildAgentDisplayInfos())
           if self.tabCoordinator.activeTabIndex > 0 { ... }
           self.tabCoordinator.delegate?.tabCoordinatorRequestUpdateTitleBar(self.tabCoordinator)
       }
   }
   ```

8. Update `SettingsDelegate` to pass config to tabCoordinator:
   ```swift
   func settingsDidUpdateConfig(_ settings: SettingsViewController, config: Config) {
       let oldPaths = Set(self.config.workspacePaths)
       self.config = config
       tabCoordinator.config = config
       terminalCoordinator.config = config
       updateCoordinator.config = config
       normalizeBackendAvailabilityIfNeeded()
       let newPaths = Set(config.workspacePaths)
       if oldPaths != newPaths {
           tabCoordinator.loadWorkspaces()
       }
   }
   ```

9. Remove all extracted methods and protocol conformances from MainWindowController

10. Add TabCoordinatorDelegate:
    ```swift
    extension MainWindowController: TabCoordinatorDelegate {
        func tabCoordinator(_ coordinator: TabCoordinator, embedViewController vc: NSViewController) {
            embedViewController(vc)
        }
        func tabCoordinatorDidSwitchTab(_ coordinator: TabCoordinator) {
            panelCoordinator.closeBothPanels()
        }
        func tabCoordinatorRequestUpdateTitleBar(_ coordinator: TabCoordinator) {
            updateTitleBar()
        }
        func tabCoordinatorRequestShowNewBranchDialog(_ coordinator: TabCoordinator) {
            showNewBranchDialog()
        }
        func tabCoordinatorRequestShowDiff(_ coordinator: TabCoordinator, worktreePath: String) {
            presentDiffOverlay(for: worktreePath)
        }
    }
    ```

11. Update `currentRepoVC` computed property to use tabCoordinator:
    ```swift
    var currentRepoVC: RepoViewController? {
        tabCoordinator.currentRepoVC
    }
    ```
    And add `currentRepoVC` to TabCoordinator.

12. Update `windowWillClose` and `cleanupBeforeTermination`:
    ```swift
    func windowWillClose(_ notification: Notification) {
        statusPublisher.stop()
        tabCoordinator.branchRefreshTimer?.invalidate()
        tabCoordinator.branchRefreshTimer = nil
        terminalCoordinator.cleanup()
    }
    ```

- [ ] **Step 6: Regenerate Xcode project and run full test suite**

Run: `cd /Users/matt.chow/workspace/pmux-swift && xcodegen generate && xcodebuild -project pmux.xcodeproj -scheme pmuxTests -configuration Debug test 2>&1 | tail -20`
Expected: All tests pass

- [ ] **Step 7: Commit**

```bash
git add Sources/App/TabCoordinator.swift Tests/TabCoordinatorTests.swift Sources/App/MainWindowController.swift
git commit -m "refactor: extract TabCoordinator from MainWindowController"
```

---

## Task 5: Final Verification and Cleanup

**Files:**
- Modify: `Sources/App/MainWindowController.swift` (cleanup pass)

- [ ] **Step 1: Verify MainWindowController line count**

Run: `wc -l Sources/App/MainWindowController.swift`
Expected: ~700 lines (±100)

- [ ] **Step 2: Verify coordinator line counts**

Run: `wc -l Sources/App/UpdateCoordinator.swift Sources/App/PanelCoordinator.swift Sources/App/TerminalCoordinator.swift Sources/App/TabCoordinator.swift`
Expected: Each < 450 lines

- [ ] **Step 3: Run full test suite**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmuxTests -configuration Debug test 2>&1 | tail -30`
Expected: All tests pass (existing + ~14 new coordinator tests)

- [ ] **Step 4: Build and launch the app**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmux -configuration Debug build 2>&1 | tail -5`
Expected: BUILD SUCCEEDED

- [ ] **Step 5: Review for dead code**

Check MainWindowController for any orphaned methods, unused properties, or stale MARK comments that reference extracted code.

Run: `grep -n 'MARK:' Sources/App/MainWindowController.swift` and verify each section is still relevant.

- [ ] **Step 6: Commit cleanup**

```bash
git add Sources/App/MainWindowController.swift
git commit -m "chore: clean up MainWindowController after coordinator extraction"
```

---

## Task 6: Update Spec Status

- [ ] **Step 1: Update spec status to "Implemented"**

In `docs/superpowers/specs/2026-03-25-architecture-optimization-design.md`, change:
```
**Status:** Draft
```
to:
```
**Status:** Phase 1 Implemented
```

- [ ] **Step 2: Commit**

```bash
git add docs/superpowers/specs/2026-03-25-architecture-optimization-design.md
git commit -m "docs: mark Phase 1 as implemented in architecture optimization spec"
```
