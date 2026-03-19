import AppKit

class MainWindowController: NSWindowController {
    private let tabBar = TabBarView()
    private let contentContainer = NSView()
    private var dashboardVC: DashboardViewController?
    private var config = Config.load()

    // All terminal surfaces, keyed by worktree path
    private var surfaces: [String: TerminalSurface] = [:]
    private var allWorktrees: [(info: WorktreeInfo, surface: TerminalSurface)] = []

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

        // View menu with shortcuts
        let viewMenuItem = NSMenuItem()
        let viewMenu = NSMenu(title: "View")

        // Cmd+G → Grid mode
        let gridItem = NSMenuItem(title: "Grid View", action: #selector(switchToGrid), keyEquivalent: "g")
        gridItem.keyEquivalentModifierMask = .command
        viewMenu.addItem(gridItem)

        // Cmd+1-9 → Spotlight focus
        for i in 1...9 {
            let item = NSMenuItem(title: "Focus Terminal \(i)", action: #selector(spotlightByNumber(_:)), keyEquivalent: "\(i)")
            item.keyEquivalentModifierMask = .command
            item.tag = i - 1
            viewMenu.addItem(item)
        }

        viewMenuItem.submenu = viewMenu
        mainMenu.addItem(viewMenuItem)

        NSApp.mainMenu = mainMenu
    }

    @objc private func switchToGrid() {
        dashboardVC?.exitSpotlight()
    }

    @objc private func spotlightByNumber(_ sender: NSMenuItem) {
        let index = sender.tag
        dashboardVC?.enterSpotlight(focusedIndex: index)
    }

    // MARK: - Layout

    private func setupLayout() {
        guard let contentView = window?.contentView else { return }

        // Tab bar at top
        tabBar.translatesAutoresizingMaskIntoConstraints = false
        tabBar.delegate = self
        contentView.addSubview(tabBar)

        // Content area below tab bar
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

        // Create dashboard
        let dashboard = DashboardViewController()
        dashboard.dashboardDelegate = self
        dashboardVC = dashboard

        // Embed dashboard in content container
        dashboard.view.translatesAutoresizingMaskIntoConstraints = false
        contentContainer.addSubview(dashboard.view)
        NSLayoutConstraint.activate([
            dashboard.view.topAnchor.constraint(equalTo: contentContainer.topAnchor),
            dashboard.view.leadingAnchor.constraint(equalTo: contentContainer.leadingAnchor),
            dashboard.view.trailingAnchor.constraint(equalTo: contentContainer.trailingAnchor),
            dashboard.view.bottomAnchor.constraint(equalTo: contentContainer.bottomAnchor),
        ])

        updateTabBar()
    }

    private func updateTabBar() {
        let tabs = [TabItem(title: "Dashboard", isClosable: false)]
        tabBar.setTabs(tabs, selected: 0)
    }

    // MARK: - Key Handling (from PmuxWindow)

    /// Handle Esc key — exit Spotlight back to Grid
    func handleEscKey() {
        guard let dashboard = dashboardVC else { return }
        if case .spotlight = dashboard.mode {
            dashboard.exitSpotlight()
        }
    }

    /// Handle Ctrl+Tab / Ctrl+Shift+Tab — cycle Spotlight focus
    func handleCycleSpotlight(forward: Bool) {
        guard let dashboard = dashboardVC else { return }
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

/// Custom window that intercepts key events before they reach the terminal.
protocol PmuxWindowKeyHandler: AnyObject {
    func handleEscKey()
    func handleCycleSpotlight(forward: Bool)
}

class PmuxWindow: NSWindow {
    weak var keyHandler: PmuxWindowKeyHandler?

    /// Override sendEvent to intercept keys BEFORE the first responder (terminal) gets them
    override func sendEvent(_ event: NSEvent) {
        if event.type == .keyDown {
            // Esc → exit Spotlight
            if event.keyCode == 53 {
                keyHandler?.handleEscKey()
                return
            }

            // Ctrl+Tab / Ctrl+Shift+Tab → cycle Spotlight
            if event.keyCode == 48 && event.modifierFlags.contains(.control) {
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
    func windowDidResize(_ notification: Notification) {
    }

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
        if index == 0 {
            dashboardVC?.view.isHidden = false
        }
    }

    func tabBar(_ tabBar: TabBarView, didCloseTabAt index: Int) {
    }
}

// MARK: - DashboardDelegate

extension MainWindowController: DashboardDelegate {
    func dashboard(_ dashboard: DashboardViewController, didSelectWorktree info: WorktreeInfo, surface: TerminalSurface) {
    }
}

// MARK: - PmuxWindowKeyHandler

extension MainWindowController: PmuxWindowKeyHandler {}
