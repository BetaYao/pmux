import XCTest
@testable import amux

final class ClaudeUsageSummaryProviderTests: XCTestCase {
    func testReadsRateLimitsFromStatuslineCache() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let cache = dir.appendingPathComponent("claude-statusline.json")
        try """
        {"rate_limits":{"five_hour":{"used_percentage":19,"resets_at":"2026-04-27T12:19:00Z"}}}
        """.write(to: cache, atomically: true, encoding: .utf8)

        let reader = ClaudeStatuslineCacheReader(cacheURL: cache, staleInterval: 3600)
        let snapshot = try reader.read(now: Date(timeIntervalSince1970: 1_772_516_340))

        XCTAssertEqual(snapshot?.usedPercent, 19)
        XCTAssertEqual(snapshot?.resetsAt, ISO8601DateFormatter().date(from: "2026-04-27T12:19:00Z"))
    }

    func testReadsFractionalSecondRateLimitReset() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let cache = dir.appendingPathComponent("claude-statusline.json")
        try """
        {"rate_limits":{"five_hour":{"used_percentage":19,"resets_at":"2026-04-27T12:19:00.123Z"}}}
        """.write(to: cache, atomically: true, encoding: .utf8)

        let reader = ClaudeStatuslineCacheReader(cacheURL: cache, staleInterval: 3600)
        let snapshot = try reader.read(now: Date(timeIntervalSince1970: 1_772_516_340))

        XCTAssertEqual(snapshot?.resetsAt, Self.iso8601WithFractionalSeconds.date(from: "2026-04-27T12:19:00.123Z"))
    }

    func testAggregatesTodayTokensAndDedupesRequestIDs() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let project = dir.appendingPathComponent("project")
        try FileManager.default.createDirectory(at: project, withIntermediateDirectories: true)
        let transcript = project.appendingPathComponent("session.jsonl")
        try [
            #"{"timestamp":"2026-04-27T01:00:00Z","requestId":"req-1","message":{"usage":{"input_tokens":10,"cache_creation_input_tokens":20,"cache_read_input_tokens":30,"output_tokens":40}}}"#,
            #"{"timestamp":"2026-04-27T01:01:00Z","requestId":"req-1","message":{"usage":{"input_tokens":999,"output_tokens":999}}}"#,
            #"{"timestamp":"2026-04-26T23:59:59Z","requestId":"old","message":{"usage":{"input_tokens":1000}}}"#
        ].joined(separator: "\n").write(to: transcript, atomically: true, encoding: .utf8)

        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(secondsFromGMT: 0)!
        let aggregator = ClaudeTranscriptUsageAggregator(rootURL: dir, calendar: calendar)
        let tokens = try aggregator.todayTokens(now: ISO8601DateFormatter().date(from: "2026-04-27T08:00:00Z")!)

        XCTAssertEqual(tokens, 100)
    }

    func testAggregatesIdenticalRowsWithoutRequestIDAsSeparateLines() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let project = dir.appendingPathComponent("project")
        try FileManager.default.createDirectory(at: project, withIntermediateDirectories: true)
        let transcript = project.appendingPathComponent("session.jsonl")
        let row = #"{"timestamp":"2026-04-27T01:00:00Z","message":{"usage":{"input_tokens":7}}}"#
        try [row, row].joined(separator: "\n").write(to: transcript, atomically: true, encoding: .utf8)

        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(secondsFromGMT: 0)!
        let aggregator = ClaudeTranscriptUsageAggregator(rootURL: dir, calendar: calendar)
        let tokens = try aggregator.todayTokens(now: ISO8601DateFormatter().date(from: "2026-04-27T08:00:00Z")!)

        XCTAssertEqual(tokens, 14)
    }

    func testAggregatesFractionalSecondTranscriptTimestamps() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let project = dir.appendingPathComponent("project")
        try FileManager.default.createDirectory(at: project, withIntermediateDirectories: true)
        let transcript = project.appendingPathComponent("session.jsonl")
        try #"{"timestamp":"2026-04-27T01:00:00.123Z","requestId":"req-1","message":{"usage":{"input_tokens":10}}}"#
            .write(to: transcript, atomically: true, encoding: .utf8)

        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(secondsFromGMT: 0)!
        let aggregator = ClaudeTranscriptUsageAggregator(rootURL: dir, calendar: calendar)
        let tokens = try aggregator.todayTokens(now: ISO8601DateFormatter().date(from: "2026-04-27T08:00:00Z")!)

        XCTAssertEqual(tokens, 10)
    }

    private static let iso8601WithFractionalSeconds: ISO8601DateFormatter = {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return formatter
    }()
}
