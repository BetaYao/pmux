import XCTest
@testable import amux

final class UsageSummaryStoreTests: XCTestCase {
    func testResolveSnapshotKeepsCachedRateLimitWhenRefreshMissesIt() {
        let cached = UsageSnapshot(
            provider: .claude,
            rateLimit: UsageRateLimitWindow(usedPercent: 19, resetsAt: Date(timeIntervalSince1970: 1_772_532_000)),
            todayTokens: 120_000,
            updatedAt: Date(timeIntervalSince1970: 1_772_523_000),
            isStale: false
        )
        let candidate = UsageSnapshot(
            provider: .claude,
            rateLimit: nil,
            todayTokens: 130_000,
            updatedAt: Date(timeIntervalSince1970: 1_772_523_600),
            isStale: true
        )

        let resolved = UsageSummaryStore.resolveSnapshotForDisplay(candidate: candidate, cached: cached)

        XCTAssertEqual(resolved.provider, .claude)
        XCTAssertEqual(resolved.rateLimit, cached.rateLimit)
        XCTAssertEqual(resolved.todayTokens, 130_000)
        XCTAssertEqual(resolved.updatedAt, candidate.updatedAt)
        XCTAssertTrue(resolved.isStale)
    }

    func testResolveSnapshotKeepsCachedTodayTokensWhenRefreshMissesThem() {
        let cached = UsageSnapshot(
            provider: .codex,
            rateLimit: UsageRateLimitWindow(usedPercent: 40, resetsAt: nil),
            todayTokens: 34_000,
            updatedAt: Date(timeIntervalSince1970: 1_772_523_000),
            isStale: false
        )
        let candidate = UsageSnapshot(
            provider: .codex,
            rateLimit: UsageRateLimitWindow(usedPercent: 42, resetsAt: nil),
            todayTokens: nil,
            updatedAt: Date(timeIntervalSince1970: 1_772_523_600),
            isStale: false
        )

        let resolved = UsageSummaryStore.resolveSnapshotForDisplay(candidate: candidate, cached: cached)

        XCTAssertEqual(resolved.provider, .codex)
        XCTAssertEqual(resolved.rateLimit, candidate.rateLimit)
        XCTAssertEqual(resolved.todayTokens, cached.todayTokens)
        XCTAssertEqual(resolved.updatedAt, candidate.updatedAt)
        XCTAssertFalse(resolved.isStale)
    }
}
