import XCTest
@testable import amux

final class CodexUsageSummaryProviderTests: XCTestCase {
    func testParsesRateLimitResponsePreferringCodexBucket() throws {
        let data = """
        {"id":2,"result":{"rateLimits":{"primary":{"usedPercent":80,"resetsAt":1772532000}},"rateLimitsByLimitId":{"codex":{"primary":{"usedPercent":28,"resetsAt":1772532000}}}}}
        """.data(using: .utf8)!

        let parsed = try CodexRateLimitParser.parseResponse(data)

        XCTAssertEqual(parsed?.usedPercent, 28)
        XCTAssertEqual(parsed?.resetsAt, Date(timeIntervalSince1970: 1_772_532_000))
    }

    func testParsesWhitespaceJSONRPCResponseLineByID() throws {
        let line = """
        { "id" : 2, "result": { "rateLimits": { "primary": { "usedPercent": 31, "resetsAt": 1772532000 } } } }
        """

        let parsed = try CodexRateLimitParser.parseResponseLine(line, expectedID: 2)

        XCTAssertEqual(parsed?.usedPercent, 31)
        XCTAssertEqual(parsed?.resetsAt, Date(timeIntervalSince1970: 1_772_532_000))
    }

    func testIgnoresUnexpectedJSONRPCResponseLineID() throws {
        let line = """
        { "id" : 1, "result": { "rateLimits": { "primary": { "usedPercent": 31 } } } }
        """

        let parsed = try CodexRateLimitParser.parseResponseLine(line, expectedID: 2)

        XCTAssertNil(parsed)
    }

    func testFallsBackToPrimaryRateLimitsWhenCodexBucketIsIncomplete() throws {
        let data = """
        {"id":2,"result":{"rateLimits":{"primary":{"usedPercent":80,"resetsAt":1772532000}},"rateLimitsByLimitId":{"codex":{"primary":{"resetsAt":1772532000}}}}}
        """.data(using: .utf8)!

        let parsed = try CodexRateLimitParser.parseResponse(data)

        XCTAssertEqual(parsed?.usedPercent, 80)
        XCTAssertEqual(parsed?.resetsAt, Date(timeIntervalSince1970: 1_772_532_000))
    }

    func testBuildsDailyTokenQueryWithLocalDayBounds() {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(secondsFromGMT: 0)!
        let query = CodexSQLiteDailyUsageReader.query(
            now: ISO8601DateFormatter().date(from: "2026-04-27T08:00:00Z")!,
            calendar: calendar
        )

        XCTAssertTrue(query.contains("updated_at >= 1777248000"))
        XCTAssertTrue(query.contains("updated_at < 1777334400"))
        XCTAssertTrue(query.contains("sum(tokens_used)"))
    }

    func testBuildsDailyTokenQueryWithNonUTCLocalDayBounds() {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(secondsFromGMT: 8 * 3600)!
        let query = CodexSQLiteDailyUsageReader.query(
            now: ISO8601DateFormatter().date(from: "2026-04-27T08:00:00Z")!,
            calendar: calendar
        )

        XCTAssertTrue(query.contains("updated_at >= 1777219200"))
        XCTAssertTrue(query.contains("updated_at < 1777305600"))
        XCTAssertTrue(query.contains("sum(tokens_used)"))
    }

    func testReadRateLimitTimesOutAndTerminatesSlowCodexProcess() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let script = dir.appendingPathComponent("slow-codex")
        try """
        #!/bin/sh
        sleep 5
        """.write(to: script, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: script.path)

        let client = CodexAppServerRateLimitClient(codexExecutable: script.path)
        let start = Date()
        let rateLimit = client.readRateLimit(timeout: 0.1)

        XCTAssertNil(rateLimit)
        XCTAssertLessThan(Date().timeIntervalSince(start), 2)
    }
}
