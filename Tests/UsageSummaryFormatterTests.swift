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

    func testBuildsRotationFramesWithShortcutClaudeAndCodex() {
        let now = Date(timeIntervalSince1970: 1_772_523_660)
        let claude = UsageSnapshot(
            provider: .claude,
            rateLimit: UsageRateLimitWindow(usedPercent: 19, resetsAt: Date(timeIntervalSince1970: 1_772_531_400)),
            todayTokens: 123_400,
            updatedAt: now,
            isStale: false
        )
        let codex = UsageSnapshot(
            provider: .codex,
            rateLimit: UsageRateLimitWindow(usedPercent: 28, resetsAt: Date(timeIntervalSince1970: 1_772_532_000)),
            todayTokens: 5_745_705,
            updatedAt: now,
            isStale: false
        )

        let frames = UsageSummaryFormatter.rotationFrames(claude: claude, codex: codex, now: now)

        XCTAssertEqual(frames.count, 10)
        XCTAssertEqual(frames.prefix(8).map(\.kind), Array(repeating: .shortcut, count: 8))
        XCTAssertEqual(frames[0].leadingText, "Tip")
        XCTAssertEqual(frames[0].bodyText, "Cmd+1..4 switch layout")
        XCTAssertEqual(frames[7].bodyText, "Cmd+Shift+F show diff")
        XCTAssertEqual(frames[8].kind, .usage)
        XCTAssertEqual(frames[8].leadingText, "Claude")
        XCTAssertEqual(frames[8].bodyText, "剩余 81%")
        XCTAssertEqual(frames[8].trailingText, "Today 123.4K")
        XCTAssertEqual(frames[9].kind, .usage)
        XCTAssertEqual(frames[9].leadingText, "Codex")
        XCTAssertEqual(frames[9].bodyText, "剩余 72%")
        XCTAssertEqual(frames[9].trailingText, "Today 5.7M")
    }
}
