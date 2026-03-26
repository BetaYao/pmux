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
    private let workspaceManager = WorkspaceManager()

    // Terminal management
    private lazy var terminalCoordinator: TerminalCoordinator = {
        let tc = TerminalCoordinator(config: config, currentRepoVC: { [weak self] in
            self?.currentRepoVC
        })
        tc.delegate = self
        return tc
    }()
    private var allWorktrees: [(info: WorktreeInfo, tree: SplitTree)] = []

    private static let iso8601: ISO8601DateFormatter = {
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withInternetDateTime]
        return f
    }()

    // Repo views, keyed by repo path
    private var repoVCs: [String: RepoViewController] = [:]
    private var activeTabIndex: Int = 0  // 0 = Dashboard

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
    private var branchRefreshTimer: Timer?

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
        updateCoordinator.setup(config: config)
        normalizeBackendAvailabilityIfNeeded()
        loadWorkspaces()

        NotificationCenter.default.addObserver(
            self, selector: #selector(handleNavigateToWorktree(_:)),
            name: .navigateToWorktree, object: nil
        )
    }

    // MARK: - Menu Shortcuts

    private func setupMenuShortcuts() {
        NSApp.mainMenu = MenuBuilder.buildMainMenu(target: self)
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

    @objc func switchToDashboard() {
        switchToTab(0)
    }

    @objc func showQuickSwitcher() {
        let worktreeInfos = allWorktrees.map { $0.info }
        var statuses: [String: AgentStatus] = [:]
        for (path, _) in terminalCoordinator.surfaceManager.all {
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

    @objc func showSettings() {
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

    @objc func showNewBranchDialog() {
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

    @objc func closeCurrentTab() {
        guard activeTabIndex > 0 else { return }
        let repoIndex = activeTabIndex - 1
        guard let tab = workspaceManager.tab(at: repoIndex) else { return }
        showCloseProjectModal(tab.displayName)
    }

    /// Cmd+W: close focused pane if multiple panes, otherwise close tab.
    @objc func closePaneOrTab() {
        if let repoVC = currentRepoVC,
           let tree = repoVC.activeSplitContainer?.tree,
           tree.leafCount > 1 {
            closeFocusedPane()
        } else {
            closeCurrentTab()
        }
    }

    @objc func selectNextTab() {
        let maxIndex = workspaceManager.tabs.count // 0=dashboard, 1..N=projects
        let next = activeTabIndex + 1 > maxIndex ? 0 : activeTabIndex + 1
        switchToTab(next)
    }

    @objc func selectPreviousTab() {
        let maxIndex = workspaceManager.tabs.count
        let prev = activeTabIndex - 1 < 0 ? maxIndex : activeTabIndex - 1
        switchToTab(prev)
    }

    @objc func showKeyboardShortcuts() {
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

    @objc func openDocumentation() {
        if let url = URL(string: "https://github.com/nicematt/pmux") {
            NSWorkspace.shared.open(url)
        }
    }

    @objc func dashboardZoomIn() {
        dashboardVC?.zoomIn()
        config.zoomIndex = dashboardVC?.zoomIndex ?? GridLayout.defaultZoomIndex
        config.save()
    }

    @objc func dashboardZoomOut() {
        dashboardVC?.zoomOut()
        config.zoomIndex = dashboardVC?.zoomIndex ?? GridLayout.defaultZoomIndex
        config.save()
    }

    @objc func showDiffOverlay() {
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
        dashboardVC = dashboard

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
        let agents = AgentHead.shared.allAgents()
        var seen = Set<String>()
        var result: [AgentDisplayInfo] = []

        for agent in agents {
            guard let surface = agent.surface else { continue }
            guard !seen.contains(agent.worktreePath) else { continue }
            seen.insert(agent.worktreePath)

            let tree = terminalCoordinator.surfaceManager.tree(forPath: agent.worktreePath)
            let paneCount = tree?.leafCount ?? 1
            let paneSurfaces: [TerminalSurface] = tree?.allLeaves.compactMap {
                SurfaceRegistry.shared.surface(forId: $0.surfaceId)
            } ?? [surface]

            let ws = statusAggregator.status(for: agent.worktreePath)
            let paneStatuses = ws?.statuses ?? [agent.status]
            let mostRecentMessage = ws?.mostRecentMessage ?? (agent.lastMessage.isEmpty ? "No active task." : agent.lastMessage)
            let mostRecentPaneIndex = ws?.mostRecentPaneIndex ?? 1

            result.append(AgentDisplayInfo(
                id: agent.id,
                name: agent.branch,
                project: agent.project,
                thread: agent.branch,
                paneStatuses: paneStatuses,
                mostRecentMessage: mostRecentMessage,
                mostRecentPaneIndex: mostRecentPaneIndex,
                totalDuration: AgentDisplayHelpers.formatDuration(agent.totalDuration),
                roundDuration: AgentDisplayHelpers.formatDuration(agent.roundDuration),
                surface: surface,
                worktreePath: agent.worktreePath,
                paneCount: paneCount,
                paneSurfaces: paneSurfaces
            ))
        }
        return result
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
            let tree = terminalCoordinator.surfaceManager.tree(for: info, backend: runtimeBackend)
            allWorktrees.append((info: info, tree: tree))
        }

        let tabIndex = workspaceManager.addTab(repoPath: repoPath, worktrees: worktrees.isEmpty ? [] : worktrees)
        let projectName = workspaceManager.tabs.first(where: { $0.repoPath == repoPath })?.displayName
            ?? URL(fileURLWithPath: repoPath).lastPathComponent

        let now = MainWindowController.iso8601.string(from: Date())
        for info in effectiveWorktrees {
            if config.worktreeStartedAt[info.path] == nil {
                config.worktreeStartedAt[info.path] = now
            }
            if let surface = terminalCoordinator.surfaceManager.primarySurface(forPath: info.path) {
                let started = config.worktreeStartedAt[info.path].flatMap { MainWindowController.iso8601.date(from: $0) }
                let sessionName = runtimeBackend == "local" ? nil : SessionManager.persistentSessionName(for: info.path)
                AgentHead.shared.register(surface: surface, worktreePath: info.path, branch: info.branch, project: projectName, startedAt: started, tmuxSessionName: sessionName, backend: runtimeBackend)
            }
        }
        config.save()

        dashboardVC?.updateAgents(buildAgentDisplayInfos())
        statusPublisher.updateSurfaces(terminalCoordinator.surfaceManager.all)
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
            existing.reconfigurePreservingSelection(worktrees: tab.worktrees, trees: terminalCoordinator.surfaceManager.all)
            return existing
        }

        let repoVC = RepoViewController()
        repoVC.repoDelegate = self
        repoVC.configure(worktrees: tab.worktrees, trees: terminalCoordinator.surfaceManager.all)
        repoVCs[tab.repoPath] = repoVC
        return repoVC
    }

    private func openRepoTab(repoPath: String, completion: (() -> Void)? = nil) {
        WorktreeDiscovery.discoverAsync(repoPath: repoPath) { [weak self] worktrees in
            guard let self else { return }
            _ = self.integrateDiscoveredRepoForTesting(repoPath: repoPath, worktrees: worktrees)
            completion?()
        }
    }


    // MARK: - Modal Helpers

    private func showCloseProjectModal(_ projectName: String) {
        panelCoordinator.closeBothPanels()

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
        panelCoordinator.closeBothPanels()

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

                var allWorktreeInfos: [(info: WorktreeInfo, tree: SplitTree)] = []

                for (repoPath, worktrees) in discoveredWorktrees {
                    if worktrees.isEmpty {
                        let info = WorktreeInfo(
                            path: repoPath,
                            branch: "main",
                            commitHash: "",
                            isMainWorktree: true
                        )
                        let tree = self.terminalCoordinator.resolveTree(for: info)
                        allWorktreeInfos.append((info: info, tree: tree))
                        self.worktreeRepoCache[info.path] = repoPath
                    } else {
                        for info in worktrees {
                            let tree = self.terminalCoordinator.resolveTree(for: info)
                            allWorktreeInfos.append((info: info, tree: tree))
                            self.worktreeRepoCache[info.path] = repoPath
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
                for (info, _) in allWorktreeInfos {
                    let repo = self.worktreeRepoCache[info.path] ?? WorktreeDiscovery.findRepoRoot(from: info.path) ?? info.path
                    let proj = self.workspaceManager.tabs.first(where: { $0.repoPath == repo })?.displayName
                        ?? URL(fileURLWithPath: repo).lastPathComponent
                    let started = self.config.worktreeStartedAt[info.path].flatMap { MainWindowController.iso8601.date(from: $0) }
                    let sessionName = self.runtimeBackend == "local" ? nil : SessionManager.persistentSessionName(for: info.path)
                    if let surface = self.terminalCoordinator.surfaceManager.primarySurface(forPath: info.path) {
                        AgentHead.shared.register(surface: surface, worktreePath: info.path, branch: info.branch, project: proj, startedAt: started, tmuxSessionName: sessionName, backend: self.runtimeBackend)
                    }
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
                self.statusPublisher.start(trees: self.terminalCoordinator.surfaceManager.all)
                self.updateStatusPollPreferences()

                // Start periodic branch name refresh
                self.startBranchRefreshTimer()

                // Start webhook server for agent hook events
                if self.config.webhook.enabled {
                    let server = WebhookServer(port: self.config.webhook.port) { [weak self] event in
                        self?.statusPublisher.webhookProvider.handleEvent(event)
                        AgentHead.shared.handleWebhookEvent(event)
                    }
                    server.start()
                    self.terminalCoordinator.webhookServer = server
                }
            }
        }
    }

    // MARK: - Worktree Deletion (forwarded to TerminalCoordinator)

    private func confirmAndDeleteWorktree(_ info: WorktreeInfo) {
        terminalCoordinator.confirmAndDeleteWorktree(info, window: window)
    }

    private func worktreeDidDelete(_ info: WorktreeInfo) {
        allWorktrees.removeAll { $0.info.path == info.path }
        worktreeRepoCache.removeValue(forKey: info.path)
        if let agent = AgentHead.shared.agent(forWorktree: info.path) {
            AgentHead.shared.unregister(terminalID: agent.id)
        }
        dashboardVC?.updateAgents(buildAgentDisplayInfos())
        statusPublisher.updateSurfaces(terminalCoordinator.surfaceManager.all)

        if activeTabIndex > 0 {
            let repoIndex = activeTabIndex - 1
            if let tab = workspaceManager.tab(at: repoIndex) {
                let repoPath = tab.repoPath
                let displayName = tab.displayName
                WorktreeDiscovery.discoverAsync(repoPath: repoPath) { [weak self] updatedWorktrees in
                    guard let self else { return }
                    self.workspaceManager.updateWorktrees(at: repoIndex, worktrees: updatedWorktrees)
                    if let repoVC = self.repoVCs[repoPath] {
                        repoVC.configure(worktrees: updatedWorktrees, trees: self.terminalCoordinator.surfaceManager.all)
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
            let primarySurface = terminalCoordinator.surfaceManager.primarySurface(forPath: worktree.path)
            terminalCoordinator.surfaceManager.removeTree(forPath: worktree.path)

            if let agent = AgentHead.shared.agent(forWorktree: worktree.path) {
                AgentHead.shared.unregister(terminalID: agent.id)
            } else if let primarySurface {
                AgentHead.shared.unregister(terminalID: primarySurface.id)
            }
            // Always kill the backend session regardless of whether a surface existed
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

        // Compute the target tab index after removal
        let uiTabIndex = tabIndex + 1  // +1 because dashboard is 0
        let targetTab: Int
        if activeTabIndex == uiTabIndex {
            // Closing the currently active tab — switch to the previous tab
            targetTab = max(0, uiTabIndex - 1)
        } else if activeTabIndex > uiTabIndex {
            targetTab = activeTabIndex - 1
        } else {
            targetTab = activeTabIndex
        }

        // Force view transition by setting a sentinel, then switching
        activeTabIndex = -1
        // Remove the old view immediately so the sidebar doesn't linger
        for child in contentContainer.subviews {
            child.removeFromSuperview()
        }

        // Update UI
        dashboardVC?.updateAgents(buildAgentDisplayInfos())
        statusPublisher.updateSurfaces(terminalCoordinator.surfaceManager.all)
        updateTitleBar()
        switchToTab(targetTab)

    }

    // MARK: - Current Repo VC Helper

    var currentRepoVC: RepoViewController? {
        guard activeTabIndex > 0 else { return nil }
        let repoIndex = activeTabIndex - 1
        guard let tab = workspaceManager.tab(at: repoIndex) else { return nil }
        return repoVCs[tab.repoPath]
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

class PmuxWindow: NSWindow {

    // performKeyEquivalent runs BEFORE menu item key equivalents,
    // so split pane shortcuts here take priority over menu bindings.
    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        guard event.type == .keyDown else { return super.performKeyEquivalent(with: event) }
        let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
        guard let mwc = windowController as? MainWindowController else {
            return super.performKeyEquivalent(with: event)
        }

        // Only handle split keybindings when a repo tab is active with split panes
        let hasSplitContext = mwc.currentRepoVC?.activeSplitContainer != nil

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
            if flags == [.command, .option] {
                switch event.keyCode {
                case 123: mwc.moveFocus(.horizontal, positive: false); return true
                case 124: mwc.moveFocus(.horizontal, positive: true); return true
                case 125: mwc.moveFocus(.vertical, positive: true); return true
                case 126: mwc.moveFocus(.vertical, positive: false); return true
                default: break
                }
            }

            // Cmd+Ctrl+Arrows: resize
            if flags == [.command, .control] {
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

        return super.performKeyEquivalent(with: event)
    }

    override func sendEvent(_ event: NSEvent) {
        if event.type == .keyDown {
            // Escape: exit spotlight (existing)
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
        branchRefreshTimer?.invalidate()
        branchRefreshTimer = nil
        terminalCoordinator.cleanup()
    }

    func cleanupBeforeTermination() {
        statusPublisher.stop()
        branchRefreshTimer?.invalidate()
        branchRefreshTimer = nil
        terminalCoordinator.cleanup()
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
        panelCoordinator.toggleNotificationPanel()
    }

    func titleBarDidToggleAI() {
        panelCoordinator.toggleAIPanel()
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

// MARK: - PanelCoordinatorDelegate

extension MainWindowController: PanelCoordinatorDelegate {
    func panelCoordinator(_ coordinator: PanelCoordinator, navigateToWorktreePath path: String) {
        // Navigation handled by NotificationCenter .navigateToWorktree
    }
}

// MARK: - NewBranchDialogDelegate

extension MainWindowController: NewBranchDialogDelegate {
    func newBranchDialog(_ dialog: NewBranchDialog, didCreateWorktree info: WorktreeInfo, inRepo repoPath: String) {
        let tree = terminalCoordinator.surfaceManager.tree(for: info, backend: runtimeBackend)
        allWorktrees.append((info: info, tree: tree))

        // Record startedAt for the new worktree
        if config.worktreeStartedAt[info.path] == nil {
            config.worktreeStartedAt[info.path] = MainWindowController.iso8601.string(from: Date())
            config.save()
        }

        dashboardVC?.updateAgents(buildAgentDisplayInfos())
        statusPublisher.updateSurfaces(terminalCoordinator.surfaceManager.all)

        // If we're on a repo tab for the same repo, stay there and update its sidebar
        if activeTabIndex > 0 {
            let repoIndex = activeTabIndex - 1
            if let tab = workspaceManager.tab(at: repoIndex),
               tab.repoPath == repoPath,
               let repoVC = repoVCs[repoPath] {
                // Add new worktree to the workspace tab and repo view
                var updatedWorktrees = tab.worktrees
                updatedWorktrees.append(info)
                workspaceManager.updateWorktrees(at: repoIndex, worktrees: updatedWorktrees)
                repoVC.addWorktree(info, tree: tree)
                return
            }
        }
    }
}

// MARK: - Branch Refresh

extension MainWindowController {
    func startBranchRefreshTimer() {
        branchRefreshTimer?.invalidate()
        branchRefreshTimer = Timer.scheduledTimer(withTimeInterval: 5.0, repeats: true) { [weak self] _ in
            self?.refreshBranches()
        }
    }

    private func refreshBranches() {
        // Re-discover worktrees for all repos on background queue
        let tabs = workspaceManager.tabs
        for (tabIndex, tab) in tabs.enumerated() {
            WorktreeDiscovery.discoverAsync(repoPath: tab.repoPath) { [weak self] freshWorktrees in
                guard let self else { return }
                let oldWorktrees = tab.worktrees

                // Check if any branch names changed (match by path)
                var changed = false
                for fresh in freshWorktrees {
                    if let old = oldWorktrees.first(where: { $0.path == fresh.path }),
                       old.branch != fresh.branch {
                        changed = true
                        break
                    }
                }
                guard changed else { return }

                // Update workspace manager
                self.workspaceManager.updateWorktrees(at: tabIndex, worktrees: freshWorktrees)

                // Update allWorktrees cache
                for (i, entry) in self.allWorktrees.enumerated() {
                    if let fresh = freshWorktrees.first(where: { $0.path == entry.info.path }) {
                        self.allWorktrees[i] = (info: fresh, tree: entry.tree)
                    }
                }

                // Push to repo VC sidebar
                if let repoVC = self.repoVCs[tab.repoPath] {
                    repoVC.updateWorktreeInfos(freshWorktrees)
                }

                // Refresh dashboard cards too
                self.dashboardVC?.updateAgents(self.buildAgentDisplayInfos())
            }
        }
    }
}

// MARK: - WorktreeStatusDelegate

extension MainWindowController: WorktreeStatusDelegate {
    func worktreeStatusDidUpdate(_ status: WorktreeStatus) {
        dashboardVC?.updateAgents(buildAgentDisplayInfos())
        // Update repo VC if showing
        if activeTabIndex > 0 {
            let repoIndex = activeTabIndex - 1
            if let tab = workspaceManager.tab(at: repoIndex),
               let repoVC = repoVCs[tab.repoPath] {
                let worktreePath = status.worktreePath
                let aggregated = status.highestPriority
                let message = status.mostRecentMessage
                repoVC.updateStatus(for: worktreePath, status: aggregated, lastMessage: message)
            }
        }
        updateTitleBar()
    }

    func paneStatusDidChange(worktreePath: String, paneIndex: Int, oldStatus: AgentStatus, newStatus: AgentStatus, lastMessage: String) {
        let branch = allWorktrees.first(where: { $0.info.path == worktreePath })?.info.branch ?? ""
        let paneCount = statusAggregator.status(for: worktreePath)?.panes.count ?? 1
        let terminalID = statusAggregator.status(for: worktreePath)?.panes.first(where: { $0.paneIndex == paneIndex })?.terminalID ?? ""
        NotificationManager.shared.notify(
            terminalID: terminalID,
            worktreePath: worktreePath,
            branch: branch,
            paneIndex: paneIndex,
            paneCount: paneCount,
            oldStatus: oldStatus,
            newStatus: newStatus,
            lastMessage: lastMessage
        )
    }
}

// MARK: - Notification Navigation

extension MainWindowController {
    @objc private func handleNavigateToWorktree(_ notification: Notification) {
        guard let worktreePath = notification.userInfo?["worktreePath"] as? String else {
            NSLog("navigateToWorktree: missing worktreePath in userInfo")
            return
        }
        let paneIndex = notification.userInfo?["paneIndex"] as? Int
        NSLog("navigateToWorktree: path=%@ paneIndex=%@", worktreePath, paneIndex.map { "\($0)" } ?? "nil")

        // Check already-open tabs first (no git calls needed)
        if let tabIndex = workspaceManager.tabs.firstIndex(where: { tab in
            tab.worktrees.contains(where: { $0.path == worktreePath })
        }) {
            let repoPath = workspaceManager.tabs[tabIndex].repoPath
            NSLog("navigateToWorktree: found tab %d, repoPath=%@", tabIndex, repoPath)
            switchToTab(tabIndex + 1)
            if let repoVC = repoVCs[repoPath] {
                repoVC.selectWorktree(byPath: worktreePath)
            } else {
                NSLog("navigateToWorktree: repoVC not found for %@", repoPath)
            }
            return
        }

        // If navigating to dashboard (tab 0), focus the specific pane
        if let paneIndex, activeTabIndex == 0 {
            // paneIndex from notification is 1-based; selectedPaneIndex is 0-based
            dashboardVC?.selectedPaneIndex = paneIndex - 1
        }
        NSLog("navigateToWorktree: no tab found, falling back to async discovery")

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
                self.openRepoTab(repoPath: repoPath) { [weak self] in
                    guard let self else { return }
                    if let repoVC = self.repoVCs[repoPath] {
                        repoVC.selectWorktree(byPath: worktreePath)
                    }
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
