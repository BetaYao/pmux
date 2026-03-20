import Foundation

protocol StatusPublisherDelegate: AnyObject {
    func statusDidChange(worktreePath: String, oldStatus: AgentStatus, newStatus: AgentStatus, lastMessage: String)
}

/// Periodically polls terminal surfaces and detects agent status changes.
/// Uses text pattern matching against visible terminal content.
class StatusPublisher {
    weak var delegate: StatusPublisherDelegate?

    private let detector = StatusDetector()
    private var trackers: [String: DebouncedStatusTracker] = [:]  // keyed by worktree path
    private var timer: Timer?
    private var surfaces: [String: TerminalSurface] = [:]
    private var agentConfig: AgentDetectConfig
    private var lastMessages: [String: String] = [:]
    private(set) var webhookProvider = WebhookStatusProvider()

    private let pollInterval: TimeInterval = 2.0
    private let pollQueue = DispatchQueue(label: "com.pmux.statusPoll", qos: .utility)

    init(agentConfig: AgentDetectConfig = .default) {
        self.agentConfig = agentConfig
    }

    func start(surfaces: [String: TerminalSurface]) {
        self.surfaces = surfaces
        stop()

        // Create trackers for each surface
        for path in surfaces.keys {
            if trackers[path] == nil {
                trackers[path] = DebouncedStatusTracker()
            }
        }

        webhookProvider.updateWorktrees(Array(surfaces.keys))

        timer = Timer.scheduledTimer(withTimeInterval: pollInterval, repeats: true) { [weak self] _ in
            self?.pollAll()
        }
        // Run immediately on start
        pollAll()
    }

    func stop() {
        timer?.invalidate()
        timer = nil
    }

    func updateSurfaces(_ surfaces: [String: TerminalSurface]) {
        self.surfaces = surfaces
        // Add trackers for new surfaces
        for path in surfaces.keys {
            if trackers[path] == nil {
                trackers[path] = DebouncedStatusTracker()
            }
        }
        webhookProvider.updateWorktrees(Array(surfaces.keys))
    }

    private func pollAll() {
        // Capture snapshot on main thread
        let surfacesSnapshot = surfaces

        pollQueue.async { [weak self] in
            guard let self = self else { return }

            var updates: [(path: String, oldStatus: AgentStatus, newStatus: AgentStatus, lastMessage: String)] = []

            for (path, surface) in surfacesSnapshot {
                let processStatus = surface.processStatus
                let content = surface.readViewportText() ?? ""

                // Try to find matching agent def from content
                let agentDef = self.findAgentDef(in: content)

                let textStatus = self.detector.detect(
                    processStatus: processStatus,
                    shellInfo: nil,  // OSC 133 requires stream interception; future enhancement
                    content: content,
                    agentDef: agentDef
                )
                let hookStatus = self.webhookProvider.status(for: path)
                let detected = AgentStatus.highestPriority([textStatus, hookStatus])

                let lastMessage = agentDef?.extractLastMessage(from: content, maxLen: 80) ?? ""

                let tracker = self.trackers[path] ?? {
                    let t = DebouncedStatusTracker()
                    self.trackers[path] = t
                    return t
                }()

                let oldStatus = tracker.currentStatus
                let statusChanged = tracker.update(status: detected)
                let messageChanged = (self.lastMessages[path] != lastMessage)
                self.lastMessages[path] = lastMessage

                if statusChanged || messageChanged {
                    updates.append((path: path, oldStatus: oldStatus, newStatus: detected, lastMessage: lastMessage))
                }
            }

            DispatchQueue.main.async { [weak self] in
                guard let self = self else { return }
                for update in updates {
                    self.delegate?.statusDidChange(
                        worktreePath: update.path,
                        oldStatus: update.oldStatus,
                        newStatus: update.newStatus,
                        lastMessage: update.lastMessage
                    )
                }
            }
        }
    }

    /// Find agent definition by checking if any known agent CLI name appears in the content
    private func findAgentDef(in content: String) -> AgentDef? {
        let lower = content.lowercased()
        for agent in agentConfig.agents {
            if lower.contains(agent.name.lowercased()) {
                return agent
            }
        }
        return nil
    }

    func status(for path: String) -> AgentStatus {
        pollQueue.sync { trackers[path]?.currentStatus ?? .unknown }
    }

    func lastMessage(for path: String) -> String {
        pollQueue.sync { lastMessages[path] ?? "" }
    }

    deinit {
        stop()
    }
}
