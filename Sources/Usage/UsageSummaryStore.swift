import Foundation

final class UsageSummaryStore {
    typealias UpdateHandler = ([PrimaryCapsuleFrame]) -> Void

    private let claudeProvider: ClaudeUsageSummaryProvider
    private let codexProvider: CodexUsageSummaryProvider
    private let refreshInterval: TimeInterval
    private let queue = DispatchQueue(label: "usage-summary-store", qos: .utility)
    private let lock = NSLock()
    private var timer: DispatchSourceTimer?
    private var isRefreshing = false
    private var cachedClaudeSnapshot: UsageSnapshot?
    private var cachedCodexSnapshot: UsageSnapshot?

    var onUpdate: UpdateHandler?

    convenience init(refreshInterval: TimeInterval = 60) {
        let home = FileManager.default.homeDirectoryForCurrentUser
        var calendar = Calendar.current
        calendar.timeZone = TimeZone.current
        self.init(
            claudeProvider: ClaudeUsageSummaryProvider(
                cacheReader: ClaudeStatuslineCacheReader(
                    cacheURL: home.appendingPathComponent("Library/Caches/amux/claude-statusline.json"),
                    staleInterval: 6 * 60 * 60
                ),
                transcriptAggregator: ClaudeTranscriptUsageAggregator(
                    rootURL: home.appendingPathComponent(".claude/projects"),
                    calendar: calendar
                )
            ),
            codexProvider: CodexUsageSummaryProvider(
                rateLimitClient: CodexAppServerRateLimitClient(),
                dailyUsageReader: CodexSQLiteDailyUsageReader(
                    databaseURL: home.appendingPathComponent(".codex/state_5.sqlite"),
                    calendar: calendar
                )
            ),
            refreshInterval: refreshInterval
        )
    }

    init(claudeProvider: ClaudeUsageSummaryProvider, codexProvider: CodexUsageSummaryProvider, refreshInterval: TimeInterval = 60) {
        self.claudeProvider = claudeProvider
        self.codexProvider = codexProvider
        self.refreshInterval = refreshInterval
    }

    deinit {
        stop()
    }

    func start() {
        lock.lock()
        defer { lock.unlock() }
        guard timer == nil else { return }

        let timer = DispatchSource.makeTimerSource(queue: queue)
        timer.schedule(deadline: .now(), repeating: refreshInterval)
        timer.setEventHandler { [weak self] in
            self?.refresh()
        }
        self.timer = timer
        timer.resume()
    }

    func stop() {
        lock.lock()
        let timer = self.timer
        self.timer = nil
        lock.unlock()

        timer?.setEventHandler {}
        timer?.cancel()
    }

    private func refresh() {
        lock.lock()
        guard !isRefreshing else {
            lock.unlock()
            return
        }
        isRefreshing = true
        lock.unlock()

        let claudeCandidate = claudeProvider.snapshot()
        let codexCandidate = codexProvider.snapshot()

        lock.lock()
        let claude = Self.resolveSnapshotForDisplay(candidate: claudeCandidate, cached: cachedClaudeSnapshot)
        let codex = Self.resolveSnapshotForDisplay(candidate: codexCandidate, cached: cachedCodexSnapshot)
        cachedClaudeSnapshot = claude
        cachedCodexSnapshot = codex
        isRefreshing = false
        lock.unlock()

        let frames = UsageSummaryFormatter.rotationFrames(claude: claude, codex: codex)
        DispatchQueue.main.async { [weak self] in
            self?.onUpdate?(frames)
        }
    }

    static func resolveSnapshotForDisplay(candidate: UsageSnapshot, cached: UsageSnapshot?) -> UsageSnapshot {
        UsageSnapshot(
            provider: candidate.provider,
            rateLimit: candidate.rateLimit ?? cached?.rateLimit,
            todayTokens: candidate.todayTokens ?? cached?.todayTokens,
            updatedAt: candidate.updatedAt ?? cached?.updatedAt,
            isStale: candidate.isStale
        )
    }
}
