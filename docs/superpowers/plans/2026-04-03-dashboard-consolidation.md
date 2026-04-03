# Dashboard Consolidation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove the separate project detail (RepoViewController) interface and consolidate all functionality into the dashboard, with split-pane terminals in the focus panel and a simplified title bar.

**Architecture:** Incremental delete-and-replace approach. DashboardViewController's FocusPanelView gains SplitContainerView embedding (replacing single terminal surfaces). TitleBarView's left capsule switches from tab list to worktree info display. TabCoordinator and MainWindowController shed repo-tab logic. TerminalCoordinator retargets split operations from RepoVC to dashboard focus panel.

**Tech Stack:** Swift 5.10, AppKit, XCTest

---

### Task 1: DashboardViewController — Embed SplitContainerView in Focus Panel

**Files:**
- Modify: `Sources/UI/Dashboard/DashboardViewController.swift`
- Modify: `Sources/UI/Dashboard/FocusPanelView.swift`

The focus panel currently embeds a single terminal surface via `embedSurface(_:in:)` (line ~722). Replace this with SplitContainerView embedding per worktree.

- [ ] **Step 1: Add splitContainers cache and activeSplitContainer to DashboardViewController**

In `DashboardViewController.swift`, add these properties near the existing layout properties (around line 52-68):

```swift
/// Cached SplitContainerView per worktree path (same pattern as RepoViewController.splitContainers)
private var splitContainers: [String: SplitContainerView] = [:]

/// Currently visible split container in the focus panel
private(set) var activeSplitContainer: SplitContainerView?
```

- [ ] **Step 2: Add embedSplitContainer method**

Replace the existing `embedSurface` / `embedPaneSurface` methods (lines ~722-741) with a method that embeds the full SplitContainerView. The new method replicates `RepoViewController.showTerminal(at:)` logic (lines 218-289 of RepoViewController.swift):

```swift
/// Embeds the SplitContainerView for the selected agent into the current focus panel's terminal container.
/// Replicates the pattern from RepoViewController.showTerminal(at:).
func embedSplitContainerForSelectedAgent() {
    guard currentLayout != .grid else { return }
    guard let refs = focusLayoutRefs(for: currentLayout) else { return }
    let container = refs.focusPanel.terminalContainer

    guard selectedAgentIndex < agents.count else { return }
    let agent = agents[selectedAgentIndex]
    guard let worktreePath = agent.worktreePaths.first else { return }

    // Deactivate previous
    activeSplitContainer?.removeFromSuperview()
    activeSplitContainer = nil

    // Get or create SplitContainerView
    let splitView: SplitContainerView
    if let cached = splitContainers[worktreePath] {
        splitView = cached
    } else {
        splitView = SplitContainerView(frame: container.bounds)
        splitView.delegate = splitContainerDelegate
        splitContainers[worktreePath] = splitView
    }

    // Populate surface views from SurfaceRegistry
    guard let tree = surfaceManager?.tree(forPath: worktreePath) else { return }
    splitView.surfaceViews = [:]
    for leaf in tree.allLeaves {
        if let surfaceView = SurfaceRegistry.shared.view(for: leaf.surfaceId) {
            splitView.surfaceViews[leaf.surfaceId] = surfaceView
        }
    }

    // Embed
    splitView.frame = container.bounds
    splitView.autoresizingMask = [.width, .height]
    container.addSubview(splitView)
    splitView.tree = tree
    activeSplitContainer = splitView

    // Focus the active leaf
    if let focusedLeaf = tree.findLeaf(id: tree.focusedId),
       let surfaceView = SurfaceRegistry.shared.view(for: focusedLeaf.surfaceId) {
        surfaceView.window?.makeFirstResponder(surfaceView)
    }
}
```

- [ ] **Step 3: Add surfaceManager and splitContainerDelegate properties**

DashboardViewController needs access to TerminalSurfaceManager and a delegate for split container events. Add near the top of the class:

```swift
/// Set by TabCoordinator during setup
weak var surfaceManager: TerminalSurfaceManager?

/// Set by MainWindowController — forwards split events to TerminalCoordinator
weak var splitContainerDelegate: SplitContainerDelegate?
```

- [ ] **Step 4: Update detachTerminals to handle SplitContainerView**

Replace the existing `detachTerminals()` method (lines ~269-273):

```swift
func detachTerminals() {
    activeSplitContainer?.removeFromSuperview()
    activeSplitContainer = nil
}
```

- [ ] **Step 5: Wire embedSplitContainer into the update/rebuild flow**

In `rebuildFocusLayout(_:)` (line ~332), after configuring the focus panel, call `embedSplitContainerForSelectedAgent()` instead of the old `embedSurface` call. Find the line that calls `embedSurface(selected, in: focusPanel.terminalContainer)` and replace with:

```swift
embedSplitContainerForSelectedAgent()
```

Similarly in `updateCurrentLayoutInPlace()` (line ~187), replace the `embedSurface` check block (lines ~222-234) with:

```swift
if activeSplitContainer == nil, view.window != nil {
    embedSplitContainerForSelectedAgent()
}
```

- [ ] **Step 6: Update card selection to re-embed split container**

In the card click handler (the method that updates `selectedAgentIndex`), after updating the index, call:

```swift
detachTerminals()
embedSplitContainerForSelectedAgent()
```

This replaces the old single-surface embedding on selection change.

- [ ] **Step 7: Remove old embedSurface / embedPaneSurface methods**

Delete the `embedSurface(_:in:)` and `embedPaneSurface(_:worktreePath:in:)` methods (lines ~722-741), as they are replaced by `embedSplitContainerForSelectedAgent()`.

- [ ] **Step 8: Remove FocusPanelView header and navigation**

The focus panel header (name, meta, duration, enter button, pane navigation) is no longer needed — that info moves to the title bar capsule. In `FocusPanelView.swift`:

Remove from `setupHeader()` (line ~134): all header labels, status dots, enter button, and their constraints.
Remove `setupNavigation()` (line ~211): prev/next buttons, counter label.
Remove `configure()` method (line ~59) and `configureNavigation()` method (line ~267).
Remove `FocusPanelDelegate` protocol methods related to navigation and enter-project.

Keep only `terminalContainer` and `setCornerMask(_:radius:)`. The FocusPanelView becomes a thin container:

```swift
final class FocusPanelView: NSView {
    let terminalContainer = NSView()

    override init(frame: CGRect) {
        super.init(frame: frame)
        setupTerminalContainer()
    }

    required init?(coder: NSCoder) { fatalError() }

    func setCornerMask(_ mask: CACornerMask, radius: CGFloat) {
        wantsLayer = true
        layer?.cornerRadius = radius
        layer?.maskedCorners = mask
        layer?.masksToBounds = true
    }

    private func setupTerminalContainer() {
        terminalContainer.translatesAutoresizingMaskIntoConstraints = false
        addSubview(terminalContainer)
        NSLayoutConstraint.activate([
            terminalContainer.topAnchor.constraint(equalTo: topAnchor),
            terminalContainer.leadingAnchor.constraint(equalTo: leadingAnchor),
            terminalContainer.trailingAnchor.constraint(equalTo: trailingAnchor),
            terminalContainer.bottomAnchor.constraint(equalTo: bottomAnchor),
        ])
    }
}
```

- [ ] **Step 9: Remove pane navigation from DashboardViewController**

Delete `focusPanelDidRequestNavigate` (lines ~867-902), `focusPanelDidSelectPane` handler, `slideSubtype` helper (lines ~904-915), `selectedPaneIndex` property, and any `configureFocusPanel` calls. Pane navigation is now handled by split container focus (Cmd+Opt+Arrow).

- [ ] **Step 10: Add invalidateSplitContainer method**

For worktree deletion cleanup:

```swift
func invalidateSplitContainer(forPath path: String) {
    splitContainers[path]?.removeFromSuperview()
    splitContainers.removeValue(forKey: path)
    if activeSplitContainer === splitContainers[path] {
        activeSplitContainer = nil
    }
}
```

- [ ] **Step 11: Build and verify**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -20`
Expected: BUILD SUCCEEDED (may have warnings about unused code in other files — those get cleaned up in later tasks)

- [ ] **Step 12: Commit**

```bash
git add Sources/UI/Dashboard/DashboardViewController.swift Sources/UI/Dashboard/FocusPanelView.swift
git commit -m "feat: embed SplitContainerView in dashboard focus panel"
```

---

### Task 2: TerminalCoordinator — Retarget from RepoVC to Dashboard

**Files:**
- Modify: `Sources/App/TerminalCoordinator.swift`

Currently TerminalCoordinator finds the active SplitContainerView via `currentRepoVC()?.activeSplitContainer` (line ~51). Retarget to dashboard.

- [ ] **Step 1: Replace currentRepoVC closure with activeSplitContainer closure**

In TerminalCoordinator (line ~14-20), replace:

```swift
// Old
var currentRepoVC: () -> RepoViewController?
init(config: Config, currentRepoVC: @escaping () -> RepoViewController?) {
    self.config = config
    self.currentRepoVC = currentRepoVC
}
```

With:

```swift
var activeSplitContainer: () -> SplitContainerView?
init(config: Config, activeSplitContainer: @escaping () -> SplitContainerView?) {
    self.config = config
    self.activeSplitContainer = activeSplitContainer
}
```

- [ ] **Step 2: Update splitFocusedPane to use new closure**

In `splitFocusedPane(axis:)` (line ~50-86), replace:

```swift
guard let repoVC = currentRepoVC(), let splitContainer = repoVC.activeSplitContainer else { return }
```

With:

```swift
guard let splitContainer = activeSplitContainer() else { return }
```

Also update the tree lookup — currently it may get the tree from repoVC. Ensure it uses `splitContainer.tree` directly:

```swift
guard let tree = splitContainer.tree else { return }
```

- [ ] **Step 3: Update closeFocusedPane similarly**

In `closeFocusedPane()` (line ~88-129), replace:

```swift
guard let repoVC = currentRepoVC(), let splitContainer = repoVC.activeSplitContainer else { return }
```

With:

```swift
guard let splitContainer = activeSplitContainer() else { return }
```

- [ ] **Step 4: Update moveFocus, resizeSplit, resetSplitRatio**

In `moveFocus` (line ~131), `resizeSplit` (line ~144), and `resetSplitRatio` (line ~163), replace all `currentRepoVC()?.activeSplitContainer` references with `activeSplitContainer()`.

- [ ] **Step 5: Build and verify**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -20`
Expected: BUILD SUCCEEDED

- [ ] **Step 6: Commit**

```bash
git add Sources/App/TerminalCoordinator.swift
git commit -m "refactor: retarget TerminalCoordinator from RepoVC to dashboard split container"
```

---

### Task 3: MainWindowController — Update Coordinator Wiring and Key Handling

**Files:**
- Modify: `Sources/App/MainWindowController.swift`

- [ ] **Step 1: Update TerminalCoordinator initialization**

In the lazy `terminalCoordinator` property (line ~62-71), replace the `currentRepoVC` closure with `activeSplitContainer`:

```swift
private lazy var terminalCoordinator: TerminalCoordinator = {
    let tc = TerminalCoordinator(config: config, activeSplitContainer: { [weak self] in
        self?.tabCoordinator.dashboardVC.activeSplitContainer
    })
    tc.delegate = self
    tc.requestSave = { [weak self] in
        self?.tabCoordinator.saveConfig()
    }
    return tc
}()
```

- [ ] **Step 2: Wire surfaceManager and splitContainerDelegate on dashboard**

After TabCoordinator creates dashboardVC, wire the new properties. In TabCoordinator setup or wherever dashboardVC is configured, add:

```swift
dashboardVC.surfaceManager = terminalCoordinator.surfaceManager
dashboardVC.splitContainerDelegate = self  // MainWindowController forwards to TerminalCoordinator
```

If MainWindowController already conforms to SplitContainerDelegate (via RepoViewDelegate), keep it. Otherwise add conformance that forwards to TerminalCoordinator.

- [ ] **Step 3: Update AmuxWindow key handling**

In `AmuxWindow.performKeyEquivalent` (line ~516-569), the guard on line ~524 currently checks `tabCoordinator.activeTabIndex > 0` (only handles splits in repo tabs). Change to always allow split keybindings when a split container is active:

```swift
// Old: guard tabCoordinator.activeTabIndex > 0 ... else { return super... }
// New: check if dashboard has an active split container
guard tabCoordinator.dashboardVC.activeSplitContainer != nil else {
    return super.performKeyEquivalent(with: event)
}
```

Remove the condition that limits split shortcuts to repo tabs only.

- [ ] **Step 4: Remove RepoViewDelegate conformance**

Delete the `RepoViewDelegate` extension (lines ~724-736) from MainWindowController. These methods (`didRequestDeleteWorktree`, `didRequestNewThread`, `didRequestShowDiff`) need to be handled differently now — worktree deletion is triggered from dashboard card context menu, new thread from title bar [+] button.

Move `confirmAndDeleteWorktree` to be callable from the dashboard card context (it likely already exists as a standalone method).

- [ ] **Step 5: Build and verify**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -20`
Expected: BUILD SUCCEEDED

- [ ] **Step 6: Commit**

```bash
git add Sources/App/MainWindowController.swift
git commit -m "refactor: update MainWindowController wiring for dashboard-only split operations"
```

---

### Task 4: TabCoordinator — Remove Repo Tab Logic

**Files:**
- Modify: `Sources/App/TabCoordinator.swift`

- [ ] **Step 1: Remove repoVCs property and getOrCreateRepoVC**

Delete:
- `repoVCs: [String: RepoViewController]` (line ~17)
- `getOrCreateRepoVC(for:)` method (lines ~130-141)
- `currentRepoVC` computed property (lines ~70-77)

- [ ] **Step 2: Simplify switchToTab**

The `switchToTab(_ index:)` method (lines ~81-113) currently handles both dashboard (index 0) and repo tabs (index > 0). Simplify to always show dashboard. If tab-switching concept is kept for future use, reduce to a no-op or remove entirely. The minimal change:

```swift
func switchToTab(_ index: Int) {
    // Always show dashboard — repo tabs removed
    let dashVC = dashboardVC
    detachDashboardTerminals()
    delegate?.tabCoordinator(self, embedViewController: dashVC)
    delegate?.tabCoordinatorDidSwitchTab(self)
}
```

- [ ] **Step 3: Remove repoViewDelegate property**

Delete `weak var repoViewDelegate: RepoViewDelegate?` and any assignment in init or setup code.

- [ ] **Step 4: Clean up workspace loading**

In `loadWorkspaces()` (line ~303-430), remove code that creates repo tabs in `WorkspaceManager`. Keep workspace/worktree discovery and agent building — just don't create tabs for repos.

- [ ] **Step 5: Simplify TabCoordinatorDelegate protocol**

In the protocol (lines ~3-10), remove methods that are repo-tab-specific if any. Keep `embedViewController`, `didSwitchTab`, `requestUpdateTitleBar`.

- [ ] **Step 6: Build and verify**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -20`
Expected: BUILD SUCCEEDED

- [ ] **Step 7: Commit**

```bash
git add Sources/App/TabCoordinator.swift
git commit -m "refactor: remove repo tab logic from TabCoordinator"
```

---

### Task 5: TitleBarView — Left Capsule Worktree Info

**Files:**
- Modify: `Sources/UI/TitleBar/TitleBarView.swift`

- [ ] **Step 1: Replace left arc block internals**

In `setupLeftArcBlock()` (line ~168-232), remove:
- `dashboardTab` button
- `leftSeparator2`
- `tabsScrollView` / `tabsStack`
- `addButton`

Replace with worktree info elements:

```swift
// Left arc block properties (replace existing tab properties around line 39-44)
private let worktreeStatusDot = NSView()
private let worktreeBranchLabel = NSTextField(labelWithString: "")
private let worktreeRepoLabel = NSTextField(labelWithString: "")
private let worktreeMetaLabel = NSTextField(labelWithString: "")
private let newWorktreeButton = NSButton()
private let collapseSidebarButton = NSButton()

// Grid-only fallback
private let dashboardTitleLabel = NSTextField(labelWithString: "AMUX Dashboard")
```

- [ ] **Step 2: Build left arc block layout**

New `setupLeftArcBlock()`:

```swift
private func setupLeftArcBlock() {
    // Status dot (8px circle)
    worktreeStatusDot.wantsLayer = true
    worktreeStatusDot.layer?.cornerRadius = 4
    worktreeStatusDot.translatesAutoresizingMaskIntoConstraints = false

    // Branch name (bold)
    worktreeBranchLabel.font = .systemFont(ofSize: 12, weight: .semibold)
    worktreeBranchLabel.textColor = Theme.titleBarText
    worktreeBranchLabel.lineBreakMode = .byTruncatingTail
    worktreeBranchLabel.translatesAutoresizingMaskIntoConstraints = false

    // Repo name (dimmed)
    worktreeRepoLabel.font = .systemFont(ofSize: 11)
    worktreeRepoLabel.textColor = Theme.titleBarTextDimmed
    worktreeRepoLabel.translatesAutoresizingMaskIntoConstraints = false

    // Status + agent (dimmed)
    worktreeMetaLabel.font = .systemFont(ofSize: 11)
    worktreeMetaLabel.textColor = Theme.titleBarTextDimmed
    worktreeMetaLabel.lineBreakMode = .byTruncatingTail
    worktreeMetaLabel.translatesAutoresizingMaskIntoConstraints = false

    // Dashboard title (shown only in grid mode)
    dashboardTitleLabel.font = .systemFont(ofSize: 12, weight: .medium)
    dashboardTitleLabel.textColor = Theme.titleBarTextDimmed
    dashboardTitleLabel.translatesAutoresizingMaskIntoConstraints = false
    dashboardTitleLabel.isHidden = true

    // New worktree button (+)
    configureIconButton(newWorktreeButton, symbolName: "plus", size: 14)
    newWorktreeButton.target = self
    newWorktreeButton.action = #selector(newWorktreeClicked)

    // Collapse sidebar button (≡)
    configureIconButton(collapseSidebarButton, symbolName: "sidebar.right", size: 14)
    collapseSidebarButton.target = self
    collapseSidebarButton.action = #selector(collapseSidebarClicked)

    // Add to left arc block
    let infoStack = NSStackView(views: [worktreeStatusDot, worktreeBranchLabel, makeSeparatorDot(), worktreeRepoLabel, makeSeparatorDot(), worktreeMetaLabel])
    infoStack.orientation = .horizontal
    infoStack.spacing = 6
    infoStack.alignment = .centerY
    infoStack.translatesAutoresizingMaskIntoConstraints = false

    let buttonStack = NSStackView(views: [newWorktreeButton, collapseSidebarButton])
    buttonStack.orientation = .horizontal
    buttonStack.spacing = 2
    buttonStack.translatesAutoresizingMaskIntoConstraints = false

    leftArcBlock.addSubview(dashboardTitleLabel)
    leftArcBlock.addSubview(infoStack)
    leftArcBlock.addSubview(buttonStack)

    NSLayoutConstraint.activate([
        worktreeStatusDot.widthAnchor.constraint(equalToConstant: 8),
        worktreeStatusDot.heightAnchor.constraint(equalToConstant: 8),

        dashboardTitleLabel.leadingAnchor.constraint(equalTo: leftArcBlock.leadingAnchor, constant: 12),
        dashboardTitleLabel.centerYAnchor.constraint(equalTo: leftArcBlock.centerYAnchor),

        infoStack.leadingAnchor.constraint(equalTo: leftArcBlock.leadingAnchor, constant: 12),
        infoStack.centerYAnchor.constraint(equalTo: leftArcBlock.centerYAnchor),
        infoStack.trailingAnchor.constraint(lessThanOrEqualTo: buttonStack.leadingAnchor, constant: -8),

        buttonStack.trailingAnchor.constraint(equalTo: leftArcBlock.trailingAnchor, constant: -8),
        buttonStack.centerYAnchor.constraint(equalTo: leftArcBlock.centerYAnchor),
    ])
}

private func makeSeparatorDot() -> NSTextField {
    let dot = NSTextField(labelWithString: "·")
    dot.font = .systemFont(ofSize: 11)
    dot.textColor = Theme.titleBarTextDimmed.withAlphaComponent(0.4)
    dot.translatesAutoresizingMaskIntoConstraints = false
    return dot
}
```

- [ ] **Step 3: Add update method for worktree info**

```swift
func updateWorktreeInfo(branch: String?, repo: String?, status: AgentStatus?, agentName: String?, isGridLayout: Bool) {
    let showWorktreeInfo = !isGridLayout && branch != nil
    dashboardTitleLabel.isHidden = showWorktreeInfo
    worktreeStatusDot.isHidden = !showWorktreeInfo
    worktreeBranchLabel.isHidden = !showWorktreeInfo
    worktreeRepoLabel.isHidden = !showWorktreeInfo
    worktreeMetaLabel.isHidden = !showWorktreeInfo
    newWorktreeButton.isHidden = !showWorktreeInfo
    collapseSidebarButton.isHidden = !showWorktreeInfo

    if showWorktreeInfo {
        worktreeBranchLabel.stringValue = branch ?? ""
        worktreeRepoLabel.stringValue = repo ?? ""
        let statusText = status?.displayName ?? "Unknown"
        let agentText = agentName ?? ""
        worktreeMetaLabel.stringValue = agentText.isEmpty ? statusText : "\(statusText) · \(agentText)"
        worktreeStatusDot.layer?.backgroundColor = (status ?? .unknown).color.cgColor
    }
}
```

- [ ] **Step 4: Add button action stubs**

```swift
@objc private func newWorktreeClicked() {
    delegate?.titleBarDidRequestNewThread()
}

@objc private func collapseSidebarClicked() {
    delegate?.titleBarDidRequestCollapseSidebar()
}
```

- [ ] **Step 5: Update TitleBarDelegate protocol**

Add the new method and remove tab-specific methods:

```swift
protocol TitleBarDelegate: AnyObject {
    // Removed: titleBarDidSelectDashboard, titleBarDidSelectProject, titleBarDidRequestCloseProject, titleBarDidRequestAddProject
    func titleBarDidRequestNewThread()
    func titleBarDidSelectLayout(_ layout: DashboardLayout)
    func titleBarDidToggleNotifications()
    func titleBarDidToggleAI()
    func titleBarDidToggleTheme()
    func titleBarDidRequestCollapseSidebar()
}
```

- [ ] **Step 6: Delete ProjectTabView inner class**

Remove the entire `ProjectTabView` class (lines ~576-762) and `renderTabs()` method (lines ~80-119).

- [ ] **Step 7: Build and verify**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -20`
Expected: BUILD SUCCEEDED

- [ ] **Step 8: Commit**

```bash
git add Sources/UI/TitleBar/TitleBarView.swift
git commit -m "feat: replace title bar tab list with worktree info capsule"
```

---

### Task 6: Sidebar Collapse Toggle

**Files:**
- Modify: `Sources/UI/Dashboard/DashboardViewController.swift`

- [ ] **Step 1: Add collapse state property**

```swift
private var isSidebarCollapsed = false
```

- [ ] **Step 2: Add toggleSidebarCollapse method**

```swift
func toggleSidebarCollapse() {
    guard currentLayout != .grid else { return }
    isSidebarCollapsed.toggle()

    guard let refs = focusLayoutRefs(for: currentLayout) else { return }

    NSAnimationContext.runAnimationGroup { context in
        context.duration = 0.2
        context.allowsImplicitAnimation = true

        refs.scrollView.animator().isHidden = isSidebarCollapsed
        refs.scrollView.animator().alphaValue = isSidebarCollapsed ? 0 : 1

        self.view.layoutSubtreeIfNeeded()
    }
}
```

Note: The exact animation depends on layout type. For leftRight the scroll view is the right sidebar; for topSmall/topLarge it's the mini card row. Since `focusLayoutRefs` already returns the correct `scrollView` per layout, hiding it will cause Auto Layout to redistribute space to the focus panel.

- [ ] **Step 3: Reset collapse state on layout change**

In `setLayout(_:)` (line ~253), add:

```swift
isSidebarCollapsed = false
```

- [ ] **Step 4: Wire from MainWindowController**

In MainWindowController's `TitleBarDelegate` implementation, handle the new method:

```swift
func titleBarDidRequestCollapseSidebar() {
    tabCoordinator.dashboardVC.toggleSidebarCollapse()
}
```

- [ ] **Step 5: Build and verify**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -20`
Expected: BUILD SUCCEEDED

- [ ] **Step 6: Commit**

```bash
git add Sources/UI/Dashboard/DashboardViewController.swift Sources/App/MainWindowController.swift
git commit -m "feat: add sidebar collapse toggle for non-grid layouts"
```

---

### Task 7: MainWindowController — Wire Title Bar Updates

**Files:**
- Modify: `Sources/App/MainWindowController.swift`

- [ ] **Step 1: Update updateTitleBar method**

The existing `updateTitleBar()` method currently calls `renderTabs()` and updates project list. Replace with worktree info update:

```swift
private func updateTitleBar() {
    let isGrid = tabCoordinator.dashboardVC.currentLayout == .grid
    let agent = tabCoordinator.selectedAgent  // New computed property on TabCoordinator

    titleBarView.updateWorktreeInfo(
        branch: agent?.name,
        repo: agent?.project,
        status: agent?.status,
        agentName: agent?.agentDef?.cliName,
        isGridLayout: isGrid
    )
}
```

- [ ] **Step 2: Add selectedAgent computed property to TabCoordinator**

In `TabCoordinator.swift`, add:

```swift
/// The currently selected agent in dashboard focus layouts
var selectedAgent: AgentDisplayInfo? {
    let index = dashboardVC.selectedAgentIndex
    let agents = dashboardVC.agents
    guard index < agents.count else { return nil }
    return agents[index]
}
```

- [ ] **Step 3: Call updateTitleBar on agent selection change**

In DashboardViewController, when `selectedAgentIndex` changes (card click), notify the delegate to update the title bar. Add a `DashboardDelegate` method call:

```swift
delegate?.dashboardDidChangeSelection(self)
```

In MainWindowController's DashboardDelegate implementation:

```swift
func dashboardDidChangeSelection(_ dashboard: DashboardViewController) {
    updateTitleBar()
}
```

- [ ] **Step 4: Call updateTitleBar on layout change**

In the layout change handler (TitleBarDelegate.titleBarDidSelectLayout), after setting the layout, call `updateTitleBar()` to toggle between grid/non-grid display.

- [ ] **Step 5: Build and verify**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -20`
Expected: BUILD SUCCEEDED

- [ ] **Step 6: Commit**

```bash
git add Sources/App/MainWindowController.swift Sources/App/TabCoordinator.swift Sources/UI/Dashboard/DashboardViewController.swift
git commit -m "feat: wire title bar worktree info updates on selection and layout change"
```

---

### Task 8: Delete RepoViewController and SidebarViewController

**Files:**
- Delete: `Sources/UI/Repo/RepoViewController.swift`
- Delete: `Sources/UI/Repo/SidebarViewController.swift`
- Modify: `amux.xcodeproj/project.pbxproj` (via xcodegen)
- Modify: `project.yml`

- [ ] **Step 1: Remove files from project.yml**

Check if `project.yml` has explicit file references for Repo/ sources. If it uses a glob like `Sources/**/*.swift`, the deleted files will be excluded automatically. Otherwise remove explicit entries.

- [ ] **Step 2: Delete the files**

```bash
rm Sources/UI/Repo/RepoViewController.swift
rm Sources/UI/Repo/SidebarViewController.swift
```

If the `Sources/UI/Repo/` directory is now empty, remove it:

```bash
rmdir Sources/UI/Repo/
```

- [ ] **Step 3: Remove all remaining references**

Search for remaining references to `RepoViewController`, `SidebarViewController`, `RepoViewDelegate`, `SidebarDelegate` across the codebase and remove them:

- `MainWindowController.swift`: Remove any remaining `RepoViewDelegate` conformance
- `TabCoordinator.swift`: Remove any remaining `RepoViewController` imports or references
- `TerminalCoordinator.swift`: Should already be clean from Task 2

- [ ] **Step 4: Regenerate Xcode project**

```bash
xcodegen generate
```

- [ ] **Step 5: Build and verify**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -20`
Expected: BUILD SUCCEEDED

- [ ] **Step 6: Run tests**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test 2>&1 | tail -30`
Expected: All tests pass. If any tests reference RepoViewController, they need to be updated or removed.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "refactor: delete RepoViewController and SidebarViewController"
```

---

### Task 9: Worktree Deletion from Dashboard

**Files:**
- Modify: `Sources/UI/Dashboard/DashboardViewController.swift`
- Modify: `Sources/App/MainWindowController.swift`

Previously worktree deletion was triggered from RepoViewController sidebar. Now it needs a path from the dashboard.

- [ ] **Step 1: Add deletion to DashboardDelegate**

If not already present, add to `DashboardDelegate`:

```swift
func dashboard(_ dashboard: DashboardViewController, didRequestDeleteWorktree info: WorktreeInfo)
```

- [ ] **Step 2: Wire card context menu or existing delete path**

If cards already have a context menu with delete, wire the delegate call. If not, add a right-click context menu on `StackedCardContainerView` / `StackedMiniCardContainerView` with a "Delete Worktree" option that calls the delegate.

- [ ] **Step 3: Handle deletion in MainWindowController**

In MainWindowController's DashboardDelegate:

```swift
func dashboard(_ dashboard: DashboardViewController, didRequestDeleteWorktree info: WorktreeInfo) {
    confirmAndDeleteWorktree(info)
}
```

After deletion completes, call `dashboard.invalidateSplitContainer(forPath:)` and select the next agent.

- [ ] **Step 4: Build and verify**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -20`
Expected: BUILD SUCCEEDED

- [ ] **Step 5: Commit**

```bash
git add Sources/UI/Dashboard/DashboardViewController.swift Sources/App/MainWindowController.swift
git commit -m "feat: wire worktree deletion from dashboard cards"
```

---

### Task 10: Config and Session Restore Cleanup

**Files:**
- Modify: `Sources/Core/Config.swift`
- Modify: `Sources/App/TabCoordinator.swift`

- [ ] **Step 1: Replace activeTabIndex with selectedWorktreePath in Config**

Currently Config stores `activeTabIndex` for session restore. Replace with the selected worktree path for non-grid layouts:

```swift
// In Config struct, replace:
// var activeTabIndex: Int?
// With:
var selectedWorktreePath: String?
```

Keep `decodeIfPresent` for backward compatibility — old configs with `activeTabIndex` will simply ignore it.

- [ ] **Step 2: Save selected worktree on change**

In TabCoordinator, when dashboard selection changes, save the worktree path:

```swift
func saveSelectedWorktree() {
    if let agent = selectedAgent, let path = agent.worktreePaths.first {
        config.selectedWorktreePath = path
    }
    saveConfig()
}
```

- [ ] **Step 3: Restore selected worktree on startup**

In `loadWorkspaces()`, after building agents and configuring dashboard, restore selection:

```swift
if let savedPath = config.selectedWorktreePath {
    dashboardVC.selectAgent(byWorktreePath: savedPath)
}
```

Add `selectAgent(byWorktreePath:)` to DashboardViewController:

```swift
func selectAgent(byWorktreePath path: String) {
    guard let index = agents.firstIndex(where: { $0.worktreePaths.contains(path) }) else { return }
    selectedAgentIndex = index
    embedSplitContainerForSelectedAgent()
}
```

- [ ] **Step 4: Build and verify**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -20`
Expected: BUILD SUCCEEDED

- [ ] **Step 5: Run all tests**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test 2>&1 | tail -30`
Expected: All tests pass

- [ ] **Step 6: Commit**

```bash
git add Sources/Core/Config.swift Sources/App/TabCoordinator.swift Sources/UI/Dashboard/DashboardViewController.swift
git commit -m "feat: persist and restore selected worktree for dashboard"
```
