# tmux → zmx Migration Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace tmux with zmx as the terminal session persistence backend, enabling OSC 133 passthrough and eliminating double VT parsing overhead.

**Architecture:** zmx is a per-session daemon that uses libghostty-vt for terminal state persistence. It passes all escape sequences through without filtering (unlike tmux which re-encodes them). Phase 1 uses shell-out to `zmx` CLI commands (same pattern as current tmux usage). Phase 2 (future) can speak the zmx Unix socket protocol directly for real-time output streaming.

**Tech Stack:** Swift 5.10, zmx CLI (installed via `brew install neurosnap/tap/zmx`), existing GhosttyKit integration unchanged.

**Prerequisite:** `zmx` must be installed. Run `brew install neurosnap/tap/zmx` before starting.

---

## File Map

| File | Action | Responsibility |
|------|--------|---------------|
| `Sources/Terminal/TerminalSurface.swift` | Modify | Replace tmux commands with zmx equivalents; remove resize workarounds |
| `Sources/Core/TmuxChannel.swift` | Rename → `ZmxChannel.swift` | Replace `capture-pane`/`send-keys` with zmx equivalents |
| `Sources/Core/AgentChannel.swift` | Modify | Rename `AgentChannelType.tmux` → `.zmx` |
| `Sources/Core/HooksChannel.swift` | Modify | Update to use ZmxChannel instead of TmuxChannel; update doc comments |
| `Sources/Core/AgentHead.swift` | Modify | Update channel creation from TmuxChannel → ZmxChannel; rename parameter |
| `Sources/App/MainWindowController.swift` | Modify | Replace tmux session naming/killing with zmx equivalents |
| `Sources/Core/Config.swift` | Modify | Change default backend, add tmux→zmx auto-migration |
| `Sources/UI/Settings/SettingsViewController.swift` | Modify | Update backend dropdown options and fallback default |
| `Tests/ConfigTests.swift` | Modify | Update expected default backend value; add migration test |
| `Tests/AgentHeadTests.swift` | Modify | Update channel type references if any |
| `IntegrationTests/TerminalFullscreenVisualTest.swift` | Modify | Replace all tmux helpers with zmx equivalents |

---

## Summary of tmux → zmx Command Mapping

| Current (tmux) | New (zmx) | Notes |
|---|---|---|
| `tmux has-session -t <name>` | Check if socket file exists at `$TMPDIR/zmx-$UID/<name>` | No process spawn needed |
| `tmux new-session -s <name> \; set-option status off` | `zmx attach <name>` | zmx auto-creates if not exists |
| `tmux attach-session -t <name> \; set-option status off` | `zmx attach <name>` | Same command for create and attach |
| `tmux send-keys -t <name> <cmd> Enter` | `zmx run <name> <cmd>` | Sends command without attaching |
| `tmux capture-pane -t <name> -p -S <N> -E -1` | `zmx history <name> --vt` | Returns terminal history |
| `tmux resize-window -t <name> -x <cols> -y <rows>` | Not needed | zmx auto-syncs PTY size via SIGWINCH |
| `tmux refresh-client -t <name> -S` | Not needed | No client refresh needed |
| `tmux kill-session -t <name>` | `zmx kill <name>` | Kills daemon + shell |

**Key simplification:** zmx handles PTY resize automatically via the client connection. The entire `refreshTmuxLayout()` method and `refreshTmuxClient()` become no-ops and can be removed.

---

### Task 1: Rename TmuxChannel → ZmxChannel

**Files:**
- Rename: `Sources/Core/TmuxChannel.swift` → `Sources/Core/ZmxChannel.swift`
- Modify: `Sources/Core/AgentChannel.swift`

- [ ] **Step 1: Create ZmxChannel.swift with zmx commands**

Replace `TmuxChannel` class with `ZmxChannel`. The key changes:
- `sendCommand` uses `zmx run <session> <command>` instead of `tmux send-keys`
- `readOutput` uses `zmx history <session>` instead of `tmux capture-pane`
- Rename internal helper methods from `runTmux` to `runZmx`

```swift
// Sources/Core/ZmxChannel.swift
import Foundation

/// Fallback channel: communicates with any agent via zmx commands.
/// Works with any CLI tool — no agent-side support needed.
class ZmxChannel: AgentChannel {
    let channelType: AgentChannelType = .zmx
    let sessionName: String

    init(sessionName: String) {
        self.sessionName = sessionName
    }

    /// Send a text command via zmx run (injects into session without attaching).
    /// zmx run passes the command string directly to the PTY (no shell interpretation),
    /// so no additional escaping is needed beyond what Process argument passing provides.
    func sendCommand(_ command: String) {
        let args = ["zmx", "run", sessionName, command]
        runZmx(args)
    }

    /// Read terminal history via zmx history.
    /// Note: zmx history returns the full terminal scrollback; the `lines` parameter
    /// is accepted for protocol conformance but zmx does not support line-count limiting.
    /// Callers that need only the last N lines should truncate the result.
    func readOutput(lines: Int = 50) -> String? {
        let args = ["zmx", "history", sessionName]
        guard let output = runZmxWithOutput(args) else { return nil }
        if lines > 0 {
            let allLines = output.components(separatedBy: "\n")
            if allLines.count > lines {
                return allLines.suffix(lines).joined(separator: "\n")
            }
        }
        return output
    }

    // MARK: - Private

    private func runZmx(_ args: [String]) {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        process.arguments = args
        process.standardOutput = FileHandle.nullDevice
        process.standardError = FileHandle.nullDevice
        do {
            try process.run()
            process.waitUntilExit()
        } catch {
            NSLog("[ZmxChannel] Failed to run: \(args.joined(separator: " ")): \(error)")
        }
    }

    private func runZmxWithOutput(_ args: [String]) -> String? {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        process.arguments = args
        let pipe = Pipe()
        process.standardOutput = pipe
        process.standardError = FileHandle.nullDevice
        do {
            try process.run()
            process.waitUntilExit()
            let data = pipe.fileHandleForReading.readDataToEndOfFile()
            let output = String(data: data, encoding: .utf8)
            return output?.isEmpty == true ? nil : output
        } catch {
            NSLog("[ZmxChannel] Failed to read: \(args.joined(separator: " ")): \(error)")
            return nil
        }
    }
}
```

- [ ] **Step 2: Update AgentChannelType enum**

In `Sources/Core/AgentChannel.swift`, rename the `.tmux` case:

```swift
enum AgentChannelType: String {
    case zmx        // Fallback: read/write via zmx commands
    case hooks      // Claude Code hooks: structured events via webhook + zmx for input
}
```

- [ ] **Step 3: Delete old TmuxChannel.swift**

```bash
git rm Sources/Core/TmuxChannel.swift
```

- [ ] **Step 4: Build to verify compilation**

```bash
xcodegen generate && xcodebuild -project pmux.xcodeproj -scheme pmux -configuration Debug build 2>&1 | tail -5
```

Expected: Build succeeds (may fail due to remaining tmux references — that's fine, fixed in subsequent tasks).

- [ ] **Step 5: Commit**

```bash
git add Sources/Core/ZmxChannel.swift Sources/Core/AgentChannel.swift
git rm Sources/Core/TmuxChannel.swift
git commit -m "refactor: rename TmuxChannel to ZmxChannel with zmx commands"
```

---

### Task 2: Update HooksChannel to use ZmxChannel

**Files:**
- Modify: `Sources/Core/HooksChannel.swift`

- [ ] **Step 1: Replace TmuxChannel reference with ZmxChannel and update doc comments**

Change line 11 from `private let tmux: TmuxChannel` to `private let zmx: ZmxChannel`, update the initializer, rename all `tmux.` calls to `zmx.`, and update all doc comments:

```swift
/// Communication channel for Claude Code via Hooks.
/// Receives structured events through the existing WebhookServer,
/// sends commands via zmx (same as ZmxChannel).
class HooksChannel: AgentChannel {
    let channelType: AgentChannelType = .hooks
    let supportsStructuredEvents = true

    private let zmx: ZmxChannel
    private let lock = NSLock()

    private(set) var events: [HookEvent] = []

    init(sessionName: String) {
        self.zmx = ZmxChannel(sessionName: sessionName)
    }

    /// Send command via zmx (hooks don't provide an input channel)
    func sendCommand(_ command: String) {
        zmx.sendCommand(command)
    }

    /// Read output via zmx (hooks provide events, not raw output)
    func readOutput(lines: Int) -> String? {
        zmx.readOutput(lines: lines)
    }
    // ... rest unchanged
}
```

- [ ] **Step 2: Build to verify**

```bash
xcodebuild -project pmux.xcodeproj -scheme pmux -configuration Debug build 2>&1 | tail -5
```

- [ ] **Step 3: Commit**

```bash
git add Sources/Core/HooksChannel.swift
git commit -m "refactor: update HooksChannel to use ZmxChannel"
```

---

### Task 3: Update AgentHead channel creation

**Files:**
- Modify: `Sources/Core/AgentHead.swift`

**Note:** Do NOT rename the `tmuxSessionName` parameter yet — that would break the call site
in MainWindowController until Task 5 updates it. Keep the parameter name as-is for now;
Task 5 will rename both the parameter and call site together.

- [ ] **Step 1: Replace TmuxChannel with ZmxChannel in register()**

In the `register` method (line 31-35), change only the channel constructor:

```swift
// Before:
if let sessionName = tmuxSessionName {
    channel = TmuxChannel(sessionName: sessionName)

// After:
if let sessionName = tmuxSessionName {
    channel = ZmxChannel(sessionName: sessionName)
```

- [ ] **Step 2: Update channel upgrade in updateAgentType()**

Change the TmuxChannel type check (line 128):

```swift
// Before:
if type == .claudeCode, let tmux = channels[worktreePath] as? TmuxChannel {
    let hooks = HooksChannel(sessionName: tmux.sessionName)

// After:
if type == .claudeCode, let zmx = channels[worktreePath] as? ZmxChannel {
    let hooks = HooksChannel(sessionName: zmx.sessionName)
```

- [ ] **Step 3: Update comment on line 31**

```swift
// Before:
// Create a default TmuxChannel if we have a session name

// After:
// Create a default ZmxChannel if we have a session name
```

- [ ] **Step 4: Build to verify**

```bash
xcodebuild -project pmux.xcodeproj -scheme pmux -configuration Debug build 2>&1 | tail -5
```

- [ ] **Step 5: Commit**

```bash
git add Sources/Core/AgentHead.swift
git commit -m "refactor: update AgentHead to use ZmxChannel"
```

---

### Task 4: Update TerminalSurface — replace tmux with zmx

**Files:**
- Modify: `Sources/Terminal/TerminalSurface.swift`

This is the largest change. zmx simplifies things significantly because:
1. `zmx attach <name>` handles both create and reattach (no need to check session existence first)
2. zmx auto-syncs PTY size — no need for `refreshTmuxLayout()` or `refreshTmuxClient()`

- [ ] **Step 1: Simplify the create() method**

Replace the async `tmuxSessionExistsAsync` check with a direct `zmx attach` command.
**Important:** Preserve the existing return-value contract — `create()` returns `true` when
a session name is provided (surface creation proceeds on main queue), maintaining the same
deferred-creation semantics callers rely on.

```swift
func create(in container: NSView, workingDirectory: String? = nil, sessionName: String? = nil) -> Bool {
    guard let app = GhosttyBridge.shared.app else {
        NSLog("GhosttyBridge not initialized")
        return false
    }

    if let sessionName {
        self.sessionName = sessionName
        // zmx attach handles both create and reattach in one command.
        // No need to check session existence first (unlike tmux).
        let zmxCommand = "zmx attach \(sessionName)"
        self._createWithCommand(app: app, container: container, workingDirectory: workingDirectory, command: zmxCommand)
        return true  // Surface creation is deferred (same contract as before)
    }

    _createWithCommand(app: app, container: container, workingDirectory: workingDirectory, command: nil)
    return surface != nil
}
```

Key changes:
- No more async `tmuxSessionExistsAsync` — zmx handles create-or-attach in one command
- No more `set-option status off` — zmx has no status bar
- Return value semantics preserved: returns `true` for session-based creation (deferred)

- [ ] **Step 2: Remove tmuxSessionExistsAsync()**

Delete the entire `tmuxSessionExistsAsync` method (lines 107-127).

- [ ] **Step 3: Remove refreshTmuxLayout() and refreshTmuxClient()**

Delete both methods (lines 166-211). zmx auto-syncs PTY size via the client connection — when Ghostty resizes the surface, the PTY size propagates through zmx automatically via SIGWINCH.

- [ ] **Step 4: Remove refreshTmuxLayout() call from reparent()**

In the `reparent()` method, remove the third deferred pass (lines 159-163):

```swift
// Before (3 deferred passes):
DispatchQueue.main.async { [weak self] in
    guard let self, let view = self.view, let surface = self.surface else { return }
    self.syncContentScale()
    self.syncSize()
    ghostty_surface_set_focus(surface, true)
    view.needsDisplay = true
    // Third pass: read the grid size AFTER Ghostty has processed the resize
    DispatchQueue.main.async { [weak self] in
        self?.refreshTmuxLayout()
    }
}

// After (2 deferred passes):
DispatchQueue.main.async { [weak self] in
    guard let self, let view = self.view, let surface = self.surface else { return }
    self.syncContentScale()
    self.syncSize()
    ghostty_surface_set_focus(surface, true)
    view.needsDisplay = true
}
```

- [ ] **Step 5: Remove refreshTmuxLayout() call from GhosttyNSView.syncSurfaceSize()**

In `syncSurfaceSize()`, remove line 400:

```swift
// Before:
ghostty_surface_refresh(surface)
needsDisplay = true

// Resize tmux to match the new terminal grid dimensions
terminalSurface?.refreshTmuxLayout()

// After:
ghostty_surface_refresh(surface)
needsDisplay = true
```

- [ ] **Step 6: Update comments**

Remove tmux references from doc comments:
- Line 11: `/// tmux session name` → `/// zmx session name (nil = no session persistence)`
- Line 14-16: Update method doc to reference zmx

- [ ] **Step 7: Build to verify**

```bash
xcodebuild -project pmux.xcodeproj -scheme pmux -configuration Debug build 2>&1 | tail -5
```

- [ ] **Step 8: Commit**

```bash
git add Sources/Terminal/TerminalSurface.swift
git commit -m "refactor: replace tmux with zmx in TerminalSurface, remove resize workarounds"
```

---

### Task 5: Update MainWindowController and AgentHead parameter rename

**Files:**
- Modify: `Sources/App/MainWindowController.swift`
- Modify: `Sources/Core/AgentHead.swift` (parameter rename only)

- [ ] **Step 1: Rename the AgentHead.register() parameter from `tmuxSessionName` to `sessionName`**

In `Sources/Core/AgentHead.swift`, update the method signature:

```swift
// Before:
func register(worktreePath: String, branch: String, project: String,
              surface: TerminalSurface, startedAt: Date?,
              tmuxSessionName: String? = nil) {
    // ...
    if let sessionName = tmuxSessionName {

// After:
func register(worktreePath: String, branch: String, project: String,
              surface: TerminalSurface, startedAt: Date?,
              sessionName: String? = nil) {
    // ...
    if let sessionName {
```

- [ ] **Step 2: Rename tmuxSessionName() → zmxSessionName() in MainWindowController**

```swift
// Before:
private static func tmuxSessionName(for path: String) -> String {
    let url = URL(fileURLWithPath: path)
    let parent = url.deletingLastPathComponent().lastPathComponent
    let name = url.lastPathComponent
    let sessionName = "pmux-\(parent)-\(name)"
        .replacingOccurrences(of: ".", with: "_")
        .replacingOccurrences(of: ":", with: "_")
    return sessionName
}

// After:
private static func zmxSessionName(for path: String) -> String {
    let url = URL(fileURLWithPath: path)
    let parent = url.deletingLastPathComponent().lastPathComponent
    let name = url.lastPathComponent
    let sessionName = "pmux-\(parent)-\(name)"
        .replacingOccurrences(of: ".", with: "_")
        .replacingOccurrences(of: ":", with: "_")
    return sessionName
}
```

- [ ] **Step 3: Update backend checks from "tmux" to "zmx"**

Three locations need updating:

```swift
// Line 779 — worktree registration (note: uses renamed `sessionName:` parameter from Step 1):
let sessionName = self.config.backend == "zmx" ? Self.zmxSessionName(for: info.path) : nil
AgentHead.shared.register(worktreePath: info.path, branch: info.branch, project: proj, surface: surface, startedAt: started, sessionName: sessionName)

// Line 814 — surface creation:
if config.backend == "zmx" {
    surface.sessionName = Self.zmxSessionName(for: info.path)
}

// Line 947 — repo close cleanup:
if config.backend == "zmx" {
    let sessionName = Self.zmxSessionName(for: worktree.path)
    killZmxSession(sessionName)
}
```

- [ ] **Step 4: Replace killTmuxSession with killZmxSession**

```swift
// Before:
private func killTmuxSession(_ name: String) {
    let process = Process()
    process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
    process.arguments = ["tmux", "kill-session", "-t", name]
    process.standardOutput = Pipe()
    process.standardError = Pipe()
    try? process.run()
}

// After:
private func killZmxSession(_ name: String) {
    let process = Process()
    process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
    process.arguments = ["zmx", "kill", name]
    process.standardOutput = Pipe()
    process.standardError = Pipe()
    try? process.run()
}
```

- [ ] **Step 5: Update UI string**

Line 686: change `"kill tmux sessions"` → `"kill terminal sessions"` in the close confirmation dialog.

- [ ] **Step 6: Build to verify**

```bash
xcodebuild -project pmux.xcodeproj -scheme pmux -configuration Debug build 2>&1 | tail -5
```

- [ ] **Step 7: Commit**

```bash
git add Sources/App/MainWindowController.swift Sources/Core/AgentHead.swift
git commit -m "refactor: replace tmux with zmx in MainWindowController, rename session parameter"
```

---

### Task 6: Update Config, Settings UI, and backward compatibility

**Files:**
- Modify: `Sources/Core/Config.swift`
- Modify: `Sources/UI/Settings/SettingsViewController.swift`
- Modify: `Tests/ConfigTests.swift`

Config default change and migration must be atomic to avoid a window where
existing `"backend": "tmux"` configs break.

- [ ] **Step 1: Change default backend and add migration in Config.swift**

In `init()` (line 35):
```swift
backend = "zmx"
```

In `init(from decoder:)` (line 51), combine default change with auto-migration:
```swift
var rawBackend = try container.decodeIfPresent(String.self, forKey: .backend) ?? "zmx"
if rawBackend == "tmux" {
    rawBackend = "zmx"  // Auto-migrate from tmux to zmx
}
backend = rawBackend
```

- [ ] **Step 2: Update Settings dropdown and fallback**

In `SettingsViewController.swift`:

Line 142 — dropdown items:
```swift
// Before:
backendPopup.addItems(withTitles: ["tmux", "local"])
// After:
backendPopup.addItems(withTitles: ["zmx", "local"])
```

Line 273 — save fallback:
```swift
// Before:
config.backend = backendPopup.titleOfSelectedItem ?? "tmux"
// After:
config.backend = backendPopup.titleOfSelectedItem ?? "zmx"
```

- [ ] **Step 3: Update tests**

In `Tests/ConfigTests.swift`, update all `"tmux"` assertions to `"zmx"`:

- Line 12: `XCTAssertEqual(config.backend, "zmx")`
- Line 45: `XCTAssertEqual(config.backend, "zmx")  // default`
- Line 53: `XCTAssertEqual(config.backend, "zmx")`
- Line 132: `XCTAssertEqual(config.backend, "zmx")`
- Line 211: `"backend": "zmx"`

Also add a migration test to verify old configs are handled:

```swift
func testConfigMigration_TmuxToZmx() throws {
    let json = """
    {
        "workspace_paths": ["/path/a"],
        "backend": "tmux"
    }
    """.data(using: .utf8)!
    let config = try JSONDecoder().decode(Config.self, from: json)
    XCTAssertEqual(config.backend, "zmx", "Old 'tmux' backend should auto-migrate to 'zmx'")
}
```

- [ ] **Step 4: Run tests**

```bash
xcodebuild -project pmux.xcodeproj -scheme pmuxTests -configuration Debug test 2>&1 | tail -20
```

Expected: All tests pass.

- [ ] **Step 5: Commit**

```bash
git add Sources/Core/Config.swift Sources/UI/Settings/SettingsViewController.swift Tests/ConfigTests.swift
git commit -m "refactor: change default backend to zmx with auto-migration from tmux"
```

---

### Task 7: Update IntegrationTests

**Files:**
- Modify: `IntegrationTests/TerminalFullscreenVisualTest.swift`

This file has extensive tmux usage — all helper methods call tmux CLI directly.
Since zmx doesn't have equivalent query commands for window dimensions,
replace with zmx-compatible approaches.

- [ ] **Step 1: Replace tmux helpers with zmx equivalents**

The key change: zmx doesn't expose `display-message` for querying window dimensions.
Instead, use `zmx list --short` to find sessions, and `zmx history` for content capture.
For window size queries, use `stty size` piped through `zmx run`.

```swift
private func listPmuxZmxSessions() -> [String] {
    let process = Process()
    process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
    process.arguments = ["zmx", "list", "--short"]
    let pipe = Pipe()
    process.standardOutput = pipe
    process.standardError = Pipe()
    do {
        try process.run()
        process.waitUntilExit()
    } catch {
        return []
    }
    let data = pipe.fileHandleForReading.readDataToEndOfFile()
    guard let output = String(data: data, encoding: .utf8) else { return [] }
    return output.components(separatedBy: "\n")
        .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
        .filter { $0.hasPrefix("pmux-") }
}

private func zmxCommand(_ args: [String]) -> String {
    let process = Process()
    process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
    process.arguments = ["zmx"] + args
    let pipe = Pipe()
    process.standardOutput = pipe
    process.standardError = Pipe()
    do {
        try process.run()
        process.waitUntilExit()
    } catch {
        return ""
    }
    let data = pipe.fileHandleForReading.readDataToEndOfFile()
    return String(data: data, encoding: .utf8) ?? ""
}

private func zmxCaptureHistory(session: String) -> String {
    return zmxCommand(["history", session])
}

private func zmxSendCommand(session: String, command: String) {
    _ = zmxCommand(["run", session, command])
}
```

- [ ] **Step 2: Update test methods to use new helpers**

Replace all `tmux`-prefixed calls:
- `listPmuxTmuxSessions()` → `listPmuxZmxSessions()`
- `tmuxWindowWidth()` / `tmuxWindowHeight()` → query via `zmx run <session> stty size` (returns `rows cols`)
- `tmuxSendKeys()` → `zmxSendCommand()`
- `tmuxCapturePane()` → `zmxCaptureHistory()`
- Update doc comments and test names (e.g., `testTmuxColumnsMatchContainerWidth` → `testColumnsMatchContainerWidth`)

- [ ] **Step 3: Build to verify**

```bash
xcodegen generate && xcodebuild -project pmux.xcodeproj -scheme pmux -configuration Debug build 2>&1 | tail -5
```

- [ ] **Step 4: Commit**

```bash
git add IntegrationTests/TerminalFullscreenVisualTest.swift
git commit -m "refactor: replace tmux with zmx in integration tests"
```

---

### Task 8: Full build and integration verification

- [ ] **Step 1: Clean build**

```bash
xcodegen generate && xcodebuild -project pmux.xcodeproj -scheme pmux clean build 2>&1 | tail -10
```

- [ ] **Step 2: Run all tests**

```bash
xcodebuild -project pmux.xcodeproj -scheme pmuxTests -configuration Debug test 2>&1 | tail -20
```

- [ ] **Step 3: Grep for any remaining tmux references**

```bash
grep -rn "tmux" Sources/ Tests/ IntegrationTests/ --include="*.swift" | grep -v "// migrat"
```

Expected: No remaining tmux references (except possibly the migration comment).

- [ ] **Step 4: Verify zmx is installed and working**

```bash
which zmx && zmx version
```

- [ ] **Step 5: Final commit if any cleanup needed**

---

### Task 9: Update CLAUDE.md documentation

**Files:**
- Modify: `CLAUDE.md`

- [ ] **Step 1: Update terminal persistence section**

Replace all tmux references in the "Key Patterns" section:

- `"Terminal persistence: tmux sessions named..."` → `"Terminal persistence: zmx sessions named pmux-<parent>-<name> are created per worktree..."`
- Remove `"Sessions are killed via tmux kill-session"` → `"Sessions are killed via zmx kill"`
- Update any tmux-related architecture notes

- [ ] **Step 2: Commit**

```bash
git add CLAUDE.md
git commit -m "docs: update CLAUDE.md to reflect zmx migration"
```

---

## Out of Scope (Future Work)

1. **Phase 2: Direct socket protocol** — Implement zmx's binary IPC protocol in Swift for real-time output streaming, eliminating the need for viewport polling entirely.
2. **COMMAND_FINISHED callback** — With zmx, OSC 133 sequences now reach Ghostty. Add `GHOSTTY_ACTION_COMMAND_FINISHED` handling in `GhosttyBridge.handleAction()` for event-driven status detection.
3. **Remove viewport polling** — Once socket protocol + COMMAND_FINISHED are in place, `StatusPublisher` can switch from 2s polling to event-driven updates.
