import XCTest
@testable import amux

final class UsageSummaryFormatterTests: XCTestCase {
    func testFormatsAvailableUsageWithRemainingAndTodayTokens() {
        let snapshot = UsageSnapshot(
            provider: .codex,
            rateLimit: UsageRateLimitWindow(usedPercent: 28, resetsAt: Date(timeIntervalSince1970: 1_772_532_000)),
            todayTokens: 5_745_705,
            updatedAt: Date(timeIntervalSince1970: 1_772_523_840),
            isStale: false
        )

        let frame = UsageSummaryFormatter.formatUsageFrame(
            snapshot,
            now: Date(timeIntervalSince1970: 1_772_523_660)
        )

        XCTAssertEqual(frame.kind, .usage)
        XCTAssertEqual(frame.leadingText, "Codex")
        XCTAssertEqual(frame.bodyText, "剩余 72%")
        XCTAssertEqual(frame.trailingText, "Today 5.7M")
        XCTAssertEqual(frame.usageProgress ?? -1, 0.28, accuracy: 0.001)
        XCTAssertEqual(frame.resetText, "2h 19m")
    }

    func testFormatsUnavailableUsageAsPlaceholder() {
        let snapshot = UsageSnapshot(
            provider: .claude,
            rateLimit: nil,
            todayTokens: nil,
            updatedAt: nil,
            isStale: true
        )

        let frame = UsageSummaryFormatter.formatUsageFrame(snapshot, now: Date())

        XCTAssertEqual(frame.leadingText, "Claude")
        XCTAssertEqual(frame.bodyText, "剩余 --")
        XCTAssertEqual(frame.trailingText, "Today --")
        XCTAssertNil(frame.usageProgress)
        XCTAssertNil(frame.resetText)
    }

    func testCompactsLargeTokenCounts() {
        XCTAssertEqual(UsageSummaryFormatter.compactTokenCount(985), "985")
        XCTAssertEqual(UsageSummaryFormatter.compactTokenCount(12_340), "12.3K")
        XCTAssertEqual(UsageSummaryFormatter.compactTokenCount(5_745_705), "5.7M")
    }
}
