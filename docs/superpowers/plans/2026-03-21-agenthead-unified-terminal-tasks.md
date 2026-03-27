# AgentHead Unified Terminal Tasks Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend AgentHead from AI-agent-only manager to unified terminal task manager that also covers traditional shell commands, with terminal ID as primary key and OSC 133 cmdline detection.

**Architecture:** Change the primary key from worktree path to terminal ID (UUID on TerminalSurface). Add shell command types to the unified AgentType enum. Extend OSC133Parser to extract cmdline parameters. Add `detect(fromCommand:)` for shell type detection. Update all consumers to use terminal ID.

**Tech Stack:** Swift 5.10, XCTest, AppKit, Ghostty C interop

**Spec:** `docs/superpowers/specs/2026-03-21-agenthead-unified-terminal-tasks-design.md`

**Task dependency graph:**
```
Task 1 (TerminalSurface.id) ──┐
Task 2 (AgentType shell)  ────┤
Task 3 (OSC133 cmdline)   ────┼── Task 4 (AgentHead + AgentInfo + ALL consumers, atomic) ── Task 5 (Final verification)
```
Tasks 1, 2, 3 are independent and can run in parallel. Task 4 is a single atomic commit that changes AgentHead, AgentInfo, and ALL consumers together (to avoid compilation deadlock). Task 5 is final verification.

---

### Task 1: Add `id` property to TerminalSurface

**Files:**
- Modify: `Sources/Terminal/TerminalSurface.swift:5`

- [ ] **Step 1: Add the `id` property**

In `Sources/Terminal/TerminalSurface.swift`, add a UUID-based id as the first property of the class:

```swift
class TerminalSurface {
    /// Unique identifier for this terminal instance (used as primary key in AgentHead)
    let id: String = UUID().uuidString

    /// The NSView that Ghostty renders into (layer-backed, Metal)
    private(set) var view: GhosttyNSView!
    // ... rest unchanged
```

- [ ] **Step 2: Build to verify no compilation errors**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -5`
Expected: BUILD SUCCEEDED

- [ ] **Step 3: Commit**

```bash
git add Sources/Terminal/TerminalSurface.swift
git commit -m "feat: add UUID-based id property to TerminalSurface"
```

---

### Task 2: Add shell task cases to AgentType

**Files:**
- Modify: `Sources/Core/AgentType.swift`
- Test: `tests/AgentTypeTests.swift`

- [ ] **Step 1: Write failing tests for shell command detection and new properties**

Add to `tests/AgentTypeTests.swift`:

```swift
// MARK: - Shell command detection from command line

func testDetectFromCommand_Brew() {
    XCTAssertEqual(AgentType.detect(fromCommand: "brew install ffmpeg"), .brew)
}

func testDetectFromCommand_Make() {
    XCTAssertEqual(AgentType.detect(fromCommand: "make build"), .make)
}

func testDetectFromCommand_Docker() {
    XCTAssertEqual(AgentType.detect(fromCommand: "docker run -it ubuntu"), .docker)
}

func testDetectFromCommand_Npm() {
    XCTAssertEqual(AgentType.detect(fromCommand: "npm run build"), .npm)
}

func testDetectFromCommand_Npx() {
    XCTAssertEqual(AgentType.detect(fromCommand: "npx create-react-app"), .npm)
}

func testDetectFromCommand_Python() {
    XCTAssertEqual(AgentType.detect(fromCommand: "python3 script.py"), .python)
}

func testDetectFromCommand_WithFullPath() {
    XCTAssertEqual(AgentType.detect(fromCommand: "/usr/local/bin/brew install ffmpeg"), .brew)
}

func testDetectFromCommand_WithEnvPrefix() {
    XCTAssertEqual(AgentType.detect(fromCommand: "ENV=val make build"), .make)
}

func testDetectFromCommand_UnknownCommand() {
    XCTAssertEqual(AgentType.detect(fromCommand: "myapp --flag"), .shellCommand)
}

func testDetectFromCommand_EmptyString() {
    XCTAssertEqual(AgentType.detect(fromCommand: ""), .unknown)
}

func testDetectFromCommand_Btop() {
    XCTAssertEqual(AgentType.detect(fromCommand: "btop"), .btop)
}

func testDetectFromCommand_Cargo() {
    XCTAssertEqual(AgentType.detect(fromCommand: "cargo build --release"), .cargo)
}

// MARK: - isAIAgent / isShellTask

func testIsAIAgent() {
    XCTAssertTrue(AgentType.claudeCode.isAIAgent)
    XCTAssertTrue(AgentType.codex.isAIAgent)
    XCTAssertFalse(AgentType.brew.isAIAgent)
    XCTAssertFalse(AgentType.shellCommand.isAIAgent)
    XCTAssertFalse(AgentType.unknown.isAIAgent)
}

func testIsShellTask() {
    XCTAssertTrue(AgentType.brew.isShellTask)
    XCTAssertTrue(AgentType.shellCommand.isShellTask)
    XCTAssertFalse(AgentType.claudeCode.isShellTask)
    XCTAssertFalse(AgentType.unknown.isShellTask)
}

// MARK: - Shell task display names

func testShellDisplayNames() {
    XCTAssertEqual(AgentType.brew.displayName, "Homebrew")
    XCTAssertEqual(AgentType.btop.displayName, "btop")
    XCTAssertEqual(AgentType.shellCommand.displayName, "Shell")
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/AgentTypeTests 2>&1 | grep -E "(error:|FAIL)" | head -5`
Expected: Compilation errors — `.brew`, `.shellCommand`, `detect(fromCommand:)`, `isAIAgent`, `isShellTask` don't exist yet.

- [ ] **Step 3: Implement AgentType changes**

Replace the entire content of `Sources/Core/AgentType.swift`:

```swift
import Foundation

enum AgentType: String, Codable, CaseIterable {
    // AI Agents
    case claudeCode
    case codex
    case openCode
    case gemini
    case cline
    case goose
    case amp
    case aider
    case cursor
    case kiro
    // Shell tasks
    case brew
    case btop
    case top
    case htop
    case docker
    case npm
    case yarn
    case make
    case cargo
    case go
    case python
    case pip
    case shellCommand   // generic fallback for any non-AI command
    case unknown

    var displayName: String {
        switch self {
        case .claudeCode:   return "Claude Code"
        case .codex:        return "Codex"
        case .openCode:     return "OpenCode"
        case .gemini:       return "Gemini"
        case .cline:        return "Cline"
        case .goose:        return "Goose"
        case .amp:          return "Amp"
        case .aider:        return "Aider"
        case .cursor:       return "Cursor"
        case .kiro:         return "Kiro"
        case .brew:         return "Homebrew"
        case .btop:         return "btop"
        case .top:          return "top"
        case .htop:         return "htop"
        case .docker:       return "Docker"
        case .npm:          return "npm"
        case .yarn:         return "Yarn"
        case .make:         return "Make"
        case .cargo:        return "Cargo"
        case .go:           return "Go"
        case .python:       return "Python"
        case .pip:          return "pip"
        case .shellCommand: return "Shell"
        case .unknown:      return "Unknown"
        }
    }

    var isAIAgent: Bool {
        switch self {
        case .claudeCode, .codex, .openCode, .gemini, .cline,
             .goose, .amp, .aider, .cursor, .kiro:
            return true
        default:
            return false
        }
    }

    var isShellTask: Bool {
        !isAIAgent && self != .unknown
    }

    // MARK: - AI Agent detection from terminal content

    // Ordered by specificity to avoid false matches (e.g., "opencode" before "code")
    private static let detectionPatterns: [(pattern: String, type: AgentType)] = [
        ("opencode", .openCode),
        ("claude", .claudeCode),
        ("codex", .codex),
        ("gemini", .gemini),
        ("cline", .cline),
        ("goose", .goose),
        ("aider", .aider),
        ("cursor", .cursor),
        ("kiro", .kiro),
        ("amp ", .amp),
    ]

    /// Detect agent type from lowercased terminal content (for AI agents)
    static func detect(fromLowercased content: String) -> AgentType {
        for (pattern, type) in detectionPatterns {
            if content.contains(pattern) {
                return type
            }
        }
        return .unknown
    }

    // MARK: - Shell command detection from command line

    private static let commandMap: [String: AgentType] = [
        "brew": .brew, "btop": .btop, "top": .top, "htop": .htop,
        "docker": .docker, "npm": .npm, "npx": .npm,
        "yarn": .yarn, "make": .make, "cargo": .cargo, "go": .go,
        "python": .python, "python3": .python,
        "pip": .pip, "pip3": .pip,
    ]

    /// Detect shell task type from a command line string.
    /// Handles full paths (/usr/local/bin/brew) and env prefixes (ENV=val make).
    static func detect(fromCommand command: String) -> AgentType {
        let tokens = command.split(separator: " ", maxSplits: 10)
        guard !tokens.isEmpty else { return .unknown }

        // Skip leading KEY=VALUE environment variable assignments
        for token in tokens {
            let str = String(token)
            if str.contains("=") && !str.hasPrefix("=") {
                continue
            }
            // Extract basename if it's a full path
            let name = (str as NSString).lastPathComponent.lowercased()
            if let type = commandMap[name] {
                return type
            }
            // First non-env token that doesn't match → generic shell
            return .shellCommand
        }
        return .unknown
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/AgentTypeTests 2>&1 | tail -3`
Expected: Test Suite 'AgentTypeTests' passed

- [ ] **Step 5: Commit**

```bash
git add Sources/Core/AgentType.swift tests/AgentTypeTests.swift
git commit -m "feat: add shell task cases and detect(fromCommand:) to AgentType"
```

---

### Task 3: Extend OSC133Parser to extract cmdline

**Files:**
- Modify: `Sources/Status/OSC133Parser.swift`
- Test: `tests/OSC133ParserTests.swift`

- [ ] **Step 1: Write failing tests for cmdline parsing**

Add to `tests/OSC133ParserTests.swift`, before the `ShellStateTests` class:

```swift
// MARK: - Command Line Parsing

func testPreExec_WithCmdline() {
    let data = Data([0x1b, 0x5d] + "133;C;cmdline=brew install ffmpeg".utf8 + [0x07])
    let markers = parser.feed(data)
    XCTAssertEqual(markers.count, 1)
    XCTAssertEqual(markers[0].kind, .preExec)
    XCTAssertEqual(markers[0].commandLine, "brew install ffmpeg")
}

func testPreExec_WithCmdlineUrl() {
    let data = Data([0x1b, 0x5d] + "133;C;cmdline_url=brew%20install%20ffmpeg".utf8 + [0x07])
    let markers = parser.feed(data)
    XCTAssertEqual(markers.count, 1)
    XCTAssertEqual(markers[0].commandLine, "brew install ffmpeg")
}

func testPreExec_NoCmdline() {
    let data = Data([0x1b, 0x5d] + "133;C".utf8 + [0x07])
    let markers = parser.feed(data)
    XCTAssertEqual(markers.count, 1)
    XCTAssertNil(markers[0].commandLine)
}

func testPreExec_EmptyCmdline() {
    let data = Data([0x1b, 0x5d] + "133;C;cmdline=".utf8 + [0x07])
    let markers = parser.feed(data)
    XCTAssertEqual(markers.count, 1)
    XCTAssertEqual(markers[0].commandLine, "")
}

func testPreExec_CmdlineUrlEncoded() {
    let data = Data([0x1b, 0x5d] + "133;C;cmdline_url=echo%20hello%3bworld".utf8 + [0x07])
    let markers = parser.feed(data)
    XCTAssertEqual(markers.count, 1)
    XCTAssertEqual(markers[0].commandLine, "echo hello;world")
}

func testLongCmdline_WithinBuffer() {
    // A command longer than the old 256 limit but under 1024
    let longCmd = "docker run --rm -v /path/to/dir:/app -e FOO=bar -e BAZ=qux --name my-container-name-that-is-very-long ubuntu:22.04 bash -c 'echo hello world && sleep 100 && echo done done done done done done done done done done done done done done done'"
    let data = Data([0x1b, 0x5d] + "133;C;cmdline=\(longCmd)".utf8 + [0x07])
    let markers = parser.feed(data)
    XCTAssertEqual(markers.count, 1)
    XCTAssertEqual(markers[0].commandLine, longCmd)
}
```

Add to `ShellStateTests`:

```swift
func testLastCommandLineFromPreExec() {
    let state = ShellState()
    state.addMarker(ParsedMarker(kind: .preExec, exitCode: nil, commandLine: "brew install ffmpeg"))
    XCTAssertEqual(state.lastCommandLine, "brew install ffmpeg")
    XCTAssertEqual(state.phase, .running)
}

func testLastCommandLinePersistsAcrossCycles() {
    let state = ShellState()
    state.addMarker(ParsedMarker(kind: .preExec, exitCode: nil, commandLine: "brew install"))
    state.addMarker(ParsedMarker(kind: .postExec, exitCode: 0, commandLine: nil))
    state.addMarker(ParsedMarker(kind: .promptStart, exitCode: nil, commandLine: nil))
    // commandLine persists until next preExec
    XCTAssertEqual(state.lastCommandLine, "brew install")
}

func testResetClearsCommandLine() {
    let state = ShellState()
    state.addMarker(ParsedMarker(kind: .preExec, exitCode: nil, commandLine: "brew install"))
    state.reset()
    XCTAssertNil(state.lastCommandLine)
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/OSC133ParserTests 2>&1 | grep -E "(error:|FAIL)" | head -5`
Expected: Compilation errors — `commandLine` property doesn't exist on `ParsedMarker`.

- [ ] **Step 3: Update ParsedMarker, OSC133Parser, and ShellState**

In `Sources/Status/OSC133Parser.swift`:

Update `ParsedMarker` to add `commandLine` with a default of `nil` (minimizes blast radius on existing callers):
```swift
struct ParsedMarker {
    let kind: MarkerKind
    let exitCode: UInt8?
    let commandLine: String? = nil   // from cmdline or cmdline_url parameter
}
```

**Wait** — Swift doesn't allow stored property defaults in structs that also use memberwise init with specific order. Instead, add `commandLine` as a parameter with default:

```swift
struct ParsedMarker {
    let kind: MarkerKind
    let exitCode: UInt8?
    let commandLine: String?
}
```

Increase buffer limit from 256 to 1024 (in the `inOSC` case of `feed(_:)`):
```swift
if oscBuffer.count > 1024 {
```

Replace the `parseOSCPayload` method:
```swift
/// Parse "133;X", "133;X;exitcode=N", or "133;C;cmdline=..." payload
private func parseOSCPayload(_ buffer: [UInt8]) -> ParsedMarker? {
    guard let str = String(bytes: buffer, encoding: .utf8) else { return nil }
    guard str.hasPrefix("133;") else { return nil }

    let remainder = String(str.dropFirst(4)) // after "133;"
    guard let kindChar = remainder.first else { return nil }

    let kind: MarkerKind
    switch kindChar {
    case "A": kind = .promptStart
    case "B": kind = .promptEnd
    case "C": kind = .preExec
    case "D": kind = .postExec
    default: return nil
    }

    let afterKind = String(remainder.dropFirst()) // after the kind char
    var exitCode: UInt8? = nil
    var commandLine: String? = nil

    if afterKind.hasPrefix(";") {
        let paramStr = String(afterKind.dropFirst()) // after ";"

        // Handle key=value pairs (may have multiple separated by ";")
        for part in paramStr.split(separator: ";") {
            let p = String(part)
            if p.hasPrefix("cmdline_url=") {
                let encoded = String(p.dropFirst(12))
                commandLine = encoded.removingPercentEncoding ?? encoded
            } else if p.hasPrefix("cmdline=") {
                commandLine = String(p.dropFirst(8))
            } else if p.hasPrefix("exitcode=") {
                exitCode = UInt8(String(p.dropFirst(9)))
            } else if kind == .postExec {
                exitCode = UInt8(p)
            }
        }
    }

    return ParsedMarker(kind: kind, exitCode: exitCode, commandLine: commandLine)
}
```

Update `ShellState` to track `lastCommandLine`:
```swift
class ShellState {
    private(set) var phase: ShellPhase = .output
    private(set) var lastExitCode: UInt8? = nil
    private(set) var lastCommandLine: String? = nil

    var phaseInfo: ShellPhaseInfo {
        ShellPhaseInfo(phase: phase, lastExitCode: lastExitCode)
    }

    func addMarker(_ marker: ParsedMarker) {
        switch marker.kind {
        case .promptStart:
            phase = .prompt
        case .promptEnd:
            phase = .input
        case .preExec:
            phase = .running
            if let cmd = marker.commandLine {
                lastCommandLine = cmd
            }
        case .postExec:
            phase = .output
            if let code = marker.exitCode {
                lastExitCode = code
            }
        }
    }

    func reset() {
        phase = .output
        lastExitCode = nil
        lastCommandLine = nil
    }
}
```

- [ ] **Step 4: Fix existing tests that construct ParsedMarker**

In `tests/OSC133ParserTests.swift`, update ALL `ParsedMarker(kind:, exitCode:)` calls in `ShellStateTests` to include `commandLine: nil`:

```swift
// testPhaseTransitions:
state.addMarker(ParsedMarker(kind: .promptStart, exitCode: nil, commandLine: nil))
state.addMarker(ParsedMarker(kind: .promptEnd, exitCode: nil, commandLine: nil))
state.addMarker(ParsedMarker(kind: .preExec, exitCode: nil, commandLine: nil))
state.addMarker(ParsedMarker(kind: .postExec, exitCode: 0, commandLine: nil))

// testExitCodePersists:
state.addMarker(ParsedMarker(kind: .postExec, exitCode: 42, commandLine: nil))
state.addMarker(ParsedMarker(kind: .promptStart, exitCode: nil, commandLine: nil))

// testReset:
state.addMarker(ParsedMarker(kind: .preExec, exitCode: nil, commandLine: nil))
```

Also check `tests/StatusDetectorTests.swift` — it constructs `ShellPhaseInfo` directly (not `ParsedMarker`), so it should not need changes.

- [ ] **Step 5: Run all tests**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/OSC133ParserTests -only-testing:amuxTests/ShellStateTests 2>&1 | tail -3`
Expected: All tests pass

- [ ] **Step 6: Commit**

```bash
git add Sources/Status/OSC133Parser.swift tests/OSC133ParserTests.swift
git commit -m "feat: extend OSC133Parser to extract cmdline from Phase C"
```

---

### Task 4: Atomic rekey — AgentHead, AgentInfo, and ALL consumers

This task changes the primary key from worktree path to terminal ID across the entire codebase in a single atomic commit. This prevents compilation deadlocks — all API changes and consumer updates happen together.

**Files:**
- Modify: `Sources/Core/AgentInfo.swift`
- Modify: `Sources/Core/AgentHead.swift`
- Modify: `Sources/App/MainWindowController.swift` (lines ~507, ~780, ~783, ~800, ~893, ~946, ~1118, ~1123)
- Modify: `Sources/Status/StatusPublisher.swift` (lines ~14-26, ~39-50, ~100-130)
- Modify: `Sources/UI/Dashboard/DashboardViewController.swift` (line ~16)
- Test: `tests/AgentHeadTests.swift`

- [ ] **Step 1: Write updated AgentHeadTests first**

Replace `tests/AgentHeadTests.swift` with the test file that tests the new terminal-ID-based API. Key changes from current tests:

- `registerTestAgent` returns `String` (terminal ID from `surface.id`)
- `unregister(terminalID:)` instead of `unregister(worktreePath:)`
- `updateStatus(terminalID:...)` instead of `updateStatus(worktreePath:...)`
- New tests for `updateDetection` (replaces `updateAgentType`)
- New test for `agent(forWorktree:)` convenience method
- New tests for type upgrade rules (shell→AI allowed, AI→shell blocked)
- `reorder(paths:)` still accepts worktree paths (config persistence)

Full test file:

```swift
import XCTest
@testable import amux

final class AgentHeadTests: XCTestCase {

    private var testSurfaces: [String: TerminalSurface] = [:]

    @discardableResult
    private func registerTestAgent(
        path: String, branch: String = "main", project: String = "TestProject",
        startedAt: Date? = nil
    ) -> String {
        let surface = TerminalSurface()
        testSurfaces[path] = surface
        AgentHead.shared.register(
            surface: surface, worktreePath: path, branch: branch,
            project: project, startedAt: startedAt
        )
        return surface.id
    }

    override func setUp() {
        super.setUp()
        for agent in AgentHead.shared.allAgents() {
            AgentHead.shared.unregister(terminalID: agent.id)
        }
        testSurfaces.removeAll()
    }

    override func tearDown() {
        for agent in AgentHead.shared.allAgents() {
            AgentHead.shared.unregister(terminalID: agent.id)
        }
        testSurfaces.removeAll()
        super.tearDown()
    }

    // MARK: - Registration

    func testRegisterAndQuery() {
        let tid = registerTestAgent(path: "/tmp/repo/main", project: "MyProject")
        let agents = AgentHead.shared.allAgents()
        XCTAssertEqual(agents.count, 1)
        XCTAssertEqual(agents[0].id, tid)
        XCTAssertEqual(agents[0].worktreePath, "/tmp/repo/main")
        XCTAssertEqual(agents[0].branch, "main")
        XCTAssertEqual(agents[0].project, "MyProject")
        XCTAssertEqual(agents[0].agentType, .unknown)
        XCTAssertEqual(agents[0].status, .unknown)
    }

    func testUnregister() {
        let tid = registerTestAgent(path: "/tmp/repo/main")
        AgentHead.shared.unregister(terminalID: tid)
        XCTAssertEqual(AgentHead.shared.allAgents().count, 0)
        XCTAssertNil(AgentHead.shared.agent(for: tid))
    }

    func testAgentForWorktreePath() {
        let tid = registerTestAgent(path: "/tmp/repo/main")
        let agent = AgentHead.shared.agent(forWorktree: "/tmp/repo/main")
        XCTAssertEqual(agent?.id, tid)
    }

    func testUnregisterClearsWorktreeIndex() {
        let tid = registerTestAgent(path: "/tmp/repo/main")
        AgentHead.shared.unregister(terminalID: tid)
        XCTAssertNil(AgentHead.shared.agent(forWorktree: "/tmp/repo/main"))
    }

    // MARK: - Status Updates

    func testUpdateStatus() {
        let tid = registerTestAgent(path: "/tmp/repo/main")
        AgentHead.shared.updateStatus(terminalID: tid, status: .running,
                                       lastMessage: "Editing file.swift", roundDuration: 30.0)
        let agent = AgentHead.shared.agent(for: tid)
        XCTAssertEqual(agent?.status, .running)
        XCTAssertEqual(agent?.lastMessage, "Editing file.swift")
        XCTAssertEqual(agent?.roundDuration, 30.0)
    }

    func testUpdateStatusForUnknownID() {
        AgentHead.shared.updateStatus(terminalID: "nonexistent", status: .running,
                                       lastMessage: "test", roundDuration: 0)
        XCTAssertNil(AgentHead.shared.agent(for: "nonexistent"))
    }

    // MARK: - Detection (Type + CommandLine)

    func testUpdateDetection_SetsType() {
        let tid = registerTestAgent(path: "/tmp/repo/main")
        AgentHead.shared.updateDetection(terminalID: tid, commandLine: nil, agentType: .claudeCode)
        XCTAssertEqual(AgentHead.shared.agent(for: tid)?.agentType, .claudeCode)
    }

    func testUpdateDetection_SetsCommandLine() {
        let tid = registerTestAgent(path: "/tmp/repo/main")
        AgentHead.shared.updateDetection(terminalID: tid, commandLine: "brew install ffmpeg", agentType: .brew)
        let agent = AgentHead.shared.agent(for: tid)
        XCTAssertEqual(agent?.commandLine, "brew install ffmpeg")
        XCTAssertEqual(agent?.agentType, .brew)
    }

    func testUpdateDetection_AIAgentCannotDemoteToShell() {
        let tid = registerTestAgent(path: "/tmp/repo/main")
        AgentHead.shared.updateDetection(terminalID: tid, commandLine: nil, agentType: .claudeCode)
        AgentHead.shared.updateDetection(terminalID: tid, commandLine: "brew install", agentType: .brew)
        XCTAssertEqual(AgentHead.shared.agent(for: tid)?.agentType, .claudeCode)
        XCTAssertEqual(AgentHead.shared.agent(for: tid)?.commandLine, "brew install")
    }

    func testUpdateDetection_ShellCanUpgradeToAIAgent() {
        let tid = registerTestAgent(path: "/tmp/repo/main")
        AgentHead.shared.updateDetection(terminalID: tid, commandLine: "brew install", agentType: .brew)
        AgentHead.shared.updateDetection(terminalID: tid, commandLine: nil, agentType: .claudeCode)
        XCTAssertEqual(AgentHead.shared.agent(for: tid)?.agentType, .claudeCode)
    }

    func testUpdateDetection_AIAgentCanSwitchToOtherAIAgent() {
        let tid = registerTestAgent(path: "/tmp/repo/main")
        AgentHead.shared.updateDetection(terminalID: tid, commandLine: nil, agentType: .claudeCode)
        AgentHead.shared.updateDetection(terminalID: tid, commandLine: nil, agentType: .codex)
        XCTAssertEqual(AgentHead.shared.agent(for: tid)?.agentType, .codex)
    }

    func testUpdateDetection_IgnoresUnknownType() {
        let tid = registerTestAgent(path: "/tmp/repo/main")
        AgentHead.shared.updateDetection(terminalID: tid, commandLine: nil, agentType: .claudeCode)
        AgentHead.shared.updateDetection(terminalID: tid, commandLine: nil, agentType: .unknown)
        XCTAssertEqual(AgentHead.shared.agent(for: tid)?.agentType, .claudeCode)
    }

    // MARK: - Ordering

    func testAllAgentsPreservesInsertionOrder() {
        let tidA = registerTestAgent(path: "/a", branch: "a")
        let tidB = registerTestAgent(path: "/b", branch: "b")
        let tidC = registerTestAgent(path: "/c", branch: "c")
        XCTAssertEqual(AgentHead.shared.allAgents().map { $0.id }, [tidA, tidB, tidC])
    }

    func testReorderByWorktreePaths() {
        let tidA = registerTestAgent(path: "/a", branch: "a")
        let tidB = registerTestAgent(path: "/b", branch: "b")
        let tidC = registerTestAgent(path: "/c", branch: "c")
        AgentHead.shared.reorder(paths: ["/c", "/a", "/b"])
        XCTAssertEqual(AgentHead.shared.allAgents().map { $0.id }, [tidC, tidA, tidB])
    }

    // MARK: - Project Filtering

    func testAgentsForProject() {
        registerTestAgent(path: "/repo1/main", branch: "main", project: "Repo1")
        registerTestAgent(path: "/repo2/main", branch: "main", project: "Repo2")
        registerTestAgent(path: "/repo1/feature", branch: "feature", project: "Repo1")
        let repo1Agents = AgentHead.shared.agentsForProject("Repo1")
        XCTAssertEqual(repo1Agents.count, 2)
        XCTAssertTrue(repo1Agents.allSatisfy { $0.project == "Repo1" })
    }

    // MARK: - Total Duration

    func testTotalDurationComputedFromStartedAt() {
        let tid = registerTestAgent(path: "/tmp/repo/main", startedAt: Date().addingTimeInterval(-300))
        let agent = AgentHead.shared.agent(for: tid)!
        XCTAssertGreaterThan(agent.totalDuration, 299)
        XCTAssertLessThan(agent.totalDuration, 302)
    }

    func testTotalDurationZeroWhenNoStartedAt() {
        let tid = registerTestAgent(path: "/tmp/repo/main", startedAt: nil)
        XCTAssertEqual(AgentHead.shared.agent(for: tid)!.totalDuration, 0)
    }
}
```

- [ ] **Step 2: Update AgentInfo.swift**

Replace `Sources/Core/AgentInfo.swift` — add `worktreePath` and `commandLine` fields, change `id` semantics:

```swift
import Foundation

struct AgentInfo {
    let id: String                     // terminal ID (TerminalSurface.id)
    let worktreePath: String           // associated worktree path
    var agentType: AgentType           // detected type (AI agent or shell command)
    let project: String                // repo display name
    let branch: String                 // git branch
    var status: AgentStatus            // current status
    var lastMessage: String            // latest message
    var commandLine: String?           // current command (from OSC 133 cmdline or text matching)
    var roundDuration: TimeInterval    // seconds in current running round
    let startedAt: Date?               // for computing totalDuration live
    weak var surface: TerminalSurface? // weak ref, MainWindowController owns
    var channel: AgentChannel?         // communication channel (strong ref, AgentHead owns)
    var taskProgress: TaskProgress     // current task progress

    var totalDuration: TimeInterval {
        guard let startedAt else { return 0 }
        return Date().timeIntervalSince(startedAt)
    }
}

struct TaskProgress {
    var totalTasks: Int = 0
    var completedTasks: Int = 0
    var currentTask: String?
    var isActive: Bool { totalTasks > 0 }
    var summary: String {
        guard isActive else { return "" }
        return "\(completedTasks)/\(totalTasks)"
    }
    var percentage: Double {
        guard totalTasks > 0 else { return 0 }
        return Double(completedTasks) / Double(totalTasks)
    }
}
```

- [ ] **Step 3: Update AgentHead.swift**

Replace `Sources/Core/AgentHead.swift` with the full new implementation (see spec for complete API). Key changes:
- `agents` dict keyed by terminal ID
- `orderedIDs` replaces `orderedPaths`
- `worktreeIndex: [String: String]` reverse index (worktreePath → terminalID)
- `register(surface:worktreePath:...)` — surface is first param
- `unregister(terminalID:)` — takes terminal ID
- `updateStatus(terminalID:...)` — takes terminal ID
- `updateDetection(terminalID:commandLine:agentType:)` — replaces `updateAgentType`
- `updateTaskProgress(terminalID:...)` — takes terminal ID
- `agent(forWorktree:)` — convenience lookup via reverse index
- `reorder(paths:)` — still accepts worktree paths, maps internally
- `handleWebhookEvent` — uses `worktreeIndex` for lookup

(Full implementation was provided in the original plan — copy the complete AgentHead.swift from the earlier Task 4.)

- [ ] **Step 4: Update MainWindowController.swift**

Apply all of these changes:

**register call (~line 780):**
```swift
// Old: AgentHead.shared.register(worktreePath: info.path, ...)
// New:
AgentHead.shared.register(surface: surface, worktreePath: info.path, branch: info.branch,
                          project: proj, startedAt: started, tmuxSessionName: sessionName)
```

**unregister calls (~lines 893, 946):**
```swift
// Old: AgentHead.shared.unregister(worktreePath: info.path)
// New:
if let agent = AgentHead.shared.agent(forWorktree: info.path) {
    AgentHead.shared.unregister(terminalID: agent.id)
}
```

**buildAgentDisplayInfos (~line 507):** `agent.id` is now terminal ID. This flows through to `AgentDisplayInfo.id`. No code change needed here — the `id` just has a different value now.

**dashboardDidReorderCards (~line 1118):**
```swift
// Old: config.cardOrder = order  (order contained worktree paths)
// New: map terminal IDs back to worktree paths
let paths = order.compactMap { AgentHead.shared.agent(for: $0)?.worktreePath }
config.cardOrder = paths
```

**dashboardDidRequestDeleteWorktree (~line 1123):** This receives `path: String` from `AgentDisplayInfo.id`, which is now a terminal ID. Need to look up the worktree path:
```swift
// Old: used path directly as worktree path
// New: look up the actual worktree path
guard let agent = AgentHead.shared.agent(for: path) else { return }
let worktreePath = agent.worktreePath
// Use worktreePath in the rest of the handler instead of path
```

- [ ] **Step 5: Update StatusPublisher.swift**

The internal dictionaries (`trackers`, `surfaces`, `lastMessages`, `runningStartTimes`, `lastViewportHashes`) need to be re-keyed from worktree path to terminal ID. The `start(surfaces:)` method receives `[String: TerminalSurface]` keyed by worktree path — internally convert to terminal ID keys:

```swift
func start(surfaces: [String: TerminalSurface]) {
    // Convert worktree-path-keyed input to terminal-ID-keyed internal storage
    self.surfaces = [:]
    for (_, surface) in surfaces {
        self.surfaces[surface.id] = surface
    }
    stop()
    // Create trackers for each terminal
    for tid in self.surfaces.keys {
        if trackers[tid] == nil {
            trackers[tid] = DebouncedStatusTracker()
        }
    }
    // ... rest of start() logic
}
```

In `pollAll()`, change the AgentHead calls:
```swift
// Old:
AgentHead.shared.updateAgentType(worktreePath: path, type: agentType)
AgentHead.shared.updateStatus(worktreePath: path, status: detected, ...)

// New:
AgentHead.shared.updateDetection(terminalID: terminalID, commandLine: nil, agentType: agentType)
AgentHead.shared.updateStatus(terminalID: terminalID, status: detected, ...)
```

Also update `StatusPublisherDelegate` if it passes worktree paths — check the delegate method signature and callers.

`webhookProvider.updateWorktrees(...)` still needs worktree paths. Either maintain a terminalID→worktreePath mapping in StatusPublisher, or have the provider look them up via AgentHead.

- [ ] **Step 6: Update DashboardViewController.swift**

Update the `AgentDisplayInfo` comment:
```swift
struct AgentDisplayInfo {
    let id: String          // terminal ID (from TerminalSurface.id)
    // ... rest unchanged
}
```

No functional code changes needed — `id` is used for identity comparison and selection tracking throughout, which works the same with terminal IDs.

- [ ] **Step 7: Build the entire project**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -5`
Expected: BUILD SUCCEEDED

- [ ] **Step 8: Run ALL tests**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test 2>&1 | tail -10`
Expected: All test suites pass

- [ ] **Step 9: Commit**

```bash
git add Sources/Core/AgentHead.swift Sources/Core/AgentInfo.swift Sources/App/MainWindowController.swift Sources/Status/StatusPublisher.swift Sources/UI/Dashboard/DashboardViewController.swift tests/AgentHeadTests.swift
git commit -m "feat: rekey AgentHead from worktree path to terminal ID

Atomic change across AgentHead, AgentInfo, and all consumers.
Primary key is now TerminalSurface.id (UUID). Adds worktreeIndex
reverse map, updateDetection() with type upgrade rules, and
agent(forWorktree:) convenience query."
```

---

### Task 5: Final verification

**Files:** None (verification only)

- [ ] **Step 1: Clean build**

Run: `xcodebuild -project amux.xcodeproj -scheme amux clean && xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -5`
Expected: BUILD SUCCEEDED

- [ ] **Step 2: Run all tests**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test 2>&1 | tail -10`
Expected: All test suites pass

- [ ] **Step 3: Verify no remaining worktreePath in AgentHead method parameters**

Run: `grep -n "worktreePath:" Sources/Core/AgentHead.swift`
Expected: Only appears in `register(surface:worktreePath:...)` parameter and `info.worktreePath` field access, NOT as the key parameter in update/query methods.

- [ ] **Step 4: Verify TerminalSurface has id**

Run: `grep -n "let id:" Sources/Terminal/TerminalSurface.swift`
Expected: Shows the `let id: String = UUID().uuidString` line.

- [ ] **Step 5: Verify AgentType has shell cases**

Run: `grep -c "case " Sources/Core/AgentType.swift`
Expected: 24 cases (10 AI + 12 shell + shellCommand + unknown)
