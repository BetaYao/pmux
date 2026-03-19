import AppKit

class MainWindowController: NSWindowController {
    private let tabBar = TabBarView()
    private let contentContainer = NSView()
    private var dashboardVC: DashboardViewController?
    private var config = Config.load()
    private let workspaceManager = WorkspaceManager()

    // All terminal surfaces, keyed by worktree path
    private var surfaces: [String: TerminalSurface] = [:]
    private var allWorktrees: [(info: WorktreeInfo, surface: TerminalSurface)] = []

    // Repo views, keyed by repo path
    private var repoVCs: [String: RepoViewController] = [:]
    private var activeTabIndex: Int = 0  // 0 = Dashboard

    // Status detection
    private lazy var statusPublisher: StatusPublisher = {
        let pub = StatusPublisher(agentConfig: config.agentDetect)
        pub.delegate = self
        return pub
    }()

    convenience init() {
        let window = PmuxWindow(
            contentRect: NSRect(x: 0, y: 0, width: 1200, height: 800),
            styleMask: [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView],
            backing: .buffered,
            defer: false
        )
        window.title = "pmux"
        window.minSize = NSSize(width: 600, height: 400)
        window.titlebarAppearsTransparent = true
        window.titleVisibility = .hidden
        window.appearance = NSAppearance(named: .darkAqua)
        window.backgroundColor = Theme.background

        self.init(window: window)
        window.setFrameAutosaveName("PmuxMainWindow")
        window.delegate = self
        window.keyHandler = self

        setupMenuShortcuts()
        setupLayout()
        loadWorkspaces()

        NotificationCenter.default.addObserver(
            self, selector: #selector(handleNavigateToWorktree(_:)),
            name: .navigateToWorktree, object: nil
        )
    }

    // MARK: - Menu Shortcuts

    private func setupMenuShortcuts() {
        let mainMenu = NSMenu()

        // App menu
        let appMenuItem = NSMenuItem()
        let appMenu = NSMenu()
        let settingsItem = NSMenuItem(title: "Settings...", action: #selector(showSettings), keyEquivalent: ",")
        settingsItem.keyEquivalentModifierMask = .command
        appMenu.addItem(settingsItem)
        appMenu.addItem(NSMenuItem.separator())
        appMenu.addItem(withTitle: "Quit pmux", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        appMenuItem.submenu = appMenu
        mainMenu.addItem(appMenuItem)

        // File menu
        let fileMenuItem = NSMenuItem()
        let fileMenu = NSMenu(title: "File")
        let newBranchItem = NSMenuItem(title: "New Branch...", action: #selector(showNewBranchDialog), keyEquivalent: "n")
        newBranchItem.keyEquivalentModifierMask = .command
        fileMenu.addItem(newBranchItem)
        fileMenuItem.submenu = fileMenu
        mainMenu.addItem(fileMenuItem)

        let quickSwitchItem = NSMenuItem(title: "Quick Switch...", action: #selector(showQuickSwitcher), keyEquivalent: "p")
        quickSwitchItem.keyEquivalentModifierMask = .command
        fileMenu.addItem(quickSwitchItem)

        // View menu
        let viewMenuItem = NSMenuItem()
        let viewMenu = NSMenu(title: "View")

        let gridItem = NSMenuItem(title: "Grid View", action: #selector(switchToGrid), keyEquivalent: "g")
        gridItem.keyEquivalentModifierMask = .command
        viewMenu.addItem(gridItem)

        let dashItem = NSMenuItem(title: "Dashboard", action: #selector(switchToDashboard), keyEquivalent: "0")
        dashItem.keyEquivalentModifierMask = .command
        viewMenu.addItem(dashItem)

        for i in 1...9 {
            let item = NSMenuItem(title: "Focus Terminal \(i)", action: #selector(spotlightByNumber(_:)), keyEquivalent: "\(i)")
            item.keyEquivalentModifierMask = .command
            item.tag = i - 1
            viewMenu.addItem(item)
        }

        let closeTabItem = NSMenuItem(title: "Close Tab", action: #selector(closeCurrentTab), keyEquivalent: "w")
        closeTabItem.keyEquivalentModifierMask = .command
        viewMenu.addItem(closeTabItem)

        let openTabItem = NSMenuItem(title: "Open in Tab", action: #selector(openSpotlightAsTab), keyEquivalent: "\r")
        openTabItem.keyEquivalentModifierMask = .command
        viewMenu.addItem(openTabItem)

        let diffItem = NSMenuItem(title: "Show Diff...", action: #selector(showDiffOverlay), keyEquivalent: "d")
        diffItem.keyEquivalentModifierMask = .command
        viewMenu.addItem(diffItem)

        viewMenu.addItem(NSMenuItem.separator())

        let splitVItem = NSMenuItem(title: "Split Vertically", action: #selector(splitPaneVertical), keyEquivalent: "d")
        splitVItem.keyEquivalentModifierMask = [.command, .shift]
        viewMenu.addItem(splitVItem)

        let splitHItem = NSMenuItem(title: "Split Horizontally", action: #selector(splitPaneHorizontal), keyEquivalent: "e")
        splitHItem.keyEquivalentModifierMask = [.command, .shift]
        viewMenu.addItem(splitHItem)

        let closePaneItem = NSMenuItem(title: "Close Pane", action: #selector(closePane), keyEquivalent: "w")
        closePaneItem.keyEquivalentModifierMask = [.command, .shift]
        viewMenu.addItem(closePaneItem)

        viewMenu.addItem(NSMenuItem.separator())

        let zoomInItem = NSMenuItem(title: "Zoom In (Smaller Cards)", action: #selector(dashboardZoomIn), keyEquivalent: "-")
        zoomInItem.keyEquivalentModifierMask = .command
        viewMenu.addItem(zoomInItem)

        let zoomOutItem = NSMenuItem(title: "Zoom Out (Larger Cards)", action: #selector(dashboardZoomOut), keyEquivalent: "=")
        zoomOutItem.keyEquivalentModifierMask = .command
        viewMenu.addItem(zoomOutItem)

        viewMenuItem.submenu = viewMenu
        mainMenu.addItem(viewMenuItem)

        NSApp.mainMenu = mainMenu
    }

    @objc private func switchToGrid() {
        if activeTabIndex == 0 {
            dashboardVC?.exitSpotlight()
        } else {
            switchToTab(0)
        }
    }

    @objc private func switchToDashboard() {
        switchToTab(0)
    }

    @objc private func showQuickSwitcher() {
        let worktreeInfos = allWorktrees.map { $0.info }
        var statuses: [String: AgentStatus] = [:]
        for (path, _) in surfaces {
            statuses[path] = statusPublisher.status(for: path)
        }
        let switcher = QuickSwitcherViewController(worktrees: worktreeInfos, statuses: statuses)
        switcher.quickSwitcherDelegate = self
        if activeTabIndex == 0 {
            dashboardVC?.presentAsSheet(switcher)
        } else {
            let repoIndex = activeTabIndex - 1
            if let tab = workspaceManager.tab(at: repoIndex),
               let repoVC = repoVCs[tab.repoPath] {
                repoVC.presentAsSheet(switcher)
            } else {
                dashboardVC?.presentAsSheet(switcher)
            }
        }
    }

    @objc private func showSettings() {
        let settingsVC = SettingsViewController(config: config)
        settingsVC.settingsDelegate = self
        if activeTabIndex == 0 {
            dashboardVC?.presentAsSheet(settingsVC)
        } else {
            let repoIndex = activeTabIndex - 1
            if let tab = workspaceManager.tab(at: repoIndex),
               let repoVC = repoVCs[tab.repoPath] {
                repoVC.presentAsSheet(settingsVC)
            } else {
                dashboardVC?.presentAsSheet(settingsVC)
            }
        }
    }

    @objc private func showNewBranchDialog() {
        let dialog = NewBranchDialog(repoPaths: config.workspacePaths)
        dialog.dialogDelegate = self
        // Present as sheet on the currently visible view controller
        if activeTabIndex == 0 {
            dashboardVC?.presentAsSheet(dialog)
        } else {
            let repoIndex = activeTabIndex - 1
            if let tab = workspaceManager.tab(at: repoIndex),
               let repoVC = repoVCs[tab.repoPath] {
                repoVC.presentAsSheet(dialog)
            } else {
                dashboardVC?.presentAsSheet(dialog)
            }
        }
    }

    @objc private func closeCurrentTab() {
        guard activeTabIndex > 0 else { return }  // Can't close Dashboard
        tabBar(tabBar, didCloseTabAt: activeTabIndex)
    }

    @objc private func openSpotlightAsTab() {
        guard activeTabIndex == 0, let dashboard = dashboardVC else { return }
        if case .spotlight(let index) = dashboard.mode {
            if let (info, _) = dashboard.worktreeAt(index: index) {
                let repoPath = WorktreeDiscovery.findRepoRoot(from: info.path) ?? info.path
                openRepoTab(repoPath: repoPath)
            }
        }
    }

    @objc private func splitPaneVertical() {
        guard activeTabIndex > 0 else { return }
        let repoIndex = activeTabIndex - 1
        if let tab = workspaceManager.tab(at: repoIndex),
           let repoVC = repoVCs[tab.repoPath] {
            repoVC.splitVertical()
        }
    }

    @objc private func splitPaneHorizontal() {
        guard activeTabIndex > 0 else { return }
        let repoIndex = activeTabIndex - 1
        if let tab = workspaceManager.tab(at: repoIndex),
           let repoVC = repoVCs[tab.repoPath] {
            repoVC.splitHorizontal()
        }
    }

    @objc private func closePane() {
        guard activeTabIndex > 0 else { return }
        let repoIndex = activeTabIndex - 1
        if let tab = workspaceManager.tab(at: repoIndex),
           let repoVC = repoVCs[tab.repoPath] {
            repoVC.closePane()
        }
    }

    @objc private func dashboardZoomIn() {
        dashboardVC?.zoomIn()
        config.zoomIndex = dashboardVC?.zoomIndex ?? GridLayout.defaultZoomIndex
        config.save()
    }

    @objc private func dashboardZoomOut() {
        dashboardVC?.zoomOut()
        config.zoomIndex = dashboardVC?.zoomIndex ?? GridLayout.defaultZoomIndex
        config.save()
    }

    @objc private func showDiffOverlay() {
        // Determine current worktree path
        var worktreePath: String?
        if activeTabIndex > 0 {
            let repoIndex = activeTabIndex - 1
            if let tab = workspaceManager.tab(at: repoIndex) {
                worktreePath = tab.repoPath
            }
        } else if let dashboard = dashboardVC {
            if case .spotlight(let index) = dashboard.mode,
               let (info, _) = dashboard.worktreeAt(index: index) {
                worktreePath = info.path
            }
        }

        guard let path = worktreePath else { return }

        let diffVC = DiffOverlayViewController(worktreePath: path)
        if activeTabIndex == 0 {
            dashboardVC?.presentAsSheet(diffVC)
        } else {
            let repoIndex = activeTabIndex - 1
            if let tab = workspaceManager.tab(at: repoIndex),
               let repoVC = repoVCs[tab.repoPath] {
                repoVC.presentAsSheet(diffVC)
            } else {
                dashboardVC?.presentAsSheet(diffVC)
            }
        }
    }

    @objc private func spotlightByNumber(_ sender: NSMenuItem) {
        let index = sender.tag
        if activeTabIndex != 0 {
            switchToTab(0)
        }
        dashboardVC?.enterSpotlight(focusedIndex: index)
    }

    // MARK: - Layout

    private func setupLayout() {
        guard let contentView = window?.contentView else { return }

        tabBar.translatesAutoresizingMaskIntoConstraints = false
        tabBar.delegate = self
        contentView.addSubview(tabBar)

        contentContainer.wantsLayer = true
        contentContainer.translatesAutoresizingMaskIntoConstraints = false
        contentView.addSubview(contentContainer)

        NSLayoutConstraint.activate([
            tabBar.topAnchor.constraint(equalTo: contentView.topAnchor),
            tabBar.leadingAnchor.constraint(equalTo: contentView.leadingAnchor),
            tabBar.trailingAnchor.constraint(equalTo: contentView.trailingAnchor),
            tabBar.heightAnchor.constraint(equalToConstant: Theme.tabBarHeight),

            contentContainer.topAnchor.constraint(equalTo: tabBar.bottomAnchor),
            contentContainer.leadingAnchor.constraint(equalTo: contentView.leadingAnchor),
            contentContainer.trailingAnchor.constraint(equalTo: contentView.trailingAnchor),
            contentContainer.bottomAnchor.constraint(equalTo: contentView.bottomAnchor),
        ])

        let dashboard = DashboardViewController()
        dashboard.dashboardDelegate = self
        dashboard.setZoomIndex(config.zoomIndex)
        dashboardVC = dashboard

        embedViewController(dashboard)
        updateTabBar()
    }

    private func embedViewController(_ vc: NSViewController) {
        // Remove all current children from content container
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

    private func updateTabBar() {
        var tabs = [TabItem(title: "Dashboard", isClosable: false)]
        for tab in workspaceManager.tabs {
            tabs.append(TabItem(title: tab.displayName, isClosable: true))
        }
        tabBar.setTabs(tabs, selected: activeTabIndex)
    }

    /// Add a new repo via folder picker
    private func addRepoViaOpenPanel() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.allowsMultipleSelection = false
        panel.message = "Select a git repository to add"
        panel.prompt = "Add Repo"

        guard let window else { return }
        panel.beginSheetModal(for: window) { [weak self] response in
            guard response == .OK, let url = panel.url else { return }
            self?.addRepo(at: url.path)
        }
    }

    private func addRepo(at path: String) {
        // Don't add duplicates
        guard !config.workspacePaths.contains(path) else {
            // Already exists — just switch to its tab
            if let tabIndex = workspaceManager.tabs.firstIndex(where: { $0.repoPath == path }) {
                switchToTab(tabIndex + 1)
            }
            return
        }

        config.workspacePaths.append(path)
        config.save()

        // Create surfaces for the new repo's worktrees
        let worktrees = WorktreeDiscovery.discover(repoPath: path)
        if worktrees.isEmpty {
            let info = WorktreeInfo(path: path, branch: "main", commitHash: "", isMainWorktree: true)
            let surface = createSurface(for: info)
            allWorktrees.append((info: info, surface: surface))
        } else {
            for info in worktrees {
                let surface = createSurface(for: info)
                allWorktrees.append((info: info, surface: surface))
            }
        }

        // Add tab for the new repo
        let tabIndex = workspaceManager.addTab(repoPath: path, worktrees: worktrees.isEmpty ? [] : worktrees)

        // Update dashboard and tab bar
        dashboardVC?.setWorktrees(allWorktrees)
        statusPublisher.updateSurfaces(surfaces)
        updateTabBar()
        switchToTab(tabIndex + 1)
    }

    private func updateStatusCounts() {
        var running = 0, waiting = 0, error = 0
        for path in surfaces.keys {
            switch statusPublisher.status(for: path) {
            case .running: running += 1
            case .waiting: waiting += 1
            case .error:   error += 1
            default: break
            }
        }
        tabBar.updateStatusCounts(running: running, waiting: waiting, error: error)
    }

    // MARK: - Tab Switching

    private func switchToTab(_ index: Int) {
        guard index != activeTabIndex else { return }

        // Detach terminals from current repo view (not dashboard — dashboard
        // terminals are reparented directly by the next view controller)
        if activeTabIndex > 0 {
            let repoIndex = activeTabIndex - 1
            if let tab = workspaceManager.tab(at: repoIndex),
               let repoVC = repoVCs[tab.repoPath] {
                repoVC.detachActiveTerminal()
            }
        }

        activeTabIndex = index
        tabBar.selectTab(at: index)

        if index == 0 {
            if let dashboard = dashboardVC {
                embedViewController(dashboard)
                dashboard.refreshAfterReturn()
            }
        } else {
            let repoIndex = index - 1
            guard let tab = workspaceManager.tab(at: repoIndex) else { return }
            let repoVC = getOrCreateRepoVC(for: tab)
            embedViewController(repoVC)
        }
    }

    private func getOrCreateRepoVC(for tab: WorkspaceTab) -> RepoViewController {
        if let existing = repoVCs[tab.repoPath] {
            // Reconfigure to pick up any surface changes
            existing.configure(worktrees: tab.worktrees, surfaces: surfaces)
            return existing
        }

        let repoVC = RepoViewController()
        repoVC.repoDelegate = self
        repoVC.configure(worktrees: tab.worktrees, surfaces: surfaces)
        repoVCs[tab.repoPath] = repoVC
        return repoVC
    }

    private func openRepoTab(repoPath: String) {
        let worktrees = WorktreeDiscovery.discover(repoPath: repoPath)
        let tabIndex = workspaceManager.addTab(repoPath: repoPath, worktrees: worktrees)
        updateTabBar()
        switchToTab(tabIndex + 1)  // +1 because Dashboard is index 0
    }

    // MARK: - Key Handling

    func handleEscKey() {
        if activeTabIndex == 0 {
            guard let dashboard = dashboardVC else { return }
            if case .spotlight = dashboard.mode {
                dashboard.exitSpotlight()
            }
        }
    }

    func handleCycleSpotlight(forward: Bool) {
        guard activeTabIndex == 0, let dashboard = dashboardVC else { return }
        if case .spotlight(let current) = dashboard.mode {
            let count = dashboard.cardCount
            guard count > 0 else { return }
            let next = forward ? (current + 1) % count : (current - 1 + count) % count
            dashboard.enterSpotlight(focusedIndex: next)
        }
    }

    // MARK: - Workspace Loading

    private func loadWorkspaces() {
        var allWorktreeInfos: [(info: WorktreeInfo, surface: TerminalSurface)] = []

        for repoPath in config.workspacePaths {
            let worktrees = WorktreeDiscovery.discover(repoPath: repoPath)
            if worktrees.isEmpty {
                let info = WorktreeInfo(
                    path: repoPath,
                    branch: "main",
                    commitHash: "",
                    isMainWorktree: true
                )
                let surface = createSurface(for: info)
                allWorktreeInfos.append((info: info, surface: surface))
            } else {
                for info in worktrees {
                    let surface = createSurface(for: info)
                    allWorktreeInfos.append((info: info, surface: surface))
                }
            }
        }

        // Apply saved card order
        if !config.cardOrder.isEmpty {
            allWorktreeInfos.sort { a, b in
                let ai = config.cardOrder.firstIndex(of: a.info.path) ?? Int.max
                let bi = config.cardOrder.firstIndex(of: b.info.path) ?? Int.max
                return ai < bi
            }
        }

        self.allWorktrees = allWorktreeInfos
        dashboardVC?.setWorktrees(allWorktreeInfos)

        // Auto-create tabs for all configured repos
        for repoPath in config.workspacePaths {
            let worktrees = WorktreeDiscovery.discover(repoPath: repoPath)
            _ = workspaceManager.addTab(repoPath: repoPath, worktrees: worktrees)
        }
        updateTabBar()

        if allWorktreeInfos.isEmpty {
            NSLog("No workspaces configured. Add paths to ~/.config/pmux/config.json")
        }

        // Start polling for agent status
        statusPublisher.start(surfaces: surfaces)
    }

    private func createSurface(for info: WorktreeInfo) -> TerminalSurface {
        if let existing = surfaces[info.path] {
            return existing
        }
        let surface = TerminalSurface()
        if config.backend == "tmux" {
            surface.sessionName = Self.tmuxSessionName(for: info.path)
        }
        surfaces[info.path] = surface
        return surface
    }

    // MARK: - Worktree Deletion

    private func confirmAndDeleteWorktree(_ info: WorktreeInfo) {
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

        // Make delete button destructive
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
        // Destroy terminal surface first
        if let surface = surfaces[info.path] {
            surface.destroy()
            surfaces.removeValue(forKey: info.path)
        }

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
                    self?.worktreeDidDelete(info)
                }
            } catch {
                DispatchQueue.main.async {
                    let errAlert = NSAlert()
                    errAlert.alertStyle = .critical
                    errAlert.messageText = "Failed to delete worktree"
                    errAlert.informativeText = error.localizedDescription
                    if let window = self?.window {
                        errAlert.beginSheetModal(for: window)
                    }
                }
            }
        }
    }

    private func worktreeDidDelete(_ info: WorktreeInfo) {
        // Remove from allWorktrees
        allWorktrees.removeAll { $0.info.path == info.path }

        // Update dashboard
        dashboardVC?.setWorktrees(allWorktrees)

        // Update status publisher
        statusPublisher.updateSurfaces(surfaces)

        // If we're in a repo tab, reconfigure the repo VC
        if activeTabIndex > 0 {
            let repoIndex = activeTabIndex - 1
            if let tab = workspaceManager.tab(at: repoIndex) {
                // Refresh worktrees for this repo
                let updatedWorktrees = WorktreeDiscovery.discover(repoPath: tab.repoPath)
                workspaceManager.updateWorktrees(at: repoIndex, worktrees: updatedWorktrees)
                if let repoVC = repoVCs[tab.repoPath] {
                    repoVC.configure(worktrees: updatedWorktrees, surfaces: surfaces)
                }
                // If no worktrees left, close the tab
                if updatedWorktrees.isEmpty {
                    tabBar(tabBar, didCloseTabAt: activeTabIndex)
                }
            }
        }

        updateStatusCounts()
    }

    deinit {
        NotificationCenter.default.removeObserver(self, name: .navigateToWorktree, object: nil)
    }

    /// Generate a stable tmux session name from a worktree path
    private static func tmuxSessionName(for path: String) -> String {
        // "pmux-<last-two-path-components>" e.g. "pmux-workspace-pmux" or "pmux-worktrees-feature-x"
        let url = URL(fileURLWithPath: path)
        let parent = url.deletingLastPathComponent().lastPathComponent
        let name = url.lastPathComponent
        // tmux session names can't contain dots or colons
        let sessionName = "pmux-\(parent)-\(name)"
            .replacingOccurrences(of: ".", with: "_")
            .replacingOccurrences(of: ":", with: "_")
        return sessionName
    }
}

// MARK: - PmuxWindow

protocol PmuxWindowKeyHandler: AnyObject {
    func handleEscKey()
    func handleCycleSpotlight(forward: Bool)
}

class PmuxWindow: NSWindow {
    weak var keyHandler: PmuxWindowKeyHandler?

    override func sendEvent(_ event: NSEvent) {
        if event.type == .keyDown {
            if event.keyCode == 53 {  // Esc
                keyHandler?.handleEscKey()
                return
            }
            if event.keyCode == 48 && event.modifierFlags.contains(.control) {  // Ctrl+Tab
                let forward = !event.modifierFlags.contains(.shift)
                keyHandler?.handleCycleSpotlight(forward: forward)
                return
            }
        }
        super.sendEvent(event)
    }
}

// MARK: - NSWindowDelegate

extension MainWindowController: NSWindowDelegate {
    func windowDidResize(_ notification: Notification) {}

    func windowWillClose(_ notification: Notification) {
        statusPublisher.stop()
        for (_, surface) in surfaces {
            surface.destroy()
        }
        surfaces.removeAll()
    }
}

// MARK: - TabBarDelegate

extension MainWindowController: TabBarDelegate {
    func tabBar(_ tabBar: TabBarView, didSelectTabAt index: Int) {
        switchToTab(index)
    }

    func tabBar(_ tabBar: TabBarView, didCloseTabAt index: Int) {
        guard index > 0 else { return }  // Can't close Dashboard
        let repoIndex = index - 1
        guard let tab = workspaceManager.tab(at: repoIndex) else { return }

        // Show confirmation dialog
        let alert = NSAlert()
        alert.alertStyle = .warning
        alert.messageText = "Close \"\(tab.displayName)\"?"
        alert.informativeText = "This will close all terminals and kill tmux sessions for this repository."
        alert.addButton(withTitle: "Close")
        alert.addButton(withTitle: "Cancel")
        alert.buttons[0].hasDestructiveAction = true

        guard let window else { return }
        alert.beginSheetModal(for: window) { [weak self] response in
            guard response == .alertFirstButtonReturn else { return }
            self?.performCloseRepo(at: index)
        }
    }

    func tabBarDidClickAdd(_ tabBar: TabBarView) {
        addRepoViaOpenPanel()
    }

    private func performCloseRepo(at index: Int) {
        guard index > 0 else { return }
        let repoIndex = index - 1
        guard let tab = workspaceManager.tab(at: repoIndex) else { return }

        // Kill tmux sessions and destroy surfaces for this repo's worktrees
        for worktree in tab.worktrees {
            if let surface = surfaces[worktree.path] {
                surface.destroy()
                surfaces.removeValue(forKey: worktree.path)
            }
            // Kill tmux session
            if config.backend == "tmux" {
                let sessionName = Self.tmuxSessionName(for: worktree.path)
                killTmuxSession(sessionName)
            }
        }

        // Remove worktrees from allWorktrees
        allWorktrees.removeAll { item in
            tab.worktrees.contains(where: { $0.path == item.info.path })
        }

        // Remove from config
        config.workspacePaths.removeAll { $0 == tab.repoPath }
        config.save()

        // Clean up
        repoVCs.removeValue(forKey: tab.repoPath)
        workspaceManager.removeTab(at: repoIndex)

        if activeTabIndex >= index {
            activeTabIndex = max(0, activeTabIndex - 1)
        }

        // Update UI
        dashboardVC?.setWorktrees(allWorktrees)
        statusPublisher.updateSurfaces(surfaces)
        updateTabBar()
        switchToTab(activeTabIndex)
        updateStatusCounts()
    }

    private func killTmuxSession(_ name: String) {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        process.arguments = ["tmux", "kill-session", "-t", name]
        process.standardOutput = Pipe()
        process.standardError = Pipe()
        try? process.run()
    }
}

// MARK: - DashboardDelegate

extension MainWindowController: DashboardDelegate {
    func dashboard(_ dashboard: DashboardViewController, didSelectWorktree info: WorktreeInfo, surface: TerminalSurface) {
        let repoPath = WorktreeDiscovery.findRepoRoot(from: info.path) ?? info.path
        openRepoTab(repoPath: repoPath)
    }

    func dashboard(_ dashboard: DashboardViewController, didRequestDeleteWorktree info: WorktreeInfo) {
        confirmAndDeleteWorktree(info)
    }

    func dashboard(_ dashboard: DashboardViewController, didReorderWorktrees paths: [String]) {
        // Reorder allWorktrees to match the new order
        var reordered: [(info: WorktreeInfo, surface: TerminalSurface)] = []
        for path in paths {
            if let item = allWorktrees.first(where: { $0.info.path == path }) {
                reordered.append(item)
            }
        }
        // Append any items not in paths (shouldn't happen, but safety)
        for item in allWorktrees where !paths.contains(item.info.path) {
            reordered.append(item)
        }
        allWorktrees = reordered

        // Persist order in config
        config.cardOrder = paths
        config.save()
    }
}

// MARK: - RepoViewDelegate

extension MainWindowController: RepoViewDelegate {
    func repoView(_ repoVC: RepoViewController, didRequestDeleteWorktree info: WorktreeInfo) {
        confirmAndDeleteWorktree(info)
    }
}

// MARK: - PmuxWindowKeyHandler

extension MainWindowController: PmuxWindowKeyHandler {}

// MARK: - NewBranchDialogDelegate

extension MainWindowController: NewBranchDialogDelegate {
    func newBranchDialog(_ dialog: NewBranchDialog, didCreateWorktree info: WorktreeInfo, inRepo repoPath: String) {
        // Create a terminal surface for the new worktree
        let surface = createSurface(for: info)
        allWorktrees.append((info: info, surface: surface))

        // Update dashboard
        dashboardVC?.setWorktrees(allWorktrees)

        // Update status publisher
        statusPublisher.updateSurfaces(surfaces)

        // Switch to dashboard to see the new card
        if activeTabIndex != 0 {
            switchToTab(0)
        }
    }
}

// MARK: - StatusPublisherDelegate

extension MainWindowController: StatusPublisherDelegate {
    func statusDidChange(worktreePath: String, oldStatus: AgentStatus, newStatus: AgentStatus, lastMessage: String) {
        // Find branch name for notification
        let branch = allWorktrees.first(where: { $0.info.path == worktreePath })?.info.branch ?? ""

        // Send macOS notification
        NotificationManager.shared.notify(
            worktreePath: worktreePath,
            branch: branch,
            oldStatus: oldStatus,
            newStatus: newStatus
        )

        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            self.dashboardVC?.updateStatus(for: worktreePath, status: newStatus, lastMessage: lastMessage)
            if self.activeTabIndex > 0 {
                let repoIndex = self.activeTabIndex - 1
                if let tab = self.workspaceManager.tab(at: repoIndex),
                   let repoVC = self.repoVCs[tab.repoPath] {
                    repoVC.updateStatus(for: worktreePath, status: newStatus, lastMessage: lastMessage)
                }
            }
            self.updateStatusCounts()
        }
    }
}

// MARK: - Notification Navigation

extension MainWindowController {
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
}

// MARK: - SettingsDelegate

extension MainWindowController: SettingsDelegate {
    func settingsDidUpdateConfig(_ settings: SettingsViewController, config: Config) {
        let oldPaths = Set(self.config.workspacePaths)
        self.config = config

        let newPaths = Set(config.workspacePaths)
        if oldPaths != newPaths {
            // Reload workspaces with updated paths
            loadWorkspaces()
        }
    }
}

// MARK: - QuickSwitcherDelegate

extension MainWindowController: QuickSwitcherDelegate {
    func quickSwitcher(_ vc: QuickSwitcherViewController, didSelect worktree: WorktreeInfo) {
        // Find the index in allWorktrees
        if let index = allWorktrees.firstIndex(where: { $0.info.path == worktree.path }) {
            // Switch to dashboard and spotlight the selected worktree
            if activeTabIndex != 0 {
                switchToTab(0)
            }
            dashboardVC?.enterSpotlight(focusedIndex: index)
        }
    }
}
