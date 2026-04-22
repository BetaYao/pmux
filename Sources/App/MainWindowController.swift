import AppKit

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
        if arguments.contains("-AmuxUITesting") {
            return false
        }
        if let idx = arguments.firstIndex(of: "-ApplePersistenceIgnoreState"),
           arguments.indices.contains(idx + 1),
           arguments[idx + 1].caseInsensitiveCompare("YES") == .orderedSame {
            return false
        }
        return true
    }

    static func shouldHandleEscShortcut() -> Bool {
        false
    }

    static func trafficLightButtonOriginY(containerHeight: CGFloat, buttonHeight: CGFloat) -> CGFloat {
        (containerHeight / 2) + TitleBarView.Layout.arcVerticalOffset - (buttonHeight / 2)
    }
}

class MainWindowController: NSWindowController {
    private static let primaryCapsuleDismissDelay: TimeInterval = 1.0
    private static let primaryCapsuleDisplayDuration: TimeInterval = 8.0

    private let titleBar = TitleBarView()
    private let backgroundEffectView = NSVisualEffectView()
    private let contentContainer = NSView()
    private var windowTrackingArea: NSTrackingArea?
    private lazy var panelCoordinator: PanelCoordinator = {
        let pc = PanelCoordinator()
        pc.delegate = self
        pc.titleBar = titleBar
        return pc
    }()
    private let titleBarAccessory = NSTitlebarAccessoryViewController()

    private var dashboardVC: DashboardViewController?
    private var config = Config.load()
    private var runtimeBackend: String = "zmx"
    private var primaryCapsuleNotification: NotificationEntry?
    private var dismissedPrimaryCapsuleNotificationIDs: Set<UUID> = []
    private var primaryCapsuleDismissWorkItem: DispatchWorkItem?

    // Terminal management
    private lazy var terminalCoordinator: TerminalCoordinator = {
        let tc = TerminalCoordinator(config: config, activeSplitContainer: { [weak self] in
            self?.tabCoordinator.dashboardVC?.activeSplitContainer
        })
        tc.delegate = self
        return tc
    }()

    // Tab/workspace management
    lazy var tabCoordinator: TabCoordinator = {
        let tc = TabCoordinator(config: config)
        tc.delegate = self
        tc.terminalCoordinator = terminalCoordinator
        tc.statusPublisher = statusPublisher
        tc.statusAggregator = statusAggregator
        tc.runtimeBackend = runtimeBackend
        tc.panelCoordinator = panelCoordinator
        return tc
    }()

    // Dialog presentation
    private lazy var dialogPresenter: DialogPresenter = {
        DialogPresenter(
            tabCoordinator: tabCoordinator,
            terminalCoordinator: terminalCoordinator,
            statusPublisher: statusPublisher
        )
    }()

    // Auto-update
    private lazy var updateCoordinator: UpdateCoordinator = {
        let uc = UpdateCoordinator(config: config)
        uc.delegate = self
        uc.banner.delegate = uc
        return uc
    }()

    // Status detection
    private let statusAggregator = WorktreeStatusAggregator()
    private lazy var statusPublisher: StatusPublisher = {
        let pub = StatusPublisher(agentConfig: config.agentDetect)
        pub.aggregator = statusAggregator
        statusAggregator.delegate = self
        return pub
    }()

    convenience init() {
        let window = AmuxWindow(
            contentRect: NSRect(x: 0, y: 0, width: 1200, height: 800),
            styleMask: [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView],
            backing: .buffered,
            defer: false
        )
        window.title = "amux"
        window.minSize = NSSize(width: 600, height: 400)
        window.titlebarAppearsTransparent = true
        window.titleVisibility = .hidden
        window.isOpaque = false
        window.backgroundColor = .clear

        // Set window appearance from config (already applied globally in main.swift)
        window.appearance = NSApp.appearance

        self.init(window: window)

        // Prevent macOS from creating duplicate windows via state restoration
        window.isRestorable = false

        if WindowStyling.shouldUseWindowFrameAutosave() {
            window.setFrameAutosaveName("AmuxMainWindow")
        } else if let visibleFrame = NSScreen.main?.visibleFrame {
            let width = min(1200, visibleFrame.width * 0.9)
            let height = min(800, visibleFrame.height * 0.9)
            let x = visibleFrame.midX - (width / 2)
            let y = visibleFrame.midY - (height / 2)
            window.setFrame(NSRect(x: x, y: y, width: width, height: height), display: false)
        }
        window.delegate = self

        setupMenuShortcuts()
        setupLayout()
        updateCoordinator.setup(config: config)
        normalizeBackendAvailabilityIfNeeded()
        tabCoordinator.loadWorkspaces()

        NotificationCenter.default.addObserver(
            self, selector: #selector(handleNavigateToWorktree(_:)),
            name: .navigateToWorktree, object: nil
        )
        NotificationCenter.default.addObserver(
            self, selector: #selector(handleNotificationHistoryDidChange(_:)),
            name: .notificationHistoryDidChange, object: nil
        )
        handleNotificationHistoryDidChange(nil)
    }

    /// Sync split layouts from TerminalCoordinator before saving config.
    /// Config is a value type — without syncing, saves here overwrite
    /// splitLayouts written by TerminalCoordinator with stale data.
    private func saveConfig() {
        // Sync fields that TabCoordinator may have updated independently
        config.workspacePaths = tabCoordinator.config.workspacePaths
        config.cardOrder = tabCoordinator.config.cardOrder
        config.worktreeStartedAt = tabCoordinator.config.worktreeStartedAt
        config.selectedWorktreePath = tabCoordinator.config.selectedWorktreePath
        config.splitLayouts = terminalCoordinator.config.splitLayouts
        config.save()
    }

    // MARK: - Menu Shortcuts

    private func setupMenuShortcuts() {
        NSApp.mainMenu = MenuBuilder.buildMainMenu(target: self)
    }

    private func normalizeBackendAvailabilityIfNeeded() {
        BackendResolver.resolveAsync(preferred: config.backend) { [weak self] resolution in
            guard let self else { return }
            self.runtimeBackend = resolution.backend
            self.tabCoordinator.runtimeBackend = resolution.backend
            if resolution.warningMessage == nil, resolution.backend != self.config.backend {
                self.config.backend = resolution.backend
                self.saveConfig()
            }
            BackendResolver.showWarningIfNeeded(resolution, configBackend: self.config.backend)
        }
    }

    @objc func switchToDashboard() {
        switchToTab(0)
    }

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

    @objc func closeCurrentTab() {
        // No-op: dashboard is always the only tab; individual project close is handled via dashboard UI.
    }

    /// Cmd+W: close focused pane if multiple panes, otherwise close tab.
    @objc func closePaneOrTab() {
        if let tree = tabCoordinator.dashboardVC?.activeSplitContainer?.tree,
           tree.leafCount > 1 {
            closeFocusedPane()
        } else {
            closeCurrentTab()
        }
    }

    @objc func selectNextTab() {
        // No-op: only the dashboard tab exists.
    }

    @objc func selectPreviousTab() {
        // No-op: only the dashboard tab exists.
    }

    @objc func showKeyboardShortcuts() {
        DialogPresenter.showKeyboardShortcuts()
    }

    @objc func openDocumentation() {
        let repositoryURL = URL(string: "https://github.com/\(UpdateChecker.repositoryOwner)/\(UpdateChecker.repositoryName)")!
        NSWorkspace.shared.open(repositoryURL)
    }

    @objc func dashboardZoomIn() {
        dashboardVC?.zoomIn()
        let zoom = dashboardVC?.zoomIndex ?? GridLayout.defaultZoomIndex
        config.zoomIndex = zoom
        tabCoordinator.config.zoomIndex = zoom
        saveConfig()
    }

    @objc func dashboardZoomOut() {
        dashboardVC?.zoomOut()
        let zoom = dashboardVC?.zoomIndex ?? GridLayout.defaultZoomIndex
        config.zoomIndex = zoom
        tabCoordinator.config.zoomIndex = zoom
        saveConfig()
    }

    @objc func showDiffOverlay() {
        // Use the currently selected agent's worktree path from the dashboard.
        guard let path = tabCoordinator.selectedAgent?.worktreePath else { return }
        presentDiffOverlay(for: path)
    }

    private func presentDiffOverlay(for worktreePath: String) {
        let diffVC = DiffOverlayViewController(worktreePath: worktreePath)
        dialogPresenter.presentSheetOnActiveVC(diffVC, tabCoordinator: tabCoordinator, dashboardVC: dashboardVC)
    }

    // MARK: - Layout

    private func setupLayout() {
        guard let contentView = window?.contentView else { return }

        setupNativeTitleBar()

        // Update banner (above title bar, hidden by default)
        updateCoordinator.banner.translatesAutoresizingMaskIntoConstraints = false
        updateCoordinator.banner.isHidden = true
        contentView.addSubview(updateCoordinator.banner)

        backgroundEffectView.translatesAutoresizingMaskIntoConstraints = false
        backgroundEffectView.state = .followsWindowActiveState
        contentView.addSubview(backgroundEffectView, positioned: .below, relativeTo: nil)

        // Content container (fills middle)
        contentContainer.wantsLayer = true
        contentContainer.translatesAutoresizingMaskIntoConstraints = false
        contentView.addSubview(contentContainer)

        NSLayoutConstraint.activate([
            backgroundEffectView.topAnchor.constraint(equalTo: contentView.topAnchor),
            backgroundEffectView.leadingAnchor.constraint(equalTo: contentView.leadingAnchor),
            backgroundEffectView.trailingAnchor.constraint(equalTo: contentView.trailingAnchor),
            backgroundEffectView.bottomAnchor.constraint(equalTo: contentView.bottomAnchor),

            updateCoordinator.banner.topAnchor.constraint(equalTo: contentView.topAnchor),
            updateCoordinator.banner.leadingAnchor.constraint(equalTo: contentView.leadingAnchor),
            updateCoordinator.banner.trailingAnchor.constraint(equalTo: contentView.trailingAnchor),

            contentContainer.topAnchor.constraint(equalTo: updateCoordinator.banner.bottomAnchor),
            contentContainer.leadingAnchor.constraint(equalTo: contentView.leadingAnchor),
            contentContainer.trailingAnchor.constraint(equalTo: contentView.trailingAnchor),
            contentContainer.bottomAnchor.constraint(equalTo: contentView.bottomAnchor),
        ])

        // Window hover tracking for arc block styling
        setupWindowHoverTracking(contentView: contentView)

        panelCoordinator.setupPopovers()

        // Create dashboard
        let savedLayout = DashboardLayout(rawValue: config.dashboardLayout) ?? .leftRight
        let dashboard = DashboardViewController()
        dashboard.dashboardDelegate = self
        dashboard.currentLayout = savedLayout
        dashboard.setZoomIndex(config.zoomIndex)
        dashboard.surfaceManager = terminalCoordinator.surfaceManager
        dashboard.splitContainerDelegate = self
        dashboardVC = dashboard
        tabCoordinator.dashboardVC = dashboard

        embedViewController(dashboard)
        updateTitleBar()


        // Set title bar layout state
        titleBar.setCurrentLayout(savedLayout)

        applyWindowBackgroundStyle()
        positionStandardWindowButtons()
    }


    private func applyWindowBackgroundStyle() {
        guard let window else { return }
        let isDark = window.effectiveAppearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
        let config = WindowStyling.glassBackgroundConfig(isDark: isDark)

        backgroundEffectView.material = config.material
        backgroundEffectView.blendingMode = config.blendingMode
        backgroundEffectView.isHidden = !config.enabled

        window.isOpaque = !config.enabled
        window.backgroundColor = config.enabled ? .clear : Theme.background
    }

    private func setupNativeTitleBar() {
        guard let window else { return }

        let toolbar = NSToolbar(identifier: "amux.mainToolbar")
        toolbar.displayMode = .iconOnly
        toolbar.showsBaselineSeparator = false
        window.toolbar = toolbar
        window.toolbarStyle = .unified

        titleBar.delegate = self
        titleBar.translatesAutoresizingMaskIntoConstraints = false

        let accessoryContainer = NSView(frame: NSRect(x: 0, y: 0, width: 860, height: TitleBarView.Layout.barHeight))
        accessoryContainer.translatesAutoresizingMaskIntoConstraints = false
        accessoryContainer.addSubview(titleBar)
        NSLayoutConstraint.activate([
            titleBar.leadingAnchor.constraint(equalTo: accessoryContainer.leadingAnchor),
            titleBar.trailingAnchor.constraint(equalTo: accessoryContainer.trailingAnchor),
            titleBar.topAnchor.constraint(equalTo: accessoryContainer.topAnchor),
            titleBar.bottomAnchor.constraint(equalTo: accessoryContainer.bottomAnchor),
            accessoryContainer.widthAnchor.constraint(greaterThanOrEqualToConstant: 860),
        ])

        titleBarAccessory.view = accessoryContainer
        titleBarAccessory.fullScreenMinHeight = TitleBarView.Layout.barHeight
        titleBarAccessory.layoutAttribute = .top
        if !window.titlebarAccessoryViewControllers.contains(where: { $0 === titleBarAccessory }) {
            window.addTitlebarAccessoryViewController(titleBarAccessory)
        }

        DispatchQueue.main.async { [weak self] in
            self?.positionStandardWindowButtons()
        }
    }

    private func positionStandardWindowButtons() {
        guard let window else { return }
        guard let close = window.standardWindowButton(.closeButton),
              let mini = window.standardWindowButton(.miniaturizeButton),
              let zoom = window.standardWindowButton(.zoomButton),
              let container = close.superview
        else {
            return
        }

        let xOffset: CGFloat = 12
        let spacing: CGFloat = 6

        let y = WindowStyling.trafficLightButtonOriginY(containerHeight: container.bounds.height, buttonHeight: close.frame.height)
        close.setFrameOrigin(NSPoint(x: xOffset, y: y))
        mini.setFrameOrigin(NSPoint(x: xOffset + close.frame.width + spacing, y: y))
        zoom.setFrameOrigin(NSPoint(x: xOffset + (close.frame.width + spacing) * 2, y: y))
    }

    private func setupWindowHoverTracking(contentView: NSView) {
        let area = NSTrackingArea(
            rect: .zero,
            options: [.mouseEnteredAndExited, .activeAlways, .inVisibleRect],
            owner: self,
            userInfo: nil
        )
        contentView.addTrackingArea(area)
        windowTrackingArea = area
    }

    override func mouseEntered(with event: NSEvent) {
        titleBar.setWindowHovered(true)
    }

    override func mouseExited(with event: NSEvent) {
        titleBar.setWindowHovered(false)
    }

    private func embedViewController(_ vc: NSViewController) {
        for child in contentContainer.subviews {
            child.removeFromSuperview()
        }

        vc.view.translatesAutoresizingMaskIntoConstraints = false
        contentContainer.addSubview(vc.view)
        NSLayoutConstraint.activate([
            vc.view.topAnchor.constraint(equalTo: contentContainer.topAnchor),
            vc.view.leadingAnchor.constraint(equalTo: contentContainer.leadingAnchor),
            vc.view.trailingAnchor.constraint(equalTo: contentContainer.trailingAnchor),
            vc.view.bottomAnchor.constraint(equalTo: contentContainer.bottomAnchor),
        ])
    }

    private func updateTitleBar() {
        let isGrid = tabCoordinator.dashboardVC?.currentLayout == .grid

        titleBar.updateChromeState(
            isGridLayout: isGrid,
            hasWorkspaces: !tabCoordinator.workspaceManager.tabs.isEmpty
        )
        updatePrimaryCapsuleNotification()
    }


    // MARK: - Forwarding to TabCoordinator

    @discardableResult
    func integrateDiscoveredRepoForTesting(repoPath: String, worktrees: [WorktreeInfo], activateTab: Bool = true) -> Int {
        tabCoordinator.integrateDiscoveredRepo(repoPath: repoPath, worktrees: worktrees, activateTab: activateTab)
    }

    private func switchToTab(_ index: Int) {
        tabCoordinator.switchToTab(index)
    }

    private func confirmAndDeleteWorktree(_ info: WorktreeInfo) {
        terminalCoordinator.confirmAndDeleteWorktree(info, window: window)
    }

    private func worktreeDidDelete(_ info: WorktreeInfo) {
        tabCoordinator.worktreeDidDelete(info)
    }

    deinit {
        primaryCapsuleDismissWorkItem?.cancel()
        NotificationCenter.default.removeObserver(self, name: .navigateToWorktree, object: nil)
        NotificationCenter.default.removeObserver(self, name: .notificationHistoryDidChange, object: nil)
    }

    // MARK: - Split Pane Actions (forwarded to TerminalCoordinator)

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

}

class AmuxWindow: NSWindow {

    // performKeyEquivalent runs BEFORE menu item key equivalents,
    // so split pane shortcuts here take priority over menu bindings.
    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        guard event.type == .keyDown else { return super.performKeyEquivalent(with: event) }
        let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
        guard let mwc = windowController as? MainWindowController else {
            return super.performKeyEquivalent(with: event)
        }

        // Only handle split keybindings when dashboard has an active split container
        let hasSplitContext = mwc.tabCoordinator.dashboardVC?.activeSplitContainer != nil

        // Arrow keys carry .numericPad and .function flags on macOS; strip them
        // so modifier comparisons match what the user actually pressed.
        let baseFlags = flags.subtracting([.numericPad, .function])

        if hasSplitContext {
            // Cmd+D: horizontal split
            if flags == .command && event.charactersIgnoringModifiers == "d" {
                mwc.splitFocusedPane(axis: .horizontal)
                return true
            }

            // Cmd+Shift+D: vertical split
            if flags == [.command, .shift] && event.charactersIgnoringModifiers?.lowercased() == "d" {
                mwc.splitFocusedPane(axis: .vertical)
                return true
            }

            // Cmd+Option+Arrows: focus navigation
            if baseFlags == [.command, .option] {
                switch event.keyCode {
                case 123: mwc.moveFocus(.horizontal, positive: false); return true
                case 124: mwc.moveFocus(.horizontal, positive: true); return true
                case 125: mwc.moveFocus(.vertical, positive: true); return true
                case 126: mwc.moveFocus(.vertical, positive: false); return true
                default: break
                }
            }

            // Cmd+Ctrl+Arrows: resize
            if baseFlags == [.command, .control] {
                switch event.keyCode {
                case 123: mwc.resizeSplit(.horizontal, delta: -0.05); return true
                case 124: mwc.resizeSplit(.horizontal, delta: 0.05); return true
                case 125: mwc.resizeSplit(.vertical, delta: 0.05); return true
                case 126: mwc.resizeSplit(.vertical, delta: -0.05); return true
                default: break
                }
            }

            // Cmd+Ctrl+=: reset ratio
            if flags == [.command, .control] && event.charactersIgnoringModifiers == "=" {
                mwc.resetSplitRatio()
                return true
            }
        }

        // Cmd+B: toggle sidebar collapse
        if flags == .command && event.charactersIgnoringModifiers == "b" {
            mwc.tabCoordinator.dashboardVC?.toggleSidebarCollapse()
            return true
        }

        // Cmd+Shift+F: show diff overlay
        if flags == [.command, .shift] && event.charactersIgnoringModifiers?.lowercased() == "f" {
            mwc.showDiffOverlay()
            return true
        }

        // Cmd+1..4: switch dashboard layout.
        if flags == .command, let chars = event.charactersIgnoringModifiers {
            let layoutMap: [String: DashboardLayout] = [
                "1": .grid,
                "2": .leftRight,
                "3": .topSmall,
                "4": .topLarge
            ]
            if let target = layoutMap[chars], let dashVC = mwc.tabCoordinator.dashboardVC {
                if dashVC.isInDStateForWindow {
                    dashVC.exitDashboardNavigation(restoreSnapshot: true)
                }
                dashVC.setLayout(target)
                if target == .grid {
                    dashVC.enterDashboardNavigation()
                }
                return true
            }
        }

        // Cmd+J: toggle D-state in focus layouts. No-op in grid (already in D).
        if flags == .command && event.charactersIgnoringModifiers == "j" {
            if let dashVC = mwc.tabCoordinator.dashboardVC {
                if dashVC.currentLayout == .grid {
                    return true  // swallow, no-op
                }
                if dashVC.isInDStateForWindow {
                    dashVC.exitDashboardNavigation(restoreSnapshot: true)
                } else {
                    dashVC.enterDashboardNavigation()
                }
                return true
            }
        }

        return super.performKeyEquivalent(with: event)
    }

    override func sendEvent(_ event: NSEvent) {
        if event.type == .keyDown {
            // Escape: exit spotlight (existing)
            if event.keyCode == 53, WindowStyling.shouldHandleEscShortcut() {
                return
            }
        }
        super.sendEvent(event)
    }
}

// MARK: - NSWindowDelegate

extension MainWindowController: NSWindowDelegate {
    func windowDidResize(_ notification: Notification) {
        positionStandardWindowButtons()
    }

    func windowDidBecomeKey(_ notification: Notification) {
        applyWindowBackgroundStyle()
        positionStandardWindowButtons()
    }

    func windowDidEndLiveResize(_ notification: Notification) {
        positionStandardWindowButtons()
    }

    func windowDidChangeEffectiveAppearance(_ notification: Notification) {
        applyWindowBackgroundStyle()
    }


    func windowWillClose(_ notification: Notification) {
        statusPublisher.stop()
        tabCoordinator.branchRefreshTimer?.invalidate()
        tabCoordinator.branchRefreshTimer = nil
        terminalCoordinator.cleanup()
    }

    func cleanupBeforeTermination() {
        statusPublisher.stop()
        tabCoordinator.branchRefreshTimer?.invalidate()
        tabCoordinator.branchRefreshTimer = nil
        terminalCoordinator.cleanup()
    }
}

// MARK: - TitleBarDelegate

extension MainWindowController: TitleBarDelegate {
    func titleBarDidRequestNewThread() {
        tabCoordinator.showNewThreadModal(window: window)
    }

    func titleBarDidRequestAddProject() {
        tabCoordinator.addRepoViaOpenPanel(window: window)
    }

    func titleBarDidSelectLayout(_ layout: DashboardLayout) {
        dashboardVC?.setLayout(layout)
        config.dashboardLayout = layout.rawValue
        tabCoordinator.config.dashboardLayout = layout.rawValue
        terminalCoordinator.config.dashboardLayout = layout.rawValue
        updateCoordinator.config.dashboardLayout = layout.rawValue
        saveConfig()
        titleBar.setCurrentLayout(layout)
        updateTitleBar()
    }

    func titleBarDidActivatePrimaryCapsule() {
        guard let entry = primaryCapsuleNotification else { return }
        panelCoordinator.notificationPanelDidSelectItem(entry)
        schedulePrimaryCapsuleDismissal(for: entry)
    }

    func titleBarDidToggleTheme() {
        let isDark = window?.effectiveAppearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
        let next: ThemeMode = isDark ? .light : .dark
        config.themeMode = next.rawValue
        tabCoordinator.config.themeMode = next.rawValue
        terminalCoordinator.config.themeMode = next.rawValue
        updateCoordinator.config.themeMode = next.rawValue
        saveConfig()
        ThemeMode.applyAppearance(next)
        // Window appearance must also be updated since it was set explicitly in init
        switch next {
        case .dark:
            window?.appearance = NSAppearance(named: .darkAqua)
        case .light:
            window?.appearance = NSAppearance(named: .aqua)
        case .system:
            window?.appearance = nil
        }
        // Update NSAppearance.current so .cgColor resolves correctly outside drawing cycles
        NSAppearance.current = window?.effectiveAppearance ?? NSApp.effectiveAppearance
        applyWindowBackgroundStyle()
    }

    func titleBarDidRequestCollapseSidebar() {
        tabCoordinator.dashboardVC?.toggleSidebarCollapse()
    }
}

// MARK: - DashboardDelegate

extension MainWindowController: DashboardDelegate {
    func dashboardDidSelectProject(_ project: String, thread: String) {
        tabCoordinator.dashboardDidSelectProject(project, thread: thread)
    }

    func dashboardDidRequestEnterProject(_ project: String) {
        tabCoordinator.dashboardDidRequestEnterProject(project)
    }

    func dashboardDidReorderCards(order: [String]) {
        config.cardOrder = order
        tabCoordinator.config.cardOrder = order
        saveConfig()
    }

    func dashboardDidRequestCloseRepo(_ project: String) {
        tabCoordinator.showCloseProjectModal(project, window: window)
    }

    func dashboardDidRequestDelete(_ terminalID: String) {
        tabCoordinator.dashboardDidRequestDelete(terminalID, window: window)
    }

    func dashboardDidRequestAddProject() {
        tabCoordinator.addRepoViaOpenPanel(window: window)
    }

    func dashboardDidChangeSelection(_ dashboard: DashboardViewController) {
        updateTitleBar()
        tabCoordinator.saveSelectedWorktree()
        config.selectedWorktreePath = tabCoordinator.config.selectedWorktreePath
        saveConfig()
    }
}

// MARK: - SplitContainerDelegate

extension MainWindowController: SplitContainerDelegate {
    func splitContainer(_ view: SplitContainerView, didChangeFocus leafId: String) {
        guard let tree = view.tree else { return }
        let worktreePath = tree.worktreePath
        NotificationCenter.default.post(
            name: .repoViewDidChangeFocusedPane,
            object: self,
            userInfo: ["worktreePath": worktreePath, "focusedLeafId": leafId]
        )
    }

    func splitContainer(_ view: SplitContainerView, didRequestSplit axis: SplitAxis) {
        splitFocusedPane(axis: axis)
    }

    func splitContainer(_ view: SplitContainerView, didRequestClosePane leafId: String) {
        closeFocusedPane()
    }

    func splitContainerDidChangeLayout(_ view: SplitContainerView) {
        guard let tree = view.tree else { return }
        terminalCoordinator.saveSplitLayout(tree)
    }
}

// MARK: - PanelCoordinatorDelegate

extension MainWindowController: PanelCoordinatorDelegate {
    func panelCoordinator(_ coordinator: PanelCoordinator, navigateToWorktreePath path: String, paneIndex: Int?) {
        tabCoordinator.handleNavigateToWorktree(worktreePath: path, paneIndex: paneIndex)
    }
}

// MARK: - NewBranchDialogDelegate

extension MainWindowController: NewBranchDialogDelegate {
    func newBranchDialog(_ dialog: NewBranchDialog, didCreateWorktree info: WorktreeInfo, inRepo repoPath: String) {
        tabCoordinator.handleNewBranch(info: info, repoPath: repoPath)
    }
}

// MARK: - WorktreeStatusDelegate

extension MainWindowController: WorktreeStatusDelegate {
    func worktreeStatusDidUpdate(_ status: WorktreeStatus) {
        tabCoordinator.handleWorktreeStatusUpdate(status)
    }

    func paneStatusDidChange(worktreePath: String, paneIndex: Int, oldStatus: AgentStatus, newStatus: AgentStatus, lastMessage: String) {
        tabCoordinator.handlePaneStatusChange(worktreePath: worktreePath, paneIndex: paneIndex, oldStatus: oldStatus, newStatus: newStatus, lastMessage: lastMessage)
    }
}

// MARK: - Notification Navigation

extension MainWindowController {
    @objc private func handleNavigateToWorktree(_ notification: Notification) {
        guard let worktreePath = notification.userInfo?["worktreePath"] as? String else { return }
        let paneIndex = notification.userInfo?["paneIndex"] as? Int
        tabCoordinator.handleNavigateToWorktree(worktreePath: worktreePath, paneIndex: paneIndex)
    }

    @objc private func handleNotificationHistoryDidChange(_ notification: Notification?) {
        updatePrimaryCapsuleNotification()
    }

    private func updatePrimaryCapsuleNotification() {
        pruneDismissedPrimaryCapsuleNotificationIDs()
        let entry = Self.selectPrimaryCapsuleNotification(
            from: NotificationHistory.shared.entries,
            excluding: dismissedPrimaryCapsuleNotificationIDs
        )
        let previousID = primaryCapsuleNotification?.id
        primaryCapsuleNotification = entry
        if let entry, entry.id != previousID {
            schedulePrimaryCapsuleAutoDismiss(for: entry)
        } else if entry == nil {
            primaryCapsuleDismissWorkItem?.cancel()
            primaryCapsuleDismissWorkItem = nil
        }
        titleBar.updateNotificationSummary(
            entry: entry,
            unreadCount: NotificationHistory.shared.unreadCount
        )
    }

    static func selectPrimaryCapsuleNotification(
        from entries: [NotificationEntry],
        excluding excludedIDs: Set<UUID> = []
    ) -> NotificationEntry? {
        let unreadEntries = entries.filter { !$0.isRead }
        let visibleEntries = unreadEntries.filter { !excludedIDs.contains($0.id) }
        guard !visibleEntries.isEmpty else { return nil }
        return highestPriorityNotification(in: visibleEntries)
    }

    private static func highestPriorityNotification(in entries: [NotificationEntry]) -> NotificationEntry? {
        entries.max { lhs, rhs in
            let left = notificationPriorityScore(for: lhs)
            let right = notificationPriorityScore(for: rhs)
            if left == right {
                return lhs.timestamp < rhs.timestamp
            }
            return left < right
        }
    }

    private static func notificationPriorityScore(for entry: NotificationEntry) -> Int {
        switch entry.status {
        case .error, .exited:
            return 4
        case .waiting:
            return 3
        case .idle:
            return entry.isRead ? 1 : 2
        default:
            return entry.isRead ? 0 : 1
        }
    }

    private func schedulePrimaryCapsuleDismissal(for entry: NotificationEntry) {
        primaryCapsuleDismissWorkItem?.cancel()
        dismissedPrimaryCapsuleNotificationIDs.insert(entry.id)
        primaryCapsuleNotification = nil
        titleBar.updateNotificationSummary(
            entry: nil,
            unreadCount: NotificationHistory.shared.unreadCount
        )
        let workItem = DispatchWorkItem {
            NotificationHistory.shared.markRead(id: entry.id)
        }
        primaryCapsuleDismissWorkItem = workItem
        DispatchQueue.main.asyncAfter(
            deadline: .now() + Self.primaryCapsuleDismissDelay,
            execute: workItem
        )
    }

    private func schedulePrimaryCapsuleAutoDismiss(for entry: NotificationEntry) {
        primaryCapsuleDismissWorkItem?.cancel()
        let workItem = DispatchWorkItem { [weak self] in
            guard let self else { return }
            self.dismissedPrimaryCapsuleNotificationIDs.insert(entry.id)
            if self.primaryCapsuleNotification?.id == entry.id {
                self.primaryCapsuleNotification = nil
                self.titleBar.updateNotificationSummary(
                    entry: nil,
                    unreadCount: NotificationHistory.shared.unreadCount
                )
            }
        }
        primaryCapsuleDismissWorkItem = workItem
        DispatchQueue.main.asyncAfter(
            deadline: .now() + Self.primaryCapsuleDisplayDuration,
            execute: workItem
        )
    }

    private func pruneDismissedPrimaryCapsuleNotificationIDs() {
        let validIDs = Set(NotificationHistory.shared.entries.map(\.id))
        dismissedPrimaryCapsuleNotificationIDs.formIntersection(validIDs)
    }
}

// MARK: - SettingsDelegate

extension MainWindowController: SettingsDelegate {
    func settingsDidUpdateConfig(_ settings: SettingsViewController, config: Config) {
        let oldPaths = Set(self.config.workspacePaths)
        let oldWecomBot = self.config.wecomBot
        let oldWechat = self.config.wechat
        // Preserve split layouts — SettingsVC doesn't track them
        var merged = config
        merged.splitLayouts = terminalCoordinator.config.splitLayouts
        self.config = merged
        tabCoordinator.config = merged
        terminalCoordinator.config = merged
        updateCoordinator.config = merged
        normalizeBackendAvailabilityIfNeeded()

        let newPaths = Set(config.workspacePaths)
        if oldPaths != newPaths {
            tabCoordinator.loadWorkspaces()
        }

        // Hot-reload external channels on config change
        if oldWecomBot != config.wecomBot || oldWechat != config.wechat {
            AgentHead.shared.unregisterAllExternalChannels()

            if let wecomConfig = config.wecomBot, wecomConfig.resolvedAutoConnect {
                let channel = WeComBotChannel(config: wecomConfig)
                AgentHead.shared.registerChannel(channel)
                channel.connect()
                NSLog("[Settings] WeCom bot reconnecting: \(wecomConfig.resolvedName)")
            }

            if let wechatConfig = config.wechat, wechatConfig.resolvedAutoConnect {
                let channel = WeChatChannel(config: wechatConfig)
                AgentHead.shared.registerChannel(channel)
                channel.connect()
                NSLog("[Settings] WeChat reconnecting")
            }
        }
    }
}

// MARK: - QuickSwitcherDelegate

extension MainWindowController: QuickSwitcherDelegate {
    func quickSwitcher(_ vc: QuickSwitcherViewController, didSelect worktree: WorktreeInfo) {
        // Navigate to dashboard — quick switcher now selects the agent card
        tabCoordinator.switchToTab(0)
    }
}

// MARK: - Auto-Update

extension MainWindowController {
    @objc func checkForUpdates() {
        updateCoordinator.checkForUpdates()
    }
}

// MARK: - UpdateCoordinatorDelegate

extension MainWindowController: UpdateCoordinatorDelegate {
    func updateCoordinator(_ coordinator: UpdateCoordinator, showBanner banner: UpdateBanner) {
        // Banner display handled by coordinator's banner property
    }
}


// MARK: - TabCoordinatorDelegate

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
    func tabCoordinatorRequestClearContentContainer(_ coordinator: TabCoordinator) {
        for child in contentContainer.subviews {
            child.removeFromSuperview()
        }
    }
}

// MARK: - TerminalCoordinatorDelegate

extension MainWindowController: TerminalCoordinatorDelegate {
    func terminalCoordinatorDidUpdateSurfaces(_ coordinator: TerminalCoordinator) {
        statusPublisher.updateSurfaces(coordinator.surfaceManager.all)
    }

    func terminalCoordinator(_ coordinator: TerminalCoordinator, didDeleteWorktree info: WorktreeInfo) {
        worktreeDidDelete(info)
    }
}

final class ViewHostController: NSViewController {
    private let hostedView: NSView

    init(hostedView: NSView) {
        self.hostedView = hostedView
        super.init(nibName: nil, bundle: nil)
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) not supported")
    }

    override func loadView() {
        hostedView.translatesAutoresizingMaskIntoConstraints = false
        view = NSView(frame: NSRect(origin: .zero, size: hostedView.frame.size))
        view.addSubview(hostedView)
        NSLayoutConstraint.activate([
            hostedView.topAnchor.constraint(equalTo: view.topAnchor),
            hostedView.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            hostedView.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            hostedView.bottomAnchor.constraint(equalTo: view.bottomAnchor),
        ])
    }
}
