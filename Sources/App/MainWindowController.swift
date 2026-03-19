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

    convenience init() {
        let window = PmuxWindow(
            contentRect: NSRect(x: 0, y: 0, width: 1200, height: 800),
            styleMask: [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView],
            backing: .buffered,
            defer: false
        )
        window.title = "pmux"
        window.center()
        window.minSize = NSSize(width: 600, height: 400)
        window.titlebarAppearsTransparent = true
        window.titleVisibility = .hidden
        window.appearance = NSAppearance(named: .darkAqua)
        window.backgroundColor = Theme.background

        self.init(window: window)
        window.delegate = self
        window.keyHandler = self

        setupMenuShortcuts()
        setupLayout()
        loadWorkspaces()
    }

    // MARK: - Menu Shortcuts

    private func setupMenuShortcuts() {
        let mainMenu = NSMenu()

        // App menu
        let appMenuItem = NSMenuItem()
        let appMenu = NSMenu()
        appMenu.addItem(withTitle: "Quit pmux", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        appMenuItem.submenu = appMenu
        mainMenu.addItem(appMenuItem)

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

        let openTabItem = NSMenuItem(title: "Open in Tab", action: #selector(openSpotlightAsTab), keyEquivalent: "\r")
        openTabItem.keyEquivalentModifierMask = .command
        viewMenu.addItem(openTabItem)

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

    @objc private func openSpotlightAsTab() {
        guard activeTabIndex == 0, let dashboard = dashboardVC else { return }
        if case .spotlight(let index) = dashboard.mode {
            let worktree = dashboard.worktreeAt(index: index)
            if let (info, _) = worktree {
                openRepoTab(repoPath: info.path)
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

    // MARK: - Tab Switching

    private func switchToTab(_ index: Int) {
        guard index != activeTabIndex else { return }

        // Detach terminal from current repo view
        if activeTabIndex > 0 {
            let repoIndex = activeTabIndex - 1
            if let tab = workspaceManager.tab(at: repoIndex),
               let repoVC = repoVCs[tab.repoPath] {
                repoVC.detachActiveTerminal()
                repoVC.view.removeFromSuperview()
            }
        }

        activeTabIndex = index
        tabBar.selectTab(at: index)

        if index == 0 {
            // Dashboard
            if let dashboard = dashboardVC {
                embedViewController(dashboard)
                // Re-embed terminals in their cards
                dashboard.refreshAfterReturn()
            }
        } else {
            // Repo tab
            let repoIndex = index - 1
            guard let tab = workspaceManager.tab(at: repoIndex) else { return }

            let repoVC = getOrCreateRepoVC(for: tab)
            dashboardVC?.view.removeFromSuperview()
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

        self.allWorktrees = allWorktreeInfos
        dashboardVC?.setWorktrees(allWorktreeInfos)

        if allWorktreeInfos.isEmpty {
            NSLog("No workspaces configured. Add paths to ~/.config/pmux/config.json")
        }
    }

    private func createSurface(for info: WorktreeInfo) -> TerminalSurface {
        if let existing = surfaces[info.path] {
            return existing
        }
        let surface = TerminalSurface()
        surfaces[info.path] = surface
        return surface
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
        if let tab = workspaceManager.tab(at: repoIndex) {
            repoVCs.removeValue(forKey: tab.repoPath)
        }
        workspaceManager.removeTab(at: repoIndex)
        if activeTabIndex >= index {
            activeTabIndex = max(0, activeTabIndex - 1)
        }
        updateTabBar()
        switchToTab(activeTabIndex)
    }
}

// MARK: - DashboardDelegate

extension MainWindowController: DashboardDelegate {
    func dashboard(_ dashboard: DashboardViewController, didSelectWorktree info: WorktreeInfo, surface: TerminalSurface) {
        // Double-click on Spotlight main terminal → open as repo tab
        openRepoTab(repoPath: info.path)
    }
}

// MARK: - PmuxWindowKeyHandler

extension MainWindowController: PmuxWindowKeyHandler {}
