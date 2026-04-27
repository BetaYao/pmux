import Foundation
import Darwin

enum CodexRateLimitParser {
    static func parseResponse(_ data: Data) throws -> UsageRateLimitWindow? {
        let root = try JSONSerialization.jsonObject(with: data) as? [String: Any]
        let result = root?["result"] as? [String: Any]
        let byLimit = result?["rateLimitsByLimitId"] as? [String: Any]
        let codex = byLimit?["codex"] as? [String: Any]
        let fallback = result?["rateLimits"] as? [String: Any]
        return parseSnapshot(codex) ?? parseSnapshot(fallback)
    }

    static func parseResponseLine(_ line: String, expectedID: Int) throws -> UsageRateLimitWindow? {
        guard let data = line.data(using: .utf8) else { return nil }
        let root = try JSONSerialization.jsonObject(with: data) as? [String: Any]
        guard intValue(root?["id"]) == expectedID else { return nil }
        return try parseResponse(data)
    }

    private static func parseSnapshot(_ snapshot: [String: Any]?) -> UsageRateLimitWindow? {
        let primary = snapshot?["primary"] as? [String: Any]
        guard let used = intValue(primary?["usedPercent"]) else { return nil }
        let resetsAt = timeIntervalValue(primary?["resetsAt"]).map { Date(timeIntervalSince1970: $0) }
        return UsageRateLimitWindow(usedPercent: used, resetsAt: resetsAt)
    }

    private static func intValue(_ value: Any?) -> Int? {
        if let int = value as? Int { return int }
        if let number = value as? NSNumber { return number.intValue }
        return nil
    }

    private static func timeIntervalValue(_ value: Any?) -> TimeInterval? {
        if let timeInterval = value as? TimeInterval { return timeInterval }
        if let number = value as? NSNumber { return number.doubleValue }
        return nil
    }
}

struct CodexAppServerRateLimitClient {
    var codexExecutable: String = "codex"

    func readRateLimit(timeout: TimeInterval = 5) -> UsageRateLimitWindow? {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        process.arguments = [codexExecutable, "app-server", "--listen", "stdio://"]
        let stdin = Pipe()
        let stdout = Pipe()
        let stderr = Pipe()
        process.standardInput = stdin
        process.standardOutput = stdout
        process.standardError = stderr

        let outputGroup = DispatchGroup()
        let stdoutQueue = DispatchQueue(label: "codex-rate-limit.stdout")
        let stderrQueue = DispatchQueue(label: "codex-rate-limit.stderr")
        var stdoutData = Data()
        do {
            try process.run()
            outputGroup.enter()
            stdoutQueue.async {
                stdoutData = stdout.fileHandleForReading.readDataToEndOfFile()
                outputGroup.leave()
            }
            outputGroup.enter()
            stderrQueue.async {
                _ = stderr.fileHandleForReading.readDataToEndOfFile()
                outputGroup.leave()
            }

            let input = [
                #"{"id":1,"method":"initialize","params":{"clientInfo":{"name":"amux","version":"2.0.0"},"capabilities":{}}}"#,
                #"{"method":"initialized"}"#,
                #"{"id":2,"method":"account/rateLimits/read"}"#
            ].joined(separator: "\n") + "\n"
            stdin.fileHandleForWriting.write(Data(input.utf8))
            stdin.fileHandleForWriting.closeFile()

            guard waitUntilExit(process, timeout: timeout) else {
                terminate(process)
                _ = outputGroup.wait(timeout: .now() + 1)
                return nil
            }
            _ = outputGroup.wait(timeout: .now() + 1)

            for line in String(decoding: stdoutData, as: UTF8.self).split(separator: "\n") {
                if let rateLimit = try? CodexRateLimitParser.parseResponseLine(String(line), expectedID: 2) {
                    return rateLimit
                }
            }
        } catch {
            NSLog("[CodexAppServerRateLimitClient] Failed to read rate limits: \(error)")
            terminate(process)
            _ = outputGroup.wait(timeout: .now() + 1)
        }
        return nil
    }

    private func waitUntilExit(_ process: Process, timeout: TimeInterval) -> Bool {
        let group = DispatchGroup()
        group.enter()
        process.terminationHandler = { _ in group.leave() }
        if !process.isRunning {
            process.terminationHandler = nil
            return true
        }
        let result = group.wait(timeout: .now() + timeout)
        process.terminationHandler = nil
        return result == .success
    }

    private func terminate(_ process: Process) {
        guard process.isRunning else { return }
        let group = DispatchGroup()
        group.enter()
        process.terminationHandler = { _ in group.leave() }
        process.terminate()
        if group.wait(timeout: .now() + 0.5) == .timedOut, process.isRunning {
            kill(process.processIdentifier, SIGKILL)
            _ = group.wait(timeout: .now() + 0.5)
        }
        process.terminationHandler = nil
    }
}

struct CodexSQLiteDailyUsageReader {
    let databaseURL: URL
    let calendar: Calendar

    static func query(now: Date, calendar: Calendar) -> String {
        let start = Int(calendar.startOfDay(for: now).timeIntervalSince1970)
        let end = Int(calendar.date(byAdding: .day, value: 1, to: calendar.startOfDay(for: now))!.timeIntervalSince1970)
        return "select coalesce(sum(tokens_used),0) from threads where updated_at >= \(start) and updated_at < \(end);"
    }

    func todayTokens(now: Date = Date()) -> Int? {
        let output = ProcessRunner.output(["sqlite3", databaseURL.path, Self.query(now: now, calendar: calendar)])
        return output.flatMap { Int($0.trimmingCharacters(in: .whitespacesAndNewlines)) }
    }
}

struct CodexUsageSummaryProvider {
    let rateLimitClient: CodexAppServerRateLimitClient
    let dailyUsageReader: CodexSQLiteDailyUsageReader

    func snapshot(now: Date = Date()) -> UsageSnapshot {
        UsageSnapshot(
            provider: .codex,
            rateLimit: rateLimitClient.readRateLimit(),
            todayTokens: dailyUsageReader.todayTokens(now: now),
            updatedAt: now,
            isStale: false
        )
    }
}
