import Foundation

protocol StatusPublisherDelegate: AnyObject {
    func statusDidChange(worktreePath: String, oldStatus: AgentStatus, newStatus: AgentStatus)
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

    private let pollInterval: TimeInterval = 2.0

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
    }

    private func pollAll() {
        for (path, surface) in surfaces {
            let tracker = trackers[path] ?? {
                let t = DebouncedStatusTracker()
                trackers[path] = t
                return t
            }()

            let processStatus = surface.processStatus
            let content = surface.readViewportText() ?? ""

            // Try to find matching agent def from content
            let agentDef = findAgentDef(in: content)

            let detected = detector.detect(
                processStatus: processStatus,
                shellInfo: nil,  // OSC 133 requires stream interception; future enhancement
                content: content,
                agentDef: agentDef
            )

            let oldStatus = tracker.currentStatus
            if tracker.update(status: detected) {
                delegate?.statusDidChange(worktreePath: path, oldStatus: oldStatus, newStatus: detected)
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
        trackers[path]?.currentStatus ?? .unknown
    }

    deinit {
        stop()
    }
}
