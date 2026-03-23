# pmux Codebase Refactoring Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix critical safety bugs, improve thread safety, decompose the MainWindowController god-object, and strengthen test coverage across the codebase.

**Architecture:** Six independent phases that can be executed sequentially. Phase 1 fixes critical safety issues (C interop, thread safety, cleanup). Phase 2 extracts reusable utilities. Phase 3-4 decompose MainWindowController and DashboardViewController. Phase 5 addresses performance. Phase 6 adds missing tests.

**Tech Stack:** Swift 5.10, AppKit, XCTest, macOS 14.0+ (Sonoma)

---

## File Structure

### New Files to Create

| File | Responsibility |
|------|---------------|
| `Sources/Core/ProcessRunner.swift` | Centralized shell command execution with error handling |
| `Sources/Core/SessionManager.swift` | tmux/zmx session lifecycle (create, kill, resize, refresh) |
| `Sources/Core/ConfigObserver.swift` | Centralized config persistence with debounced save |
| `Sources/App/TerminalSurfaceManager.swift` | Surface creation, caching, and lifecycle (keyed by worktree path) |
| `Sources/App/WorkspaceCoordinator.swift` | Workspace loading, repo add/remove, worktree discovery orchestration |
| `Sources/App/TabCoordinator.swift` | Tab switching, RepoVC cache, embed/detach logic |
| `Sources/App/MenuBuilder.swift` | Menu bar construction and shortcut actions |
| `Tests/ProcessRunnerTests.swift` | Tests for ProcessRunner |
| `Tests/SessionManagerTests.swift` | Tests for SessionManager |
| `Tests/ConfigObserverTests.swift` | Tests for ConfigObserver |
| `Tests/StatusPublisherThreadTests.swift` | Thread safety tests for StatusPublisher |

### Files to Modify

| File | Changes |
|------|---------|
| `Sources/Terminal/TerminalSurface.swift` | Fix `withCString` pointer lifetime; extract session ops to SessionManager |
| `Sources/Terminal/GhosttyBridge.swift` | Fix config leak on init failure; add clipboard impl stub |
| `Sources/Status/StatusPublisher.swift` | Add lock for shared dictionaries; replace `hashValue` with stable hash |
| `Sources/Status/StatusDetector.swift` | Pre-lowercase patterns at init time |
| `Sources/Core/AgentHead.swift` | Fix `unregister` key bug; make surface ref weak |
| `Sources/Core/Config.swift` | Replace scattered `save()` calls with observer pattern |
| `Sources/App/MainWindowController.swift` | Decompose into coordinators (~1719→~500 lines) |
| `Sources/App/AppDelegate.swift` | Add proper shutdown sequence |
| `Sources/UI/Dashboard/DashboardViewController.swift` | Extract layout logic |

---

## Phase 1: Critical Safety Fixes

### Task 1: Fix withCString Dangling Pointer in TerminalSurface

The `_createWithCommand` method stores C string pointers in a struct, but the pointers become dangling after the `withCString` closure exits. The `createBlock()` closure captures the config by reference, and the C pointers stored in `config.working_directory` and `config.command` are only valid within the `withCString` scope — but `createBlock` is called inside the scope so this is actually safe. However, the nested closures make this fragile. Refactor to make the safety explicit.

**Files:**
- Modify: `Sources/Terminal/TerminalSurface.swift:54-88`
- Test: `Tests/TerminalSurfaceReparentTests.swift`

- [ ] **Step 1: Write test verifying surface creation with command and working directory**

```swift
// In TerminalSurfaceReparentTests.swift, add:
func testCreateWithCommandCallsCreateSurface() {
    // This test verifies the refactored _createWithCommand doesn't crash
    // We can't easily test Ghostty C calls, but we verify the method signature
    // and that TerminalSurface.create handles nil app gracefully
    let surface = TerminalSurface()
    let container = NSView(frame: NSRect(x: 0, y: 0, width: 400, height: 300))
    let result = surface.create(in: container, workingDirectory: "/tmp", sessionName: nil)
    // Without GhosttyBridge initialized, this should return false gracefully
    XCTAssertFalse(result)
}
```

- [ ] **Step 2: Run test to verify it passes (testing graceful failure)**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmuxTests -configuration Debug test -only-testing:pmuxTests/TerminalSurfaceReparentTests/testCreateWithCommandCallsCreateSurface 2>&1 | tail -5`
Expected: PASS

- [ ] **Step 3: Refactor _createWithCommand to make pointer lifetime explicit**

Replace the nested closure approach with a flat structure. In `Sources/Terminal/TerminalSurface.swift`, replace `_createWithCommand` (lines 54-88):

```swift
private func _createWithCommand(app: ghostty_app_t, container: NSView, workingDirectory: String?, command: String?) {
    let termView = GhosttyNSView(frame: container.bounds)
    termView.wantsLayer = true

    var config = ghostty_surface_config_new()
    config.platform_tag = GHOSTTY_PLATFORM_MACOS
    config.platform.macos.nsview = Unmanaged.passUnretained(termView).toOpaque()
    config.scale_factor = Double(container.window?.backingScaleFactor ?? 2.0)

    // Use withExtendedLifetime to guarantee strings outlive C pointer usage.
    // The pointers from withCString are only valid within the closure scope,
    // so we must call _createSurface inside the innermost closure.
    let create = { [self] (wdPtr: UnsafePointer<CChar>?, cmdPtr: UnsafePointer<CChar>?) in
        if let wdPtr { config.working_directory = wdPtr }
        if let cmdPtr { config.command = cmdPtr }
        self._createSurface(app: app, config: &config, view: termView, container: container)
    }

    switch (workingDirectory, command) {
    case let (wd?, cmd?):
        wd.withCString { wdPtr in cmd.withCString { cmdPtr in create(wdPtr, cmdPtr) } }
    case let (wd?, nil):
        wd.withCString { wdPtr in create(wdPtr, nil) }
    case let (nil, cmd?):
        cmd.withCString { cmdPtr in create(nil, cmdPtr) }
    case (nil, nil):
        create(nil, nil)
    }
}
```

- [ ] **Step 4: Run existing tests to verify no regressions**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmuxTests -configuration Debug test -only-testing:pmuxTests/TerminalSurfaceReparentTests 2>&1 | tail -5`
Expected: All PASS

- [ ] **Step 5: Commit**

```bash
git add Sources/Terminal/TerminalSurface.swift Tests/TerminalSurfaceReparentTests.swift
git commit -m "fix: make withCString pointer lifetime explicit in TerminalSurface"
```

---

### Task 2: Fix GhosttyBridge Config Leak on Init Failure

When `ghostty_app_new` fails, config is freed. But when `ghostty_init` fails or `ghostty_config_new` returns nil, config is not freed.

**Files:**
- Modify: `Sources/Terminal/GhosttyBridge.swift:13-82`

- [ ] **Step 1: Fix the config leak path**

In `Sources/Terminal/GhosttyBridge.swift`, the `ghostty_config_new()` failure path at line 27 already returns without leaking (config hasn't been created yet). But we should add defensive cleanup. Replace the initialize method (lines 13-82):

```swift
func initialize() {
    guard !isInitialized else { return }

    let argc = CommandLine.argc
    let argv = CommandLine.unsafeArgv
    let result = ghostty_init(UInt(argc), argv)
    guard result == GHOSTTY_SUCCESS else {
        NSLog("Failed to initialize Ghostty: \(result)")
        return
    }

    guard let config = ghostty_config_new() else {
        NSLog("Failed to create Ghostty config")
        return
    }
    ghostty_config_load_default_files(config)
    ghostty_config_finalize(config)

    // Always free config — ghostty_app_new copies what it needs
    defer { ghostty_config_free(config) }

    var runtimeConfig = ghostty_runtime_config_s()
    runtimeConfig.userdata = Unmanaged.passUnretained(self).toOpaque()
    runtimeConfig.supports_selection_clipboard = false
    runtimeConfig.wakeup_cb = { userData in
        guard let userData else { return }
        let bridge = Unmanaged<GhosttyBridge>.fromOpaque(userData).takeUnretainedValue()
        DispatchQueue.main.async {
            bridge.tick()
        }
    }
    runtimeConfig.action_cb = { app, target, action in
        return GhosttyBridge.handleAction(app: app, target: target, action: action)
    }
    runtimeConfig.read_clipboard_cb = { userData, clipboard, state in
        GhosttyBridge.readClipboard(userData: userData, clipboard: clipboard, state: state)
    }
    runtimeConfig.confirm_read_clipboard_cb = { userData, text, state, request in
        guard let userData, let state else { return }
    }
    runtimeConfig.write_clipboard_cb = { userData, clipboard, content, contentLen, confirm in
        guard let content, contentLen > 0 else { return }
        let item = content.pointee
        if let data = item.data {
            let str = String(cString: data)
            let pasteboard = NSPasteboard.general
            pasteboard.clearContents()
            pasteboard.setString(str, forType: .string)
        }
    }
    runtimeConfig.close_surface_cb = { userData, processAlive in
        NotificationCenter.default.post(name: .ghosttySurfaceCloseRequested, object: nil)
    }

    guard let ghosttyApp = ghostty_app_new(&runtimeConfig, config) else {
        NSLog("Failed to create Ghostty app")
        return  // defer handles config cleanup
    }

    self.app = ghosttyApp
    self.isInitialized = true
    NSLog("Ghostty initialized successfully")
}
```

- [ ] **Step 2: Run build to verify compilation**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmux -configuration Debug build 2>&1 | tail -3`
Expected: BUILD SUCCEEDED

- [ ] **Step 3: Commit**

```bash
git add Sources/Terminal/GhosttyBridge.swift
git commit -m "fix: use defer to prevent config leak on GhosttyBridge init failure"
```

---

### Task 3: Fix StatusPublisher Thread Safety

`surfaces`, `worktreePaths`, `lastViewportHashes`, and `trackers` are accessed from both main thread and poll queue without synchronization.

**Files:**
- Modify: `Sources/Status/StatusPublisher.swift`
- Create: `Tests/StatusPublisherThreadTests.swift`

- [ ] **Step 1: Write thread safety test**

```swift
// Tests/StatusPublisherThreadTests.swift
import XCTest
@testable import pmux

class StatusPublisherThreadTests: XCTestCase {
    func testConcurrentUpdateAndPollDoesNotCrash() {
        let publisher = StatusPublisher()
        let expectation = expectation(description: "concurrent access")
        expectation.expectedFulfillmentCount = 10

        // Simulate concurrent updateSurfaces calls
        for _ in 0..<10 {
            DispatchQueue.global().async {
                publisher.updateSurfaces([:])
                expectation.fulfill()
            }
        }

        wait(for: [expectation], timeout: 5.0)
    }
}
```

- [ ] **Step 2: Run test — may crash or pass depending on timing**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmuxTests -configuration Debug test -only-testing:pmuxTests/StatusPublisherThreadTests 2>&1 | tail -5`

- [ ] **Step 3: Add NSLock to StatusPublisher**

In `Sources/Status/StatusPublisher.swift`, add a lock and protect shared state:

```swift
// Add after line 22 (webhookProvider):
private let lock = NSLock()
```

Wrap `start()`, `updateSurfaces()`, and `schedulePoll()` snapshot captures with the lock:

In `start(surfaces:)` — wrap the dictionary mutations:
```swift
func start(surfaces: [String: TerminalSurface]) {
    let inputWorktreePaths = Array(surfaces.keys)
    lock.lock()
    self.surfaces = [:]
    self.worktreePaths = [:]
    for (worktreePath, surface) in surfaces {
        self.surfaces[surface.id] = surface
        self.worktreePaths[surface.id] = worktreePath
    }
    for terminalID in self.surfaces.keys {
        if trackers[terminalID] == nil {
            trackers[terminalID] = DebouncedStatusTracker()
        }
    }
    lock.unlock()

    stop()
    webhookProvider.updateWorktrees(inputWorktreePaths)

    timer = Timer.scheduledTimer(withTimeInterval: pollInterval, repeats: true) { [weak self] _ in
        self?.schedulePoll()
    }
    schedulePoll()
}
```

In `updateSurfaces()` — same pattern with lock around mutations.

In `schedulePoll()` — snapshot under lock:
```swift
private func schedulePoll() {
    lock.lock()
    let surfaceSnapshot = surfaces
    let pathSnapshot = worktreePaths
    lock.unlock()
    pollCycle &+= 1
    let cycle = pollCycle
    let preferredSnapshot = preferredPaths
    pollQueue.async { [weak self] in
        self?.pollAll(surfaceSnapshot, preferredPaths: preferredSnapshot, pollCycle: cycle, paths: pathSnapshot)
    }
}
```

In `pollAll()` — protect `lastViewportHashes` and `trackers` mutations:
```swift
// Before checking lastViewportHashes:
lock.lock()
let lastHash = lastViewportHashes[terminalID]
lock.unlock()

if let lastHash, lastHash == contentHash {
    continue
}

lock.lock()
lastViewportHashes[terminalID] = contentHash
let tracker = trackers[terminalID] ?? {
    let t = DebouncedStatusTracker()
    trackers[terminalID] = t
    return t
}()
lock.unlock()
```

- [ ] **Step 4: Run thread safety test**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmuxTests -configuration Debug test -only-testing:pmuxTests/StatusPublisherThreadTests 2>&1 | tail -5`
Expected: PASS

- [ ] **Step 5: Run all StatusPublisher-related tests**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmuxTests -configuration Debug test -only-testing:pmuxTests/StatusDetectorTests 2>&1 | tail -5`
Expected: All PASS

- [ ] **Step 6: Commit**

```bash
git add Sources/Status/StatusPublisher.swift Tests/StatusPublisherThreadTests.swift
git commit -m "fix: add thread synchronization to StatusPublisher shared state"
```

---

### Task 4: Fix AgentHead unregister Key Bug and Weak Surface Reference

Line 80 reads `worktreeIndex[terminalID]` AFTER line 76 already removed the entry from `worktreeIndex`. Also, `AgentInfo.surface` holds a strong reference preventing cleanup.

**Files:**
- Modify: `Sources/Core/AgentHead.swift:71-82`
- Modify: `Sources/Core/AgentInfo.swift` (make surface weak)
- Test: `Tests/AgentHeadTests.swift`

- [ ] **Step 1: Write test exposing the bug**

```swift
// Add to AgentHeadTests.swift:
func testUnregisterCleansUpBackendsByPath() {
    let surface = TerminalSurface()
    AgentHead.shared.register(
        surface: surface, worktreePath: "/tmp/test-repo/main",
        branch: "main", project: "test", startedAt: nil,
        tmuxSessionName: "pmux-test-main", backend: "zmx"
    )

    AgentHead.shared.unregister(terminalID: surface.id)

    // After unregister, agent should be completely gone
    XCTAssertNil(AgentHead.shared.agent(for: surface.id))
    XCTAssertNil(AgentHead.shared.agent(forWorktree: "/tmp/test-repo/main"))
}
```

- [ ] **Step 2: Run test**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmuxTests -configuration Debug test -only-testing:pmuxTests/AgentHeadTests/testUnregisterCleansUpBackendsByPath 2>&1 | tail -5`

- [ ] **Step 3: Fix the unregister method**

In `Sources/Core/AgentHead.swift`, replace `unregister` (lines 71-82):

```swift
func unregister(terminalID: String) {
    lock.lock()
    defer { lock.unlock() }

    if let info = agents[terminalID] {
        worktreeIndex.removeValue(forKey: info.worktreePath)
        backendsByPath.removeValue(forKey: info.worktreePath)
    }
    agents.removeValue(forKey: terminalID)
    channels.removeValue(forKey: terminalID)
    orderedIDs.removeAll { $0 == terminalID }
}
```

- [ ] **Step 4: Verify surface is already weak in AgentInfo**

`Sources/Core/AgentInfo.swift:14` already has `weak var surface: TerminalSurface?`. No changes needed — just verify during implementation.

- [ ] **Step 5: Run all AgentHead tests**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmuxTests -configuration Debug test -only-testing:pmuxTests/AgentHeadTests 2>&1 | tail -5`
Expected: All PASS

- [ ] **Step 6: Commit**

```bash
git add Sources/Core/AgentHead.swift Sources/Core/AgentInfo.swift Tests/AgentHeadTests.swift
git commit -m "fix: correct unregister key ordering in AgentHead; document surface ownership"
```

---

### Task 5: Fix App Shutdown Cleanup

`AppDelegate.applicationWillTerminate` only calls `GhosttyBridge.shared.shutdown()`. Missing: StatusPublisher stop, WebhookServer stop, config save drain, surface cleanup.

**Files:**
- Modify: `Sources/App/AppDelegate.swift`
- Modify: `Sources/App/MainWindowController.swift` (expose cleanup method)

- [ ] **Step 1: Add a public cleanup method to MainWindowController**

The cleanup already exists in `windowWillClose` (lines 1307-1315). Extract it to a public method:

```swift
// In MainWindowController, add:
func cleanupBeforeTermination() {
    statusPublisher.stop()
    webhookServer?.stop()
    webhookServer = nil
    for (_, surface) in surfaces {
        surface.destroy()
    }
    surfaces.removeAll()
}
```

- [ ] **Step 2: Update AppDelegate to call cleanup**

```swift
func applicationWillTerminate(_ notification: Notification) {
    mainWindowController?.cleanupBeforeTermination()
    GhosttyBridge.shared.shutdown()
}
```

- [ ] **Step 3: Run build**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmux -configuration Debug build 2>&1 | tail -3`
Expected: BUILD SUCCEEDED

- [ ] **Step 4: Commit**

```bash
git add Sources/App/AppDelegate.swift Sources/App/MainWindowController.swift
git commit -m "fix: ensure proper cleanup of StatusPublisher, WebhookServer, and surfaces on app quit"
```

---

## Phase 2: Extract Reusable Utilities

### Task 6: Extract ProcessRunner

Centralize the repeated Process spawning pattern found in MainWindowController (lines 340-372), TerminalSurface (lines 186-223), and WorktreeDiscovery.

**Files:**
- Create: `Sources/Core/ProcessRunner.swift`
- Create: `Tests/ProcessRunnerTests.swift`

- [ ] **Step 1: Write tests for ProcessRunner**

```swift
// Tests/ProcessRunnerTests.swift
import XCTest
@testable import pmux

class ProcessRunnerTests: XCTestCase {
    func testCommandExistsForKnownCommand() {
        XCTAssertTrue(ProcessRunner.commandExists("ls"))
    }

    func testCommandExistsForUnknownCommand() {
        XCTAssertFalse(ProcessRunner.commandExists("definitely_not_a_real_command_12345"))
    }

    func testCommandOutputReturnsResult() {
        let output = ProcessRunner.output(["echo", "hello"])
        XCTAssertEqual(output, "hello")
    }

    func testCommandOutputReturnsNilOnFailure() {
        let output = ProcessRunner.output(["false"])
        XCTAssertNil(output)
    }

    func testRunFireAndForgetDoesNotThrow() {
        // Just verify it doesn't crash
        ProcessRunner.runFireAndForget(["echo", "test"])
    }
}
```

- [ ] **Step 2: Run tests to see them fail**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmuxTests -configuration Debug test -only-testing:pmuxTests/ProcessRunnerTests 2>&1 | tail -5`
Expected: FAIL (ProcessRunner not found)

- [ ] **Step 3: Implement ProcessRunner**

```swift
// Sources/Core/ProcessRunner.swift
import Foundation

enum ProcessRunner {
    /// Check if a command exists on PATH using login shell
    static func commandExists(_ command: String) -> Bool {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        process.arguments = ["bash", "-lc", "command -v \(command)"]
        process.standardOutput = Pipe()
        process.standardError = Pipe()
        do {
            try process.run()
            process.waitUntilExit()
            return process.terminationStatus == 0
        } catch {
            return false
        }
    }

    /// Run a command and return trimmed stdout, or nil on failure
    static func output(_ args: [String]) -> String? {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        process.arguments = args
        let pipe = Pipe()
        process.standardOutput = pipe
        process.standardError = Pipe()
        do {
            try process.run()
            process.waitUntilExit()
            guard process.terminationStatus == 0 else { return nil }
            let data = pipe.fileHandleForReading.readDataToEndOfFile()
            guard let str = String(data: data, encoding: .utf8) else { return nil }
            return str.trimmingCharacters(in: .whitespacesAndNewlines)
        } catch {
            return nil
        }
    }

    /// Run a command, ignoring output. Logs errors.
    static func runFireAndForget(_ args: [String]) {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        process.arguments = args
        process.standardOutput = Pipe()
        process.standardError = Pipe()
        do {
            try process.run()
        } catch {
            NSLog("ProcessRunner: failed to run \(args.first ?? "?"): \(error)")
        }
    }

    /// Run a command synchronously, waiting for exit. Logs errors.
    static func runSync(_ args: [String]) {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        process.arguments = args
        process.standardOutput = Pipe()
        process.standardError = Pipe()
        do {
            try process.run()
            process.waitUntilExit()
        } catch {
            NSLog("ProcessRunner: failed to run \(args.first ?? "?"): \(error)")
        }
    }
}
```

- [ ] **Step 4: Run tests**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmuxTests -configuration Debug test -only-testing:pmuxTests/ProcessRunnerTests 2>&1 | tail -5`
Expected: All PASS

- [ ] **Step 5: Commit**

```bash
git add Sources/Core/ProcessRunner.swift Tests/ProcessRunnerTests.swift
git commit -m "feat: extract ProcessRunner utility for centralized command execution"
```

---

### Task 7: Extract SessionManager

Centralize tmux/zmx session operations currently scattered across MainWindowController and TerminalSurface.

**Files:**
- Create: `Sources/Core/SessionManager.swift`
- Create: `Tests/SessionManagerTests.swift`

- [ ] **Step 1: Write tests**

```swift
// Tests/SessionManagerTests.swift
import XCTest
@testable import pmux

class SessionManagerTests: XCTestCase {
    func testPersistentSessionNameSanitizesDots() {
        let name = SessionManager.persistentSessionName(for: "/Users/test/repos/my.project/feature-1")
        XCTAssertFalse(name.contains("."))
        XCTAssertTrue(name.hasPrefix("pmux-"))
    }

    func testPersistentSessionNameSanitizesColons() {
        let name = SessionManager.persistentSessionName(for: "/Users/test/repo:name/branch")
        XCTAssertFalse(name.contains(":"))
    }

    func testPersistentSessionNameFormat() {
        let name = SessionManager.persistentSessionName(for: "/Users/test/myrepo/feature-branch")
        XCTAssertEqual(name, "pmux-myrepo-feature-branch")
    }
}
```

- [ ] **Step 2: Run tests to see them fail**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmuxTests -configuration Debug test -only-testing:pmuxTests/SessionManagerTests 2>&1 | tail -5`
Expected: FAIL

- [ ] **Step 3: Implement SessionManager**

```swift
// Sources/Core/SessionManager.swift
import Foundation

enum SessionManager {
    /// Generate a stable persistent session name from a worktree path.
    /// Format: pmux-<parent>-<name>, with dots and colons replaced by underscores.
    static func persistentSessionName(for path: String) -> String {
        let url = URL(fileURLWithPath: path)
        let parent = url.deletingLastPathComponent().lastPathComponent
        let name = url.lastPathComponent
        return "pmux-\(parent)-\(name)"
            .replacingOccurrences(of: ".", with: "_")
            .replacingOccurrences(of: ":", with: "_")
    }

    /// Kill a persistent session (tmux or zmx)
    static func killSession(_ name: String, backend: String) {
        DispatchQueue.global(qos: .utility).async {
            if backend == "tmux" {
                ProcessRunner.runSync(["tmux", "kill-session", "-t", name])
            } else {
                ProcessRunner.runSync(["zmx", "kill", name])
            }
        }
    }

    /// Resize a tmux session to match terminal grid size
    static func resizeTmuxSession(_ sessionName: String, cols: Int, rows: Int) {
        ProcessRunner.runSync(["tmux", "resize-window", "-t", sessionName, "-x", "\(cols)", "-y", "\(rows)"])
        ProcessRunner.runSync(["tmux", "refresh-client", "-t", sessionName, "-S"])
    }

    /// Refresh a tmux client display
    static func refreshTmuxClient(_ sessionName: String) {
        ProcessRunner.runSync(["tmux", "resize-window", "-t", sessionName, "-A"])
        ProcessRunner.runSync(["tmux", "refresh-client", "-t", sessionName, "-S"])
    }

    /// Check if a tmux session exists (blocking)
    static func tmuxSessionExists(_ name: String) -> Bool {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        process.arguments = ["tmux", "has-session", "-t", name]
        process.standardOutput = Pipe()
        process.standardError = Pipe()
        do {
            try process.run()
            process.waitUntilExit()
            return process.terminationStatus == 0
        } catch {
            return false
        }
    }
}
```

- [ ] **Step 4: Run tests**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmuxTests -configuration Debug test -only-testing:pmuxTests/SessionManagerTests 2>&1 | tail -5`
Expected: All PASS

- [ ] **Step 5: Replace usages in MainWindowController**

In `Sources/App/MainWindowController.swift`:
- Replace `Self.persistentSessionName(for:)` calls with `SessionManager.persistentSessionName(for:)`
- Replace `killSession(_:backend:)` calls with `SessionManager.killSession(_:backend:)`
- Remove the private `persistentSessionName` and `killSession` methods
- Replace `Self.commandExists` and `Self.commandOutput` with `ProcessRunner.commandExists` and `ProcessRunner.output`

- [ ] **Step 6: Replace usages in TerminalSurface**

In `Sources/Terminal/TerminalSurface.swift`:
- Replace `refreshSessionLayout()` body with call to `SessionManager.resizeTmuxSession`
- Replace `refreshTmuxClient` static method body with `SessionManager.refreshTmuxClient`

- [ ] **Step 7: Run full test suite**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmuxTests -configuration Debug test 2>&1 | tail -5`
Expected: All PASS

- [ ] **Step 8: Commit**

```bash
git add Sources/Core/SessionManager.swift Sources/Core/ProcessRunner.swift Tests/SessionManagerTests.swift Sources/App/MainWindowController.swift Sources/Terminal/TerminalSurface.swift
git commit -m "refactor: extract SessionManager for centralized tmux/zmx session operations"
```

---

## Phase 3: Decompose MainWindowController

### Task 8: Extract MenuBuilder

Move the 105-line `setupMenuShortcuts()` method and all `@objc` shortcut handlers into a dedicated class.

**Files:**
- Create: `Sources/App/MenuBuilder.swift`
- Modify: `Sources/App/MainWindowController.swift`

- [ ] **Step 1: Create MenuBuilder**

```swift
// Sources/App/MenuBuilder.swift
import AppKit

/// Builds the main menu bar. All menu items use nil target (responder chain),
/// so MainWindowController's existing @objc methods are found automatically.
/// This purely extracts the menu *construction* — no new types needed.
enum MenuBuilder {
    static func buildMainMenu() -> NSMenu {
        let mainMenu = NSMenu()

        // App menu
        let appMenuItem = NSMenuItem()
        let appMenu = NSMenu()
        appMenu.addItem(NSMenuItem(title: "Settings...", action: #selector(MainWindowController.showSettings), keyEquivalent: ","))
        appMenu.addItem(NSMenuItem.separator())
        appMenu.addItem(NSMenuItem(title: "Check for Updates...", action: #selector(MainWindowController.checkForUpdates), keyEquivalent: "u"))
        appMenu.addItem(NSMenuItem.separator())
        appMenu.addItem(withTitle: "Quit pmux", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        appMenuItem.submenu = appMenu
        mainMenu.addItem(appMenuItem)

        // File menu
        let fileMenuItem = NSMenuItem()
        let fileMenu = NSMenu(title: "File")
        fileMenu.addItem(NSMenuItem(title: "New Branch...", action: #selector(MainWindowController.showNewBranchDialog), keyEquivalent: "n"))
        fileMenu.addItem(NSMenuItem(title: "Quick Switch...", action: #selector(MainWindowController.showQuickSwitcher), keyEquivalent: "p"))
        fileMenuItem.submenu = fileMenu
        mainMenu.addItem(fileMenuItem)

        // Edit menu
        let editMenuItem = NSMenuItem()
        let editMenu = NSMenu(title: "Edit")
        editMenu.addItem(withTitle: "Undo", action: Selector(("undo:")), keyEquivalent: "z")
        let redoItem = NSMenuItem(title: "Redo", action: Selector(("redo:")), keyEquivalent: "z")
        redoItem.keyEquivalentModifierMask = [.command, .shift]
        editMenu.addItem(redoItem)
        editMenu.addItem(NSMenuItem.separator())
        editMenu.addItem(withTitle: "Cut", action: #selector(NSText.cut(_:)), keyEquivalent: "x")
        editMenu.addItem(withTitle: "Copy", action: #selector(NSText.copy(_:)), keyEquivalent: "c")
        editMenu.addItem(withTitle: "Paste", action: #selector(NSText.paste(_:)), keyEquivalent: "v")
        editMenu.addItem(withTitle: "Select All", action: #selector(NSText.selectAll(_:)), keyEquivalent: "a")
        editMenuItem.submenu = editMenu
        mainMenu.addItem(editMenuItem)

        // View menu
        let viewMenuItem = NSMenuItem()
        let viewMenu = NSMenu(title: "View")
        viewMenu.addItem(NSMenuItem(title: "Dashboard", action: #selector(MainWindowController.switchToDashboard), keyEquivalent: "0"))
        viewMenu.addItem(NSMenuItem(title: "Close Tab", action: #selector(MainWindowController.closeCurrentTab), keyEquivalent: "w"))
        viewMenu.addItem(NSMenuItem(title: "Show Diff...", action: #selector(MainWindowController.showDiffOverlay), keyEquivalent: ""))
        viewMenu.addItem(NSMenuItem.separator())
        viewMenu.addItem(NSMenuItem(title: "Zoom In (Smaller Cards)", action: #selector(MainWindowController.dashboardZoomIn), keyEquivalent: "-"))
        viewMenu.addItem(NSMenuItem(title: "Zoom Out (Larger Cards)", action: #selector(MainWindowController.dashboardZoomOut), keyEquivalent: "="))
        viewMenuItem.submenu = viewMenu
        mainMenu.addItem(viewMenuItem)

        // Window menu
        let windowMenuItem = NSMenuItem()
        let windowMenu = NSMenu(title: "Window")
        windowMenu.addItem(withTitle: "Minimize", action: #selector(NSWindow.miniaturize(_:)), keyEquivalent: "m")
        windowMenu.addItem(withTitle: "Zoom", action: #selector(NSWindow.zoom(_:)), keyEquivalent: "")
        windowMenu.addItem(NSMenuItem.separator())
        windowMenu.addItem(NSMenuItem(title: "Next Tab", action: #selector(MainWindowController.selectNextTab), keyEquivalent: "}"))
        windowMenu.addItem(NSMenuItem(title: "Previous Tab", action: #selector(MainWindowController.selectPreviousTab), keyEquivalent: "{"))
        windowMenu.addItem(NSMenuItem.separator())
        windowMenu.addItem(withTitle: "Bring All to Front", action: #selector(NSApplication.arrangeInFront(_:)), keyEquivalent: "")
        windowMenuItem.submenu = windowMenu
        mainMenu.addItem(windowMenuItem)
        NSApp.windowsMenu = windowMenu

        // Help menu
        let helpMenuItem = NSMenuItem()
        let helpMenu = NSMenu(title: "Help")
        helpMenu.addItem(NSMenuItem(title: "Keyboard Shortcuts", action: #selector(MainWindowController.showKeyboardShortcuts), keyEquivalent: ""))
        helpMenu.addItem(NSMenuItem.separator())
        helpMenu.addItem(NSMenuItem(title: "pmux Documentation", action: #selector(MainWindowController.openDocumentation), keyEquivalent: ""))
        helpMenuItem.submenu = helpMenu
        mainMenu.addItem(helpMenuItem)
        NSApp.helpMenu = helpMenu

        return mainMenu
    }
}
```

Note: Menu items use the responder chain (nil target). MainWindowController's existing `@objc` methods (showSettings, checkForUpdates, etc.) must remain on MainWindowController but their visibility changes from `private` to `internal` so the selectors resolve. The menu construction logic (~100 lines) moves out of MainWindowController.

- [ ] **Step 2: Update MainWindowController to use MenuBuilder**

Replace `setupMenuShortcuts()` with:
```swift
private func setupMenuShortcuts() {
    NSApp.mainMenu = MenuBuilder.buildMainMenu()
}
```

Change the `@objc` menu action methods from `private` to `internal` (remove the `private` keyword) so the responder chain can find them. Remove the 105 lines of manual menu construction from MainWindowController.

- [ ] **Step 3: Run build**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmux -configuration Debug build 2>&1 | tail -3`
Expected: BUILD SUCCEEDED

- [ ] **Step 4: Commit**

```bash
git add Sources/App/MenuBuilder.swift Sources/App/MainWindowController.swift
git commit -m "refactor: extract MenuBuilder from MainWindowController"
```

---

### Task 9: Extract TerminalSurfaceManager

Move surface creation, caching, and lifecycle management out of MainWindowController.

**Files:**
- Create: `Sources/App/TerminalSurfaceManager.swift`
- Modify: `Sources/App/MainWindowController.swift`

- [ ] **Step 1: Create TerminalSurfaceManager**

```swift
// Sources/App/TerminalSurfaceManager.swift
import Foundation

/// Manages the lifecycle of TerminalSurface instances, keyed by worktree path.
/// Single source of truth for surface ownership.
class TerminalSurfaceManager {
    private(set) var surfaces: [String: TerminalSurface] = [:]

    /// Get or create a surface for the given worktree info.
    func surface(for worktreePath: String, backend: String) -> TerminalSurface {
        if let existing = surfaces[worktreePath] {
            return existing
        }
        let surface = TerminalSurface()
        if backend != "local" {
            surface.sessionName = SessionManager.persistentSessionName(for: worktreePath)
            surface.backend = backend
        }
        surfaces[worktreePath] = surface
        return surface
    }

    /// Destroy and remove a surface for the given worktree path.
    func destroySurface(at worktreePath: String) {
        if let surface = surfaces.removeValue(forKey: worktreePath) {
            surface.destroy()
        }
    }

    /// Destroy all surfaces and clear the cache.
    func destroyAll() {
        for (_, surface) in surfaces {
            surface.destroy()
        }
        surfaces.removeAll()
    }

    /// Remove a surface without destroying it (for reparenting scenarios).
    func removeSurface(at worktreePath: String) -> TerminalSurface? {
        surfaces.removeValue(forKey: worktreePath)
    }
}
```

- [ ] **Step 2: Update MainWindowController to use TerminalSurfaceManager**

Replace `private var surfaces: [String: TerminalSurface] = [:]` with:
```swift
private let surfaceManager = TerminalSurfaceManager()
```

Replace all `surfaces[...]` accesses with `surfaceManager.surfaces[...]` or `surfaceManager.surface(for:backend:)`.

Replace `createSurface(for:)` calls with `surfaceManager.surface(for: info.path, backend: runtimeBackend)`.

Remove the private `createSurface(for:)` method.

- [ ] **Step 3: Run full test suite**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmuxTests -configuration Debug test 2>&1 | tail -5`
Expected: All PASS

- [ ] **Step 4: Commit**

```bash
git add Sources/App/TerminalSurfaceManager.swift Sources/App/MainWindowController.swift
git commit -m "refactor: extract TerminalSurfaceManager from MainWindowController"
```

---

### Task 10: Extract WorkspaceCoordinator

Move workspace loading, repo add/remove, worktree discovery orchestration, and AgentHead registration out of MainWindowController.

**Files:**
- Create: `Sources/App/WorkspaceCoordinator.swift`
- Modify: `Sources/App/MainWindowController.swift`

- [ ] **Step 1: Create WorkspaceCoordinator**

```swift
// Sources/App/WorkspaceCoordinator.swift
import Foundation

protocol WorkspaceCoordinatorDelegate: AnyObject {
    func workspaceCoordinatorDidLoadWorkspaces(_ coordinator: WorkspaceCoordinator)
    func workspaceCoordinator(_ coordinator: WorkspaceCoordinator, didAddRepoAt tabIndex: Int)
    func workspaceCoordinator(_ coordinator: WorkspaceCoordinator, didRemoveRepo projectName: String)
    func workspaceCoordinatorDidUpdateWorktrees(_ coordinator: WorkspaceCoordinator)
}

class WorkspaceCoordinator {
    weak var delegate: WorkspaceCoordinatorDelegate?

    let workspaceManager = WorkspaceManager()
    let surfaceManager: TerminalSurfaceManager
    var config: Config
    var runtimeBackend: String

    private static let iso8601: ISO8601DateFormatter = {
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withInternetDateTime]
        return f
    }()

    var allWorktrees: [(info: WorktreeInfo, surface: TerminalSurface)] = []

    init(surfaceManager: TerminalSurfaceManager, config: Config, backend: String) {
        self.surfaceManager = surfaceManager
        self.config = config
        self.runtimeBackend = backend
    }

    func loadWorkspaces() {
        let repoPaths = config.workspacePaths
        let cardOrder = config.cardOrder

        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            guard let self else { return }

            var discoveredWorktrees: [(repoPath: String, worktrees: [WorktreeInfo])] = []
            for repoPath in repoPaths {
                let worktrees = WorktreeDiscovery.discover(repoPath: repoPath)
                discoveredWorktrees.append((repoPath, worktrees))
            }

            DispatchQueue.main.async { [weak self] in
                guard let self else { return }
                self.integrateDiscoveredWorkspaces(discoveredWorktrees, cardOrder: cardOrder)
                self.delegate?.workspaceCoordinatorDidLoadWorkspaces(self)
            }
        }
    }

    private func integrateDiscoveredWorkspaces(
        _ discovered: [(repoPath: String, worktrees: [WorktreeInfo])],
        cardOrder: [String]
    ) {
        var allInfos: [(info: WorktreeInfo, surface: TerminalSurface)] = []

        for (repoPath, worktrees) in discovered {
            let effectiveWorktrees: [WorktreeInfo]
            if worktrees.isEmpty {
                effectiveWorktrees = [WorktreeInfo(path: repoPath, branch: "main", commitHash: "", isMainWorktree: true)]
            } else {
                effectiveWorktrees = worktrees
            }

            for info in effectiveWorktrees {
                let surface = surfaceManager.surface(for: info.path, backend: runtimeBackend)
                allInfos.append((info: info, surface: surface))
            }
            _ = workspaceManager.addTab(repoPath: repoPath, worktrees: worktrees)
        }

        // Record startedAt
        let now = Self.iso8601.string(from: Date())
        var configChanged = false
        for (info, _) in allInfos {
            if config.worktreeStartedAt[info.path] == nil {
                config.worktreeStartedAt[info.path] = now
                configChanged = true
            }
        }
        if configChanged { config.save() }

        // Apply saved card order
        if !cardOrder.isEmpty {
            allInfos.sort { a, b in
                let ai = cardOrder.firstIndex(of: a.info.path) ?? Int.max
                let bi = cardOrder.firstIndex(of: b.info.path) ?? Int.max
                return ai < bi
            }
        }

        self.allWorktrees = allInfos

        // Register with AgentHead
        for (info, surface) in allInfos {
            let repo = WorktreeDiscovery.findRepoRoot(from: info.path) ?? info.path
            let proj = workspaceManager.tabs.first(where: { $0.repoPath == repo })?.displayName
                ?? URL(fileURLWithPath: repo).lastPathComponent
            let started = config.worktreeStartedAt[info.path].flatMap { Self.iso8601.date(from: $0) }
            let sessionName = runtimeBackend == "local" ? nil : SessionManager.persistentSessionName(for: info.path)
            AgentHead.shared.register(
                surface: surface, worktreePath: info.path, branch: info.branch,
                project: proj, startedAt: started,
                tmuxSessionName: sessionName, backend: runtimeBackend
            )
        }
        if !cardOrder.isEmpty {
            AgentHead.shared.reorder(paths: cardOrder)
        }
    }

    func addRepo(at path: String) -> Bool {
        guard !config.workspacePaths.contains(path) else { return false }
        config.workspacePaths.append(path)
        config.save()
        return true
    }

    func closeRepo(projectName: String) {
        guard let tabIndex = workspaceManager.tabs.firstIndex(where: { $0.displayName == projectName }) else { return }
        let tab = workspaceManager.tabs[tabIndex]

        for worktree in tab.worktrees {
            surfaceManager.destroySurface(at: worktree.path)
            if let agent = AgentHead.shared.agent(forWorktree: worktree.path) {
                AgentHead.shared.unregister(terminalID: agent.id)
            }
            if runtimeBackend != "local" {
                let sessionName = SessionManager.persistentSessionName(for: worktree.path)
                SessionManager.killSession(sessionName, backend: runtimeBackend)
            }
        }

        allWorktrees.removeAll { item in
            tab.worktrees.contains(where: { $0.path == item.info.path })
        }

        config.workspacePaths.removeAll { $0 == tab.repoPath }
        config.save()
        workspaceManager.removeTab(at: tabIndex)
    }
}
```

- [ ] **Step 2: Update MainWindowController to use WorkspaceCoordinator**

Replace the workspace-related properties and methods with delegation to WorkspaceCoordinator. MainWindowController creates the coordinator in init and forwards delegate calls.

- [ ] **Step 3: Run full test suite**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmuxTests -configuration Debug test 2>&1 | tail -5`
Expected: All PASS

- [ ] **Step 4: Commit**

```bash
git add Sources/App/WorkspaceCoordinator.swift Sources/App/MainWindowController.swift
git commit -m "refactor: extract WorkspaceCoordinator from MainWindowController"
```

---

## Phase 4: Performance Fixes

### Task 11: Replace hashValue with Stable Hash in StatusPublisher

`String.hashValue` is non-deterministic across app runs. Replace with a simple stable hash.

**Files:**
- Modify: `Sources/Status/StatusPublisher.swift:119`

- [ ] **Step 1: Add stable hash function**

Add to StatusPublisher (or as a private extension):

```swift
/// Simple stable hash for viewport content change detection.
/// Uses djb2 algorithm — deterministic across runs, unlike Swift's Hasher.
private func stableHash(_ string: String) -> UInt64 {
    var hash: UInt64 = 5381
    for byte in string.utf8 {
        hash = ((hash &<< 5) &+ hash) &+ UInt64(byte)
    }
    return hash
}
```

- [ ] **Step 2: Replace hashValue usage**

Change line 119 from:
```swift
let contentHash = content.hashValue
```
to:
```swift
let contentHash = stableHash(content)
```

Change `lastViewportHashes` type from `[String: Int]` to `[String: UInt64]`.

- [ ] **Step 3: Run tests**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmuxTests -configuration Debug test 2>&1 | tail -5`
Expected: All PASS

- [ ] **Step 4: Commit**

```bash
git add Sources/Status/StatusPublisher.swift
git commit -m "fix: replace non-deterministic hashValue with stable djb2 hash in StatusPublisher"
```

---

### Task 12: Pre-lowercase Patterns in StatusDetector

`pattern.lowercased()` is called inside a loop every 2 seconds per surface. Pre-compute at init time.

**Files:**
- Modify: `Sources/Status/StatusDetector.swift:64-74`
- Modify: `Sources/Core/Config.swift` (add pre-lowercased cache to AgentDef)

- [ ] **Step 1: Add lowercased pattern cache to AgentDef**

In `Sources/Core/Config.swift`, add a computed/cached property:

```swift
// In AgentDef, add:
/// Pre-lowercased patterns for each rule, avoiding repeated lowercasing during detection
var lowercasedRules: [(status: String, patterns: [String])] {
    rules.map { ($0.status, $0.patterns.map { $0.lowercased() }) }
}

/// Pre-lowercased message skip patterns
var lowercasedMessageSkipPatterns: [String] {
    messageSkipPatterns.map { $0.lowercased() }
}
```

- [ ] **Step 2: Update detectStatus to use pre-lowercased patterns**

In `Sources/Status/StatusDetector.swift`, replace `detectStatus(fromLowercased:)`:

```swift
func detectStatus(fromLowercased lower: String) -> AgentStatus {
    for (status, patterns) in lowercasedRules {
        for pattern in patterns {
            if lower.contains(pattern) {
                return AgentStatus(rawValue: status) ?? .unknown
            }
        }
    }
    return AgentStatus(rawValue: defaultStatus) ?? .idle
}
```

- [ ] **Step 3: Update extractLastMessage to use pre-lowercased skip patterns**

Replace line 91:
```swift
if !lowercasedMessageSkipPatterns.contains(where: { trimmedLower.contains($0) }) {
```

- [ ] **Step 4: Run tests**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmuxTests -configuration Debug test -only-testing:pmuxTests/StatusDetectorTests 2>&1 | tail -5`
Expected: All PASS

- [ ] **Step 5: Commit**

```bash
git add Sources/Status/StatusDetector.swift Sources/Core/Config.swift
git commit -m "perf: pre-lowercase detection patterns to avoid repeated String.lowercased() in poll loop"
```

---

## Phase 5: Strengthen Test Coverage

### Task 13: Add MainWindowController Static Method Tests

The pure/static methods in MainWindowController are testable without a window. Add tests for the currently untested ones.

**Files:**
- Modify: `Tests/GridLayoutTests.swift` (or create a new test file)

- [ ] **Step 1: Add tests for resolvePreferredBackend edge cases**

```swift
// Add to existing test file or create Tests/MainWindowControllerTests.swift
func testResolveBackendPrefersZmxWhenAvailable() {
    XCTAssertEqual(
        MainWindowController.resolvePreferredBackend(preferred: "zmx", zmxAvailable: true, tmuxAvailable: true),
        "zmx"
    )
}

func testResolveBackendFallsToTmuxWhenZmxUnavailable() {
    XCTAssertEqual(
        MainWindowController.resolvePreferredBackend(preferred: "zmx", zmxAvailable: false, tmuxAvailable: true),
        "tmux"
    )
}

func testResolveBackendFallsToLocalWhenNoneAvailable() {
    XCTAssertEqual(
        MainWindowController.resolvePreferredBackend(preferred: "zmx", zmxAvailable: false, tmuxAvailable: false),
        "local"
    )
}

func testIsSupportedZmxVersionAccepts042() {
    XCTAssertTrue(MainWindowController.isSupportedZmxVersion("0.4.2"))
}

func testIsSupportedZmxVersionRejectsOld() {
    XCTAssertFalse(MainWindowController.isSupportedZmxVersion("0.4.1"))
}

func testIsSupportedZmxVersionAcceptsNewer() {
    XCTAssertTrue(MainWindowController.isSupportedZmxVersion("1.0.0"))
}

func testIsSupportedZmxVersionHandlesPrefix() {
    XCTAssertTrue(MainWindowController.isSupportedZmxVersion("v0.5.0"))
}
```

- [ ] **Step 2: Run tests**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmuxTests -configuration Debug test 2>&1 | tail -5`
Expected: All PASS

- [ ] **Step 3: Commit**

```bash
git add Tests/
git commit -m "test: add MainWindowController static method tests"
```

---

### Task 14: Add ProcessRunner and SessionManager Integration Tests

**Files:**
- Modify: `Tests/ProcessRunnerTests.swift`
- Modify: `Tests/SessionManagerTests.swift`

- [ ] **Step 1: Add edge case tests**

```swift
// ProcessRunnerTests.swift additions:
func testCommandOutputWithEmptyResult() {
    let output = ProcessRunner.output(["echo", ""])
    XCTAssertEqual(output, "")
}

func testCommandExistsWithPath() {
    XCTAssertTrue(ProcessRunner.commandExists("git"))
}

// SessionManagerTests.swift additions:
func testSessionNameWithNestedPath() {
    let name = SessionManager.persistentSessionName(for: "/home/user/workspace/org/repo/feature")
    XCTAssertEqual(name, "pmux-repo-feature")
}

func testSessionNameWithSingleComponent() {
    let name = SessionManager.persistentSessionName(for: "/repo")
    XCTAssertEqual(name, "pmux--repo")
}
```

- [ ] **Step 2: Run tests**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmuxTests -configuration Debug test 2>&1 | tail -5`
Expected: All PASS

- [ ] **Step 3: Commit**

```bash
git add Tests/ProcessRunnerTests.swift Tests/SessionManagerTests.swift
git commit -m "test: add edge case tests for ProcessRunner and SessionManager"
```

---

## Phase 6: Cleanup

### Task 15: Remove Dead Code and Empty Directories

**Files:**
- Delete: `Sources/Runtime/` (empty directory)
- Modify: `project.yml` if it references Runtime

- [ ] **Step 1: Check if Runtime is referenced**

Run: `grep -r "Runtime" project.yml`

- [ ] **Step 2: Remove empty directory if unreferenced**

```bash
rmdir Sources/Runtime 2>/dev/null || true
```

- [ ] **Step 3: Remove duplicate `writeClipboard` static method in GhosttyBridge**

Lines 143-152 in GhosttyBridge.swift define a `writeClipboard` static method that is never called (the closure at lines 56-64 handles it inline). Remove the dead method.

- [ ] **Step 4: Run full test suite**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmuxTests -configuration Debug test 2>&1 | tail -5`
Expected: All PASS

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "chore: remove empty Runtime directory and dead writeClipboard method"
```

---

## Execution Summary

| Phase | Tasks | Focus | Est. Reduction |
|-------|-------|-------|----------------|
| 1 | Tasks 1-5 | Critical safety fixes | — |
| 2 | Tasks 6-7 | Extract utilities | ~80 lines from MWC |
| 3 | Tasks 8-10 | Decompose MWC | ~700 lines from MWC |
| 4 | Tasks 11-12 | Performance | — |
| 5 | Tasks 13-14 | Test coverage | +15 tests |
| 6 | Task 15 | Cleanup | Dead code removal |

**Total expected MainWindowController reduction:** ~1719 → ~500-600 lines
**New test files:** 3-4
**New source files:** 5-6
