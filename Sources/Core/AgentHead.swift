import Foundation

protocol AgentHeadDelegate: AnyObject {
    func agentDidUpdate(_ info: AgentInfo)
}

/// Single source of truth for all agent information.
/// Consumers query AgentHead instead of assembling data from multiple sources.
/// Also manages communication channels for each agent.
class AgentHead {
    static let shared = AgentHead()

    weak var delegate: AgentHeadDelegate?

    private var agents: [String: AgentInfo] = [:]
    private var orderedPaths: [String] = []
    /// Strong references to channels (keyed by worktree path)
    private var channels: [String: AgentChannel] = [:]
    private var backendsByPath: [String: String] = [:]
    private let lock = NSLock()

    private init() {}

    // MARK: - Registration

    func register(worktreePath: String, branch: String, project: String,
                  surface: TerminalSurface, startedAt: Date?,
                  sessionName: String? = nil, backend: String = "zmx") {
        lock.lock()
        defer { lock.unlock() }

        // Create a default channel if we have a session name
        var channel: AgentChannel?
        if let sessionName {
            if backend == "tmux" {
                channel = TmuxChannel(sessionName: sessionName)
            } else {
                channel = ZmxChannel(sessionName: sessionName)
            }
            channels[worktreePath] = channel
        }
        backendsByPath[worktreePath] = backend

        let info = AgentInfo(
            id: worktreePath,
            agentType: .unknown,
            project: project,
            branch: branch,
            status: .unknown,
            lastMessage: "",
            roundDuration: 0,
            startedAt: startedAt,
            surface: surface,
            channel: channel,
            taskProgress: TaskProgress()
        )
        agents[worktreePath] = info
        if !orderedPaths.contains(worktreePath) {
            orderedPaths.append(worktreePath)
        }
    }

    func unregister(worktreePath: String) {
        lock.lock()
        defer { lock.unlock() }

        agents.removeValue(forKey: worktreePath)
        channels.removeValue(forKey: worktreePath)
        backendsByPath.removeValue(forKey: worktreePath)
        orderedPaths.removeAll { $0 == worktreePath }
    }

    // MARK: - Updates

    func updateStatus(worktreePath: String, status: AgentStatus,
                      lastMessage: String, roundDuration: TimeInterval) {
        lock.lock()
        guard var info = agents[worktreePath] else {
            lock.unlock()
            return
        }
        let changed = info.status != status || info.lastMessage != lastMessage
        info.status = status
        info.lastMessage = lastMessage
        info.roundDuration = roundDuration
        agents[worktreePath] = info
        lock.unlock()

        if changed {
            DispatchQueue.main.async { [weak self] in
                self?.delegate?.agentDidUpdate(info)
            }
        }
    }

    /// Update task progress for an agent
    func updateTaskProgress(worktreePath: String, totalTasks: Int,
                            completedTasks: Int, currentTask: String?) {
        lock.lock()
        guard var info = agents[worktreePath] else {
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
        agents[worktreePath] = info
        lock.unlock()

        if changed {
            DispatchQueue.main.async { [weak self] in
                self?.delegate?.agentDidUpdate(info)
            }
        }
    }

    /// Only updates agent type if current type is .unknown (prevents thrashing).
    /// When type is detected as .claudeCode, upgrades the channel to HooksChannel.
    func updateAgentType(worktreePath: String, type: AgentType) {
        guard type != .unknown else { return }

        lock.lock()
        guard var info = agents[worktreePath], info.agentType == .unknown else {
            lock.unlock()
            return
        }
        info.agentType = type

        // Upgrade channel for Claude Code: backend channel -> HooksChannel
        if type == .claudeCode {
            let backend = backendsByPath[worktreePath] ?? "zmx"
            if let zmx = channels[worktreePath] as? ZmxChannel {
                let hooks = HooksChannel(sessionName: zmx.sessionName, backend: backend)
                channels[worktreePath] = hooks
                info.channel = hooks
            } else if let tmux = channels[worktreePath] as? TmuxChannel {
                let hooks = HooksChannel(sessionName: tmux.sessionName, backend: backend)
                channels[worktreePath] = hooks
                info.channel = hooks
            }
        }

        agents[worktreePath] = info
        lock.unlock()

        DispatchQueue.main.async { [weak self] in
            self?.delegate?.agentDidUpdate(info)
        }
    }

    // MARK: - Channel Communication

    /// Send a command to a specific agent
    func sendCommand(to worktreePath: String, command: String) {
        lock.lock()
        let channel = channels[worktreePath]
        lock.unlock()

        channel?.sendCommand(command)
    }

    /// Read recent output from a specific agent
    func readOutput(from worktreePath: String, lines: Int = 50) -> String? {
        lock.lock()
        let channel = channels[worktreePath]
        lock.unlock()

        return channel?.readOutput(lines: lines)
    }

    /// Get the channel for a specific agent (for direct access)
    func channel(for worktreePath: String) -> AgentChannel? {
        lock.lock()
        defer { lock.unlock() }
        return channels[worktreePath]
    }

    /// Route a webhook event to the appropriate HooksChannel based on cwd matching
    func handleWebhookEvent(_ event: WebhookEvent) {
        lock.lock()
        // Find the agent whose path matches the event's cwd
        let matchingPath = agents.keys.first { path in
            event.cwd == path || event.cwd.hasPrefix(path + "/")
        }
        guard let path = matchingPath,
              let hooks = channels[path] as? HooksChannel else {
            lock.unlock()
            return
        }
        lock.unlock()

        hooks.handleWebhookEvent(event)
    }

    // MARK: - Ordering

    /// Reorder agents to match card ordering from config
    func reorder(paths: [String]) {
        lock.lock()
        defer { lock.unlock() }

        orderedPaths.sort { a, b in
            let ai = paths.firstIndex(of: a) ?? Int.max
            let bi = paths.firstIndex(of: b) ?? Int.max
            return ai < bi
        }
    }

    // MARK: - Queries

    func allAgents() -> [AgentInfo] {
        lock.lock()
        defer { lock.unlock() }

        return orderedPaths.compactMap { agents[$0] }
    }

    func agent(for worktreePath: String) -> AgentInfo? {
        lock.lock()
        defer { lock.unlock() }

        return agents[worktreePath]
    }

    func agentsForProject(_ project: String) -> [AgentInfo] {
        lock.lock()
        defer { lock.unlock() }

        return orderedPaths.compactMap { agents[$0] }.filter { $0.project == project }
    }
}
