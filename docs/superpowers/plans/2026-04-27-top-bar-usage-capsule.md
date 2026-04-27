# Top Bar Usage Capsule Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the primary title-bar capsule notification display with rotating shortcut, Claude Code usage, and Codex usage frames.

**Architecture:** Add a small usage subsystem that collects data off the main thread, formats it into title-bar frames, and lets `TitleBarView` render only prepared presentation data. Claude remaining comes from a statusline wrapper cache, Claude daily tokens from transcript JSONL, Codex remaining from the local Codex app-server rate-limit method, and Codex daily tokens from the local Codex SQLite state database via `sqlite3`.

**Tech Stack:** Swift 5.10, AppKit, Foundation `Process`, JSONSerialization/Codable, XCTest, XcodeGen, `xcodebuild`.

---

## File Structure

- Create `Sources/Usage/UsageSummary.swift`
  - Usage domain types: provider enum, rate-limit window, snapshot, title-bar frame, formatter.
- Create `Sources/Usage/ClaudeUsageSummaryProvider.swift`
  - Claude statusline cache reader and transcript JSONL daily-token aggregator.
- Create `Sources/Usage/CodexUsageSummaryProvider.swift`
  - Codex app-server rate-limit reader and SQLite daily-token reader.
- Create `Sources/Usage/UsageSummaryStore.swift`
  - Background refresh timer and main-thread callback to the UI.
- Create `Sources/Core/ClaudeStatuslineBridgeInstaller.swift`
  - Preserves existing Claude statusline command, writes a wrapper script, and installs the wrapper in `~/.claude/settings.json`.
- Modify `Sources/App/AppDelegate.swift`
  - Install the Claude statusline bridge after Claude hook setup when webhook integrations are enabled.
- Modify `Sources/App/MainWindowController.swift`
  - Own `UsageSummaryStore`, start it after layout setup, and feed updates into `TitleBarView`.
- Modify `Sources/UI/TitleBar/TitleBarView.swift`
  - Replace notification/tip capsule mode with utility frame rotation and add a compact usage progress bar.
- Create `Tests/UsageSummaryFormatterTests.swift`
- Create `Tests/ClaudeUsageSummaryProviderTests.swift`
- Create `Tests/CodexUsageSummaryProviderTests.swift`
- Create `Tests/ClaudeStatuslineBridgeInstallerTests.swift`
- Regenerate `amux.xcodeproj` with `xcodegen generate` after creating new Swift files.

## Task 1: Usage Models And Formatter

**Files:**
- Create: `Sources/Usage/UsageSummary.swift`
- Test: `Tests/UsageSummaryFormatterTests.swift`

- [ ] **Step 1: Write the failing formatter tests**

Create `Tests/UsageSummaryFormatterTests.swift`:

```swift
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
            now: Date(timeIntervalSince1970: 1_772_523_060)
        )

        XCTAssertEqual(frame.kind, .usage)
        XCTAssertEqual(frame.leadingText, "Codex")
        XCTAssertEqual(frame.bodyText, "剩余 72%")
        XCTAssertEqual(frame.trailingText, "Today 5.7M")
        XCTAssertEqual(frame.usageProgress, 0.28, accuracy: 0.001)
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
```

- [ ] **Step 2: Run the formatter tests and verify they fail**

Run: `xcodegen generate && xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/UsageSummaryFormatterTests`

Expected: FAIL because `UsageSnapshot`, `UsageRateLimitWindow`, and `UsageSummaryFormatter` do not exist.

- [ ] **Step 3: Add the minimal usage model and formatter**

Create `Sources/Usage/UsageSummary.swift`:

```swift
import Foundation

enum UsageProvider: String, Codable, Equatable {
    case claude
    case codex

    var displayName: String {
        switch self {
        case .claude: return "Claude"
        case .codex: return "Codex"
        }
    }
}

struct UsageRateLimitWindow: Codable, Equatable {
    let usedPercent: Int
    let resetsAt: Date?

    var remainingPercent: Int {
        max(0, min(100, 100 - usedPercent))
    }

    var progress: Double {
        Double(max(0, min(100, usedPercent))) / 100.0
    }
}

struct UsageSnapshot: Equatable {
    let provider: UsageProvider
    let rateLimit: UsageRateLimitWindow?
    let todayTokens: Int?
    let updatedAt: Date?
    let isStale: Bool
}

enum PrimaryCapsuleFrameKind: Equatable {
    case shortcut
    case usage
}

struct PrimaryCapsuleFrame: Equatable {
    let kind: PrimaryCapsuleFrameKind
    let iconName: String
    let leadingText: String
    let bodyText: String
    let trailingText: String
    let usageProgress: Double?
    let resetText: String?

    static func shortcut(leading: String, body: String) -> PrimaryCapsuleFrame {
        PrimaryCapsuleFrame(
            kind: .shortcut,
            iconName: "command",
            leadingText: leading,
            bodyText: body,
            trailingText: "Shortcuts",
            usageProgress: nil,
            resetText: nil
        )
    }
}

enum UsageSummaryFormatter {
    static func formatUsageFrame(_ snapshot: UsageSnapshot, now: Date = Date()) -> PrimaryCapsuleFrame {
        let remaining = snapshot.rateLimit.map { "\($0.remainingPercent)%" } ?? "--"
        let today = snapshot.todayTokens.map(compactTokenCount) ?? "--"
        let resetText = snapshot.rateLimit?.resetsAt.flatMap { compactResetText(until: $0, now: now) }
        return PrimaryCapsuleFrame(
            kind: .usage,
            iconName: snapshot.provider == .claude ? "sparkles" : "terminal",
            leadingText: snapshot.provider.displayName,
            bodyText: "剩余 \(remaining)",
            trailingText: "Today \(today)",
            usageProgress: snapshot.rateLimit?.progress,
            resetText: resetText
        )
    }

    static func compactTokenCount(_ count: Int) -> String {
        if count >= 1_000_000 {
            return String(format: "%.1fM", Double(count) / 1_000_000.0)
        }
        if count >= 1_000 {
            return String(format: "%.1fK", Double(count) / 1_000.0)
        }
        return "\(count)"
    }

    private static func compactResetText(until resetsAt: Date, now: Date) -> String? {
        let seconds = max(0, Int(resetsAt.timeIntervalSince(now)))
        guard seconds > 0 else { return nil }
        let hours = seconds / 3600
        let minutes = (seconds % 3600) / 60
        if hours > 0 {
            return "\(hours)h \(minutes)m"
        }
        return "\(max(1, minutes))m"
    }
}
```

- [ ] **Step 4: Run the formatter tests and verify they pass**

Run: `xcodegen generate && xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/UsageSummaryFormatterTests`

Expected: PASS.

## Task 2: Claude Usage Provider And Statusline Bridge Installer

**Files:**
- Create: `Sources/Usage/ClaudeUsageSummaryProvider.swift`
- Create: `Sources/Core/ClaudeStatuslineBridgeInstaller.swift`
- Test: `Tests/ClaudeUsageSummaryProviderTests.swift`
- Test: `Tests/ClaudeStatuslineBridgeInstallerTests.swift`

- [ ] **Step 1: Write failing Claude provider tests**

Create `Tests/ClaudeUsageSummaryProviderTests.swift`:

```swift
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
}
```

- [ ] **Step 2: Write failing Claude bridge installer tests**

Create `Tests/ClaudeStatuslineBridgeInstallerTests.swift`:

```swift
import XCTest
@testable import amux

final class ClaudeStatuslineBridgeInstallerTests: XCTestCase {
    func testInstallsWrapperAndPreservesExistingCommand() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let claudeDir = root.appendingPathComponent(".claude")
        let supportDir = root.appendingPathComponent("Application Support/amux")
        try FileManager.default.createDirectory(at: claudeDir, withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: supportDir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let settings = claudeDir.appendingPathComponent("settings.json")
        try #"{"statusLine":{"type":"command","command":"claude-hud"}}"#.write(to: settings, atomically: true, encoding: .utf8)

        let changed = ClaudeStatuslineBridgeInstaller.ensureInstalledForTests(
            settingsURL: settings,
            supportDirectory: supportDir,
            cacheDirectory: root.appendingPathComponent("Caches/amux")
        )

        XCTAssertTrue(changed)
        let preserved = try String(contentsOf: supportDir.appendingPathComponent("claude-statusline-original-command"), encoding: .utf8)
        XCTAssertEqual(preserved, "claude-hud")
        let data = try Data(contentsOf: settings)
        let json = try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
        let statusLine = try XCTUnwrap(json["statusLine"] as? [String: Any])
        XCTAssertEqual(statusLine["type"] as? String, "command")
        XCTAssertTrue((statusLine["command"] as? String)?.contains("claude-statusline-bridge.sh") == true)
    }
}
```

- [ ] **Step 3: Run the Claude tests and verify they fail**

Run: `xcodegen generate && xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/ClaudeUsageSummaryProviderTests -only-testing:amuxTests/ClaudeStatuslineBridgeInstallerTests`

Expected: FAIL because the Claude provider and installer types do not exist.

- [ ] **Step 4: Implement Claude cache reader, transcript aggregator, and provider**

Create `Sources/Usage/ClaudeUsageSummaryProvider.swift`:

```swift
import Foundation

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
        let resetsAt = (window?["resets_at"] as? String).flatMap { ISO8601DateFormatter().date(from: $0) }
        return ClaudeStatuslineRateLimit(usedPercent: used, resetsAt: resetsAt)
    }
}

struct ClaudeTranscriptUsageAggregator {
    let rootURL: URL
    let calendar: Calendar

    func todayTokens(now: Date = Date()) throws -> Int {
        let start = calendar.startOfDay(for: now)
        let end = calendar.date(byAdding: .day, value: 1, to: start)!
        var seen = Set<String>()
        var total = 0
        let files = FileManager.default.enumerator(at: rootURL, includingPropertiesForKeys: nil)?
            .compactMap { $0 as? URL }
            .filter { $0.pathExtension == "jsonl" } ?? []
        for file in files {
            let text = try String(contentsOf: file, encoding: .utf8)
            for line in text.split(separator: "\n") {
                guard let data = String(line).data(using: .utf8),
                      let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
                      let timestamp = object["timestamp"] as? String,
                      let date = ISO8601DateFormatter().date(from: timestamp),
                      date >= start && date < end,
                      let message = object["message"] as? [String: Any],
                      let usage = message["usage"] as? [String: Any] else { continue }
                let dedupeKey = (object["requestId"] as? String) ?? (object["uuid"] as? String) ?? "\(file.path)#\(line.hashValue)"
                guard seen.insert(dedupeKey).inserted else { continue }
                total += usage["input_tokens"] as? Int ?? 0
                total += usage["cache_creation_input_tokens"] as? Int ?? 0
                total += usage["cache_read_input_tokens"] as? Int ?? 0
                total += usage["output_tokens"] as? Int ?? 0
            }
        }
        return total
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
            isStale: rateLimit == nil
        )
    }
}
```

- [ ] **Step 5: Implement the Claude statusline bridge installer**

Create `Sources/Core/ClaudeStatuslineBridgeInstaller.swift`:

```swift
import Foundation

enum ClaudeStatuslineBridgeInstaller {
    @discardableResult
    static func ensureInstalled() -> Bool {
        let home = FileManager.default.homeDirectoryForCurrentUser
        return ensureInstalled(
            settingsURL: home.appendingPathComponent(".claude/settings.json"),
            supportDirectory: home.appendingPathComponent("Library/Application Support/amux"),
            cacheDirectory: home.appendingPathComponent("Library/Caches/amux")
        )
    }

    @discardableResult
    private static func ensureInstalled(settingsURL: URL, supportDirectory: URL, cacheDirectory: URL) -> Bool {
        guard let data = try? Data(contentsOf: settingsURL),
              var settings = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let statusLine = settings["statusLine"] as? [String: Any],
              statusLine["type"] as? String == "command",
              let originalCommand = statusLine["command"] as? String,
              !originalCommand.contains("claude-statusline-bridge.sh") else { return false }

        do {
            try FileManager.default.createDirectory(at: supportDirectory, withIntermediateDirectories: true)
            try FileManager.default.createDirectory(at: cacheDirectory, withIntermediateDirectories: true)
            let originalURL = supportDirectory.appendingPathComponent("claude-statusline-original-command")
            try originalCommand.write(to: originalURL, atomically: true, encoding: .utf8)
            let scriptURL = supportDirectory.appendingPathComponent("claude-statusline-bridge.sh")
            try bridgeScript(originalCommandURL: originalURL, cacheURL: cacheDirectory.appendingPathComponent("claude-statusline.json"))
                .write(to: scriptURL, atomically: true, encoding: .utf8)
            try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: scriptURL.path)
            settings["statusLine"] = ["type": "command", "command": scriptURL.path]
            let output = try JSONSerialization.data(withJSONObject: settings, options: [.prettyPrinted, .sortedKeys])
            try output.write(to: settingsURL, options: .atomic)
            return true
        } catch {
            NSLog("[ClaudeStatuslineBridgeInstaller] Failed to install bridge: \(error)")
            return false
        }
    }

    private static func bridgeScript(originalCommandURL: URL, cacheURL: URL) -> String {
        """
        #!/bin/sh
        input=$(cat)
        mkdir -p "\(cacheURL.deletingLastPathComponent().path)"
        tmp="\(cacheURL.path).tmp"
        printf '%s' "$input" > "$tmp" && mv "$tmp" "\(cacheURL.path)"
        if [ -f "\(originalCommandURL.path)" ]; then
          original=$(cat "\(originalCommandURL.path)")
          if [ -n "$original" ]; then
            printf '%s' "$input" | /bin/sh -lc "$original"
          fi
        fi
        """
    }
}

#if DEBUG
extension ClaudeStatuslineBridgeInstaller {
    static func ensureInstalledForTests(settingsURL: URL, supportDirectory: URL, cacheDirectory: URL) -> Bool {
        ensureInstalled(settingsURL: settingsURL, supportDirectory: supportDirectory, cacheDirectory: cacheDirectory)
    }
}
#endif
```

- [ ] **Step 6: Run the Claude tests and verify they pass**

Run: `xcodegen generate && xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/ClaudeUsageSummaryProviderTests -only-testing:amuxTests/ClaudeStatuslineBridgeInstallerTests`

Expected: PASS.

## Task 3: Codex Usage Provider

**Files:**
- Create: `Sources/Usage/CodexUsageSummaryProvider.swift`
- Test: `Tests/CodexUsageSummaryProviderTests.swift`

- [ ] **Step 1: Write failing Codex provider tests**

Create `Tests/CodexUsageSummaryProviderTests.swift`:

```swift
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
}
```

- [ ] **Step 2: Run the Codex tests and verify they fail**

Run: `xcodegen generate && xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/CodexUsageSummaryProviderTests`

Expected: FAIL because `CodexRateLimitParser` and `CodexSQLiteDailyUsageReader` do not exist.

- [ ] **Step 3: Implement Codex rate-limit and daily-token readers**

Create `Sources/Usage/CodexUsageSummaryProvider.swift`:

```swift
import Foundation

enum CodexRateLimitParser {
    static func parseResponse(_ data: Data) throws -> UsageRateLimitWindow? {
        let root = try JSONSerialization.jsonObject(with: data) as? [String: Any]
        let result = root?["result"] as? [String: Any]
        let byLimit = result?["rateLimitsByLimitId"] as? [String: Any]
        let codex = byLimit?["codex"] as? [String: Any]
        let fallback = result?["rateLimits"] as? [String: Any]
        let snapshot = codex ?? fallback
        let primary = snapshot?["primary"] as? [String: Any]
        guard let used = primary?["usedPercent"] as? Int else { return nil }
        let resetsAt = (primary?["resetsAt"] as? TimeInterval).map { Date(timeIntervalSince1970: $0) }
        return UsageRateLimitWindow(usedPercent: used, resetsAt: resetsAt)
    }
}

struct CodexAppServerRateLimitClient {
    var codexExecutable: String = "codex"

    func readRateLimit() -> UsageRateLimitWindow? {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        process.arguments = [codexExecutable, "app-server", "--listen", "stdio://"]
        let stdin = Pipe()
        let stdout = Pipe()
        process.standardInput = stdin
        process.standardOutput = stdout
        process.standardError = Pipe()
        do {
            try process.run()
            let input = [
                #"{"id":1,"method":"initialize","params":{"clientInfo":{"name":"amux","version":"2.0.0"},"capabilities":{}}}"#,
                #"{"method":"initialized"}"#,
                #"{"id":2,"method":"account/rateLimits/read"}"#
            ].joined(separator: "\n") + "\n"
            stdin.fileHandleForWriting.write(Data(input.utf8))
            stdin.fileHandleForWriting.closeFile()
            process.waitUntilExit()
            let output = stdout.fileHandleForReading.readDataToEndOfFile()
            for line in String(decoding: output, as: UTF8.self).split(separator: "\n") {
                guard line.contains(#""id":2"#), let data = String(line).data(using: .utf8) else { continue }
                return try? CodexRateLimitParser.parseResponse(data)
            }
        } catch {
            NSLog("[CodexAppServerRateLimitClient] Failed to read rate limits: \(error)")
        }
        return nil
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
```

- [ ] **Step 4: Run the Codex tests and verify they pass**

Run: `xcodegen generate && xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/CodexUsageSummaryProviderTests`

Expected: PASS.

## Task 4: Usage Store And Title Bar UI

**Files:**
- Create: `Sources/Usage/UsageSummaryStore.swift`
- Modify: `Sources/UI/TitleBar/TitleBarView.swift`
- Test: `Tests/UsageSummaryFormatterTests.swift`

- [ ] **Step 1: Add a failing frame construction test**

Append to `Tests/UsageSummaryFormatterTests.swift`:

```swift
func testBuildsRotationFramesWithShortcutClaudeAndCodex() {
    let frames = UsageSummaryFormatter.rotationFrames(
        claude: UsageSnapshot(provider: .claude, rateLimit: nil, todayTokens: 12_000, updatedAt: nil, isStale: true),
        codex: UsageSnapshot(provider: .codex, rateLimit: UsageRateLimitWindow(usedPercent: 40, resetsAt: nil), todayTokens: 34_000, updatedAt: nil, isStale: false)
    )

    XCTAssertEqual(frames.count, 10)
    XCTAssertEqual(frames.first?.kind, .shortcut)
    XCTAssertEqual(frames[8].leadingText, "Claude")
    XCTAssertEqual(frames[9].leadingText, "Codex")
}
```

- [ ] **Step 2: Run the test and verify it fails**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/UsageSummaryFormatterTests/testBuildsRotationFramesWithShortcutClaudeAndCodex`

Expected: FAIL because `rotationFrames` does not exist.

- [ ] **Step 3: Add rotation frame construction**

Update `Sources/Usage/UsageSummary.swift`:

```swift
extension UsageSummaryFormatter {
    static let shortcutTips: [(leading: String, body: String)] = [
        ("Tip", "Cmd+1..4 switch layout"),
        ("Tip", "Cmd+J toggle dashboard focus"),
        ("Tip", "Cmd+B toggle sidebar"),
        ("Tip", "Cmd+D split horizontally"),
        ("Tip", "Cmd+Shift+D split vertically"),
        ("Tip", "Cmd+Option+Arrow move focus"),
        ("Tip", "Cmd+Ctrl+Arrow resize split"),
        ("Tip", "Cmd+Shift+F show diff"),
    ]

    static func rotationFrames(claude: UsageSnapshot, codex: UsageSnapshot, now: Date = Date()) -> [PrimaryCapsuleFrame] {
        shortcutTips.map { PrimaryCapsuleFrame.shortcut(leading: $0.leading, body: $0.body) }
            + [formatUsageFrame(claude, now: now), formatUsageFrame(codex, now: now)]
    }
}
```

- [ ] **Step 4: Add the usage summary store**

Create `Sources/Usage/UsageSummaryStore.swift`:

```swift
import Foundation

final class UsageSummaryStore {
    typealias UpdateHandler = ([PrimaryCapsuleFrame]) -> Void

    private let claudeProvider: ClaudeUsageSummaryProvider
    private let codexProvider: CodexUsageSummaryProvider
    private let refreshInterval: TimeInterval
    private var timer: Timer?
    var onUpdate: UpdateHandler?

    init(claudeProvider: ClaudeUsageSummaryProvider, codexProvider: CodexUsageSummaryProvider, refreshInterval: TimeInterval = 60) {
        self.claudeProvider = claudeProvider
        self.codexProvider = codexProvider
        self.refreshInterval = refreshInterval
    }

    func start() {
        refresh()
        timer = Timer.scheduledTimer(withTimeInterval: refreshInterval, repeats: true) { [weak self] _ in
            self?.refresh()
        }
    }

    func stop() {
        timer?.invalidate()
        timer = nil
    }

    private func refresh() {
        DispatchQueue.global(qos: .utility).async { [claudeProvider, codexProvider, weak self] in
            let claude = claudeProvider.snapshot()
            let codex = codexProvider.snapshot()
            let frames = UsageSummaryFormatter.rotationFrames(claude: claude, codex: codex)
            DispatchQueue.main.async {
                self?.onUpdate?(frames)
            }
        }
    }
}
```

- [ ] **Step 5: Update `TitleBarView` to render utility frames**

Modify `Sources/UI/TitleBar/TitleBarView.swift`:

```swift
private enum PrimaryCapsuleMode {
    case utility
}

private var primaryCapsuleMode: PrimaryCapsuleMode = .utility
private var primaryCapsuleFrames: [PrimaryCapsuleFrame] = UsageSummaryFormatter.rotationFrames(
    claude: UsageSnapshot(provider: .claude, rateLimit: nil, todayTokens: nil, updatedAt: nil, isStale: true),
    codex: UsageSnapshot(provider: .codex, rateLimit: nil, todayTokens: nil, updatedAt: nil, isStale: true)
)
private var currentPrimaryCapsuleIndex = 0
private let usageProgressTrack = NSView()
private let usageProgressFill = NSView()
private var usageProgressWidthConstraint: NSLayoutConstraint?

func updateNotificationSummary(entry: NotificationEntry?, unreadCount: Int) {
    highlightedNotificationStatus = nil
    updateArcBlockColors()
}

func updatePrimaryCapsuleFrames(_ frames: [PrimaryCapsuleFrame]) {
    guard !frames.isEmpty else { return }
    primaryCapsuleFrames = frames
    currentPrimaryCapsuleIndex = min(currentPrimaryCapsuleIndex, frames.count - 1)
    showCurrentPrimaryCapsuleFrame()
    startTipRotationIfNeeded()
}
```

Inside `setupLeftArcBlock()`, configure the progress bar before adding arranged subviews:

```swift
usageProgressTrack.wantsLayer = true
usageProgressTrack.layer?.cornerRadius = 3
usageProgressTrack.layer?.backgroundColor = NSColor(hex: 0x5E6A75).withAlphaComponent(0.35).cgColor
usageProgressTrack.translatesAutoresizingMaskIntoConstraints = false
usageProgressTrack.isHidden = true

usageProgressFill.wantsLayer = true
usageProgressFill.layer?.cornerRadius = 3
usageProgressFill.layer?.backgroundColor = SemanticColors.accent.cgColor
usageProgressFill.translatesAutoresizingMaskIntoConstraints = false
usageProgressTrack.addSubview(usageProgressFill)

NSLayoutConstraint.activate([
    usageProgressTrack.widthAnchor.constraint(equalToConstant: 78),
    usageProgressTrack.heightAnchor.constraint(equalToConstant: 10),
    usageProgressFill.leadingAnchor.constraint(equalTo: usageProgressTrack.leadingAnchor),
    usageProgressFill.topAnchor.constraint(equalTo: usageProgressTrack.topAnchor),
    usageProgressFill.bottomAnchor.constraint(equalTo: usageProgressTrack.bottomAnchor),
])
usageProgressWidthConstraint = usageProgressFill.widthAnchor.constraint(equalToConstant: 0)
usageProgressWidthConstraint?.isActive = true
```

Change the arranged subview order to:

```swift
primaryCapsuleStack.addArrangedSubview(capsuleIconView)
primaryCapsuleStack.addArrangedSubview(capsuleLeadingLabel)
primaryCapsuleStack.addArrangedSubview(capsuleSep1Label)
primaryCapsuleStack.addArrangedSubview(capsuleBodyLabel)
primaryCapsuleStack.addArrangedSubview(usageProgressTrack)
primaryCapsuleStack.addArrangedSubview(capsuleSep2Label)
primaryCapsuleStack.addArrangedSubview(capsuleTrailingLabel)
```

Replace `showCurrentTip()` with:

```swift
private func showCurrentPrimaryCapsuleFrame() {
    let frame = primaryCapsuleFrames[currentPrimaryCapsuleIndex]
    capsuleIconView.image = NSImage(systemSymbolName: frame.iconName, accessibilityDescription: frame.leadingText)
    capsuleIconView.contentTintColor = frame.kind == .usage ? SemanticColors.accent : SemanticColors.muted
    capsuleLeadingLabel.attributedStringValue = NSAttributedString(
        string: frame.leadingText,
        attributes: [
            .font: NSFont.systemFont(ofSize: 11, weight: .semibold),
            .foregroundColor: SemanticColors.text
        ]
    )
    capsuleBodyLabel.stringValue = frame.bodyText
    capsuleTrailingLabel.stringValue = [frame.resetText, frame.trailingText]
        .compactMap { $0 }
        .filter { !$0.isEmpty }
        .joined(separator: "  ")
    usageProgressTrack.isHidden = frame.usageProgress == nil
    usageProgressWidthConstraint?.constant = 78 * CGFloat(frame.usageProgress ?? 0)
    updatePrimaryCapsuleSeparators()
    updateArcBlockColors()
}
```

Replace `advanceTipIfNeeded()` with:

```swift
private func advanceTipIfNeeded() {
    guard !isPrimaryCapsuleHovered else { return }
    currentPrimaryCapsuleIndex = (currentPrimaryCapsuleIndex + 1) % primaryCapsuleFrames.count
    showCurrentPrimaryCapsuleFrame()
}
```

Update `setup()` to call `showCurrentPrimaryCapsuleFrame()` instead of `showCurrentTip()`.

- [ ] **Step 6: Run formatter tests and build**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/UsageSummaryFormatterTests`

Expected: PASS.

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build`

Expected: PASS.

## Task 5: Wire Providers Into App Launch And Main Window

**Files:**
- Modify: `Sources/App/AppDelegate.swift`
- Modify: `Sources/App/MainWindowController.swift`

- [ ] **Step 1: Install the Claude statusline bridge on launch**

Modify `Sources/App/AppDelegate.swift` inside the existing `if config.webhook.enabled` block:

```swift
if config.webhook.enabled {
    ClaudeHooksSetup.ensureHooksConfigured(port: config.webhook.port)
    ClaudeStatuslineBridgeInstaller.ensureInstalled()
    CodexHooksSetup.ensureHooksConfigured(port: config.webhook.port)
}
```

- [ ] **Step 2: Add usage store wiring to `MainWindowController`**

Modify `Sources/App/MainWindowController.swift` by adding a lazy store property:

```swift
private lazy var usageSummaryStore: UsageSummaryStore = {
    let home = FileManager.default.homeDirectoryForCurrentUser
    var calendar = Calendar.current
    calendar.timeZone = TimeZone.current
    return UsageSummaryStore(
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
        )
    )
}()
```

After `handleNotificationHistoryDidChange(nil)` in `convenience init()`:

```swift
usageSummaryStore.onUpdate = { [weak self] frames in
    self?.titleBar.updatePrimaryCapsuleFrames(frames)
}
usageSummaryStore.start()
```

Inside `cleanupBeforeTermination()`:

```swift
usageSummaryStore.stop()
```

- [ ] **Step 3: Run targeted tests**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/UsageSummaryFormatterTests -only-testing:amuxTests/ClaudeUsageSummaryProviderTests -only-testing:amuxTests/CodexUsageSummaryProviderTests -only-testing:amuxTests/ClaudeStatuslineBridgeInstallerTests`

Expected: PASS.

- [ ] **Step 4: Run final build**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build`

Expected: PASS.

## Self-Review Checklist

- Spec coverage:
  - Shortcut, Claude, and Codex rotation is covered by Tasks 1 and 4.
  - Reference-style progress display is covered by Task 4.
  - Claude statusline bridge is covered by Task 2 and wired in Task 5.
  - Claude daily token aggregation is covered by Task 2.
  - Codex rate limits and daily tokens are covered by Task 3.
  - Non-blocking fallback behavior is covered by nil-returning providers and formatter placeholder tests.
- Placeholder scan:
  - No `TBD` or open-ended implementation steps remain.
- Type consistency:
  - `UsageSnapshot`, `UsageRateLimitWindow`, `PrimaryCapsuleFrame`, and provider names are introduced in Task 1 before use in later tasks.
  - `ClaudeStatuslineBridgeInstaller.ensureInstalled()` is introduced before AppDelegate wiring.
  - `UsageSummaryStore` accepts concrete Claude/Codex providers and produces `[PrimaryCapsuleFrame]` consumed by `TitleBarView`.
