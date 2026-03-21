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
    private var trackers: [String: DebouncedStatusTracker] = [:]  // keyed by terminal ID
    private var timer: Timer?
    private var surfaces: [String: TerminalSurface] = [:]         // keyed by terminal ID
    /// Reverse mapping: terminal ID → worktree path (for delegate callbacks and webhook provider)
    private var worktreePaths: [String: String] = [:]
    private var agentConfig: AgentDetectConfig
    private var lastMessages: [String: String] = [:]              // keyed by terminal ID
    private var runningStartTimes: [String: Date] = [:]           // keyed by terminal ID
    private(set) var webhookProvider = WebhookStatusProvider()

    private let pollInterval: TimeInterval = 2.0
    private let pollQueue = DispatchQueue(label: "com.pmux.status-poll", qos: .utility)

    // Cache: skip detection when viewport text hasn't changed
    private var lastViewportHashes: [String: Int] = [:]           // keyed by terminal ID
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
        self.surfaces = [:]
        self.worktreePaths = [:]
        for (worktreePath, surface) in surfaces {
            self.surfaces[surface.id] = surface
            self.worktreePaths[surface.id] = worktreePath
        }
        stop()

        // Create trackers for each surface
        for terminalID in self.surfaces.keys {
            if trackers[terminalID] == nil {
                trackers[terminalID] = DebouncedStatusTracker()
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
        self.surfaces = [:]
        self.worktreePaths = [:]
        for (worktreePath, surface) in surfaces {
            self.surfaces[surface.id] = surface
            self.worktreePaths[surface.id] = worktreePath
        }
        // Add trackers for new surfaces
        for terminalID in self.surfaces.keys {
            if trackers[terminalID] == nil {
                trackers[terminalID] = DebouncedStatusTracker()
            }
        }
        webhookProvider.updateWorktrees(Array(surfaces.keys))
    }

    private func schedulePoll() {
        // Capture surfaces snapshot on main thread, then poll on background
        let surfaceSnapshot = surfaces
        let pathSnapshot = worktreePaths
        pollQueue.async { [weak self] in
            self?.pollAll(surfaceSnapshot, paths: pathSnapshot)
        }
    }

    private func pollAll(_ surfaceSnapshot: [String: TerminalSurface], paths: [String: String]) {
        for (terminalID, surface) in surfaceSnapshot {
            let worktreePath = paths[terminalID] ?? ""
            let processStatus = surface.processStatus
            let content = surface.readViewportText() ?? ""

            // Skip expensive text analysis if viewport hasn't changed
            let contentHash = content.hashValue
            if let lastHash = lastViewportHashes[terminalID], lastHash == contentHash {
                continue
            }
            lastViewportHashes[terminalID] = contentHash

            let tracker = trackers[terminalID] ?? {
                let t = DebouncedStatusTracker()
                trackers[terminalID] = t
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
            let hookStatus = webhookProvider.status(for: worktreePath)
            let detected = AgentStatus.highestPriority([textStatus, hookStatus])

            // Prefer structured webhook message over terminal text scan
            let webhookMessage = webhookProvider.lastMessage(for: worktreePath)
            let terminalMessage = agentDef?.extractLastMessage(from: content, maxLen: 80) ?? ""
            let lastMessage = webhookMessage ?? (terminalMessage.isEmpty ? nil : terminalMessage) ?? ""

            let oldStatus = tracker.currentStatus
            let statusChanged = tracker.update(status: detected)
            lastMessages[terminalID] = lastMessage

            // Feed AgentHead with structured data on every poll
            let agentType = AgentType.detect(fromLowercased: lowerContent)
            let roundDur = runningStartTimes[terminalID].map { Date().timeIntervalSince($0) } ?? 0
            AgentHead.shared.updateDetection(terminalID: terminalID, commandLine: nil, agentType: agentType)
            AgentHead.shared.updateStatus(terminalID: terminalID, status: detected, lastMessage: lastMessage, roundDuration: roundDur)
            // Track round duration: record when entering Running, clear when leaving
            if statusChanged {
                if detected == .running && oldStatus != .running {
                    runningStartTimes[terminalID] = Date()
                } else if detected != .running && oldStatus == .running {
                    runningStartTimes[terminalID] = nil
                }
                DispatchQueue.main.async { [weak self] in
                    self?.delegate?.statusDidChange(worktreePath: worktreePath, oldStatus: oldStatus, newStatus: detected, lastMessage: lastMessage)
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

    func status(for terminalID: String) -> AgentStatus {
        trackers[terminalID]?.currentStatus ?? .unknown
    }

    func lastMessage(for terminalID: String) -> String {
        lastMessages[terminalID] ?? ""
    }

    /// Returns seconds since the current Running round started, or 0 if not running
    func roundDuration(for terminalID: String) -> TimeInterval {
        guard let start = runningStartTimes[terminalID] else { return 0 }
        return Date().timeIntervalSince(start)
    }

    deinit {
        stop()
    }
}
