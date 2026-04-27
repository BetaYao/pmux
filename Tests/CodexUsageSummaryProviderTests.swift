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

    func testAggregatesSessionTokenDeltasForLocalDay() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let daySession = dir.appendingPathComponent("rollout-today.jsonl")
        try [
            #"{"timestamp":"2026-04-27T01:00:00Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"total_tokens":999999},"last_token_usage":{"total_tokens":100}}}}"#,
            #"{"timestamp":"2026-04-27T02:00:00Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"total_tokens":50}}}}"#,
            #"{"timestamp":"2026-04-27T03:00:00Z","type":"event_msg","payload":{"type":"task_started"}}"#,
            #"{"timestamp":"2026-04-26T23:59:59Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"total_tokens":1000}}}}"#
        ].joined(separator: "\n").write(to: daySession, atomically: true, encoding: .utf8)

        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(secondsFromGMT: 0)!
        let aggregator = CodexSessionUsageAggregator(
            rootURL: dir,
            calendar: calendar
        )

        let tokens = try aggregator.todayTokens(now: ISO8601DateFormatter().date(from: "2026-04-27T08:00:00Z")!)

        XCTAssertEqual(tokens, 150)
    }

    func testAggregatesSessionTokenDeltasWithNonUTCLocalDayBounds() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let daySession = dir.appendingPathComponent("rollout-today.jsonl")
        try [
            #"{"timestamp":"2026-04-26T15:59:59Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"total_tokens":1000}}}}"#,
            #"{"timestamp":"2026-04-26T16:00:00Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"total_tokens":25}}}}"#,
            #"{"timestamp":"2026-04-27T15:59:59Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"total_tokens":75}}}}"#,
            #"{"timestamp":"2026-04-27T16:00:00Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"total_tokens":1000}}}}"#
        ].joined(separator: "\n").write(to: daySession, atomically: true, encoding: .utf8)

        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(secondsFromGMT: 8 * 3600)!
        let aggregator = CodexSessionUsageAggregator(
            rootURL: dir,
            calendar: calendar
        )

        let tokens = try aggregator.todayTokens(now: ISO8601DateFormatter().date(from: "2026-04-27T08:00:00Z")!)

        XCTAssertEqual(tokens, 100)
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
