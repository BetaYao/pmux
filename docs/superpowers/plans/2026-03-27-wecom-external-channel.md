# WeCom External Channel — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enable AMUX to communicate bidirectionally with users via WeCom (企业微信) smart bot WebSocket, with AgentHead as the central hub routing all external messages.

**Architecture:** A new `ExternalChannel` protocol defines the interface between external platforms and AgentHead. `WeComBotChannel` is the first concrete implementation, connecting to `wss://openws.work.weixin.qq.com` via `URLSessionWebSocketTask`. AgentHead gains `handleInbound()` for processing external messages through a slash-command system, and `broadcast()` for pushing agent status notifications outward. `GatewayStateMachine` (from RemoteChannel plan) manages connection lifecycle.

**Tech Stack:** Swift 5.10, URLSessionWebSocketTask (macOS 14+), XCTest

---

## File Structure

| Action | File | Responsibility |
|--------|------|----------------|
| Create | `Sources/Core/GatewayState.swift` | Connection lifecycle enum + state machine |
| Create | `Sources/Core/ExternalChannel.swift` | `ExternalChannel` protocol + `InboundMessage` / `OutboundMessage` |
| Create | `Sources/Core/CommandParser.swift` | Slash command parsing |
| Create | `Sources/Core/WeComBotConfig.swift` | WeCom bot configuration (Codable) |
| Create | `Sources/Core/WeComFrameParser.swift` | WeCom JSON frame parsing (pure functions) |
| Create | `Sources/Core/WeComBotChannel.swift` | WebSocket client, conforms to `ExternalChannel` |
| Modify | `Sources/Core/AgentHead.swift` | Add external channel management + inbound handling + broadcast |
| Modify | `Sources/Core/Config.swift` | Add `wecomBot` field |
| Create | `tests/GatewayStateTests.swift` | State machine tests |
| Create | `tests/CommandParserTests.swift` | Command parsing tests |
| Create | `tests/WeComFrameParserTests.swift` | Frame parsing tests |
| Create | `tests/AgentHeadExternalTests.swift` | External channel integration tests |

---

### Task 1: GatewayState — Connection Lifecycle State Machine

**Files:**
- Create: `Sources/Core/GatewayState.swift`
- Create: `tests/GatewayStateTests.swift`

- [ ] **Step 1: Write failing tests for GatewayState**

```swift
// tests/GatewayStateTests.swift
import XCTest
@testable import amux

final class GatewayStateTests: XCTestCase {

    func testInitialStateIsDisconnected() {
        let sm = GatewayStateMachine()
        XCTAssertEqual(sm.state, .disconnected)
    }

    func testDisconnectedToConnecting() {
        var sm = GatewayStateMachine()
        let changed = sm.transition(to: .connecting)
        XCTAssertTrue(changed)
        XCTAssertEqual(sm.state, .connecting)
    }

    func testConnectingToConnected() {
        var sm = GatewayStateMachine()
        sm.transition(to: .connecting)
        let changed = sm.transition(to: .connected)
        XCTAssertTrue(changed)
        XCTAssertEqual(sm.state, .connected)
    }

    func testConnectingToError() {
        var sm = GatewayStateMachine()
        sm.transition(to: .connecting)
        let changed = sm.transition(to: .error("timeout"))
        XCTAssertTrue(changed)
        if case .error(let msg) = sm.state {
            XCTAssertEqual(msg, "timeout")
        } else {
            XCTFail("Expected error state")
        }
    }

    func testErrorToConnecting() {
        var sm = GatewayStateMachine()
        sm.transition(to: .connecting)
        sm.transition(to: .error("fail"))
        let changed = sm.transition(to: .connecting)
        XCTAssertTrue(changed)
        XCTAssertEqual(sm.state, .connecting)
    }

    func testConnectedToDisconnected() {
        var sm = GatewayStateMachine()
        sm.transition(to: .connecting)
        sm.transition(to: .connected)
        let changed = sm.transition(to: .disconnected)
        XCTAssertTrue(changed)
        XCTAssertEqual(sm.state, .disconnected)
    }

    func testSameStateReturnsFalse() {
        var sm = GatewayStateMachine()
        let changed = sm.transition(to: .disconnected)
        XCTAssertFalse(changed)
    }

    func testCannotGoDirectlyToConnected() {
        var sm = GatewayStateMachine()
        let changed = sm.transition(to: .connected)
        XCTAssertFalse(changed)
        XCTAssertEqual(sm.state, .disconnected)
    }

    func testIsConnectedProperty() {
        var sm = GatewayStateMachine()
        XCTAssertFalse(sm.isConnected)
        sm.transition(to: .connecting)
        sm.transition(to: .connected)
        XCTAssertTrue(sm.isConnected)
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/GatewayStateTests 2>&1 | tail -5`
Expected: Compilation error — `GatewayStateMachine` not defined

- [ ] **Step 3: Implement GatewayState**

```swift
// Sources/Core/GatewayState.swift
import Foundation

enum GatewayState: Equatable {
    case disconnected
    case connecting
    case connected
    case error(String)

    static func == (lhs: GatewayState, rhs: GatewayState) -> Bool {
        switch (lhs, rhs) {
        case (.disconnected, .disconnected),
             (.connecting, .connecting),
             (.connected, .connected):
            return true
        case (.error(let a), .error(let b)):
            return a == b
        default:
            return false
        }
    }
}

struct GatewayStateMachine {
    private(set) var state: GatewayState = .disconnected

    var isConnected: Bool { state == .connected }

    /// Transition to a new state. Returns true if the state changed.
    /// Valid transitions:
    ///   disconnected → connecting
    ///   connecting   → connected | error | disconnected
    ///   connected    → disconnected | error
    ///   error        → connecting | disconnected
    @discardableResult
    mutating func transition(to newState: GatewayState) -> Bool {
        guard newState != state else { return false }

        let valid: Bool
        switch (state, newState) {
        case (.disconnected, .connecting):
            valid = true
        case (.connecting, .connected),
             (.connecting, .disconnected),
             (.connecting, .error):
            valid = true
        case (.connected, .disconnected),
             (.connected, .error):
            valid = true
        case (.error, .connecting),
             (.error, .disconnected):
            valid = true
        default:
            valid = false
        }

        guard valid else { return false }
        state = newState
        return true
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/GatewayStateTests 2>&1 | tail -5`
Expected: All tests PASS

- [ ] **Step 5: Commit**

```bash
git add Sources/Core/GatewayState.swift tests/GatewayStateTests.swift
git commit -m "feat: add GatewayState enum and state machine for connection lifecycle"
```

---

### Task 2: ExternalChannel Protocol + Message Types

**Files:**
- Create: `Sources/Core/ExternalChannel.swift`

- [ ] **Step 1: Create ExternalChannel protocol and message types**

```swift
// Sources/Core/ExternalChannel.swift
import Foundation

// MARK: - Message Types

enum ChatType: String {
    case direct
    case group
}

enum MessageFormat: String {
    case text
    case markdown
    case templateCard
}

struct InboundMessage {
    let channelId: String
    let senderId: String
    let senderName: String
    let chatId: String?
    let chatType: ChatType
    let content: String
    let messageId: String
    let timestamp: Date
    let replyTo: String?
    let metadata: [String: Any]?
}

struct OutboundMessage {
    let channelId: String
    let targetChatId: String?
    let targetUserId: String?
    let content: String
    let format: MessageFormat
    let replyToMessageId: String?
    let streaming: Bool
    let streamId: String?

    init(channelId: String, targetChatId: String? = nil, targetUserId: String? = nil,
         content: String, format: MessageFormat = .text,
         replyToMessageId: String? = nil, streaming: Bool = false, streamId: String? = nil) {
        self.channelId = channelId
        self.targetChatId = targetChatId
        self.targetUserId = targetUserId
        self.content = content
        self.format = format
        self.replyToMessageId = replyToMessageId
        self.streaming = streaming
        self.streamId = streamId
    }
}

// MARK: - ExternalChannel Protocol

enum ExternalChannelType: String {
    case wecom
}

protocol ExternalChannel: AnyObject {
    var channelId: String { get }
    var channelType: ExternalChannelType { get }
    var gatewayState: GatewayState { get }

    /// Called by the channel when a message arrives from the external platform
    var onMessage: ((InboundMessage) -> Void)? { get set }

    /// Send a message out to the external platform
    func send(_ message: OutboundMessage)

    /// Connection management
    func connect()
    func disconnect()
}
```

- [ ] **Step 2: Verify build**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -3`
Expected: BUILD SUCCEEDED

- [ ] **Step 3: Commit**

```bash
git add Sources/Core/ExternalChannel.swift
git commit -m "feat: add ExternalChannel protocol and unified message types"
```

---

### Task 3: CommandParser — Slash Command Parsing

**Files:**
- Create: `Sources/Core/CommandParser.swift`
- Create: `tests/CommandParserTests.swift`

- [ ] **Step 1: Write failing tests for CommandParser**

```swift
// tests/CommandParserTests.swift
import XCTest
@testable import amux

final class CommandParserTests: XCTestCase {

    private func makeMessage(content: String) -> InboundMessage {
        InboundMessage(
            channelId: "test-ch",
            senderId: "user1",
            senderName: "Test User",
            chatId: nil,
            chatType: .direct,
            content: content,
            messageId: "msg-1",
            timestamp: Date(),
            replyTo: nil,
            metadata: nil
        )
    }

    func testParseIdeaCommand() {
        let msg = makeMessage(content: "/idea 做一个登录页")
        let cmd = CommandParser.parse(msg)
        XCTAssertNotNil(cmd)
        XCTAssertEqual(cmd?.command, "idea")
        XCTAssertEqual(cmd?.args, "做一个登录页")
    }

    func testParseStatusCommand() {
        let msg = makeMessage(content: "/status")
        let cmd = CommandParser.parse(msg)
        XCTAssertNotNil(cmd)
        XCTAssertEqual(cmd?.command, "status")
        XCTAssertEqual(cmd?.args, "")
    }

    func testParseListCommand() {
        let msg = makeMessage(content: "/list")
        let cmd = CommandParser.parse(msg)
        XCTAssertNotNil(cmd)
        XCTAssertEqual(cmd?.command, "list")
    }

    func testParseSendCommand() {
        let msg = makeMessage(content: "/send my-project run tests")
        let cmd = CommandParser.parse(msg)
        XCTAssertNotNil(cmd)
        XCTAssertEqual(cmd?.command, "send")
        XCTAssertEqual(cmd?.args, "my-project run tests")
    }

    func testParseHelpCommand() {
        let msg = makeMessage(content: "/help")
        let cmd = CommandParser.parse(msg)
        XCTAssertNotNil(cmd)
        XCTAssertEqual(cmd?.command, "help")
    }

    func testNonCommandReturnsNil() {
        let msg = makeMessage(content: "hello world")
        let cmd = CommandParser.parse(msg)
        XCTAssertNil(cmd)
    }

    func testEmptyMessageReturnsNil() {
        let msg = makeMessage(content: "")
        let cmd = CommandParser.parse(msg)
        XCTAssertNil(cmd)
    }

    func testSlashOnlyReturnsNil() {
        let msg = makeMessage(content: "/")
        let cmd = CommandParser.parse(msg)
        XCTAssertNil(cmd)
    }

    func testCommandIsCaseInsensitive() {
        let msg = makeMessage(content: "/STATUS")
        let cmd = CommandParser.parse(msg)
        XCTAssertNotNil(cmd)
        XCTAssertEqual(cmd?.command, "status")
    }

    func testCommandWithLeadingWhitespace() {
        let msg = makeMessage(content: "  /idea  trim this  ")
        let cmd = CommandParser.parse(msg)
        XCTAssertNotNil(cmd)
        XCTAssertEqual(cmd?.command, "idea")
        XCTAssertEqual(cmd?.args, "trim this")
    }

    func testPreservesRawMessage() {
        let msg = makeMessage(content: "/idea test")
        let cmd = CommandParser.parse(msg)
        XCTAssertEqual(cmd?.rawMessage.messageId, "msg-1")
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/CommandParserTests 2>&1 | tail -5`
Expected: Compilation error — `CommandParser` not defined

- [ ] **Step 3: Implement CommandParser**

```swift
// Sources/Core/CommandParser.swift
import Foundation

struct ParsedCommand {
    let command: String
    let args: String
    let rawMessage: InboundMessage
}

enum CommandParser {
    /// Parse a slash command from an inbound message.
    /// "/idea 做一个登录页" → ParsedCommand(command: "idea", args: "做一个登录页")
    /// "hello" → nil (not a command)
    static func parse(_ message: InboundMessage) -> ParsedCommand? {
        let trimmed = message.content.trimmingCharacters(in: .whitespaces)
        guard trimmed.hasPrefix("/") else { return nil }

        let withoutSlash = String(trimmed.dropFirst())
        guard !withoutSlash.isEmpty else { return nil }

        let parts = withoutSlash.split(separator: " ", maxSplits: 1)
        let command = String(parts[0]).lowercased()
        let args = parts.count > 1
            ? String(parts[1]).trimmingCharacters(in: .whitespaces)
            : ""

        return ParsedCommand(command: command, args: args, rawMessage: message)
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/CommandParserTests 2>&1 | tail -5`
Expected: All tests PASS

- [ ] **Step 5: Commit**

```bash
git add Sources/Core/CommandParser.swift tests/CommandParserTests.swift
git commit -m "feat: add CommandParser for slash command parsing"
```

---

### Task 4: AgentHead — External Channel Management + Inbound Handling

**Files:**
- Modify: `Sources/Core/AgentHead.swift:11-351`
- Create: `tests/AgentHeadExternalTests.swift`

- [ ] **Step 1: Write failing tests for external channel management**

```swift
// tests/AgentHeadExternalTests.swift
import XCTest
@testable import amux

/// Mock ExternalChannel for testing
final class MockExternalChannel: ExternalChannel {
    let channelId: String
    let channelType: ExternalChannelType = .wecom
    var gatewayState: GatewayState = .disconnected
    var onMessage: ((InboundMessage) -> Void)?
    var sentMessages: [OutboundMessage] = []
    var connectCalled = false
    var disconnectCalled = false

    init(channelId: String = "mock-ch") {
        self.channelId = channelId
    }

    func send(_ message: OutboundMessage) {
        sentMessages.append(message)
    }

    func connect() { connectCalled = true }
    func disconnect() { disconnectCalled = true }
}

final class AgentHeadExternalTests: XCTestCase {

    override func setUp() {
        super.setUp()
        // Clean up external channels
        AgentHead.shared.unregisterAllExternalChannels()
    }

    override func tearDown() {
        AgentHead.shared.unregisterAllExternalChannels()
        super.tearDown()
    }

    // MARK: - Channel Registration

    func testRegisterExternalChannel() {
        let ch = MockExternalChannel()
        AgentHead.shared.registerChannel(ch)

        XCTAssertNotNil(ch.onMessage, "onMessage callback should be wired up")
    }

    func testUnregisterExternalChannel() {
        let ch = MockExternalChannel()
        AgentHead.shared.registerChannel(ch)
        AgentHead.shared.unregisterChannel("mock-ch")

        XCTAssertTrue(ch.disconnectCalled)
    }

    // MARK: - Command Handling

    func testHelpCommand() {
        let ch = MockExternalChannel()
        AgentHead.shared.registerChannel(ch)

        let msg = InboundMessage(
            channelId: "mock-ch", senderId: "u1", senderName: "Test",
            chatId: nil, chatType: .direct, content: "/help",
            messageId: "m1", timestamp: Date(), replyTo: nil, metadata: nil
        )
        AgentHead.shared.handleInbound(msg)

        XCTAssertEqual(ch.sentMessages.count, 1)
        XCTAssertTrue(ch.sentMessages[0].content.contains("/idea"))
        XCTAssertTrue(ch.sentMessages[0].content.contains("/status"))
    }

    func testStatusCommandNoAgents() {
        let ch = MockExternalChannel()
        AgentHead.shared.registerChannel(ch)

        let msg = InboundMessage(
            channelId: "mock-ch", senderId: "u1", senderName: "Test",
            chatId: nil, chatType: .direct, content: "/status",
            messageId: "m1", timestamp: Date(), replyTo: nil, metadata: nil
        )
        AgentHead.shared.handleInbound(msg)

        XCTAssertEqual(ch.sentMessages.count, 1)
        XCTAssertTrue(ch.sentMessages[0].content.lowercased().contains("no agent")
                      || ch.sentMessages[0].content.contains("没有"))
    }

    func testListCommandNoAgents() {
        let ch = MockExternalChannel()
        AgentHead.shared.registerChannel(ch)

        let msg = InboundMessage(
            channelId: "mock-ch", senderId: "u1", senderName: "Test",
            chatId: nil, chatType: .direct, content: "/list",
            messageId: "m1", timestamp: Date(), replyTo: nil, metadata: nil
        )
        AgentHead.shared.handleInbound(msg)

        XCTAssertEqual(ch.sentMessages.count, 1)
    }

    func testIdeaCommand() {
        let ch = MockExternalChannel()
        AgentHead.shared.registerChannel(ch)

        // Use a test IdeaStore to avoid polluting real data
        let msg = InboundMessage(
            channelId: "mock-ch", senderId: "u1", senderName: "Test",
            chatId: nil, chatType: .direct, content: "/idea 做一个暗色模式",
            messageId: "m1", timestamp: Date(), replyTo: nil, metadata: nil
        )
        AgentHead.shared.handleInbound(msg)

        XCTAssertEqual(ch.sentMessages.count, 1)
        XCTAssertTrue(ch.sentMessages[0].content.contains("做一个暗色模式"))
    }

    func testNonCommandShowsHelp() {
        let ch = MockExternalChannel()
        AgentHead.shared.registerChannel(ch)

        let msg = InboundMessage(
            channelId: "mock-ch", senderId: "u1", senderName: "Test",
            chatId: nil, chatType: .direct, content: "hello",
            messageId: "m1", timestamp: Date(), replyTo: nil, metadata: nil
        )
        AgentHead.shared.handleInbound(msg)

        XCTAssertEqual(ch.sentMessages.count, 1)
        XCTAssertTrue(ch.sentMessages[0].content.contains("/help"))
    }

    func testUnknownCommandShowsHelp() {
        let ch = MockExternalChannel()
        AgentHead.shared.registerChannel(ch)

        let msg = InboundMessage(
            channelId: "mock-ch", senderId: "u1", senderName: "Test",
            chatId: nil, chatType: .direct, content: "/foobar",
            messageId: "m1", timestamp: Date(), replyTo: nil, metadata: nil
        )
        AgentHead.shared.handleInbound(msg)

        XCTAssertEqual(ch.sentMessages.count, 1)
        XCTAssertTrue(ch.sentMessages[0].content.contains("/help"))
    }

    // MARK: - Broadcast

    func testBroadcastSendsToAllChannels() {
        let ch1 = MockExternalChannel(channelId: "ch-1")
        let ch2 = MockExternalChannel(channelId: "ch-2")
        AgentHead.shared.registerChannel(ch1)
        AgentHead.shared.registerChannel(ch2)

        AgentHead.shared.broadcast("test alert")

        XCTAssertEqual(ch1.sentMessages.count, 1)
        XCTAssertEqual(ch1.sentMessages[0].content, "test alert")
        XCTAssertEqual(ch2.sentMessages.count, 1)
        XCTAssertEqual(ch2.sentMessages[0].content, "test alert")
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/AgentHeadExternalTests 2>&1 | tail -5`
Expected: Compilation error — `registerChannel`, `handleInbound`, `broadcast` not defined

- [ ] **Step 3: Add external channel management to AgentHead**

Add the following to `Sources/Core/AgentHead.swift`, after the existing `private let lock = NSLock()` (line 23) and before `private init()` (line 25):

```swift
    /// External channels (WeCom, future: Slack, etc.) — keyed by channelId
    private var externalChannels: [String: ExternalChannel] = [:]
```

Add the following after the `updateTodoFromWebhook` method (after line 351), before the closing `}` of the class:

```swift
    // MARK: - External Channel Management

    /// Register an external channel (WeCom, Slack, etc.)
    func registerChannel(_ channel: ExternalChannel) {
        lock.lock()
        externalChannels[channel.channelId] = channel
        lock.unlock()

        channel.onMessage = { [weak self] message in
            self?.handleInbound(message)
        }
    }

    /// Unregister and disconnect an external channel
    func unregisterChannel(_ channelId: String) {
        lock.lock()
        let channel = externalChannels.removeValue(forKey: channelId)
        lock.unlock()

        channel?.disconnect()
    }

    /// Remove all external channels (for testing)
    func unregisterAllExternalChannels() {
        lock.lock()
        let channels = externalChannels
        externalChannels.removeAll()
        lock.unlock()

        for (_, channel) in channels {
            channel.disconnect()
        }
    }

    // MARK: - Inbound Message Handling

    /// Process an inbound message from an external channel.
    /// Phase 1: slash command routing.
    /// Phase 2 (future): LLM intent understanding.
    func handleInbound(_ message: InboundMessage) {
        if let cmd = CommandParser.parse(message) {
            executeCommand(cmd)
        } else {
            reply(to: message, content: "请使用 /help 查看支持的命令")
        }
    }

    private func executeCommand(_ cmd: ParsedCommand) {
        switch cmd.command {
        case "help":
            let help = """
            **AMUX 命令列表**
            `/idea <描述>` — 新增一个 idea
            `/status` — 查看所有 agent 状态
            `/list` — 列出所有 agent
            `/send <project> <command>` — 给指定 agent 下指令
            `/help` — 显示帮助
            """
            reply(to: cmd.rawMessage, content: help)

        case "idea":
            guard !cmd.args.isEmpty else {
                reply(to: cmd.rawMessage, content: "用法: `/idea <描述>`")
                return
            }
            let item = IdeaStore.shared.add(
                text: cmd.args,
                project: "external",
                source: "wecom:\(cmd.rawMessage.senderId)",
                tags: []
            )
            reply(to: cmd.rawMessage, content: "Idea added: \(item.text)")

        case "status":
            let agents = allAgents()
            if agents.isEmpty {
                reply(to: cmd.rawMessage, content: "No agents running.")
                return
            }
            var lines = ["**Agent Status**", ""]
            for a in agents {
                lines.append("\(a.status.icon) **\(a.project)** [\(a.branch)] — \(a.status.rawValue): \(a.lastMessage)")
            }
            reply(to: cmd.rawMessage, content: lines.joined(separator: "\n"), format: .markdown)

        case "list":
            let agents = allAgents()
            if agents.isEmpty {
                reply(to: cmd.rawMessage, content: "No agents registered.")
                return
            }
            var lines = ["**Agents**", ""]
            for a in agents {
                lines.append("- \(a.project) / \(a.branch) — \(a.status.rawValue)")
            }
            reply(to: cmd.rawMessage, content: lines.joined(separator: "\n"), format: .markdown)

        case "send":
            let parts = cmd.args.split(separator: " ", maxSplits: 1)
            guard parts.count == 2 else {
                reply(to: cmd.rawMessage, content: "用法: `/send <project> <command>`")
                return
            }
            let project = String(parts[0])
            let command = String(parts[1])
            let matched = agentsForProject(project)
            guard let target = matched.first else {
                reply(to: cmd.rawMessage, content: "未找到 project: \(project)")
                return
            }
            sendCommand(to: target.id, command: command)
            reply(to: cmd.rawMessage, content: "Command sent to \(target.project): \(command)")

        default:
            reply(to: cmd.rawMessage, content: "未知命令: /\(cmd.command)\n请使用 /help 查看支持的命令")
        }
    }

    /// Send a reply back through the same external channel
    private func reply(to message: InboundMessage, content: String, format: MessageFormat = .text) {
        let outbound = OutboundMessage(
            channelId: message.channelId,
            targetChatId: message.chatId,
            targetUserId: message.chatId == nil ? message.senderId : nil,
            content: content,
            format: format,
            replyToMessageId: message.messageId
        )
        pushToChannel(message.channelId, message: outbound)
    }

    /// Push a message to a specific external channel
    func pushToChannel(_ channelId: String, message: OutboundMessage) {
        lock.lock()
        let channel = externalChannels[channelId]
        lock.unlock()

        channel?.send(message)
    }

    /// Broadcast a message to all registered external channels
    func broadcast(_ content: String, format: MessageFormat = .text) {
        lock.lock()
        let channels = Array(externalChannels.values)
        lock.unlock()

        for channel in channels {
            let message = OutboundMessage(
                channelId: channel.channelId,
                content: content,
                format: format
            )
            channel.send(message)
        }
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/AgentHeadExternalTests 2>&1 | tail -5`
Expected: All tests PASS

- [ ] **Step 5: Run full test suite to check for regressions**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test 2>&1 | grep -E "(Test Suite|Passed|Failed)" | tail -10`
Expected: All existing tests still PASS

- [ ] **Step 6: Commit**

```bash
git add Sources/Core/AgentHead.swift tests/AgentHeadExternalTests.swift
git commit -m "feat: add external channel management and slash command handling to AgentHead"
```

---

### Task 5: AgentHead — Status Change Notifications

**Files:**
- Modify: `Sources/Core/AgentHead.swift:120-142`

- [ ] **Step 1: Add status notification broadcast to updateStatus**

In `Sources/Core/AgentHead.swift`, modify the `updateStatus` method. Replace the existing block at lines 120-142:

```swift
    func updateStatus(terminalID: String, status: AgentStatus,
                      lastMessage: String, roundDuration: TimeInterval,
                      tasks: [TaskItem] = []) {
        lock.lock()
        guard var info = agents[terminalID] else {
            lock.unlock()
            return
        }
        let previousStatus = info.status
        let changed = info.status != status || info.lastMessage != lastMessage
            || info.tasks.count != tasks.count
        info.status = status
        info.lastMessage = lastMessage
        info.roundDuration = roundDuration
        info.tasks = tasks
        agents[terminalID] = info
        let hasExternalChannels = !externalChannels.isEmpty
        lock.unlock()

        if changed {
            DispatchQueue.main.async { [weak self] in
                self?.delegate?.agentDidUpdate(info)
            }

            // Notify external channels on critical status transitions
            if hasExternalChannels && previousStatus != status
                && (status == .waiting || status == .error) {
                let text = "[\(info.project)] \(status.icon) \(status.rawValue): \(lastMessage)"
                broadcast(text, format: .markdown)
            }
        }
    }
```

- [ ] **Step 2: Run full test suite**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test 2>&1 | grep -E "(Test Suite|Passed|Failed)" | tail -10`
Expected: All tests PASS

- [ ] **Step 3: Commit**

```bash
git add Sources/Core/AgentHead.swift
git commit -m "feat: broadcast agent status changes to external channels"
```

---

### Task 6: WeComBotConfig + Config Persistence

**Files:**
- Create: `Sources/Core/WeComBotConfig.swift`
- Modify: `Sources/Core/Config.swift:3-17,21-38,40-57,59-81`

- [ ] **Step 1: Create WeComBotConfig**

```swift
// Sources/Core/WeComBotConfig.swift
import Foundation

struct WeComBotConfig: Codable, Equatable {
    let botId: String
    let secret: String
    var name: String?
    var autoConnect: Bool?
    var maxReconnectInterval: TimeInterval?

    var resolvedAutoConnect: Bool { autoConnect ?? true }
    var resolvedMaxReconnectInterval: TimeInterval { maxReconnectInterval ?? 30.0 }
    var resolvedName: String { name ?? "AMUX Bot" }

    enum CodingKeys: String, CodingKey {
        case botId = "bot_id"
        case secret
        case name
        case autoConnect = "auto_connect"
        case maxReconnectInterval = "max_reconnect_interval"
    }
}
```

- [ ] **Step 2: Add `wecomBot` field to Config**

In `Sources/Core/Config.swift`, add the field to the struct (after line 19, `var focusedPaneIds: [String: String]`):

```swift
    var wecomBot: WeComBotConfig?
```

Add the coding key (inside `CodingKeys` enum, after line 37, the `focusedPaneIds` case):

```swift
        case wecomBot = "wecom_bot"
```

Add to `init()` (after line 56, `focusedPaneIds = [:]`):

```swift
        wecomBot = nil
```

Add to `init(from decoder:)` (after line 80, the `focusedPaneIds` decode):

```swift
        wecomBot = try container.decodeIfPresent(WeComBotConfig.self, forKey: .wecomBot)
```

- [ ] **Step 3: Verify build**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -3`
Expected: BUILD SUCCEEDED

- [ ] **Step 4: Run existing ConfigTests to verify backward compatibility**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/ConfigTests 2>&1 | tail -5`
Expected: All tests PASS

- [ ] **Step 5: Commit**

```bash
git add Sources/Core/WeComBotConfig.swift Sources/Core/Config.swift
git commit -m "feat: add WeComBotConfig and persist in Config"
```

---

### Task 7: WeComFrameParser — WeCom Frame Parsing

**Files:**
- Create: `Sources/Core/WeComFrameParser.swift`
- Create: `tests/WeComFrameParserTests.swift`

- [ ] **Step 1: Write failing tests for WeComFrameParser**

```swift
// tests/WeComFrameParserTests.swift
import XCTest
@testable import amux

final class WeComFrameParserTests: XCTestCase {

    // MARK: - Parse Incoming Frames

    func testParseMessageCallback() {
        let json = """
        {
            "cmd": "aibot_msg_callback",
            "headers": { "req_id": "req-001" },
            "body": {
                "msgid": "msg-123",
                "aibotid": "aib-001",
                "chatid": "group-456",
                "chattype": "group",
                "from": { "userid": "matt", "name": "Matt" },
                "msgtype": "text",
                "text": { "content": "@Bot /status" }
            }
        }
        """.data(using: .utf8)!

        let frame = WeComFrameParser.parse(json)
        XCTAssertNotNil(frame)
        XCTAssertEqual(frame?.cmd, "aibot_msg_callback")
        XCTAssertEqual(frame?.reqId, "req-001")
    }

    func testParseEventCallback() {
        let json = """
        {
            "cmd": "aibot_event_callback",
            "headers": { "req_id": "req-002" },
            "body": {
                "event_type": "enter_chat",
                "chatid": "group-456"
            }
        }
        """.data(using: .utf8)!

        let frame = WeComFrameParser.parse(json)
        XCTAssertNotNil(frame)
        XCTAssertEqual(frame?.cmd, "aibot_event_callback")
    }

    func testParseInvalidJSON() {
        let json = "not json".data(using: .utf8)!
        XCTAssertNil(WeComFrameParser.parse(json))
    }

    func testParseMissingCmd() {
        let json = """
        { "headers": { "req_id": "r1" }, "body": {} }
        """.data(using: .utf8)!
        XCTAssertNil(WeComFrameParser.parse(json))
    }

    // MARK: - Frame → InboundMessage

    func testToInboundMessageText() {
        let json = """
        {
            "cmd": "aibot_msg_callback",
            "headers": { "req_id": "req-001" },
            "body": {
                "msgid": "msg-123",
                "aibotid": "aib-001",
                "chatid": "group-456",
                "chattype": "group",
                "from": { "userid": "matt", "name": "Matt" },
                "msgtype": "text",
                "text": { "content": "@Bot /idea new feature" }
            }
        }
        """.data(using: .utf8)!

        let frame = WeComFrameParser.parse(json)!
        let msg = WeComFrameParser.toInboundMessage(frame, channelId: "wecom-1")
        XCTAssertNotNil(msg)
        XCTAssertEqual(msg?.channelId, "wecom-1")
        XCTAssertEqual(msg?.senderId, "matt")
        XCTAssertEqual(msg?.senderName, "Matt")
        XCTAssertEqual(msg?.chatId, "group-456")
        XCTAssertEqual(msg?.chatType, .group)
        XCTAssertEqual(msg?.content, "/idea new feature")
        XCTAssertEqual(msg?.messageId, "msg-123")
    }

    func testToInboundMessageStripsMention() {
        let json = """
        {
            "cmd": "aibot_msg_callback",
            "headers": { "req_id": "req-001" },
            "body": {
                "msgid": "msg-123",
                "chattype": "group",
                "from": { "userid": "matt", "name": "Matt" },
                "msgtype": "text",
                "text": { "content": "@Bot hello world" }
            }
        }
        """.data(using: .utf8)!

        let frame = WeComFrameParser.parse(json)!
        let msg = WeComFrameParser.toInboundMessage(frame, channelId: "ch")
        // Should strip the @Bot mention prefix
        XCTAssertEqual(msg?.content, "hello world")
    }

    func testToInboundMessageDirectChat() {
        let json = """
        {
            "cmd": "aibot_msg_callback",
            "headers": { "req_id": "req-001" },
            "body": {
                "msgid": "msg-456",
                "chattype": "single",
                "from": { "userid": "matt", "name": "Matt" },
                "msgtype": "text",
                "text": { "content": "/help" }
            }
        }
        """.data(using: .utf8)!

        let frame = WeComFrameParser.parse(json)!
        let msg = WeComFrameParser.toInboundMessage(frame, channelId: "ch")
        XCTAssertNotNil(msg)
        XCTAssertEqual(msg?.chatType, .direct)
        XCTAssertNil(msg?.chatId)
    }

    func testNonMessageFrameReturnsNil() {
        let json = """
        {
            "cmd": "aibot_event_callback",
            "headers": { "req_id": "req-001" },
            "body": { "event_type": "enter_chat" }
        }
        """.data(using: .utf8)!

        let frame = WeComFrameParser.parse(json)!
        let msg = WeComFrameParser.toInboundMessage(frame, channelId: "ch")
        XCTAssertNil(msg)
    }

    // MARK: - OutboundMessage → Send Frame

    func testToSendFrame() {
        let outbound = OutboundMessage(
            channelId: "ch", targetChatId: "group-1",
            content: "Hello!", format: .text
        )
        let data = WeComFrameParser.toSendFrame(outbound, botId: "aib-001")
        XCTAssertNotNil(data)

        let json = try? JSONSerialization.jsonObject(with: data!) as? [String: Any]
        XCTAssertEqual(json?["cmd"] as? String, "aibot_send_msg")
        let body = json?["body"] as? [String: Any]
        XCTAssertEqual(body?["chatid"] as? String, "group-1")
    }

    func testToSendFrameMarkdown() {
        let outbound = OutboundMessage(
            channelId: "ch", targetChatId: "group-1",
            content: "**bold**", format: .markdown
        )
        let data = WeComFrameParser.toSendFrame(outbound, botId: "aib-001")
        XCTAssertNotNil(data)

        let json = try? JSONSerialization.jsonObject(with: data!) as? [String: Any]
        let body = json?["body"] as? [String: Any]
        XCTAssertEqual(body?["msgtype"] as? String, "markdown")
    }

    // MARK: - Subscribe Frame

    func testSubscribeFrame() {
        let data = WeComFrameParser.subscribeFrame(botId: "aib-001", secret: "s3cr3t")
        XCTAssertNotNil(data)

        let json = try? JSONSerialization.jsonObject(with: data!) as? [String: Any]
        XCTAssertEqual(json?["cmd"] as? String, "aibot_subscribe")
        let body = json?["body"] as? [String: Any]
        XCTAssertEqual(body?["bot_id"] as? String, "aib-001")
        XCTAssertEqual(body?["secret"] as? String, "s3cr3t")
    }

    // MARK: - Respond Frame

    func testToRespondFrame() {
        let outbound = OutboundMessage(
            channelId: "ch", content: "reply text", format: .markdown,
            replyToMessageId: "msg-1"
        )
        let data = WeComFrameParser.toRespondFrame(outbound, reqId: "req-001")
        XCTAssertNotNil(data)

        let json = try? JSONSerialization.jsonObject(with: data!) as? [String: Any]
        XCTAssertEqual(json?["cmd"] as? String, "aibot_respond_msg")
        let headers = json?["headers"] as? [String: Any]
        XCTAssertEqual(headers?["req_id"] as? String, "req-001")
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/WeComFrameParserTests 2>&1 | tail -5`
Expected: Compilation error — `WeComFrameParser` not defined

- [ ] **Step 3: Implement WeComFrameParser**

```swift
// Sources/Core/WeComFrameParser.swift
import Foundation

struct WeComFrame {
    let cmd: String
    let reqId: String
    let body: [String: Any]
}

enum WeComFrameParser {

    // MARK: - Parse Incoming

    /// Parse raw WebSocket data into a WeComFrame
    static func parse(_ data: Data) -> WeComFrame? {
        guard let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let cmd = json["cmd"] as? String else {
            return nil
        }
        let headers = json["headers"] as? [String: Any] ?? [:]
        let reqId = headers["req_id"] as? String ?? ""
        let body = json["body"] as? [String: Any] ?? [:]
        return WeComFrame(cmd: cmd, reqId: reqId, body: body)
    }

    /// Convert a message callback frame to an InboundMessage.
    /// Returns nil for non-message frames (events, heartbeats, etc.).
    static func toInboundMessage(_ frame: WeComFrame, channelId: String) -> InboundMessage? {
        guard frame.cmd == "aibot_msg_callback" else { return nil }

        let body = frame.body
        let msgtype = body["msgtype"] as? String ?? "text"
        guard msgtype == "text" else { return nil } // Phase 1: text only

        let from = body["from"] as? [String: Any] ?? [:]
        let textDict = body["text"] as? [String: Any] ?? [:]
        let rawContent = textDict["content"] as? String ?? ""
        let chatTypeStr = body["chattype"] as? String ?? "single"
        let isGroup = chatTypeStr == "group"

        // Strip @mention prefix in group messages
        let content = stripMention(rawContent)

        return InboundMessage(
            channelId: channelId,
            senderId: from["userid"] as? String ?? "",
            senderName: from["name"] as? String ?? from["userid"] as? String ?? "",
            chatId: isGroup ? body["chatid"] as? String : nil,
            chatType: isGroup ? .group : .direct,
            content: content,
            messageId: body["msgid"] as? String ?? UUID().uuidString,
            timestamp: Date(),
            replyTo: nil,
            metadata: body
        )
    }

    // MARK: - Build Outgoing

    /// Build an aibot_send_msg frame (proactive push)
    static func toSendFrame(_ message: OutboundMessage, botId: String) -> Data? {
        let isMarkdown = message.format == .markdown
        var body: [String: Any] = [
            "bot_id": botId,
            "msgtype": isMarkdown ? "markdown" : "text",
        ]

        if let chatId = message.targetChatId {
            body["chatid"] = chatId
        }
        if let userId = message.targetUserId {
            body["userid"] = userId
        }

        if isMarkdown {
            body["markdown"] = ["content": message.content]
        } else {
            body["text"] = ["content": message.content]
        }

        let frame: [String: Any] = [
            "cmd": "aibot_send_msg",
            "headers": ["req_id": UUID().uuidString],
            "body": body
        ]

        return try? JSONSerialization.data(withJSONObject: frame)
    }

    /// Build an aibot_respond_msg frame (passive reply)
    static func toRespondFrame(_ message: OutboundMessage, reqId: String) -> Data? {
        let isMarkdown = message.format == .markdown
        var body: [String: Any] = [
            "msgtype": isMarkdown ? "markdown" : "text",
        ]

        if isMarkdown {
            body["markdown"] = ["content": message.content]
        } else {
            body["text"] = ["content": message.content]
        }

        let frame: [String: Any] = [
            "cmd": "aibot_respond_msg",
            "headers": ["req_id": reqId],
            "body": body
        ]

        return try? JSONSerialization.data(withJSONObject: frame)
    }

    /// Build the aibot_subscribe authentication frame
    static func subscribeFrame(botId: String, secret: String) -> Data? {
        let frame: [String: Any] = [
            "cmd": "aibot_subscribe",
            "headers": ["req_id": UUID().uuidString],
            "body": [
                "bot_id": botId,
                "secret": secret
            ]
        ]
        return try? JSONSerialization.data(withJSONObject: frame)
    }

    // MARK: - Helpers

    /// Strip "@BotName " prefix from group message content
    private static func stripMention(_ content: String) -> String {
        let trimmed = content.trimmingCharacters(in: .whitespaces)
        // WeCom @mentions start with @ followed by bot name and a space
        if trimmed.hasPrefix("@") {
            if let spaceIndex = trimmed.firstIndex(of: " ") {
                let afterMention = trimmed[trimmed.index(after: spaceIndex)...]
                return String(afterMention).trimmingCharacters(in: .whitespaces)
            }
        }
        return trimmed
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/WeComFrameParserTests 2>&1 | tail -5`
Expected: All tests PASS

- [ ] **Step 5: Commit**

```bash
git add Sources/Core/WeComFrameParser.swift tests/WeComFrameParserTests.swift
git commit -m "feat: implement WeComFrameParser for WeCom protocol frame handling"
```

---

### Task 8: WeComBotChannel — WebSocket Client

**Files:**
- Create: `Sources/Core/WeComBotChannel.swift`

- [ ] **Step 1: Implement WeComBotChannel**

```swift
// Sources/Core/WeComBotChannel.swift
import Foundation

/// WeCom smart bot channel — connects to enterprise WeChat via WebSocket long connection.
/// Pure protocol adapter: receives WeCom frames → translates to InboundMessage → forwards to AgentHead.
/// Outbound: OutboundMessage → WeCom frame → sends via WebSocket.
class WeComBotChannel: ExternalChannel {
    let channelId: String
    let channelType: ExternalChannelType = .wecom
    var onMessage: ((InboundMessage) -> Void)?

    private let config: WeComBotConfig
    private var stateMachine = GatewayStateMachine()
    private(set) var gatewayState: GatewayState = .disconnected

    private var webSocket: URLSessionWebSocketTask?
    private var urlSession: URLSession?
    private var reconnectAttempt = 0
    private var reconnectTimer: Timer?

    /// Maps req_id → WeComFrame for passive replies (aibot_respond_msg needs original req_id)
    private var pendingReqIds: [String: String] = [:] // messageId → reqId
    private let lock = NSLock()

    /// Callback when gateway state changes
    var onStateChange: ((GatewayState) -> Void)?

    init(config: WeComBotConfig, channelId: String? = nil) {
        self.config = config
        self.channelId = channelId ?? "wecom-\(config.botId)"
    }

    deinit {
        disconnect()
    }

    // MARK: - ExternalChannel

    func connect() {
        guard stateMachine.transition(to: .connecting) else { return }
        gatewayState = stateMachine.state
        onStateChange?(gatewayState)
        reconnectAttempt = 0

        guard let url = URL(string: "wss://openws.work.weixin.qq.com") else {
            updateState(.error("Invalid WebSocket URL"))
            return
        }

        NSLog("[WeComBot] Connecting to \(url)")

        let session = URLSession(configuration: .default)
        urlSession = session
        let ws = session.webSocketTask(with: url)
        webSocket = ws
        ws.resume()

        // Send subscribe frame after connection
        authenticate()

        // Start listening
        receiveLoop()
    }

    func disconnect() {
        reconnectTimer?.invalidate()
        reconnectTimer = nil
        webSocket?.cancel(with: .normalClosure, reason: nil)
        webSocket = nil
        urlSession?.invalidateAndCancel()
        urlSession = nil
        updateState(.disconnected)
    }

    func send(_ message: OutboundMessage) {
        // Try passive reply first (if we have a pending req_id for this message)
        lock.lock()
        let reqId = pendingReqIds.removeValue(forKey: message.replyToMessageId ?? "")
        lock.unlock()

        let frameData: Data?
        if let reqId {
            frameData = WeComFrameParser.toRespondFrame(message, reqId: reqId)
        } else {
            frameData = WeComFrameParser.toSendFrame(message, botId: config.botId)
        }

        guard let data = frameData,
              let text = String(data: data, encoding: .utf8) else {
            NSLog("[WeComBot] Failed to build outbound frame")
            return
        }

        webSocket?.send(.string(text)) { error in
            if let error {
                NSLog("[WeComBot] Send error: \(error)")
            }
        }
    }

    // MARK: - Authentication

    private func authenticate() {
        guard let data = WeComFrameParser.subscribeFrame(botId: config.botId, secret: config.secret),
              let text = String(data: data, encoding: .utf8) else {
            updateState(.error("Failed to build subscribe frame"))
            return
        }

        webSocket?.send(.string(text)) { [weak self] error in
            if let error {
                NSLog("[WeComBot] Subscribe send error: \(error)")
                self?.updateState(.error(error.localizedDescription))
                return
            }
            NSLog("[WeComBot] Subscribe frame sent")
        }
    }

    // MARK: - Receive Loop

    private func receiveLoop() {
        webSocket?.receive { [weak self] result in
            guard let self else { return }

            switch result {
            case .success(let message):
                switch message {
                case .string(let text):
                    if let data = text.data(using: .utf8) {
                        self.handleFrame(data)
                    }
                case .data(let data):
                    self.handleFrame(data)
                @unknown default:
                    break
                }
                // Continue listening
                self.receiveLoop()

            case .failure(let error):
                // Don't treat cancellation as error
                if (error as NSError).code == 57 { return } // Socket not connected
                NSLog("[WeComBot] Receive error: \(error)")
                self.updateState(.error(error.localizedDescription))
                self.scheduleReconnect()
            }
        }
    }

    // MARK: - Frame Handling

    private func handleFrame(_ data: Data) {
        guard let frame = WeComFrameParser.parse(data) else { return }

        switch frame.cmd {
        case "aibot_msg_callback":
            // Mark connected on first message
            if stateMachine.state == .connecting {
                updateState(.connected)
            }

            guard let inbound = WeComFrameParser.toInboundMessage(frame, channelId: channelId) else { return }

            // Store req_id for passive reply
            lock.lock()
            pendingReqIds[inbound.messageId] = frame.reqId
            // Keep map bounded
            if pendingReqIds.count > 100 {
                let oldest = pendingReqIds.keys.prefix(50)
                for key in oldest { pendingReqIds.removeValue(forKey: key) }
            }
            lock.unlock()

            onMessage?(inbound)

        case "aibot_event_callback":
            if stateMachine.state == .connecting {
                updateState(.connected)
            }
            // Phase 1: log events but don't process (enter_chat, template_card_event)
            let eventType = frame.body["event_type"] as? String ?? "unknown"
            NSLog("[WeComBot] Event: \(eventType)")

        default:
            // Subscribe response, heartbeat, etc.
            if stateMachine.state == .connecting {
                updateState(.connected)
            }
            NSLog("[WeComBot] Frame: \(frame.cmd)")
        }
    }

    // MARK: - State & Reconnect

    private func updateState(_ newState: GatewayState) {
        guard stateMachine.transition(to: newState) else { return }
        gatewayState = stateMachine.state
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            self.onStateChange?(self.gatewayState)
        }
    }

    private func scheduleReconnect() {
        reconnectAttempt += 1
        let maxInterval = config.resolvedMaxReconnectInterval
        let delay = min(pow(2.0, Double(reconnectAttempt)), maxInterval)

        NSLog("[WeComBot] Scheduling reconnect in \(delay)s (attempt \(reconnectAttempt))")
        DispatchQueue.main.async { [weak self] in
            self?.reconnectTimer = Timer.scheduledTimer(withTimeInterval: delay, repeats: false) { [weak self] _ in
                self?.connect()
            }
        }
    }
}
```

- [ ] **Step 2: Verify build**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -3`
Expected: BUILD SUCCEEDED

- [ ] **Step 3: Commit**

```bash
git add Sources/Core/WeComBotChannel.swift
git commit -m "feat: implement WeComBotChannel with WebSocket long connection"
```

---

### Task 9: Wire Up — App Launch + Config Auto-Connect

**Files:**
- Modify: `Sources/App/AppDelegate.swift` (or wherever app initialization happens)

- [ ] **Step 1: Read AppDelegate to find initialization point**

Read `Sources/App/AppDelegate.swift` to find where initialization happens (look for `applicationDidFinishLaunching` or similar).

- [ ] **Step 2: Add WeCom auto-connect on launch**

Add the following after existing initialization code in `applicationDidFinishLaunching`:

```swift
        // Auto-connect WeCom bot if configured
        let config = Config.load()
        if let wecomConfig = config.wecomBot, wecomConfig.resolvedAutoConnect {
            let channel = WeComBotChannel(config: wecomConfig)
            AgentHead.shared.registerChannel(channel)
            channel.connect()
            NSLog("[App] WeCom bot auto-connecting: \(wecomConfig.resolvedName)")
        }
```

- [ ] **Step 3: Verify build**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -3`
Expected: BUILD SUCCEEDED

- [ ] **Step 4: Run full test suite**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test 2>&1 | grep -E "(Test Suite|Passed|Failed)" | tail -10`
Expected: All tests PASS

- [ ] **Step 5: Commit**

```bash
git add Sources/App/AppDelegate.swift
git commit -m "feat: auto-connect WeCom bot on app launch from config"
```

---

## Summary

| Task | What it builds | Tests |
|------|---------------|-------|
| 1 | `GatewayStateMachine` — connection lifecycle | GatewayStateTests (9 tests) |
| 2 | `ExternalChannel` protocol + message types | Build verification |
| 3 | `CommandParser` — slash command parsing | CommandParserTests (11 tests) |
| 4 | `AgentHead` — external channel + inbound handling | AgentHeadExternalTests (8 tests) |
| 5 | `AgentHead` — status change broadcast | Regression suite |
| 6 | `WeComBotConfig` + Config persistence | ConfigTests (backward compat) |
| 7 | `WeComFrameParser` — WeCom protocol frames | WeComFrameParserTests (12 tests) |
| 8 | `WeComBotChannel` — WebSocket client | Build verification |
| 9 | App launch auto-connect | Build + regression suite |

**Not included (future work):**
- LLM intent understanding (Phase 2 — replace `handleInbound` internals)
- Media messages (image/file/voice)
- Template card interactions
- Streaming responses
- Other external channels (Slack, Discord, Web)
- Welcome message on `enter_chat` event
