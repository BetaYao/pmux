import AppKit

protocol TabCoordinatorDelegate: AnyObject {
    func tabCoordinator(_ coordinator: TabCoordinator, embedViewController vc: NSViewController)
    func tabCoordinatorDidSwitchTab(_ coordinator: TabCoordinator)
    func tabCoordinatorRequestUpdateTitleBar(_ coordinator: TabCoordinator)
    func tabCoordinatorRequestShowNewBranchDialog(_ coordinator: TabCoordinator)
    func tabCoordinatorRequestShowDiff(_ coordinator: TabCoordinator, worktreePath: String)
    func tabCoordinatorRequestClearContentContainer(_ coordinator: TabCoordinator)
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

    // References provided by MainWindowController
    var terminalCoordinator: TerminalCoordinator!
    var statusPublisher: StatusPublisher!
    var statusAggregator: WorktreeStatusAggregator!
    var runtimeBackend: String = "local"

    private static let iso8601: ISO8601DateFormatter = {
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withInternetDateTime]
        return f
    }()

    init(config: Config) {
        self.config = config
    }

    // MARK: - Current Repo VC

    var currentRepoVC: RepoViewController? {
        guard activeTabIndex > 0 else { return nil }
        let repoIndex = activeTabIndex - 1
        guard let tab = workspaceManager.tab(at: repoIndex) else { return nil }
        return repoVCs[tab.repoPath]
    }

    // MARK: - Tab Switching

    func switchToTab(_ index: Int) {
        guard index != activeTabIndex else { return }

        // Detach terminals from current repo view
        if activeTabIndex > 0 {
            let repoIndex = activeTabIndex - 1
            if let tab = workspaceManager.tab(at: repoIndex),
               let repoVC = repoVCs[tab.repoPath] {
                repoVC.detachActiveTerminal()
            }
        } else {
            dashboardVC?.detachTerminals()
        }

        activeTabIndex = index

        if index == 0 {
            if let dashboard = dashboardVC {
                delegate?.tabCoordinator(self, embedViewController: dashboard)
                dashboard.updateAgents(buildAgentDisplayInfos())
            }
        } else {
            let repoIndex = index - 1
            guard let tab = workspaceManager.tab(at: repoIndex) else { return }
            let repoVC = getOrCreateRepoVC(for: tab)
            delegate?.tabCoordinator(self, embedViewController: repoVC)
        }

        delegate?.tabCoordinatorRequestUpdateTitleBar(self)
        updateStatusPollPreferences()
        delegate?.tabCoordinatorDidSwitchTab(self)
    }

    func updateStatusPollPreferences() {
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

    // MARK: - Repo VC Management

    func getOrCreateRepoVC(for tab: WorkspaceTab) -> RepoViewController {
        if let existing = repoVCs[tab.repoPath] {
            existing.reconfigurePreservingSelection(worktrees: tab.worktrees, trees: terminalCoordinator.surfaceManager.all)
            return existing
        }

        let repoVC = RepoViewController()
        repoVC.repoDelegate = repoViewDelegate
        repoVC.configure(worktrees: tab.worktrees, trees: terminalCoordinator.surfaceManager.all)
        repoVCs[tab.repoPath] = repoVC
        return repoVC
    }

    /// Weak reference to the object implementing RepoViewDelegate (MainWindowController)
    weak var repoViewDelegate: RepoViewDelegate?

    func openRepoTab(repoPath: String, completion: (() -> Void)? = nil) {
        WorktreeDiscovery.discoverAsync(repoPath: repoPath) { [weak self] worktrees in
            guard let self else { return }
            _ = self.integrateDiscoveredRepo(repoPath: repoPath, worktrees: worktrees)
            completion?()
        }
    }

    // MARK: - Add Repo

    func addRepoViaOpenPanel(window: NSWindow?) {
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

    func addRepo(at path: String) {
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
            _ = self.integrateDiscoveredRepo(repoPath: path, worktrees: worktrees)
        }
    }

    // MARK: - Worktree Integration

    @discardableResult
    func integrateDiscoveredRepo(repoPath: String, worktrees: [WorktreeInfo], activateTab: Bool = true) -> Int {
        let effectiveWorktrees: [WorktreeInfo]
        if worktrees.isEmpty {
            effectiveWorktrees = [WorktreeInfo(path: repoPath, branch: "main", commitHash: "", isMainWorktree: true)]
        } else {
            effectiveWorktrees = worktrees
        }

        let tabIndex = workspaceManager.addTab(repoPath: repoPath, worktrees: effectiveWorktrees)

        for info in effectiveWorktrees {
            let tree = terminalCoordinator.resolveTree(for: info)
            allWorktrees.append((info: info, tree: tree))
            worktreeRepoCache[info.path] = repoPath

            let proj = workspaceManager.tabs.first(where: { $0.repoPath == repoPath })?.displayName
                ?? URL(fileURLWithPath: repoPath).lastPathComponent
            let started = config.worktreeStartedAt[info.path].flatMap { Self.iso8601.date(from: $0) }
            let sessionName = runtimeBackend == "local" ? nil : SessionManager.persistentSessionName(for: info.path)
            if let surface = terminalCoordinator.surfaceManager.primarySurface(forPath: info.path) {
                AgentHead.shared.register(surface: surface, worktreePath: info.path, branch: info.branch, project: proj, startedAt: started, tmuxSessionName: sessionName, backend: runtimeBackend)
            }
        }

        // Record startedAt for newly discovered worktrees
        let now = Self.iso8601.string(from: Date())
        var configChanged = false
        for info in effectiveWorktrees {
            if config.worktreeStartedAt[info.path] == nil {
                config.worktreeStartedAt[info.path] = now
                configChanged = true
            }
        }
        if configChanged { config.save() }

        dashboardVC?.updateAgents(buildAgentDisplayInfos())
        statusPublisher.updateSurfaces(terminalCoordinator.surfaceManager.all)
        delegate?.tabCoordinatorRequestUpdateTitleBar(self)

        if activateTab {
            switchToTab(tabIndex + 1)
        }
        return tabIndex
    }

    // MARK: - Build Agent Display Infos

    func buildAgentDisplayInfos() -> [AgentDisplayInfo] {
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

    // MARK: - Workspace Loading

    func loadWorkspaces() {
        let repoPaths = config.workspacePaths
        let cardOrder = config.cardOrder

        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            guard let self else { return }

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

                    _ = self.workspaceManager.addTab(repoPath: repoPath, worktrees: worktrees)
                }

                // Record startedAt for newly discovered worktrees
                let now = Self.iso8601.string(from: Date())
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
                    let started = self.config.worktreeStartedAt[info.path].flatMap { Self.iso8601.date(from: $0) }
                    let sessionName = self.runtimeBackend == "local" ? nil : SessionManager.persistentSessionName(for: info.path)
                    if let surface = self.terminalCoordinator.surfaceManager.primarySurface(forPath: info.path) {
                        AgentHead.shared.register(surface: surface, worktreePath: info.path, branch: info.branch, project: proj, startedAt: started, tmuxSessionName: sessionName, backend: self.runtimeBackend)
                    }
                }
                if !cardOrder.isEmpty {
                    AgentHead.shared.reorder(paths: cardOrder)
                }

                self.dashboardVC?.updateAgents(self.buildAgentDisplayInfos())
                self.delegate?.tabCoordinatorRequestUpdateTitleBar(self)

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

    // MARK: - Worktree Lifecycle

    func worktreeDidDelete(_ info: WorktreeInfo) {
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

        delegate?.tabCoordinatorRequestUpdateTitleBar(self)
    }

    // MARK: - Close Repo

    func performCloseRepo(projectName: String) {
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
            if runtimeBackend != "local" {
                let sessionName = SessionManager.persistentSessionName(for: worktree.path)
                SessionManager.killSession(sessionName, backend: runtimeBackend)
            }
        }

        allWorktrees.removeAll { item in
            tab.worktrees.contains(where: { $0.path == item.info.path })
        }

        config.workspacePaths.removeAll { $0 == tab.repoPath }
        config.save()

        repoVCs.removeValue(forKey: tab.repoPath)
        workspaceManager.removeTab(at: tabIndex)

        let uiTabIndex = tabIndex + 1
        let targetTab: Int
        if activeTabIndex == uiTabIndex {
            targetTab = max(0, uiTabIndex - 1)
        } else if activeTabIndex > uiTabIndex {
            targetTab = activeTabIndex - 1
        } else {
            targetTab = activeTabIndex
        }

        activeTabIndex = -1
        delegate?.tabCoordinatorRequestClearContentContainer(self)

        dashboardVC?.updateAgents(buildAgentDisplayInfos())
        statusPublisher.updateSurfaces(terminalCoordinator.surfaceManager.all)
        delegate?.tabCoordinatorRequestUpdateTitleBar(self)
        switchToTab(targetTab)
    }

    // MARK: - Modals

    func showCloseProjectModal(_ projectName: String, window: NSWindow?) {
        panelCoordinator?.closeBothPanels()

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

    weak var panelCoordinator: PanelCoordinator?

    func showAddProjectModal(window: NSWindow?) {
        addRepoViaOpenPanel(window: window)
    }

    func showNewThreadModal(window: NSWindow?) {
        panelCoordinator?.closeBothPanels()
        delegate?.tabCoordinatorRequestShowNewBranchDialog(self)
    }

    // MARK: - Branch Refresh

    func startBranchRefreshTimer() {
        branchRefreshTimer?.invalidate()
        branchRefreshTimer = Timer.scheduledTimer(withTimeInterval: 5.0, repeats: true) { [weak self] _ in
            self?.refreshBranches()
        }
    }

    private func refreshBranches() {
        let tabs = workspaceManager.tabs
        for (tabIndex, tab) in tabs.enumerated() {
            WorktreeDiscovery.discoverAsync(repoPath: tab.repoPath) { [weak self] freshWorktrees in
                guard let self else { return }
                let oldWorktrees = tab.worktrees

                var changed = false
                for fresh in freshWorktrees {
                    if let old = oldWorktrees.first(where: { $0.path == fresh.path }),
                       old.branch != fresh.branch {
                        changed = true
                        break
                    }
                }
                guard changed else { return }

                self.workspaceManager.updateWorktrees(at: tabIndex, worktrees: freshWorktrees)

                for (i, entry) in self.allWorktrees.enumerated() {
                    if let fresh = freshWorktrees.first(where: { $0.path == entry.info.path }) {
                        self.allWorktrees[i] = (info: fresh, tree: entry.tree)
                    }
                }

                if let repoVC = self.repoVCs[tab.repoPath] {
                    repoVC.updateWorktreeInfos(freshWorktrees)
                }

                self.dashboardVC?.updateAgents(self.buildAgentDisplayInfos())
            }
        }
    }

    // MARK: - Navigation

    func handleNavigateToWorktree(worktreePath: String, paneIndex: Int?) {
        // Check already-open tabs first
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

        // If navigating to dashboard (tab 0), focus the specific pane
        if let paneIndex, activeTabIndex == 0 {
            dashboardVC?.selectedPaneIndex = paneIndex - 1
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
