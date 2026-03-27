# P1 Refactor: MainWindowController & DashboardViewController

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reduce MainWindowController from 983→~500 lines by extracting DialogPresenter, BackendResolver, and ThemeCoordinator. Reduce DashboardViewController from 986→~550 lines by unifying 3 duplicated spotlight rebuild methods into a single parameterized method.

**Architecture:** Extract pure-logic static methods and dialog presentation into standalone types. Unify the three spotlight layouts (leftRight, topSmall, topLarge) behind a shared `rebuildFocusLayout()` method parameterized by layout-specific config structs, keeping all views in DashboardViewController but eliminating code duplication.

**Tech Stack:** Swift 5.10, AppKit, XCTest

---

## File Map

### MainWindowController Extractions

| File | Action | Responsibility |
|------|--------|---------------|
| `Sources/App/BackendResolver.swift` | **Create** | Static backend resolution logic + version checking |
| `Sources/App/DialogPresenter.swift` | **Create** | Sheet presentation for settings, new branch, quick switcher, diff, shortcuts |
| `Sources/App/MainWindowController.swift` | **Modify** | Remove extracted code, delegate to new types |

### DashboardViewController Deduplication

| File | Action | Responsibility |
|------|--------|---------------|
| `Sources/UI/Dashboard/DashboardViewController.swift` | **Modify** | Unify 3 spotlight rebuild methods into 1, unify 3 setup methods' shared pattern |

---

## Part A: MainWindowController Extractions

### Task 1: Extract BackendResolver

**Files:**
- Create: `Sources/App/BackendResolver.swift`
- Modify: `Sources/App/MainWindowController.swift`

- [ ] **Step 1: Create BackendResolver with extracted static methods**

Create `Sources/App/BackendResolver.swift`:

```swift
import AppKit

enum BackendResolver {
    struct Resolution {
        let backend: String
        let warningMessage: String?
        let zmxAvailable: Bool
    }

    static func resolvePreferredBackend(preferred: String, zmxAvailable: Bool, tmuxAvailable: Bool) -> String {
        switch preferred {
        case "local":
            if zmxAvailable { return "zmx" }
            return tmuxAvailable ? "tmux" : "local"
        case "tmux":
            if zmxAvailable { return "zmx" }
            if tmuxAvailable { return "tmux" }
            return zmxAvailable ? "zmx" : "local"
        case "zmx":
            if zmxAvailable {
                return "zmx"
            }
            return tmuxAvailable ? "tmux" : "local"
        default:
            if zmxAvailable {
                return "zmx"
            }
            return tmuxAvailable ? "tmux" : "local"
        }
    }

    static func isSupportedZmxVersion(_ rawVersion: String) -> Bool {
        let trimmed = rawVersion.trimmingCharacters(in: .whitespacesAndNewlines)
        let parts = trimmed.split(separator: ".").compactMap { Int($0) }
        guard parts.count >= 3 else { return false }
        let major = parts[0]
        let minor = parts[1]
        let patch = parts[2]

        if major > 0 { return true }
        if minor > 4 { return true }
        if minor < 4 { return false }
        return patch >= 2
    }

    /// Resolve backend asynchronously, then call completion on main thread with Resolution.
    static func resolveAsync(preferred: String, completion: @escaping (Resolution) -> Void) {
        DispatchQueue.global(qos: .userInitiated).async {
            let zmxAvailable = ProcessRunner.commandExists("zmx")
            let tmuxAvailable = ProcessRunner.commandExists("tmux")

            var zmxVersion: String?
            if preferred == "zmx" && zmxAvailable {
                zmxVersion = ProcessRunner.output(["zmx", "version"])
            }

            var targetBackend = resolvePreferredBackend(
                preferred: preferred,
                zmxAvailable: zmxAvailable,
                tmuxAvailable: tmuxAvailable
            )

            var warningMessage: String?
            if preferred == "zmx" {
                if !zmxAvailable {
                    warningMessage = "zmx is not installed. Install with `brew install neurosnap/tap/zmx`."
                } else if let version = zmxVersion, !isSupportedZmxVersion(version) {
                    warningMessage = "zmx version is too old. Please upgrade to zmx 0.4.2+ for stability."
                }
            }

            if warningMessage != nil, targetBackend == "zmx" {
                targetBackend = tmuxAvailable ? "tmux" : "local"
            }

            let resolution = Resolution(
                backend: targetBackend,
                warningMessage: warningMessage,
                zmxAvailable: zmxAvailable
            )
            DispatchQueue.main.async { completion(resolution) }
        }
    }

    /// Show backend fallback alert if needed. Call from main thread.
    static func showWarningIfNeeded(_ resolution: Resolution, configBackend: String) {
        guard let warningMessage = resolution.warningMessage else { return }
        let alert = NSAlert()
        alert.messageText = "Backend Fallback Activated"
        alert.informativeText = "\(warningMessage)\nCurrent backend: \(resolution.backend)."
        alert.alertStyle = .warning
        if configBackend == "zmx" && !resolution.zmxAvailable {
            alert.addButton(withTitle: "Copy Install Command")
            alert.addButton(withTitle: "Open zmx Docs")
            alert.addButton(withTitle: "OK")
            let response = alert.runModal()
            if response == .alertFirstButtonReturn {
                NSPasteboard.general.clearContents()
                NSPasteboard.general.setString("brew install neurosnap/tap/zmx", forType: .string)
            } else if response == .alertSecondButtonReturn,
                      let url = URL(string: "https://zmx.sh") {
                NSWorkspace.shared.open(url)
            }
        } else {
            alert.addButton(withTitle: "OK")
            alert.runModal()
        }
    }
}
```

- [ ] **Step 2: Build to verify BackendResolver compiles**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -5`
Expected: BUILD SUCCEEDED

- [ ] **Step 3: Replace MainWindowController static methods and normalizeBackendAvailabilityIfNeeded**

In `MainWindowController.swift`, remove:
- The `static func resolvePreferredBackend(...)` method (lines 98-118)
- The `static func isSupportedZmxVersion(...)` method (lines 120-146)
- The `normalizeBackendAvailabilityIfNeeded()` method
- The `applyBackendResolution(...)` method

Replace with:

```swift
    private func normalizeBackendAvailabilityIfNeeded() {
        BackendResolver.resolveAsync(preferred: config.backend) { [weak self] resolution in
            guard let self else { return }
            self.runtimeBackend = resolution.backend
            self.tabCoordinator.runtimeBackend = resolution.backend

            if resolution.warningMessage == nil, resolution.backend != self.config.backend {
                self.config.backend = resolution.backend
                self.config.save()
            }

            BackendResolver.showWarningIfNeeded(resolution, configBackend: self.config.backend)
        }
    }
```

Also update any remaining references to `Self.resolvePreferredBackend` or `Self.isSupportedZmxVersion` to use `BackendResolver.` prefix.

- [ ] **Step 4: Build to verify refactored MWC compiles**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -5`
Expected: BUILD SUCCEEDED

- [ ] **Step 5: Write tests for BackendResolver**

Create test cases in `Tests/BackendResolverTests.swift`:

```swift
import XCTest
@testable import amux

final class BackendResolverTests: XCTestCase {
    // MARK: - resolvePreferredBackend

    func testPreferZmxWhenAvailable() {
        let result = BackendResolver.resolvePreferredBackend(preferred: "zmx", zmxAvailable: true, tmuxAvailable: true)
        XCTAssertEqual(result, "zmx")
    }

    func testFallbackToTmuxWhenZmxUnavailable() {
        let result = BackendResolver.resolvePreferredBackend(preferred: "zmx", zmxAvailable: false, tmuxAvailable: true)
        XCTAssertEqual(result, "tmux")
    }

    func testFallbackToLocalWhenNoneAvailable() {
        let result = BackendResolver.resolvePreferredBackend(preferred: "zmx", zmxAvailable: false, tmuxAvailable: false)
        XCTAssertEqual(result, "local")
    }

    func testLocalPreferredUpgradesToZmx() {
        let result = BackendResolver.resolvePreferredBackend(preferred: "local", zmxAvailable: true, tmuxAvailable: true)
        XCTAssertEqual(result, "zmx")
    }

    func testTmuxPreferredUpgradesToZmx() {
        let result = BackendResolver.resolvePreferredBackend(preferred: "tmux", zmxAvailable: true, tmuxAvailable: true)
        XCTAssertEqual(result, "zmx")
    }

    func testUnknownPreferredDefaultsToZmx() {
        let result = BackendResolver.resolvePreferredBackend(preferred: "unknown", zmxAvailable: true, tmuxAvailable: false)
        XCTAssertEqual(result, "zmx")
    }

    // MARK: - isSupportedZmxVersion

    func testSupportedVersion042() {
        XCTAssertTrue(BackendResolver.isSupportedZmxVersion("0.4.2"))
    }

    func testSupportedVersion050() {
        XCTAssertTrue(BackendResolver.isSupportedZmxVersion("0.5.0"))
    }

    func testSupportedVersion100() {
        XCTAssertTrue(BackendResolver.isSupportedZmxVersion("1.0.0"))
    }

    func testUnsupportedVersion041() {
        XCTAssertFalse(BackendResolver.isSupportedZmxVersion("0.4.1"))
    }

    func testUnsupportedVersion030() {
        XCTAssertFalse(BackendResolver.isSupportedZmxVersion("0.3.0"))
    }

    func testVersionWithWhitespace() {
        XCTAssertTrue(BackendResolver.isSupportedZmxVersion("  0.4.2\n"))
    }

    func testInvalidVersionString() {
        XCTAssertFalse(BackendResolver.isSupportedZmxVersion("abc"))
    }
}
```

- [ ] **Step 6: Run tests**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/BackendResolverTests 2>&1 | tail -10`
Expected: All tests pass

- [ ] **Step 7: Commit**

```bash
git add Sources/App/BackendResolver.swift Sources/App/MainWindowController.swift Tests/BackendResolverTests.swift
git commit -m "refactor: extract BackendResolver from MainWindowController"
```

---

### Task 2: Extract DialogPresenter

**Files:**
- Create: `Sources/App/DialogPresenter.swift`
- Modify: `Sources/App/MainWindowController.swift`

- [ ] **Step 1: Create DialogPresenter**

Create `Sources/App/DialogPresenter.swift`:

```swift
import AppKit

/// Encapsulates all sheet/dialog presentation logic for the main window.
/// Holds no state — receives dependencies as method parameters.
final class DialogPresenter {
    private weak var tabCoordinator: TabCoordinator?
    private weak var terminalCoordinator: TerminalCoordinator?
    private weak var statusPublisher: StatusPublisher?

    init(tabCoordinator: TabCoordinator, terminalCoordinator: TerminalCoordinator, statusPublisher: StatusPublisher) {
        self.tabCoordinator = tabCoordinator
        self.terminalCoordinator = terminalCoordinator
        self.statusPublisher = statusPublisher
    }

    func presentSheetOnActiveVC(_ vc: NSViewController, tabCoordinator: TabCoordinator, dashboardVC: DashboardViewController?) {
        if let activeVC = tabCoordinator.currentRepoVC {
            activeVC.presentAsSheet(vc)
        } else {
            dashboardVC?.presentAsSheet(vc)
        }
    }

    func makeQuickSwitcher(quickSwitcherDelegate: QuickSwitcherDelegate) -> QuickSwitcherViewController {
        let worktreeInfos = tabCoordinator?.allWorktrees.map { $0.info } ?? []
        var statuses: [String: AgentStatus] = [:]
        if let surfaceManager = terminalCoordinator?.surfaceManager {
            for (path, _) in surfaceManager.all {
                statuses[path] = statusPublisher?.status(for: path)
            }
        }
        let switcher = QuickSwitcherViewController(worktrees: worktreeInfos, statuses: statuses)
        switcher.quickSwitcherDelegate = quickSwitcherDelegate
        return switcher
    }

    func makeSettings(config: Config, settingsDelegate: SettingsDelegate) -> SettingsViewController {
        let settingsVC = SettingsViewController(config: config)
        settingsVC.settingsDelegate = settingsDelegate
        return settingsVC
    }

    func makeNewBranchDialog(repoPaths: [String], dialogDelegate: NewBranchDialogDelegate) -> NewBranchDialog {
        let dialog = NewBranchDialog(repoPaths: repoPaths)
        dialog.dialogDelegate = dialogDelegate
        return dialog
    }

    static func showKeyboardShortcuts() {
        let alert = NSAlert()
        alert.messageText = "Keyboard Shortcuts"
        alert.informativeText = """
        ⌘N  New Branch
        ⌘P  Quick Switch
        ⌘W  Close Tab
        ⌘0  Dashboard
        ⌘,  Settings
        ⌘}  Next Tab
        ⌘{  Previous Tab
        ⌘-  Zoom In (Smaller Cards)
        ⌘=  Zoom Out (Larger Cards)
        Esc  Close Dialog / Exit Spotlight
        """
        alert.alertStyle = .informational
        alert.addButton(withTitle: "OK")
        alert.runModal()
    }
}
```

- [ ] **Step 2: Build to verify DialogPresenter compiles**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -5`
Expected: BUILD SUCCEEDED

- [ ] **Step 3: Wire DialogPresenter into MainWindowController**

In `MainWindowController.swift`, add a lazy property:

```swift
    private lazy var dialogPresenter: DialogPresenter = {
        DialogPresenter(
            tabCoordinator: tabCoordinator,
            terminalCoordinator: terminalCoordinator,
            statusPublisher: statusPublisher
        )
    }()
```

Replace the body of these methods:

```swift
    @objc func showQuickSwitcher() {
        let switcher = dialogPresenter.makeQuickSwitcher(quickSwitcherDelegate: self)
        dialogPresenter.presentSheetOnActiveVC(switcher, tabCoordinator: tabCoordinator, dashboardVC: dashboardVC)
    }

    @objc func showSettings() {
        let settingsVC = dialogPresenter.makeSettings(config: config, settingsDelegate: self)
        dialogPresenter.presentSheetOnActiveVC(settingsVC, tabCoordinator: tabCoordinator, dashboardVC: dashboardVC)
    }

    @objc func showNewBranchDialog() {
        let dialog = dialogPresenter.makeNewBranchDialog(repoPaths: config.workspacePaths, dialogDelegate: self)
        dialogPresenter.presentSheetOnActiveVC(dialog, tabCoordinator: tabCoordinator, dashboardVC: dashboardVC)
    }

    @objc func showKeyboardShortcuts() {
        DialogPresenter.showKeyboardShortcuts()
    }
```

Remove the old `presentSheetOnActiveVC(_:)` private method from MWC.

- [ ] **Step 4: Build to verify**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -5`
Expected: BUILD SUCCEEDED

- [ ] **Step 5: Commit**

```bash
git add Sources/App/DialogPresenter.swift Sources/App/MainWindowController.swift
git commit -m "refactor: extract DialogPresenter from MainWindowController"
```

---

### Task 3: Extract GlassBackgroundConfig to standalone type

**Files:**
- Modify: `Sources/App/MainWindowController.swift`

This is a minor extraction — move the `GlassBackgroundConfig` struct and `glassBackgroundConfig(isDark:)` static method out of the class body into a top-level enum.

- [ ] **Step 1: Move GlassBackgroundConfig outside of MainWindowController**

At the top of `MainWindowController.swift` (before the class), add:

```swift
enum WindowStyling {
    struct GlassBackgroundConfig {
        let enabled: Bool
        let material: NSVisualEffectView.Material
        let blendingMode: NSVisualEffectView.BlendingMode
    }

    static func glassBackgroundConfig(isDark: Bool) -> GlassBackgroundConfig {
        if isDark {
            return GlassBackgroundConfig(enabled: true, material: .hudWindow, blendingMode: .behindWindow)
        }
        return GlassBackgroundConfig(enabled: true, material: .underWindowBackground, blendingMode: .behindWindow)
    }

    static func shouldUseWindowFrameAutosave(
        environment: [String: String] = ProcessInfo.processInfo.environment,
        arguments: [String] = ProcessInfo.processInfo.arguments
    ) -> Bool {
        if environment["XCTestConfigurationFilePath"] != nil {
            return false
        }
        if arguments.contains("-PmuxUITesting") {
            return false
        }
        if let idx = arguments.firstIndex(of: "-ApplePersistenceIgnoreState"),
           arguments.indices.contains(idx + 1),
           arguments[idx + 1].caseInsensitiveCompare("YES") == .orderedSame {
            return false
        }
        return true
    }

    static func trafficLightButtonOriginY(containerHeight: CGFloat, buttonHeight: CGFloat) -> CGFloat {
        (containerHeight / 2) + TitleBarView.Layout.arcVerticalOffset - (buttonHeight / 2)
    }
}
```

Remove the corresponding `struct GlassBackgroundConfig`, `static func glassBackgroundConfig`, `static func shouldUseWindowFrameAutosave`, `static func trafficLightButtonOriginY`, and `static func shouldHandleEscShortcut` from inside the MainWindowController class.

Update all call sites from `Self.glassBackgroundConfig(...)` → `WindowStyling.glassBackgroundConfig(...)`, `Self.shouldUseWindowFrameAutosave()` → `WindowStyling.shouldUseWindowFrameAutosave()`, `Self.trafficLightButtonOriginY(...)` → `WindowStyling.trafficLightButtonOriginY(...)`.

Note: `shouldHandleEscShortcut()` always returns `false` — check if `AmuxWindow` references it. If so, move it to `WindowStyling` too. If unused, delete it.

- [ ] **Step 2: Build to verify**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -5`
Expected: BUILD SUCCEEDED

- [ ] **Step 3: Commit**

```bash
git add Sources/App/MainWindowController.swift
git commit -m "refactor: extract WindowStyling from MainWindowController static methods"
```

---

## Part B: DashboardViewController Deduplication

### Task 4: Unify three spotlight rebuild methods

The three methods `rebuildLeftRight()`, `rebuildTopSmall()`, `rebuildTopLarge()` are 85-95% identical. The differences are:
1. Which views (container, focusPanel, stack, miniCards array) to operate on
2. Width constraint style: leftRight uses fixed `sidebarWidth`, topSmall/topLarge use `220` with min/max

**Files:**
- Modify: `Sources/UI/Dashboard/DashboardViewController.swift`

- [ ] **Step 1: Define FocusLayoutRefs helper struct**

Add inside DashboardViewController, near the top (after LayoutMetrics):

```swift
    /// References to the views for a single focus layout variant.
    private struct FocusLayoutRefs {
        let focusPanel: FocusPanelView
        let scrollView: NSScrollView
        let stack: NSStackView
        var miniCards: [StackedMiniCardContainerView]

        /// Layout style for mini card width constraints.
        enum WidthStyle {
            case fixed          // Uses scroll view's bounds width (leftRight sidebar)
            case flexible       // Uses 220pt nominal with 180-260 range (topSmall, topLarge)
        }
        let widthStyle: WidthStyle
    }
```

- [ ] **Step 2: Add a computed accessor for the current layout's refs**

```swift
    private func focusLayoutRefs(for layout: DashboardLayout) -> FocusLayoutRefs? {
        switch layout {
        case .grid:
            return nil
        case .leftRight:
            return FocusLayoutRefs(
                focusPanel: leftRightFocusPanel,
                scrollView: leftRightSidebarScroll,
                stack: leftRightSidebarStack,
                miniCards: leftRightMiniCards,
                widthStyle: .fixed
            )
        case .topSmall:
            return FocusLayoutRefs(
                focusPanel: topSmallFocusPanel,
                scrollView: topSmallTopScroll,
                stack: topSmallTopStack,
                miniCards: topSmallMiniCards,
                widthStyle: .flexible
            )
        case .topLarge:
            return FocusLayoutRefs(
                focusPanel: topLargeFocusPanel,
                scrollView: topLargeBottomScroll,
                stack: topLargeBottomStack,
                miniCards: topLargeMiniCards,
                widthStyle: .flexible
            )
        }
    }
```

- [ ] **Step 3: Create unified rebuildFocusLayout method**

```swift
    private func rebuildFocusLayout(_ layout: DashboardLayout) {
        guard var refs = focusLayoutRefs(for: layout) else { return }

        // Clear old mini cards
        refs.miniCards.forEach { $0.removeFromSuperview() }
        refs.miniCards.removeAll()
        refs.stack.arrangedSubviews.forEach { $0.removeFromSuperview() }

        let sorted = sortedAgents()
        guard !sorted.isEmpty else { return }

        // Configure focus panel with selected agent
        if let selected = sorted.first(where: { $0.id == selectedAgentId }) ?? sorted.first {
            selectedAgentId = selected.id
            configureFocusPanel(refs.focusPanel, with: selected)
            embedSurface(selected, in: refs.focusPanel.terminalContainer)
        }

        // Build mini cards
        let fixedWidth = refs.scrollView.bounds.width > 0 ? refs.scrollView.bounds.width : 240
        for agent in sorted {
            let container = StackedMiniCardContainerView()
            container.delegate = self
            container.configure(paneCount: agent.paneCount)
            container.miniCardView.configure(
                id: agent.id,
                project: agent.project,
                thread: agent.thread,
                status: agent.status,
                lastMessage: agent.lastMessage,
                totalDuration: agent.totalDuration,
                roundDuration: agent.roundDuration,
                paneStatuses: agent.paneStatuses
            )
            container.isSelected = (agent.id == selectedAgentId)
            container.translatesAutoresizingMaskIntoConstraints = false
            refs.miniCards.append(container)
            refs.stack.addArrangedSubview(container)

            switch refs.widthStyle {
            case .fixed:
                NSLayoutConstraint.activate([
                    container.widthAnchor.constraint(equalToConstant: fixedWidth),
                    container.heightAnchor.constraint(equalTo: container.widthAnchor, multiplier: 9.0 / 16.0),
                ])
            case .flexible:
                let widthConstraint = container.widthAnchor.constraint(equalToConstant: 220)
                widthConstraint.priority = .defaultHigh
                let minWidth = container.widthAnchor.constraint(greaterThanOrEqualToConstant: 180)
                let maxWidth = container.widthAnchor.constraint(lessThanOrEqualToConstant: 260)
                let heightConstraint = container.heightAnchor.constraint(equalTo: container.widthAnchor, multiplier: 9.0 / 16.0)
                NSLayoutConstraint.activate([widthConstraint, minWidth, maxWidth, heightConstraint])
            }
        }

        // Write back the updated miniCards array
        switch layout {
        case .leftRight: leftRightMiniCards = refs.miniCards
        case .topSmall: topSmallMiniCards = refs.miniCards
        case .topLarge: topLargeMiniCards = refs.miniCards
        case .grid: break
        }
    }
```

- [ ] **Step 4: Replace rebuildLeftRight, rebuildTopSmall, rebuildTopLarge**

Delete the three methods `rebuildLeftRight()`, `rebuildTopSmall()`, `rebuildTopLarge()`.

Update `rebuildCurrentLayout()`:

```swift
    private func rebuildCurrentLayout() {
        switch currentLayout {
        case .grid:
            rebuildGrid()
        case .leftRight, .topSmall, .topLarge:
            rebuildFocusLayout(currentLayout)
        }
    }
```

- [ ] **Step 5: Build to verify**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -5`
Expected: BUILD SUCCEEDED

- [ ] **Step 6: Commit**

```bash
git add Sources/UI/Dashboard/DashboardViewController.swift
git commit -m "refactor: unify 3 spotlight rebuild methods into rebuildFocusLayout"
```

---

### Task 5: Unify detachTerminals and updateMiniCardSelection using FocusLayoutRefs

**Files:**
- Modify: `Sources/UI/Dashboard/DashboardViewController.swift`

- [ ] **Step 1: Simplify detachTerminals**

Replace:

```swift
    func detachTerminals() {
        leftRightFocusPanel.terminalContainer.subviews.forEach { $0.removeFromSuperview() }
        topSmallFocusPanel.terminalContainer.subviews.forEach { $0.removeFromSuperview() }
        topLargeFocusPanel.terminalContainer.subviews.forEach { $0.removeFromSuperview() }
    }
```

With:

```swift
    func detachTerminals() {
        for layout in [DashboardLayout.leftRight, .topSmall, .topLarge] {
            focusLayoutRefs(for: layout)?.focusPanel.terminalContainer.subviews.forEach { $0.removeFromSuperview() }
        }
    }
```

- [ ] **Step 2: Simplify updateMiniCardSelection**

Replace:

```swift
    private func updateMiniCardSelection() {
        let updateCards: ([StackedMiniCardContainerView]) -> Void = { cards in
            for card in cards {
                card.isSelected = (card.agentId == self.selectedAgentId)
            }
        }
        switch currentLayout {
        case .leftRight: updateCards(leftRightMiniCards)
        case .topSmall: updateCards(topSmallMiniCards)
        case .topLarge: updateCards(topLargeMiniCards)
        case .grid: break
        }
    }
```

With:

```swift
    private func updateMiniCardSelection() {
        guard let refs = focusLayoutRefs(for: currentLayout) else { return }
        for card in refs.miniCards {
            card.isSelected = (card.agentId == selectedAgentId)
        }
    }
```

- [ ] **Step 3: Simplify focusPanelDidRequestNavigate focus panel lookup**

In `focusPanelDidRequestNavigate(_:direction:)`, replace the switch statement that determines `focusPanel`:

```swift
        // Old:
        // let focusPanel: FocusPanelView
        // switch currentLayout {
        // case .leftRight: focusPanel = leftRightFocusPanel
        // ...

        // New:
        guard let refs = focusLayoutRefs(for: currentLayout) else { return }
        let focusPanel = refs.focusPanel
```

- [ ] **Step 4: Simplify terminalSurfaceDidRecover focus panel check**

In `terminalSurfaceDidRecover(_:)`, replace the three-way focus panel check:

```swift
    func terminalSurfaceDidRecover(_ surface: TerminalSurface) {
        guard let agent = agents.first(where: { $0.surface === surface }) else { return }
        // Try grid card first
        if let container = gridCards.first(where: { $0.agentId == agent.id }) {
            embedSurface(agent, in: container.cardView.terminalContainer)
            return
        }
        // Try current focus panel
        if agent.id == selectedAgentId, let refs = focusLayoutRefs(for: currentLayout) {
            embedSurface(agent, in: refs.focusPanel.terminalContainer)
        }
    }
```

- [ ] **Step 5: Simplify updateCurrentLayoutInPlace**

Replace:

```swift
    private func updateCurrentLayoutInPlace() {
        let sorted = sortedAgents()
        switch currentLayout {
        case .grid:
            updateGridInPlace(sorted)
        case .leftRight:
            updateFocusLayoutInPlace(sorted, miniCards: leftRightMiniCards, focusPanel: leftRightFocusPanel)
        case .topSmall:
            updateFocusLayoutInPlace(sorted, miniCards: topSmallMiniCards, focusPanel: topSmallFocusPanel)
        case .topLarge:
            updateFocusLayoutInPlace(sorted, miniCards: topLargeMiniCards, focusPanel: topLargeFocusPanel)
        }
    }
```

With:

```swift
    private func updateCurrentLayoutInPlace() {
        let sorted = sortedAgents()
        if currentLayout == .grid {
            updateGridInPlace(sorted)
        } else if let refs = focusLayoutRefs(for: currentLayout) {
            updateFocusLayoutInPlace(sorted, miniCards: refs.miniCards, focusPanel: refs.focusPanel)
        }
    }
```

- [ ] **Step 6: Build to verify**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -5`
Expected: BUILD SUCCEEDED

- [ ] **Step 7: Commit**

```bash
git add Sources/UI/Dashboard/DashboardViewController.swift
git commit -m "refactor: use FocusLayoutRefs to eliminate layout switch duplication"
```

---

### Task 6: Add project.yml entries for new files

If the project uses XcodeGen, the new files need to be included.

**Files:**
- Modify: `project.yml`

- [ ] **Step 1: Check if project.yml uses file globs or explicit file lists**

Read `project.yml` and look at the `sources` section for the main target. If it uses a glob like `Sources/**`, no changes needed. If it lists files explicitly, add the new files.

- [ ] **Step 2: Regenerate Xcode project if needed**

Run: `xcodegen generate`

- [ ] **Step 3: Build and verify**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -5`
Expected: BUILD SUCCEEDED

- [ ] **Step 4: Run all passing tests**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/BackendResolverTests -only-testing:amuxTests/ConfigTests -only-testing:amuxTests/SplitNodeTests 2>&1 | tail -10`
Expected: All tests pass

- [ ] **Step 5: Commit**

```bash
git add project.yml amux.xcodeproj Tests/BackendResolverTests.swift
git commit -m "chore: regenerate Xcode project with new extracted files"
```

---

## Expected Line Count Changes

| File | Before | After (est.) | Reduction |
|------|--------|-------------|-----------|
| MainWindowController.swift | 983 | ~550 | ~44% |
| DashboardViewController.swift | 986 | ~700 | ~29% |
| **New: BackendResolver.swift** | — | ~105 | — |
| **New: DialogPresenter.swift** | — | ~70 | — |
| **Total net lines** | 1969 | ~1425 | ~28% reduction |

The key win for Dashboard isn't just line count — it's eliminating 3-way duplication so that future layout changes only need to be made once.
