import AppKit

class MainWindowController: NSWindowController {
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

    private let titleBar = TitleBarView()
    private let backgroundEffectView = NSVisualEffectView()
    private let contentContainer = NSView()
    private var windowTrackingArea: NSTrackingArea?
    private let notificationPanel = NotificationPanelView()
    private let aiPanel = AIPanelView()
    private let notificationPopover = NSPopover()
    private let aiPopover = NSPopover()
    private let titleBarAccessory = NSTitlebarAccessoryViewController()

    private var dashboardVC: DashboardViewController?
    private var config = Config.load()
    private var runtimeBackend: String = "zmx"
    private let workspaceManager = WorkspaceManager()

    // All terminal surfaces, keyed by worktree path
    private var surfaces: [String: TerminalSurface] = [:]
    private var allWorktrees: [(info: WorktreeInfo, surface: TerminalSurface)] = []

    private static let iso8601: ISO8601DateFormatter = {
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withInternetDateTime]
        return f
    }()

    // Repo views, keyed by repo path
    private var repoVCs: [String: RepoViewController] = [:]
    private var activeTabIndex: Int = 0  // 0 = Dashboard

    // Auto-update
    private let updateChecker = UpdateChecker()
    private let updateManager = UpdateManager()
    private let updateBanner = UpdateBanner()
    private var pendingRelease: ReleaseInfo?

    // Status detection
    private lazy var statusPublisher: StatusPublisher = {
        let pub = StatusPublisher(agentConfig: config.agentDetect)
        pub.delegate = self
        return pub
    }()
    private var webhookServer: WebhookServer?

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

    static func shouldHandleEscShortcut() -> Bool {
        false
    }

    static func trafficLightButtonOriginY(containerHeight: CGFloat, buttonHeight: CGFloat) -> CGFloat {
        (containerHeight / 2) + TitleBarView.Layout.arcVerticalOffset - (buttonHeight / 2)
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
        guard !trimmed.isEmpty else { return false }

        let semver = trimmed
            .components(separatedBy: CharacterSet.whitespacesAndNewlines)
            .first { token in
                token.contains(".") && token.range(of: #"^v?\d+\.\d+\.\d+"#, options: .regularExpression) != nil
            }
            ?? trimmed

        let normalized = semver.hasPrefix("v") ? String(semver.dropFirst()) : semver
        let parts = normalized
            .split(separator: ".")
            .prefix(3)
            .compactMap { Int($0.filter(\.isNumber)) }

        guard parts.count == 3 else { return false }
        let major = parts[0]
        let minor = parts[1]
        let patch = parts[2]

        if major > 0 { return true }
        if minor > 4 { return true }
        if minor < 4 { return false }
        return patch >= 2
    }

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
        window.isOpaque = false
        window.backgroundColor = .clear

        // Set window appearance from config (already applied globally in main.swift)
        window.appearance = NSApp.appearance

        self.init(window: window)

        if Self.shouldUseWindowFrameAutosave() {
            window.setFrameAutosaveName("PmuxMainWindow")
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
        setupAutoUpdate()
        normalizeBackendAvailabilityIfNeeded()
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
        let checkUpdateItem = NSMenuItem(title: "Check for Updates...", action: #selector(checkForUpdates), keyEquivalent: "u")
        checkUpdateItem.keyEquivalentModifierMask = .command
        appMenu.addItem(checkUpdateItem)
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
        let quickSwitchItem = NSMenuItem(title: "Quick Switch...", action: #selector(showQuickSwitcher), keyEquivalent: "p")
        quickSwitchItem.keyEquivalentModifierMask = .command
        fileMenu.addItem(quickSwitchItem)
        fileMenuItem.submenu = fileMenu
        mainMenu.addItem(fileMenuItem)

        // Edit menu (standard Cut/Copy/Paste/Undo/Redo)
        let editMenuItem = NSMenuItem()
        let editMenu = NSMenu(title: "Edit")
        editMenu.addItem(withTitle: "Undo", action: Selector(("undo:")), keyEquivalent: "z")
        let redoItem = NSMenuItem(title: "Redo", action: Selector(("redo:")), keyEquivalent: "z")
        redoItem.keyEquivalentModifierMask = [.command, .shift]
        editMenu.addItem(redoItem)
        editMenu.addItem(NSMenuItem.separator())
        editMenu.addItem(withTitle: "Cut", action: #selector(NSText.cut(_:)), keyEquivalent: "x")
        editMenu.addItem(withTitle: "Copy", action: #selector(NSText.copy(_:)), keyEquivalent: "c")
        editMenu.addItem(withTitle: "Paste", action: #selector(NSText.paste(_:)), keyEquivalent: "v")
        editMenu.addItem(withTitle: "Select All", action: #selector(NSText.selectAll(_:)), keyEquivalent: "a")
        editMenuItem.submenu = editMenu
        mainMenu.addItem(editMenuItem)

        // View menu
        let viewMenuItem = NSMenuItem()
        let viewMenu = NSMenu(title: "View")

        let dashItem = NSMenuItem(title: "Dashboard", action: #selector(switchToDashboard), keyEquivalent: "0")
        dashItem.keyEquivalentModifierMask = .command
        viewMenu.addItem(dashItem)

        let closeTabItem = NSMenuItem(title: "Close Tab", action: #selector(closeCurrentTab), keyEquivalent: "w")
        closeTabItem.keyEquivalentModifierMask = .command
        viewMenu.addItem(closeTabItem)

        let diffItem = NSMenuItem(title: "Show Diff...", action: #selector(showDiffOverlay), keyEquivalent: "")
        viewMenu.addItem(diffItem)

        viewMenu.addItem(NSMenuItem.separator())

        let zoomInItem = NSMenuItem(title: "Zoom In (Smaller Cards)", action: #selector(dashboardZoomIn), keyEquivalent: "-")
        zoomInItem.keyEquivalentModifierMask = .command
        viewMenu.addItem(zoomInItem)

        let zoomOutItem = NSMenuItem(title: "Zoom Out (Larger Cards)", action: #selector(dashboardZoomOut), keyEquivalent: "=")
        zoomOutItem.keyEquivalentModifierMask = .command
        viewMenu.addItem(zoomOutItem)

        viewMenuItem.submenu = viewMenu
        mainMenu.addItem(viewMenuItem)

        // Window menu (standard macOS window management)
        let windowMenuItem = NSMenuItem()
        let windowMenu = NSMenu(title: "Window")
        windowMenu.addItem(withTitle: "Minimize", action: #selector(NSWindow.miniaturize(_:)), keyEquivalent: "m")
        windowMenu.addItem(withTitle: "Zoom", action: #selector(NSWindow.zoom(_:)), keyEquivalent: "")
        windowMenu.addItem(NSMenuItem.separator())
        let nextTabItem = NSMenuItem(title: "Next Tab", action: #selector(selectNextTab), keyEquivalent: "}")
        nextTabItem.keyEquivalentModifierMask = .command
        windowMenu.addItem(nextTabItem)
        let prevTabItem = NSMenuItem(title: "Previous Tab", action: #selector(selectPreviousTab), keyEquivalent: "{")
        prevTabItem.keyEquivalentModifierMask = .command
        windowMenu.addItem(prevTabItem)
        windowMenu.addItem(NSMenuItem.separator())
        windowMenu.addItem(withTitle: "Bring All to Front", action: #selector(NSApplication.arrangeInFront(_:)), keyEquivalent: "")
        windowMenuItem.submenu = windowMenu
        mainMenu.addItem(windowMenuItem)
        NSApp.windowsMenu = windowMenu

        // Help menu
        let helpMenuItem = NSMenuItem()
        let helpMenu = NSMenu(title: "Help")
        let keyboardShortcutsItem = NSMenuItem(title: "Keyboard Shortcuts", action: #selector(showKeyboardShortcuts), keyEquivalent: "")
        helpMenu.addItem(keyboardShortcutsItem)
        helpMenu.addItem(NSMenuItem.separator())
        let docsItem = NSMenuItem(title: "pmux Documentation", action: #selector(openDocumentation), keyEquivalent: "")
        helpMenu.addItem(docsItem)
        helpMenuItem.submenu = helpMenu
        mainMenu.addItem(helpMenuItem)
        NSApp.helpMenu = helpMenu

        NSApp.mainMenu = mainMenu
    }

    private func normalizeBackendAvailabilityIfNeeded() {
        let zmxAvailable = ProcessRunner.commandExists("zmx")
        let tmuxAvailable = ProcessRunner.commandExists("tmux")

        var targetBackend = Self.resolvePreferredBackend(
            preferred: config.backend,
            zmxAvailable: zmxAvailable,
            tmuxAvailable: tmuxAvailable
        )

        var warningMessage: String?
        if config.backend == "zmx" {
            if !zmxAvailable {
                warningMessage = "zmx is not installed. Install with `brew install neurosnap/tap/zmx`."
            } else if let version = ProcessRunner.output(["zmx", "version"]), !Self.isSupportedZmxVersion(version) {
                warningMessage = "zmx version is too old. Please upgrade to zmx 0.4.2+ for stability."
            }
        }

        if warningMessage != nil, targetBackend == "zmx" {
            targetBackend = tmuxAvailable ? "tmux" : "local"
        }

        runtimeBackend = targetBackend

        if warningMessage == nil, targetBackend != config.backend {
            config.backend = targetBackend
            config.save()
        }

        if let warningMessage {
            let alert = NSAlert()
            alert.messageText = "Backend Fallback Activated"
            alert.informativeText = "\(warningMessage)\nCurrent backend: \(targetBackend)."
            alert.alertStyle = .warning
            if config.backend == "zmx" && !zmxAvailable {
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
        guard activeTabIndex > 0 else { return }
        let repoIndex = activeTabIndex - 1
        guard let tab = workspaceManager.tab(at: repoIndex) else { return }
        showCloseProjectModal(tab.displayName)
    }

    @objc private func selectNextTab() {
        let maxIndex = workspaceManager.tabs.count // 0=dashboard, 1..N=projects
        let next = activeTabIndex + 1 > maxIndex ? 0 : activeTabIndex + 1
        switchToTab(next)
    }

    @objc private func selectPreviousTab() {
        let maxIndex = workspaceManager.tabs.count
        let prev = activeTabIndex - 1 < 0 ? maxIndex : activeTabIndex - 1
        switchToTab(prev)
    }

    @objc private func showKeyboardShortcuts() {
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

    @objc private func openDocumentation() {
        if let url = URL(string: "https://github.com/nicematt/pmux") {
            NSWorkspace.shared.open(url)
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
        var worktreePath: String?
        if activeTabIndex > 0 {
            let repoIndex = activeTabIndex - 1
            if let tab = workspaceManager.tab(at: repoIndex) {
                worktreePath = tab.repoPath
            }
        }

        guard let path = worktreePath else { return }
        presentDiffOverlay(for: path)
    }

    private func presentDiffOverlay(for worktreePath: String) {
        let diffVC = DiffOverlayViewController(worktreePath: worktreePath)
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

    // MARK: - Layout

    private func setupLayout() {
        guard let contentView = window?.contentView else { return }

        setupNativeTitleBar()

        // Update banner (above title bar, hidden by default)
        updateBanner.translatesAutoresizingMaskIntoConstraints = false
        updateBanner.isHidden = true
        updateBanner.delegate = self
        contentView.addSubview(updateBanner)

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

            updateBanner.topAnchor.constraint(equalTo: contentView.topAnchor),
            updateBanner.leadingAnchor.constraint(equalTo: contentView.leadingAnchor),
            updateBanner.trailingAnchor.constraint(equalTo: contentView.trailingAnchor),

            contentContainer.topAnchor.constraint(equalTo: updateBanner.bottomAnchor),
            contentContainer.leadingAnchor.constraint(equalTo: contentView.leadingAnchor),
            contentContainer.trailingAnchor.constraint(equalTo: contentView.trailingAnchor),
            contentContainer.bottomAnchor.constraint(equalTo: contentView.bottomAnchor),
        ])

        // Window hover tracking for arc block styling
        setupWindowHoverTracking(contentView: contentView)

        setupPanelPopovers()

        // Create dashboard
        let savedLayout = DashboardLayout(rawValue: config.dashboardLayout) ?? .leftRight
        let dashboard = DashboardViewController()
        dashboard.dashboardDelegate = self
        dashboard.currentLayout = savedLayout
        dashboard.setZoomIndex(config.zoomIndex)
        dashboardVC = dashboard

        embedViewController(dashboard)
        updateTitleBar()


        // Set title bar layout state
        titleBar.setCurrentLayout(savedLayout)

        applyWindowBackgroundStyle()
        positionStandardWindowButtons()
    }

    private func setupPanelPopovers() {
        notificationPanel.delegate = self
        notificationPanel.frame = NSRect(x: 0, y: 0, width: 360, height: 460)
        notificationPopover.contentSize = notificationPanel.frame.size
        notificationPopover.behavior = .transient
        notificationPopover.animates = true
        notificationPopover.delegate = self
        notificationPopover.contentViewController = ViewHostController(hostedView: notificationPanel)

        aiPanel.delegate = self
        aiPanel.frame = NSRect(x: 0, y: 0, width: 360, height: 460)
        aiPopover.contentSize = aiPanel.frame.size
        aiPopover.behavior = .transient
        aiPopover.animates = true
        aiPopover.delegate = self
        aiPopover.contentViewController = ViewHostController(hostedView: aiPanel)
    }

    private func applyWindowBackgroundStyle() {
        guard let window else { return }
        let isDark = window.effectiveAppearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
        let config = Self.glassBackgroundConfig(isDark: isDark)

        backgroundEffectView.material = config.material
        backgroundEffectView.blendingMode = config.blendingMode
        backgroundEffectView.isHidden = !config.enabled

        window.isOpaque = !config.enabled
        window.backgroundColor = config.enabled ? .clear : Theme.background
    }

    private func setupNativeTitleBar() {
        guard let window else { return }

        let toolbar = NSToolbar(identifier: "pmux.mainToolbar")
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

        let y = Self.trafficLightButtonOriginY(containerHeight: container.bounds.height, buttonHeight: close.frame.height)
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
        titleBar.projects = workspaceManager.tabs.map { $0.displayName }
        titleBar.currentView = activeTabIndex == 0 ? "dashboard" : "project"
        if activeTabIndex > 0 {
            let repoIndex = activeTabIndex - 1
            titleBar.currentProject = workspaceManager.tab(at: repoIndex)?.displayName ?? ""
        } else {
            titleBar.currentProject = ""
        }

        titleBar.projectStatusProvider = { [weak self] projectName -> String in
            guard let self else { return "idle" }
            guard let tab = self.workspaceManager.tabs.first(where: { $0.displayName == projectName }) else { return "idle" }
            var hasError = false, hasWaiting = false, hasRunning = false
            for wt in tab.worktrees {
                switch self.statusPublisher.status(for: wt.path) {
                case .error: hasError = true
                case .waiting: hasWaiting = true
                case .running: hasRunning = true
                default: break
                }
            }
            if hasError { return "error" }
            if hasWaiting { return "waiting" }
            if hasRunning { return "running" }
            return "idle"
        }

        titleBar.renderTabs()
    }


    // Cache worktree path -> repo path mapping to avoid repeated git calls
    private var worktreeRepoCache: [String: String] = [:]

    /// Build AgentDisplayInfo array from current worktree data
    private func buildAgentDisplayInfos() -> [AgentDisplayInfo] {
        return AgentHead.shared.allAgents().compactMap { agent in
            guard let surface = agent.surface else { return nil }
            return AgentDisplayInfo(
                id: agent.id,
                name: agent.branch,
                project: agent.project,
                thread: agent.branch,
                status: agent.status.rawValue.lowercased(),
                lastMessage: agent.lastMessage.isEmpty ? "No active task." : agent.lastMessage,
                totalDuration: AgentDisplayHelpers.formatDuration(agent.totalDuration),
                roundDuration: AgentDisplayHelpers.formatDuration(agent.roundDuration),
                surface: surface
            )
        }
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
        guard !config.workspacePaths.contains(path) else {
            if let tabIndex = workspaceManager.tabs.firstIndex(where: { $0.repoPath == path }) {
                switchToTab(tabIndex + 1)
            }
            return
        }

        config.workspacePaths.append(path)
        config.save()

        WorktreeDiscovery.discoverAsync(repoPath: path) { [weak self] worktrees in
            guard let self else { return }
            _ = self.integrateDiscoveredRepoForTesting(repoPath: path, worktrees: worktrees)
        }
    }

    @discardableResult
    func integrateDiscoveredRepoForTesting(repoPath: String, worktrees: [WorktreeInfo], activateTab: Bool = true) -> Int {
        let effectiveWorktrees: [WorktreeInfo]
        if worktrees.isEmpty {
            effectiveWorktrees = [WorktreeInfo(path: repoPath, branch: "main", commitHash: "", isMainWorktree: true)]
        } else {
            effectiveWorktrees = worktrees
        }

        for info in effectiveWorktrees {
            let surface = createSurface(for: info)
            allWorktrees.append((info: info, surface: surface))
        }

        let tabIndex = workspaceManager.addTab(repoPath: repoPath, worktrees: worktrees.isEmpty ? [] : worktrees)
        let projectName = workspaceManager.tabs.first(where: { $0.repoPath == repoPath })?.displayName
            ?? URL(fileURLWithPath: repoPath).lastPathComponent

        let now = MainWindowController.iso8601.string(from: Date())
        for info in effectiveWorktrees {
            if config.worktreeStartedAt[info.path] == nil {
                config.worktreeStartedAt[info.path] = now
            }
            let surface = createSurface(for: info)
            let started = config.worktreeStartedAt[info.path].flatMap { MainWindowController.iso8601.date(from: $0) }
            let sessionName = runtimeBackend == "local" ? nil : SessionManager.persistentSessionName(for: info.path)
            AgentHead.shared.register(surface: surface, worktreePath: info.path, branch: info.branch, project: projectName, startedAt: started, tmuxSessionName: sessionName, backend: runtimeBackend)
        }
        config.save()

        dashboardVC?.updateAgents(buildAgentDisplayInfos())
        statusPublisher.updateSurfaces(surfaces)
        updateTitleBar()
        if activateTab {
            switchToTab(tabIndex + 1)
        }
        return tabIndex
    }

    // MARK: - Tab Switching

    private func switchToTab(_ index: Int) {
        guard index != activeTabIndex else { return }

        // Detach terminals from current repo view
        if activeTabIndex > 0 {
            let repoIndex = activeTabIndex - 1
            if let tab = workspaceManager.tab(at: repoIndex),
               let repoVC = repoVCs[tab.repoPath] {
                repoVC.detachActiveTerminal()
            }
        } else {
            // Detach dashboard terminals when leaving dashboard
            dashboardVC?.detachTerminals()
        }

        activeTabIndex = index

        if index == 0 {
            if let dashboard = dashboardVC {
                embedViewController(dashboard)
                dashboard.updateAgents(buildAgentDisplayInfos())
            }
        } else {
            let repoIndex = index - 1
            guard let tab = workspaceManager.tab(at: repoIndex) else { return }
            let repoVC = getOrCreateRepoVC(for: tab)
            embedViewController(repoVC)
        }

        updateTitleBar()
        updateStatusPollPreferences()

    }

    private func updateStatusPollPreferences() {
        guard activeTabIndex > 0 else {
            statusPublisher.setPreferredPaths([])
            return
        }

        let repoIndex = activeTabIndex - 1
        guard let tab = workspaceManager.tab(at: repoIndex) else {
            statusPublisher.setPreferredPaths([])
            return
        }
        statusPublisher.setPreferredPaths(tab.worktrees.map(\.path))
    }

    private func getOrCreateRepoVC(for tab: WorkspaceTab) -> RepoViewController {
        if let existing = repoVCs[tab.repoPath] {
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
        WorktreeDiscovery.discoverAsync(repoPath: repoPath) { [weak self] worktrees in
            guard let self else { return }
            _ = self.integrateDiscoveredRepoForTesting(repoPath: repoPath, worktrees: worktrees)
        }
    }

    // MARK: - Panel Management

    private func closeBothPanels() {
        notificationPopover.performClose(nil)
        aiPopover.performClose(nil)
        notificationPanel.setOpen(false, animated: false)
        aiPanel.setOpen(false, animated: false)
    }

    private func toggleNotificationPanel() {
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

        let anchor = titleBar.notificationsAnchorView()
        notificationPopover.show(relativeTo: anchor.bounds, of: anchor, preferredEdge: .maxY)
    }

    private func toggleAIPanel() {
        if aiPopover.isShown {
            aiPopover.performClose(nil)
            aiPanel.setOpen(false, animated: false)
            return
        }

        notificationPopover.performClose(nil)
        notificationPanel.setOpen(false, animated: false)

        aiPanel.setOpen(true, animated: false)
        let anchor = titleBar.aiAnchorView()
        aiPopover.show(relativeTo: anchor.bounds, of: anchor, preferredEdge: .maxY)
    }

    // MARK: - Modal Helpers

    private func showCloseProjectModal(_ projectName: String) {
        closeBothPanels()

        let alert = NSAlert()
        alert.messageText = "Close \"\(projectName)\"?"
        alert.informativeText = "This will close all terminals and kill persisted sessions for this repository."
        alert.alertStyle = .warning
        alert.addButton(withTitle: "Close")
        alert.addButton(withTitle: "Cancel")

        if alert.runModal() == .alertFirstButtonReturn {
            performCloseRepo(projectName: projectName)
        }
    }

    private func showAddProjectModal() {
        addRepoViaOpenPanel()
    }

    private func showNewThreadModal() {
        closeBothPanels()

        let alert = NSAlert()
        alert.messageText = "New Thread"
        alert.informativeText = "Create a new branch/thread for the current project."
        alert.alertStyle = .informational

        let input = NSTextField(frame: NSRect(x: 0, y: 0, width: 260, height: 24))
        input.placeholderString = "branch-name"
        alert.accessoryView = input

        alert.addButton(withTitle: "Create")
        alert.addButton(withTitle: "Cancel")

        let response = alert.runModal()
        if response == .alertFirstButtonReturn {
            showNewBranchDialog()
        }
    }

    // MARK: - Workspace Loading

    private func loadWorkspaces() {
        let repoPaths = config.workspacePaths
        let cardOrder = config.cardOrder

        // Discover all worktrees on a background queue, then update UI on main
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            guard let self else { return }

            // Discover worktrees for all repos (single pass, no duplication)
            var discoveredWorktrees: [(repoPath: String, worktrees: [WorktreeInfo])] = []
            for repoPath in repoPaths {
                let worktrees = WorktreeDiscovery.discover(repoPath: repoPath)
                discoveredWorktrees.append((repoPath, worktrees))
            }

            DispatchQueue.main.async { [weak self] in
                guard let self else { return }

                var allWorktreeInfos: [(info: WorktreeInfo, surface: TerminalSurface)] = []

                for (repoPath, worktrees) in discoveredWorktrees {
                    if worktrees.isEmpty {
                        let info = WorktreeInfo(
                            path: repoPath,
                            branch: "main",
                            commitHash: "",
                            isMainWorktree: true
                        )
                        let surface = self.createSurface(for: info)
                        allWorktreeInfos.append((info: info, surface: surface))
                    } else {
                        for info in worktrees {
                            let surface = self.createSurface(for: info)
                            allWorktreeInfos.append((info: info, surface: surface))
                        }
                    }

                    // Reuse already-discovered worktrees for tab creation (no second discover call)
                    _ = self.workspaceManager.addTab(repoPath: repoPath, worktrees: worktrees)
                }

                // Record startedAt for newly discovered worktrees
                let now = MainWindowController.iso8601.string(from: Date())
                var configChanged = false
                for (info, _) in allWorktreeInfos {
                    if self.config.worktreeStartedAt[info.path] == nil {
                        self.config.worktreeStartedAt[info.path] = now
                        configChanged = true
                    }
                }
                if configChanged { self.config.save() }

                // Apply saved card order
                if !cardOrder.isEmpty {
                    allWorktreeInfos.sort { a, b in
                        let ai = cardOrder.firstIndex(of: a.info.path) ?? Int.max
                        let bi = cardOrder.firstIndex(of: b.info.path) ?? Int.max
                        return ai < bi
                    }
                }

                self.allWorktrees = allWorktreeInfos

                // Register all agents with AgentHead
                for (info, surface) in allWorktreeInfos {
                    let repo = self.worktreeRepoCache[info.path] ?? WorktreeDiscovery.findRepoRoot(from: info.path) ?? info.path
                    let proj = self.workspaceManager.tabs.first(where: { $0.repoPath == repo })?.displayName
                        ?? URL(fileURLWithPath: repo).lastPathComponent
                    let started = self.config.worktreeStartedAt[info.path].flatMap { MainWindowController.iso8601.date(from: $0) }
                    let sessionName = self.runtimeBackend == "local" ? nil : SessionManager.persistentSessionName(for: info.path)
                    AgentHead.shared.register(surface: surface, worktreePath: info.path, branch: info.branch, project: proj, startedAt: started, tmuxSessionName: sessionName, backend: self.runtimeBackend)
                }
                if !cardOrder.isEmpty {
                    AgentHead.shared.reorder(paths: cardOrder)
                }

                self.dashboardVC?.updateAgents(self.buildAgentDisplayInfos())
                self.updateTitleBar()

                if allWorktreeInfos.isEmpty {
                    NSLog("No workspaces configured. Add paths to ~/.config/pmux/config.json")
                }

                // Start polling for agent status
                self.statusPublisher.start(surfaces: self.surfaces)
                self.updateStatusPollPreferences()

                // Start webhook server for agent hook events
                if self.config.webhook.enabled {
                    let server = WebhookServer(port: self.config.webhook.port) { [weak self] event in
                        self?.statusPublisher.webhookProvider.handleEvent(event)
                        AgentHead.shared.handleWebhookEvent(event)
                    }
                    server.start()
                    self.webhookServer = server
                }
            }
        }
    }

    private func createSurface(for info: WorktreeInfo) -> TerminalSurface {
        if let existing = surfaces[info.path] {
            return existing
        }
        let surface = TerminalSurface()
        if runtimeBackend != "local" {
            surface.sessionName = SessionManager.persistentSessionName(for: info.path)
            surface.backend = runtimeBackend
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
        allWorktrees.removeAll { $0.info.path == info.path }
        worktreeRepoCache.removeValue(forKey: info.path)
        if let agent = AgentHead.shared.agent(forWorktree: info.path) {
            AgentHead.shared.unregister(terminalID: agent.id)
        }
        dashboardVC?.updateAgents(buildAgentDisplayInfos())
        statusPublisher.updateSurfaces(surfaces)

        if activeTabIndex > 0 {
            let repoIndex = activeTabIndex - 1
            if let tab = workspaceManager.tab(at: repoIndex) {
                let repoPath = tab.repoPath
                let displayName = tab.displayName
                WorktreeDiscovery.discoverAsync(repoPath: repoPath) { [weak self] updatedWorktrees in
                    guard let self else { return }
                    self.workspaceManager.updateWorktrees(at: repoIndex, worktrees: updatedWorktrees)
                    if let repoVC = self.repoVCs[repoPath] {
                        repoVC.configure(worktrees: updatedWorktrees, surfaces: self.surfaces)
                    }
                    if updatedWorktrees.isEmpty {
                        self.performCloseRepo(projectName: displayName)
                    }
                }
            }
        }

        updateTitleBar()

    }

    deinit {
        NotificationCenter.default.removeObserver(self, name: .navigateToWorktree, object: nil)
    }

    // MARK: - Close Repo

    private func performCloseRepo(projectName: String) {
        guard let tabIndex = workspaceManager.tabs.firstIndex(where: { $0.displayName == projectName }) else { return }
        let tab = workspaceManager.tabs[tabIndex]

        // Kill persisted sessions and destroy surfaces for this repo's worktrees
        for worktree in tab.worktrees {
            guard let surface = surfaces[worktree.path] else { continue }
            surface.destroy()
            surfaces.removeValue(forKey: worktree.path)
            
            if let agent = AgentHead.shared.agent(forWorktree: worktree.path) {
                AgentHead.shared.unregister(terminalID: agent.id)
            } else {
                AgentHead.shared.unregister(terminalID: surface.id)
            }
            if runtimeBackend != "local" {
                let sessionName = SessionManager.persistentSessionName(for: worktree.path)
                SessionManager.killSession(sessionName, backend: runtimeBackend)
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
        workspaceManager.removeTab(at: tabIndex)

        // Adjust active tab index
        let uiTabIndex = tabIndex + 1  // +1 because dashboard is 0
        if activeTabIndex >= uiTabIndex {
            activeTabIndex = max(0, activeTabIndex - 1)
        }

        // Update UI
        dashboardVC?.updateAgents(buildAgentDisplayInfos())
        statusPublisher.updateSurfaces(surfaces)
        updateTitleBar()
        switchToTab(activeTabIndex)

    }

}

class PmuxWindow: NSWindow {
    override func sendEvent(_ event: NSEvent) {
        if event.type == .keyDown {
            if event.keyCode == 53, MainWindowController.shouldHandleEscShortcut() {
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
        webhookServer?.stop()
        webhookServer = nil
        for (_, surface) in surfaces {
            surface.destroy()
        }
        surfaces.removeAll()
    }

    func cleanupBeforeTermination() {
        statusPublisher.stop()
        webhookServer?.stop()
        webhookServer = nil
        for (_, surface) in surfaces {
            surface.destroy()
        }
        surfaces.removeAll()
    }
}

// MARK: - TitleBarDelegate

extension MainWindowController: TitleBarDelegate {
    func titleBarDidSelectDashboard() {
        switchToTab(0)
    }

    func titleBarDidSelectProject(_ projectName: String) {
        guard let tabIndex = workspaceManager.tabs.firstIndex(where: { $0.displayName == projectName }) else { return }
        switchToTab(tabIndex + 1)
    }

    func titleBarDidRequestCloseProject(_ projectName: String) {
        showCloseProjectModal(projectName)
    }

    func titleBarDidRequestAddProject() {
        showAddProjectModal()
    }

    func titleBarDidRequestNewThread() {
        showNewThreadModal()
    }

    func titleBarDidSelectLayout(_ layout: DashboardLayout) {
        dashboardVC?.setLayout(layout)
        config.dashboardLayout = layout.rawValue
        config.save()
        titleBar.setCurrentLayout(layout)
    }

    func titleBarDidToggleNotifications() {
        toggleNotificationPanel()
    }

    func titleBarDidToggleAI() {
        toggleAIPanel()
    }

    func titleBarDidToggleTheme() {
        let isDark = window?.effectiveAppearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
        let next: ThemeMode = isDark ? .light : .dark
        config.themeMode = next.rawValue
        config.save()
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
        if let appearance = window?.appearance {
            NSAppearance.current = appearance
        }
        applyWindowBackgroundStyle()
    }
    
    func titleBarDidRequestCloseWindow() {
        window?.close()
    }
    
    func titleBarDidRequestMiniaturizeWindow() {
        window?.miniaturize(nil)
    }
    
    func titleBarDidRequestZoomWindow() {
        window?.zoom(nil)
    }
}

// MARK: - DashboardDelegate

extension MainWindowController: DashboardDelegate {
    func dashboardDidSelectProject(_ project: String, thread: String) {
        guard let tab = workspaceManager.tabs.first(where: { $0.displayName == project }) else { return }
        let tabIndex = workspaceManager.tabs.firstIndex(where: { $0.repoPath == tab.repoPath }) ?? 0
        switchToTab(tabIndex + 1)
        if let repoVC = repoVCs[tab.repoPath] {
            repoVC.selectWorktree(branch: thread)
        }
    }

    func dashboardDidRequestEnterProject(_ project: String) {
        guard let tabIndex = workspaceManager.tabs.firstIndex(where: { $0.displayName == project }) else { return }
        switchToTab(tabIndex + 1)
    }

    func dashboardDidReorderCards(order: [String]) {
        // order contains terminal IDs; map back to worktree paths for config persistence
        let paths = order.compactMap { AgentHead.shared.agent(for: $0)?.worktreePath }
        config.cardOrder = paths
        config.save()
    }

    func dashboardDidRequestDelete(_ terminalID: String) {
        guard let agent = AgentHead.shared.agent(for: terminalID) else { return }
        let worktreePath = agent.worktreePath
        guard let item = allWorktrees.first(where: { $0.info.path == worktreePath }) else { return }
        confirmAndDeleteWorktree(item.info)
    }

    func dashboardDidRequestAddProject() {
        addRepoViaOpenPanel()
    }
}

// MARK: - RepoViewDelegate

extension MainWindowController: RepoViewDelegate {
    func repoView(_ repoVC: RepoViewController, didRequestDeleteWorktree info: WorktreeInfo) {
        confirmAndDeleteWorktree(info)
    }

    func repoViewDidRequestNewThread(_ repoVC: RepoViewController) {
        showNewBranchDialog()
    }

    func repoView(_ repoVC: RepoViewController, didRequestShowDiffForWorktreePath worktreePath: String) {
        presentDiffOverlay(for: worktreePath)
    }
}

// MARK: - NotificationPanelDelegate

extension MainWindowController: NotificationPanelDelegate {
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

extension MainWindowController: AIPanelDelegate {
    func aiPanelDidRequestClose() {
        aiPopover.performClose(nil)
        aiPanel.setOpen(false, animated: false)
    }
}

// MARK: - NewBranchDialogDelegate

extension MainWindowController: NewBranchDialogDelegate {
    func newBranchDialog(_ dialog: NewBranchDialog, didCreateWorktree info: WorktreeInfo, inRepo repoPath: String) {
        let surface = createSurface(for: info)
        allWorktrees.append((info: info, surface: surface))

        // Record startedAt for the new worktree
        if config.worktreeStartedAt[info.path] == nil {
            config.worktreeStartedAt[info.path] = MainWindowController.iso8601.string(from: Date())
            config.save()
        }

        dashboardVC?.updateAgents(buildAgentDisplayInfos())
        statusPublisher.updateSurfaces(surfaces)

        if activeTabIndex != 0 {
            switchToTab(0)
        }
    }
}

// MARK: - StatusPublisherDelegate

extension MainWindowController: StatusPublisherDelegate {
    func statusDidChange(worktreePath: String, oldStatus: AgentStatus, newStatus: AgentStatus, lastMessage: String) {
        let branch = allWorktrees.first(where: { $0.info.path == worktreePath })?.info.branch ?? ""

        NotificationManager.shared.notify(
            worktreePath: worktreePath,
            branch: branch,
            oldStatus: oldStatus,
            newStatus: newStatus,
            lastMessage: lastMessage
        )

        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            // Update dashboard with fresh agent data
            self.dashboardVC?.updateAgents(self.buildAgentDisplayInfos())
            // Update repo VC if showing
            if self.activeTabIndex > 0 {
                let repoIndex = self.activeTabIndex - 1
                if let tab = self.workspaceManager.tab(at: repoIndex),
                   let repoVC = self.repoVCs[tab.repoPath] {
                    repoVC.updateStatus(for: worktreePath, status: newStatus, lastMessage: lastMessage)
                }
            }
            self.updateTitleBar()
        }
    }
}

// MARK: - Notification Navigation

extension MainWindowController {
    @objc private func handleNavigateToWorktree(_ notification: Notification) {
        guard let worktreePath = notification.userInfo?["worktreePath"] as? String else { return }

        // Check already-open tabs first (no git calls needed)
        if let tabIndex = workspaceManager.tabs.firstIndex(where: { tab in
            tab.worktrees.contains(where: { $0.path == worktreePath })
        }) {
            let repoPath = workspaceManager.tabs[tabIndex].repoPath
            switchToTab(tabIndex + 1)
            if let repoVC = repoVCs[repoPath] {
                repoVC.selectWorktree(byPath: worktreePath)
            }
            return
        }

        // Fall back: search workspace paths asynchronously
        let workspacePaths = config.workspacePaths
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            var foundRepoPath: String?
            for wsPath in workspacePaths {
                let worktrees = WorktreeDiscovery.discover(repoPath: wsPath)
                if worktrees.contains(where: { $0.path == worktreePath }) {
                    foundRepoPath = wsPath
                    break
                }
            }
            DispatchQueue.main.async {
                guard let self, let repoPath = foundRepoPath else { return }
                self.openRepoTab(repoPath: repoPath)
                if let repoVC = self.repoVCs[repoPath] {
                    repoVC.selectWorktree(byPath: worktreePath)
                }
            }
        }
    }
}

// MARK: - SettingsDelegate

extension MainWindowController: SettingsDelegate {
    func settingsDidUpdateConfig(_ settings: SettingsViewController, config: Config) {
        let oldPaths = Set(self.config.workspacePaths)
        self.config = config
        normalizeBackendAvailabilityIfNeeded()

        let newPaths = Set(config.workspacePaths)
        if oldPaths != newPaths {
            loadWorkspaces()
        }
    }
}

// MARK: - QuickSwitcherDelegate

extension MainWindowController: QuickSwitcherDelegate {
    func quickSwitcher(_ vc: QuickSwitcherViewController, didSelect worktree: WorktreeInfo) {
        // Find the repo containing this worktree and open it
        let repoPath = WorktreeDiscovery.findRepoRoot(from: worktree.path) ?? worktree.path
        if let tabIndex = workspaceManager.tabs.firstIndex(where: { $0.repoPath == repoPath }) {
            switchToTab(tabIndex + 1)
            if let repoVC = repoVCs[repoPath] {
                repoVC.selectWorktree(byPath: worktree.path)
            }
        } else {
            openRepoTab(repoPath: repoPath)
            if let repoVC = repoVCs[repoPath] {
                repoVC.selectWorktree(byPath: worktree.path)
            }
        }
    }
}

// MARK: - Auto-Update

extension MainWindowController {
    func setupAutoUpdate() {
        guard config.autoUpdate.enabled else { return }
        updateChecker.delegate = self
        updateChecker.skippedVersion = config.autoUpdate.skippedVersion
        updateManager.delegate = self
        updateChecker.startPolling(intervalHours: config.autoUpdate.checkIntervalHours)
    }

    @objc func checkForUpdates() {
        Task {
            do {
                if let release = try await updateChecker.checkNow() {
                    pendingRelease = release
                    updateBanner.showNewVersion(release.version)
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
}

// MARK: - UpdateCheckerDelegate

extension MainWindowController: UpdateCheckerDelegate {
    func updateChecker(_ checker: UpdateChecker, didFindRelease release: ReleaseInfo) {
        pendingRelease = release
        updateBanner.showNewVersion(release.version)
    }
}

// MARK: - UpdateManagerDelegate

extension MainWindowController: UpdateManagerDelegate {
    func updateManager(_ manager: UpdateManager, didChangeState state: UpdateManager.State) {
        updateBanner.update(state: state)
    }
}

// MARK: - NotificationHistoryDelegate

extension MainWindowController: NotificationHistoryDelegate {
    func notificationHistory(_ vc: NotificationHistoryViewController, didSelectWorktreePath path: String) {
        NotificationCenter.default.post(
            name: .navigateToWorktree,
            object: nil,
            userInfo: ["worktreePath": path]
        )
    }
}

// MARK: - UpdateBannerDelegate

extension MainWindowController: UpdateBannerDelegate {
    func updateBannerDidClickInstall(_ banner: UpdateBanner) {
        guard let release = pendingRelease else { return }
        updateManager.download(release: release)
    }

    func updateBannerDidClickSkip(_ banner: UpdateBanner) {
        config.autoUpdate.skippedVersion = banner.version
        config.save()
        updateChecker.skippedVersion = banner.version
        updateBanner.dismiss()
        pendingRelease = nil
    }

    func updateBannerDidClickRestart(_ banner: UpdateBanner) {
        updateManager.installAndRestart()
    }

    func updateBannerDidClickRetry(_ banner: UpdateBanner) {
        guard let release = pendingRelease else { return }
        updateManager.download(release: release)
    }
}

extension MainWindowController: NSPopoverDelegate {
    func popoverDidClose(_ notification: Notification) {
        guard let popover = notification.object as? NSPopover else { return }
        if popover === notificationPopover {
            notificationPanel.setOpen(false, animated: false)
        } else if popover === aiPopover {
            aiPanel.setOpen(false, animated: false)
        }
    }
}

private final class ViewHostController: NSViewController {
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
