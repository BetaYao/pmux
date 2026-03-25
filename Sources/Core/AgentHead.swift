import Foundation

protocol AgentHeadDelegate: AnyObject {
    func agentDidUpdate(_ info: AgentInfo)
}

/// Single source of truth for all agent information.
/// Consumers query AgentHead instead of assembling data from multiple sources.
/// Also manages communication channels for each agent.
/// Primary key: terminal ID (TerminalSurface.id).
class AgentHead {
    static let shared = AgentHead()

    weak var delegate: AgentHeadDelegate?

    private var agents: [String: AgentInfo] = [:]       // keyed by terminal ID
    private var orderedIDs: [String] = []
    /// Reverse index: worktree path → terminal IDs (1:N)
    private var worktreeIndex: [String: [String]] = [:]
    /// Strong references to channels (keyed by terminal ID)
    private var channels: [String: AgentChannel] = [:]
    private var backendsByPath: [String: String] = [:]
    private let lock = NSLock()

    private init() {}

    // MARK: - Registration

    func register(surface: TerminalSurface, worktreePath: String, branch: String,
                  project: String, startedAt: Date?,
                  tmuxSessionName: String? = nil, backend: String = "zmx") {
        lock.lock()
        defer { lock.unlock() }

        let terminalID = surface.id

        // Create a default channel if we have a session name
        var channel: AgentChannel?
        if let sessionName = tmuxSessionName {
            if backend == "tmux" {
                channel = TmuxChannel(sessionName: sessionName)
            } else {
                channel = ZmxChannel(sessionName: sessionName)
            }
            channels[terminalID] = channel
        }
        backendsByPath[worktreePath] = backend

        let info = AgentInfo(
            id: terminalID,
            worktreePath: worktreePath,
            agentType: .unknown,
            project: project,
            branch: branch,
            status: .unknown,
            lastMessage: "",
            commandLine: nil,
            roundDuration: 0,
            startedAt: startedAt,
            surface: surface,
            channel: channel,
            taskProgress: TaskProgress()
        )
        agents[terminalID] = info
        var ids = worktreeIndex[worktreePath] ?? []
        if !ids.contains(terminalID) {
            ids.append(terminalID)
        }
        worktreeIndex[worktreePath] = ids
        if !orderedIDs.contains(terminalID) {
            orderedIDs.append(terminalID)
        }
    }

    func unregister(terminalID: String) {
        lock.lock()
        defer { lock.unlock() }

        if let info = agents[terminalID] {
            worktreeIndex[info.worktreePath]?.removeAll { $0 == terminalID }
            if worktreeIndex[info.worktreePath]?.isEmpty == true {
                worktreeIndex.removeValue(forKey: info.worktreePath)
            }
            backendsByPath.removeValue(forKey: info.worktreePath)
        }
        agents.removeValue(forKey: terminalID)
        channels.removeValue(forKey: terminalID)
        orderedIDs.removeAll { $0 == terminalID }
    }

    // MARK: - Worktree Index (1:N)

    func registerTerminalID(_ terminalID: String, forWorktree worktreePath: String) {
        lock.lock()
        defer { lock.unlock() }
        var ids = worktreeIndex[worktreePath] ?? []
        if !ids.contains(terminalID) {
            ids.append(terminalID)
        }
        worktreeIndex[worktreePath] = ids
    }

    func unregisterTerminalID(_ terminalID: String, forWorktree worktreePath: String) {
        lock.lock()
        defer { lock.unlock() }
        worktreeIndex[worktreePath]?.removeAll { $0 == terminalID }
        if worktreeIndex[worktreePath]?.isEmpty == true {
            worktreeIndex.removeValue(forKey: worktreePath)
        }
    }

    func terminalIDs(forWorktree worktreePath: String) -> [String] {
        lock.lock()
        defer { lock.unlock() }
        return worktreeIndex[worktreePath] ?? []
    }

    // MARK: - Updates

    func updateStatus(terminalID: String, status: AgentStatus,
                      lastMessage: String, roundDuration: TimeInterval) {
        lock.lock()
        guard var info = agents[terminalID] else {
            lock.unlock()
            return
        }
        let changed = info.status != status || info.lastMessage != lastMessage
        info.status = status
        info.lastMessage = lastMessage
        info.roundDuration = roundDuration
        agents[terminalID] = info
        lock.unlock()

        if changed {
            DispatchQueue.main.async { [weak self] in
                self?.delegate?.agentDidUpdate(info)
            }
        }
    }

    /// Update task progress for an agent
    func updateTaskProgress(terminalID: String, totalTasks: Int,
                            completedTasks: Int, currentTask: String?) {
        lock.lock()
        guard var info = agents[terminalID] else {
            lock.unlock()
            return
        }
        let changed = info.taskProgress.totalTasks != totalTasks
            || info.taskProgress.completedTasks != completedTasks
            || info.taskProgress.currentTask != currentTask
        info.taskProgress = TaskProgress(
            totalTasks: totalTasks,
            completedTasks: completedTasks,
            currentTask: currentTask
        )
        agents[terminalID] = info
        lock.unlock()

        if changed {
            DispatchQueue.main.async { [weak self] in
                self?.delegate?.agentDidUpdate(info)
            }
        }
    }

    /// Update detection results for an agent (command line and/or agent type).
    /// Type update rules:
    /// - .unknown → any type allowed
    /// - shell task (isShellTask) → any type allowed
    /// - AI agent (isAIAgent) → only another AI agent allowed (no demotion)
    /// When type is .claudeCode, upgrades TmuxChannel → HooksChannel.
    func updateDetection(terminalID: String, commandLine: String?, agentType: AgentType) {
        lock.lock()
        guard var info = agents[terminalID] else {
            lock.unlock()
            return
        }

        let worktreePath = info.worktreePath

        // Upgrade channel for Claude Code: backend channel -> HooksChannel
        if agentType == .claudeCode {
            let backend = backendsByPath[worktreePath] ?? "zmx"
            if let zmx = channels[terminalID] as? ZmxChannel {
                let hooks = HooksChannel(sessionName: zmx.sessionName, backend: backend)
                channels[terminalID] = hooks
                info.channel = hooks
            } else if let tmux = channels[terminalID] as? TmuxChannel {
                let hooks = HooksChannel(sessionName: tmux.sessionName, backend: backend)
                channels[terminalID] = hooks
                info.channel = hooks
            }
        }

        var changed = false

        // Update command line if provided
        if let cl = commandLine, info.commandLine != cl {
            info.commandLine = cl
            changed = true
        }

        // Apply type update rules
        if agentType != .unknown {
            let currentType = info.agentType
            let allowed: Bool
            if currentType == .unknown {
                allowed = true
            } else if currentType.isShellTask {
                allowed = true
            } else if currentType.isAIAgent {
                allowed = agentType.isAIAgent
            } else {
                allowed = true
            }

            if allowed && currentType != agentType {
                info.agentType = agentType
                changed = true

                // Upgrade channel for Claude Code: TmuxChannel → HooksChannel
                if agentType == .claudeCode, let tmux = channels[terminalID] as? TmuxChannel {
                    let hooks = HooksChannel(sessionName: tmux.sessionName)
                    channels[terminalID] = hooks
                    info.channel = hooks
                }
            }
        }

        agents[terminalID] = info
        lock.unlock()

        if changed {
            DispatchQueue.main.async { [weak self] in
                self?.delegate?.agentDidUpdate(info)
            }
        }
    }

    // MARK: - Channel Communication

    /// Send a command to a specific agent
    func sendCommand(to terminalID: String, command: String) {
        lock.lock()
        let channel = channels[terminalID]
        lock.unlock()

        channel?.sendCommand(command)
    }

    /// Read recent output from a specific agent
    func readOutput(from terminalID: String, lines: Int = 50) -> String? {
        lock.lock()
        let channel = channels[terminalID]
        lock.unlock()

        return channel?.readOutput(lines: lines)
    }

    /// Get the channel for a specific agent (for direct access)
    func channel(for terminalID: String) -> AgentChannel? {
        lock.lock()
        defer { lock.unlock() }
        return channels[terminalID]
    }

    /// Route a webhook event to the appropriate HooksChannel based on cwd matching
    func handleWebhookEvent(_ event: WebhookEvent) {
        lock.lock()
        // Find the agent whose worktree path matches the event's cwd
        let matchingTIDs = worktreeIndex.first { (worktreePath, _) in
            event.cwd == worktreePath || event.cwd.hasPrefix(worktreePath + "/")
        }?.value
        guard let tid = matchingTIDs?.first,
              let hooks = channels[tid] as? HooksChannel else {
            lock.unlock()
            return
        }
        lock.unlock()

        hooks.handleWebhookEvent(event)
    }

    // MARK: - Ordering

    /// Reorder agents to match card ordering from config.
    /// Accepts worktree paths (for config persistence) and maps internally via worktreeIndex.
    func reorder(paths: [String]) {
        lock.lock()
        defer { lock.unlock() }

        orderedIDs.sort { a, b in
            let pathA = agents[a]?.worktreePath ?? ""
            let pathB = agents[b]?.worktreePath ?? ""
            let ai = paths.firstIndex(of: pathA) ?? Int.max
            let bi = paths.firstIndex(of: pathB) ?? Int.max
            return ai < bi
        }
    }

    // MARK: - Queries

    func allAgents() -> [AgentInfo] {
        lock.lock()
        defer { lock.unlock() }

        return orderedIDs.compactMap { agents[$0] }
    }

    /// Look up agent by terminal ID
    func agent(for terminalID: String) -> AgentInfo? {
        lock.lock()
        defer { lock.unlock() }

        return agents[terminalID]
    }

    /// Convenience lookup by worktree path via reverse index
    func agent(forWorktree worktreePath: String) -> AgentInfo? {
        lock.lock()
        defer { lock.unlock() }

        guard let tid = worktreeIndex[worktreePath]?.first else { return nil }
        return agents[tid]
    }

    func agentsForProject(_ project: String) -> [AgentInfo] {
        lock.lock()
        defer { lock.unlock() }

        return orderedIDs.compactMap { agents[$0] }.filter { $0.project == project }
    }
}
