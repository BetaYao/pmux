import Foundation

private enum ClaudeISO8601Parser {
    private static let defaultFormatter = ISO8601DateFormatter()
    private static let fractionalSecondsFormatter: ISO8601DateFormatter = {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return formatter
    }()

    static func date(from string: String) -> Date? {
        defaultFormatter.date(from: string) ?? fractionalSecondsFormatter.date(from: string)
    }
}

struct ClaudeStatuslineRateLimit: Equatable {
    let usedPercent: Int
    let resetsAt: Date?
}

struct ClaudeStatuslineCacheReader {
    let cacheURL: URL
    let staleInterval: TimeInterval

    func read(now: Date = Date()) throws -> ClaudeStatuslineRateLimit? {
        let values = try cacheURL.resourceValues(forKeys: [.contentModificationDateKey])
        guard let modified = values.contentModificationDate,
              now.timeIntervalSince(modified) <= staleInterval else { return nil }
        let data = try Data(contentsOf: cacheURL)
        let root = try JSONSerialization.jsonObject(with: data) as? [String: Any]
        let rateLimits = root?["rate_limits"] as? [String: Any]
        let window = rateLimits?["five_hour"] as? [String: Any]
            ?? rateLimits?["seven_day"] as? [String: Any]
        guard let used = window?["used_percentage"] as? Int else { return nil }
        let resetsAt = (window?["resets_at"] as? String).flatMap { ClaudeISO8601Parser.date(from: $0) }
        return ClaudeStatuslineRateLimit(usedPercent: used, resetsAt: resetsAt)
    }
}

struct ClaudeTranscriptUsageAggregator {
    let rootURL: URL
    let calendar: Calendar
    var modificationGraceInterval: TimeInterval? = nil

    func todayTokens(now: Date = Date()) throws -> Int {
        let start = calendar.startOfDay(for: now)
        let end = calendar.date(byAdding: .day, value: 1, to: start)!
        var seen = Set<String>()
        var total = 0
        let files = try transcriptFiles(dayStart: start)
        for file in files {
            let text = try String(contentsOf: file, encoding: .utf8)
            for (lineIndex, line) in text.split(separator: "\n").enumerated() {
                guard let data = String(line).data(using: .utf8),
                      let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
                      let timestamp = object["timestamp"] as? String,
                      let date = ClaudeISO8601Parser.date(from: timestamp),
                      date >= start && date < end,
                      let message = object["message"] as? [String: Any],
                      let usage = message["usage"] as? [String: Any] else { continue }
                let dedupeKey = (object["requestId"] as? String) ?? (object["uuid"] as? String) ?? "\(file.path)#\(lineIndex + 1)"
                guard seen.insert(dedupeKey).inserted else { continue }
                total += usage["input_tokens"] as? Int ?? 0
                total += usage["cache_creation_input_tokens"] as? Int ?? 0
                total += usage["cache_read_input_tokens"] as? Int ?? 0
                total += usage["output_tokens"] as? Int ?? 0
            }
        }
        return total
    }

    private func transcriptFiles(dayStart: Date) throws -> [URL] {
        var isDirectory: ObjCBool = false
        guard FileManager.default.fileExists(atPath: rootURL.path, isDirectory: &isDirectory),
              isDirectory.boolValue else {
            throw CocoaError(.fileReadNoSuchFile)
        }
        guard let enumerator = FileManager.default.enumerator(
            at: rootURL,
            includingPropertiesForKeys: [.contentModificationDateKey]
        ) else {
            throw CocoaError(.fileReadUnknown)
        }
        return enumerator
            .compactMap { $0 as? URL }
            .filter { $0.pathExtension == "jsonl" && shouldReadFile($0, dayStart: dayStart) }
    }

    private func shouldReadFile(_ file: URL, dayStart: Date) -> Bool {
        guard let modificationGraceInterval else { return true }
        guard let modified = try? file.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate else {
            return true
        }
        return modified >= dayStart.addingTimeInterval(-modificationGraceInterval)
    }
}

struct ClaudeUsageSummaryProvider {
    let cacheReader: ClaudeStatuslineCacheReader
    let transcriptAggregator: ClaudeTranscriptUsageAggregator

    func snapshot(now: Date = Date()) -> UsageSnapshot {
        let rateLimit = try? cacheReader.read(now: now)
        let tokens = try? transcriptAggregator.todayTokens(now: now)
        return UsageSnapshot(
            provider: .claude,
            rateLimit: rateLimit.map { UsageRateLimitWindow(usedPercent: $0.usedPercent, resetsAt: $0.resetsAt) },
            todayTokens: tokens,
            updatedAt: now,
            isStale: rateLimit == nil && tokens == nil
        )
    }
}
