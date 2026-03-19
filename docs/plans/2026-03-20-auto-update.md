# Implementation Plan: Auto-Update

**Spec:** `docs/superpowers/specs/2026-03-19-auto-update-design.md`

## Task 1: Add `skippedVersion` to UpdateConfig

**File:** `Sources/Core/Config.swift`

Add `skippedVersion` field to `UpdateConfig` (~line 130):

```swift
struct UpdateConfig: Codable {
    var enabled: Bool = true
    var checkIntervalHours: Int = 6
    var skippedVersion: String? = nil

    enum CodingKeys: String, CodingKey {
        case enabled
        case checkIntervalHours = "check_interval_hours"
        case skippedVersion = "skipped_version"
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        enabled = try container.decodeIfPresent(Bool.self, forKey: .enabled) ?? true
        checkIntervalHours = try container.decodeIfPresent(Int.self, forKey: .checkIntervalHours) ?? 6
        skippedVersion = try container.decodeIfPresent(String.self, forKey: .skippedVersion)
    }
}
```

---

## Task 2: Create SemVer comparison utility

**File:** `Sources/Update/SemVer.swift` (new)

```swift
struct SemVer: Comparable {
    let major: Int
    let minor: Int
    let patch: Int

    init?(_ string: String) {
        let stripped = string.hasPrefix("v") ? String(string.dropFirst()) : string
        let parts = stripped.split(separator: ".").compactMap { Int($0) }
        guard parts.count >= 2 else { return nil }
        major = parts[0]
        minor = parts[1]
        patch = parts.count > 2 ? parts[2] : 0
    }

    static func < (lhs: SemVer, rhs: SemVer) -> Bool {
        if lhs.major != rhs.major { return lhs.major < rhs.major }
        if lhs.minor != rhs.minor { return lhs.minor < rhs.minor }
        return lhs.patch < rhs.patch
    }

    var string: String { "\(major).\(minor).\(patch)" }
}
```

---

## Task 3: Create UpdateError enum

**File:** `Sources/Update/UpdateError.swift` (new)

```swift
enum UpdateError: Error, LocalizedError {
    case networkError(underlying: Error)
    case extractionFailed
    case signatureInvalid
    case noMatchingAsset
    case invalidAppPath
    case versionParseError(String)
    case rateLimited(retryAfter: Date)

    var errorDescription: String? {
        switch self {
        case .networkError(let e): return "网络错误: \(e.localizedDescription)"
        case .extractionFailed: return "解压失败"
        case .signatureInvalid: return "签名验证失败"
        case .noMatchingAsset: return "未找到匹配的安装包"
        case .invalidAppPath: return "应用路径无效"
        case .versionParseError(let v): return "版本号解析失败: \(v)"
        case .rateLimited: return "请求过于频繁，稍后再试"
        }
    }
}
```

---

## Task 4: Create UpdateChecker

**File:** `Sources/Update/UpdateChecker.swift` (new)

Key implementation points:

```swift
struct ReleaseInfo {
    let version: String
    let downloadURL: URL
    let releaseNotes: String
    let publishedAt: Date
}

protocol UpdateCheckerDelegate: AnyObject {
    func updateChecker(_ checker: UpdateChecker, didFindRelease release: ReleaseInfo)
}

class UpdateChecker {
    static let repoOwner = "user"  // TODO: set to real owner
    static let repoName = "pmux"

    #if arch(arm64)
    static let assetSuffix = "arm64.zip"
    #else
    static let assetSuffix = "x86_64.zip"
    #endif

    weak var delegate: UpdateCheckerDelegate?
    private var timer: Timer?
    private var rateLimitResetDate: Date?

    let currentVersion: String  // Bundle.main MARKETING_VERSION

    init() {
        currentVersion = Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "0.0.0"
    }

    func startPolling(intervalHours: Int) {
        let interval = TimeInterval(intervalHours * 3600)
        // Check immediately, then repeat
        Task { await checkAndNotify(skippedVersion: nil) }
        timer = Timer.scheduledTimer(withTimeInterval: interval, repeats: true) { [weak self] _ in
            Task { await self?.checkAndNotify(skippedVersion: nil) }
        }
    }

    func stopPolling() { timer?.invalidate(); timer = nil }

    func checkNow() async throws -> ReleaseInfo? {
        // GET https://api.github.com/repos/{owner}/{repo}/releases/latest
        // Check rate limit (X-RateLimit-Remaining header)
        // Parse JSON: tag_name, body, published_at, assets[]
        // Find asset matching assetSuffix
        // Compare SemVer(tag) > SemVer(currentVersion)
        // Return ReleaseInfo or nil
    }

    private func checkAndNotify(skippedVersion: String?) async {
        guard let release = try? await checkNow() else { return }
        if let skipped = skippedVersion, release.version == skipped { return }
        await MainActor.run { delegate?.updateChecker(self, didFindRelease: release) }
    }
}
```

**GitHub API JSON fields to parse:**
- `tag_name` → strip "v" prefix → version string
- `body` → release notes
- `published_at` → ISO 8601 date
- `assets[].name` → match against `assetSuffix`
- `assets[].browser_download_url` → download URL

**Rate limit handling:** Check `X-RateLimit-Remaining` response header. If 0, store `X-RateLimit-Reset` as `rateLimitResetDate` and skip checks until that time.

---

## Task 5: Create UpdateManager

**File:** `Sources/Update/UpdateManager.swift` (new)

```swift
protocol UpdateManagerDelegate: AnyObject {
    func updateManager(_ manager: UpdateManager, didChangeState state: UpdateManager.State)
}

class UpdateManager: NSObject {
    enum State {
        case idle
        case downloading(progress: Double)
        case extracting
        case verifying
        case readyToInstall(appPath: URL)
        case failed(UpdateError)
    }

    weak var delegate: UpdateManagerDelegate?
    private(set) var state: State = .idle
    private var downloadTask: URLSessionDownloadTask?
}
```

**download(release:) implementation:**
1. Create `URLSessionDownloadTask` with delegate for progress tracking
2. On completion, move zip to `FileManager.default.temporaryDirectory`
3. Extract: `Process("/usr/bin/ditto", ["-xk", zipPath, extractDir])`
4. Find `.app` in extractDir
5. Verify signature: `Process("/usr/bin/codesign", ["--verify", "--deep", "--strict", appPath])`
6. If verification fails → `.failed(.signatureInvalid)`
7. If success → `.readyToInstall(appPath:)`

**cancelDownload():**
- `downloadTask?.cancel()`
- State → `.idle`

**installAndRestart():**
1. `let currentApp = Bundle.main.bundlePath`
2. Validate: must end in `.app`
3. Create unique helper script in `FileManager.default.temporaryDirectory`:
   ```bash
   #!/bin/bash
   PID=$1
   while kill -0 "$PID" 2>/dev/null; do sleep 0.5; done
   mv "$CURRENT_APP" "$CURRENT_APP.bak"
   mv "$NEW_APP" "$CURRENT_APP"
   xattr -d com.apple.quarantine "$CURRENT_APP" 2>/dev/null
   open "$CURRENT_APP"
   rm -rf "$CURRENT_APP.bak"
   rm -f "$0"
   ```
4. `Process.launchedProcess(launchPath: "/bin/bash", arguments: [scriptPath, "\(ProcessInfo.processInfo.processIdentifier)"])`
5. `NSApp.terminate(nil)`

Use `URLSessionDownloadDelegate` for progress:
```swift
extension UpdateManager: URLSessionDownloadDelegate {
    func urlSession(_ session: URLSession, downloadTask: URLSessionDownloadTask,
                    didWriteData bytesWritten: Int64, totalBytesWritten: Int64,
                    totalBytesExpectedToWrite: Int64) {
        let progress = Double(totalBytesWritten) / Double(totalBytesExpectedToWrite)
        DispatchQueue.main.async { self.setState(.downloading(progress: progress)) }
    }
}
```

---

## Task 6: Create UpdateBanner

**File:** `Sources/UI/Update/UpdateBanner.swift` (new)

32px-high NSView with:
- `statusLabel` (NSTextField) — "v2.1.0 可用" / "下载中..." / etc.
- `progressBar` (NSProgressIndicator) — linear, determinate, hidden until downloading
- `actionButton` (NSButton) — "更新" / "立即重启" / "重试" depending on state
- `skipButton` (NSButton) — "跳过", only visible when new version found

```swift
protocol UpdateBannerDelegate: AnyObject {
    func updateBannerDidClickInstall(_ banner: UpdateBanner)
    func updateBannerDidClickSkip(_ banner: UpdateBanner)
    func updateBannerDidClickRestart(_ banner: UpdateBanner)
    func updateBannerDidClickRetry(_ banner: UpdateBanner)
}

class UpdateBanner: NSView {
    weak var delegate: UpdateBannerDelegate?

    func update(state: UpdateManager.State, version: String)
    func showNewVersion(_ version: String)
}
```

Accessibility identifiers per spec: `update.banner`, `update.installButton`, `update.skipButton`, `update.restartButton`, `update.statusLabel`. All with `setAccessibilityElement(true)` + appropriate role.

---

## Task 7: Integrate into MainWindowController

**File:** `Sources/App/MainWindowController.swift`

1. Add properties:
   ```swift
   private let updateChecker = UpdateChecker()
   private let updateManager = UpdateManager()
   private let updateBanner = UpdateBanner()
   ```

2. In `init()`, after `setupLayout()`:
   ```swift
   if config.autoUpdate.enabled {
       updateChecker.delegate = self
       updateManager.delegate = self
       updateBanner.delegate = self
       updateChecker.startPolling(intervalHours: config.autoUpdate.checkIntervalHours)
   }
   ```

3. Add `updateBanner` to window layout — above tabBar, hidden by default. When shown, shift tabBar + contentContainer down 32px.

4. Add "Check for Updates..." menu item in app menu (~line 64, after Settings):
   ```swift
   let checkUpdateItem = NSMenuItem(title: "Check for Updates...",
       action: #selector(checkForUpdates), keyEquivalent: "u")
   checkUpdateItem.keyEquivalentModifierMask = .command
   appMenu.addItem(checkUpdateItem)
   ```

5. Implement delegate methods:
   - `UpdateCheckerDelegate` → show banner with new version
   - `UpdateManagerDelegate` → update banner state
   - `UpdateBannerDelegate` → trigger download/skip/restart

6. Skip button → save `config.autoUpdate.skippedVersion = version; config.save()`

---

## Task 8: Write unit tests

**File:** `Tests/SemVerTests.swift` (new)

- `testParseValidVersion` — "2.1.0", "v2.1.0", "1.0"
- `testParseMalformed` — "abc", "", "1"
- `testComparisonMajor` — 3.0.0 > 2.9.9
- `testComparisonMinor` — 2.1.0 > 2.0.99
- `testComparisonPatch` — 2.0.1 > 2.0.0
- `testEqual` — 2.0.0 == 2.0.0
- `testVPrefix` — "v2.1.0" parses same as "2.1.0"

**File:** `Tests/UpdateCheckerTests.swift` (new)

- `testParseGitHubReleaseJSON` — mock JSON → ReleaseInfo
- `testArchitectureSelection` — arm64/x86_64 asset matching
- `testNoMatchingAsset` — returns nil
- `testVersionSkipped` — skipped version not reported
- `testOlderVersion` — remote <= current returns nil

---

## Task 9: Compile and run tests

1. `xcodegen generate`
2. `xcodebuild test -scheme pmux -destination 'platform=macOS' -only-testing:pmuxTests`
3. Fix any issues

---

## Execution Order

Task 1 (config) → Task 2-3 (SemVer + Error, parallel) → Task 4 (UpdateChecker) → Task 5 (UpdateManager) → Task 6 (UpdateBanner) → Task 7 (integration) → Task 8 (tests) → Task 9 (verify)
