import Foundation

protocol StatusPublisherDelegate: AnyObject {
    func statusDidChange(worktreePath: String, oldStatus: AgentStatus, newStatus: AgentStatus, lastMessage: String)
}

/// Periodically polls terminal surfaces and detects agent status changes.
/// Uses text pattern matching against visible terminal content.
/// Polling runs on a background queue to avoid blocking the main thread.
class StatusPublisher {
    weak var delegate: StatusPublisherDelegate?

    private let detector = StatusDetector()
    private var trackers: [String: DebouncedStatusTracker] = [:]  // keyed by worktree path
    private var timer: Timer?
    private var surfaces: [String: TerminalSurface] = [:]
    private var agentConfig: AgentDetectConfig
    private var lastMessages: [String: String] = [:]
    private var runningStartTimes: [String: Date] = [:]
    private(set) var webhookProvider = WebhookStatusProvider()

    private let pollInterval: TimeInterval = 2.0
    private let pollQueue = DispatchQueue(label: "com.pmux.status-poll", qos: .utility)

    // Cache: skip detection when viewport text hasn't changed
    private var lastViewportHashes: [String: Int] = [:]
    // Pre-lowercased agent names for faster matching
    private var lowercasedAgentNames: [(name: String, def: AgentDef)] = []

    init(agentConfig: AgentDetectConfig = .default) {
        self.agentConfig = agentConfig
        rebuildAgentNameCache()
    }

    private func rebuildAgentNameCache() {
        lowercasedAgentNames = agentConfig.agents.map { ($0.name.lowercased(), $0) }
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
            self?.schedulePoll()
        }
        // Run immediately on start
        schedulePoll()
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

    private func schedulePoll() {
        // Capture surfaces snapshot on main thread, then poll on background
        let surfaceSnapshot = surfaces
        pollQueue.async { [weak self] in
            self?.pollAll(surfaceSnapshot)
        }
    }

    private func pollAll(_ surfaceSnapshot: [String: TerminalSurface]) {
        for (path, surface) in surfaceSnapshot {
            let processStatus = surface.processStatus
            let content = surface.readViewportText() ?? ""

            // Skip expensive text analysis if viewport hasn't changed
            let contentHash = content.hashValue
            if let lastHash = lastViewportHashes[path], lastHash == contentHash {
                continue
            }
            lastViewportHashes[path] = contentHash

            let tracker = trackers[path] ?? {
                let t = DebouncedStatusTracker()
                trackers[path] = t
                return t
            }()

            // Lowercase once, reuse for both agent matching and status detection
            let lowerContent = content.lowercased()
            let agentDef = findAgentDef(inLowercased: lowerContent)

            let textStatus = detector.detect(
                processStatus: processStatus,
                shellInfo: nil,
                content: content,
                agentDef: agentDef,
                lowercasedContent: lowerContent
            )
            let hookStatus = webhookProvider.status(for: path)
            let detected = AgentStatus.highestPriority([textStatus, hookStatus])

            // Prefer structured webhook message over terminal text scan
            let webhookMessage = webhookProvider.lastMessage(for: path)
            let terminalMessage = agentDef?.extractLastMessage(from: content, maxLen: 80) ?? ""
            let lastMessage = webhookMessage ?? (terminalMessage.isEmpty ? nil : terminalMessage) ?? ""

            let oldStatus = tracker.currentStatus
            let statusChanged = tracker.update(status: detected)
            lastMessages[path] = lastMessage

            // Feed AgentHead with structured data on every poll
            let agentType = AgentType.detect(fromLowercased: lowerContent)
            let roundDur = runningStartTimes[path].map { Date().timeIntervalSince($0) } ?? 0
            AgentHead.shared.updateAgentType(worktreePath: path, type: agentType)
            AgentHead.shared.updateStatus(worktreePath: path, status: detected, lastMessage: lastMessage, roundDuration: roundDur)
            // Track round duration: record when entering Running, clear when leaving
            if statusChanged {
                if detected == .running && oldStatus != .running {
                    runningStartTimes[path] = Date()
                } else if detected != .running && oldStatus == .running {
                    runningStartTimes[path] = nil
                }
                DispatchQueue.main.async { [weak self] in
                    self?.delegate?.statusDidChange(worktreePath: path, oldStatus: oldStatus, newStatus: detected, lastMessage: lastMessage)
                }
            }
        }
    }

    /// Find agent definition using pre-lowercased content and names
    private func findAgentDef(inLowercased lowerContent: String) -> AgentDef? {
        for (name, def) in lowercasedAgentNames {
            if lowerContent.contains(name) {
                return def
            }
        }
        return nil
    }

    func status(for path: String) -> AgentStatus {
        trackers[path]?.currentStatus ?? .unknown
    }

    func lastMessage(for path: String) -> String {
        lastMessages[path] ?? ""
    }

    /// Returns seconds since the current Running round started, or 0 if not running
    func roundDuration(for path: String) -> TimeInterval {
        guard let start = runningStartTimes[path] else { return 0 }
        return Date().timeIntervalSince(start)
    }

    deinit {
        stop()
    }
}
