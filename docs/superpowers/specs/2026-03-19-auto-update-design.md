# Auto-Update Design

## Overview

Add automatic update checking and one-click install to amux-swift. Uses GitHub Releases API to detect new versions, downloads the correct architecture-specific zip, and replaces the running app via a helper script.

## Decision

- **Source:** GitHub Releases API (`/repos/{owner}/{repo}/releases/latest`)
- **Distribution:** Architecture-specific zip files (`amux-macos-arm64.zip`, `amux-macos-x86_64.zip`)
- **Install method:** Helper shell script (app exits → script replaces → launches new version)
- **No external dependencies** — pure URLSession + Foundation + Process

## Architecture

```
UpdateChecker (polling)  →  GitHub API  →  ReleaseInfo
       │
       ▼
UpdateManager (download + install)
       │
       ▼
UpdateBanner (top-of-window UI)
```

Three modules:
- **UpdateChecker** — periodic GitHub API polling, version comparison, architecture detection
- **UpdateManager** — download zip, extract, replace app, restart
- **UpdateBanner** — NSView banner in MainWindow showing update status

## Module Design

### UpdateChecker

**File:** `Sources/Update/UpdateChecker.swift`

```swift
struct ReleaseInfo {
    let version: String        // "2.1.0" (tag "v2.1.0" stripped)
    let downloadURL: URL       // architecture-specific .zip asset URL
    let releaseNotes: String   // release body markdown
    let publishedAt: Date
}

class UpdateChecker {
    let repoOwner: String
    let repoName: String
    let currentVersion: String  // from Bundle.main marketing version

    /// Start periodic checking (interval from Config.autoUpdate.checkIntervalHours)
    func startPolling(intervalHours: Int)
    func stopPolling()

    /// Manual check (e.g. from menu item)
    func checkNow() async throws -> ReleaseInfo?
}
```

**GitHub API call:**
- `GET https://api.github.com/repos/{owner}/{repo}/releases/latest`
- No authentication needed (public repo, 60 req/hour rate limit)
- Parse `tag_name`, `body`, `published_at`, `assets[]`

**Architecture detection:**
```swift
#if arch(arm64)
static let assetSuffix = "arm64.zip"
#else
static let assetSuffix = "x86_64.zip"
#endif
```

**Asset naming convention:**
- `amux-macos-arm64.zip` — Apple Silicon
- `amux-macos-x86_64.zip` — Intel

**Version comparison:**
- Semantic versioning: compare major.minor.patch as integers
- `remote > current` → return ReleaseInfo
- `remote <= current` → return nil

### UpdateManager

**File:** `Sources/Update/UpdateManager.swift`

```swift
class UpdateManager {
    enum State {
        case idle
        case downloading(progress: Double)  // 0.0 ~ 1.0
        case extracting
        case readyToInstall(appPath: URL)
        case failed(Error)
    }

    weak var delegate: UpdateManagerDelegate?
    private(set) var state: State = .idle

    func download(release: ReleaseInfo) async
    func cancelDownload()
    func installAndRestart()
}

enum UpdateError: Error, LocalizedError {
    case networkError(underlying: Error)
    case extractionFailed
    case signatureInvalid
    case noMatchingAsset
    case invalidAppPath
    case versionParseError(String)

    var errorDescription: String? {
        switch self {
        case .networkError(let e): return "网络错误: \(e.localizedDescription)"
        case .extractionFailed: return "解压失败"
        case .signatureInvalid: return "签名验证失败"
        case .noMatchingAsset: return "未找到匹配的安装包"
        case .invalidAppPath: return "应用路径无效"
        case .versionParseError(let v): return "版本号解析失败: \(v)"
        }
    }
}

protocol UpdateManagerDelegate: AnyObject {
    func updateManager(_ manager: UpdateManager, didChangeState state: UpdateManager.State)
}
```

**Download flow:**
1. `URLSession.shared.download(from: release.downloadURL)` with progress delegate
2. Extract with `/usr/bin/ditto -xk <zipPath> <tempDir>`
3. Verify extracted `.app` exists and is executable
4. Verify code signature: `codesign --verify --deep --strict <extractedApp>`
5. State → `.readyToInstall(appPath:)`

**Install flow (`installAndRestart`):**
1. Determine current app path: `Bundle.main.bundlePath`
2. Validate path ends in `.app` and is within `/Applications` or user home
3. Write helper script to a unique temp file via `FileManager.default.temporaryDirectory` (user-private `$TMPDIR`, not `/tmp`):
   ```bash
   #!/bin/bash
   PID=$1
   # Wait for the app process to fully exit
   while kill -0 "$PID" 2>/dev/null; do sleep 0.5; done
   mv "<currentAppPath>" "<currentAppPath>.bak"
   mv "<newAppPath>" "<currentAppPath>"
   xattr -d com.apple.quarantine "<currentAppPath>" 2>/dev/null
   open "<currentAppPath>"
   rm -rf "<currentAppPath>.bak"
   rm -f "$0"  # self-delete
   ```
4. Launch helper via `Process` (detached), passing current PID as argument
5. `NSApp.terminate(nil)`

**Security:**
- Download over HTTPS (GitHub CDN)
- Post-extraction code signature verification via `codesign --verify --deep --strict <newApp>`
- Extract with system `ditto`
- Remove quarantine attribute with `xattr -d` after signature verification
- Helper script written to user-private `$TMPDIR` (not world-writable `/tmp`)
- Previous version kept as `.bak` for rollback until new version launches successfully

### UpdateBanner

**File:** `Sources/UI/Update/UpdateBanner.swift`

A 32px-high NSView shown at the top of MainWindow (above TabBar), only visible when there's an update.

**Layout:**
```
┌──────────────────────────────────────────────────────┐
│ ● 新版本 v2.1.0 可用   [更新]  [跳过]    ██████░░ 60% │
└──────────────────────────────────────────────────────┘
```

**State-driven display:**

| State | Message | Actions |
|-------|---------|---------|
| New version found | `"v2.1.0 可用"` | [更新] [跳过] |
| Downloading | `"下载中..."` + progress bar | — |
| Extracting | `"正在准备安装..."` | — |
| Ready to install | `"准备就绪"` | [立即重启] |
| Failed | `"更新失败: {error}"` | [重试] |

**Accessibility identifiers:**
- Banner container: `update.banner` (role: `.group`)
- Update button: `update.installButton`
- Skip button: `update.skipButton`
- Restart button: `update.restartButton`
- Progress text: `update.statusLabel`

## Integration

### MainWindowController

```swift
// On launch
if config.autoUpdate.enabled {
    updateChecker.startPolling(intervalHours: config.autoUpdate.checkIntervalHours)
}

// UpdateChecker finds new version → show UpdateBanner
// [更新] button → updateManager.download(release)
// [立即重启] → updateManager.installAndRestart()
// [跳过] → save skippedVersion to config, hide banner
```

### Menu Item

Add "Check for Updates..." menu item under the app menu. Triggers `updateChecker.checkNow()` manually.

### Settings

Existing `Config.autoUpdate` fields (`enabled`, `checkIntervalHours`) already in place. No new UI needed — Settings General tab can display these if desired in the future.

## Config

Existing fields in `Config.swift`:

```swift
struct UpdateConfig: Codable {
    var enabled: Bool = true
    var checkIntervalHours: Int = 6
    var skippedVersion: String? = nil  // version user chose to skip
}
```

Add `skippedVersion` to persist the user's "skip" choice. When a user clicks [跳过], the current release version is saved. UpdateChecker skips that version on subsequent checks until a newer version appears.

**GitHub repo coordinates** are compile-time constants in `UpdateChecker`, not config fields:

```swift
class UpdateChecker {
    static let repoOwner = "user"
    static let repoName = "amux"
}
```

### Rate Limit Handling

When GitHub API returns 403 with `X-RateLimit-Remaining: 0`, the checker backs off silently until `X-RateLimit-Reset` timestamp. No error is surfaced to the user.

## Testing

### Unit Tests

- **VersionComparison tests** — `"2.0.0" < "2.0.1"`, `"2.1.0" > "2.0.99"`, `"3.0.0" > "2.9.9"`, equal versions, malformed versions
- **ReleaseInfo parsing tests** — mock GitHub API JSON, verify tag parsing, asset URL extraction, architecture matching
- **Architecture selection tests** — arm64 selects `amux-macos-arm64.zip`, x86_64 selects `amux-macos-x86_64.zip`
- **Asset matching tests** — no matching asset returns nil, multiple assets selects correct one

### UI Tests

- **testCheckForUpdatesMenuItem** — menu item exists and is clickable
- **testUpdateBannerAppears** — when update available, banner appears (would need mock server or test flag)

## File Structure

```
Sources/Update/
├── UpdateChecker.swift      # GitHub API polling, version comparison
├── UpdateManager.swift      # Download, extract, install, restart
└── UpdateBanner.swift       # Top-of-window update notification UI
```
