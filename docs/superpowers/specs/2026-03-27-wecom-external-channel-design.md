# WeCom External Channel — 企业微信智能机器人集成

**Goal:** 通过企业微信智能机器人长连接（WebSocket），实现 AMUX 与外部用户的双向通信。用户可在企业微信中查看 agent 状态、下达指令、新增 idea；agent 关键状态变化时自动推送通知。

**Architecture:** 引入 `ExternalChannel` 协议层，将外部通道与现有 `AgentChannel`（agent 通信）分离。`WeComBotChannel` 作为第一个具体实现，通过 WebSocket 连接企业微信服务端。所有外部消息统一经过 `AgentHead` 中枢处理，Channel 本身是"哑管道"，不直接与 agent 交互。

**Tech Stack:** Swift 5.10, URLSessionWebSocketTask (macOS 14+), XCTest

---

## Core Principle: Hub-and-Spoke

```
External Channels (WeCom, future: Slack, Web...)
        │
        │  InboundMessage / OutboundMessage (统一格式)
        ▼
   AgentHead (中枢)
        │
        │  AgentChannel.sendCommand() / readOutput()
        ▼
   Agents (Claude Code, OpenCode, ...)
```

- **ExternalChannel** = 外部世界 ↔ AgentHead（用户消息收发）
- **AgentChannel** = AgentHead ↔ Agent（命令下发、输出读取）
- AgentHead 是唯一决策点：理解输入、选择 agent、汇总回复
- Phase 1: 规则路由（斜杠命令）；Phase 2（未来）: LLM 意图理解

---

## Unified Message Protocol

### InboundMessage（Channel → AgentHead）

```swift
struct InboundMessage {
    let channelId: String        // "wecom-bot-1"
    let senderId: String         // userid
    let senderName: String       // 显示名
    let chatId: String?          // 群聊 ID（私聊为 nil）
    let chatType: ChatType       // .direct / .group
    let content: String          // 文本内容
    let messageId: String        // 原始消息 ID（去重）
    let timestamp: Date
    let replyTo: String?         // 引用消息 ID
    let metadata: [String: Any]? // 平台特有数据
}

enum ChatType { case direct, group }
```

### OutboundMessage（AgentHead → Channel）

```swift
struct OutboundMessage {
    let channelId: String
    let targetChatId: String?
    let targetUserId: String?
    let content: String
    let format: MessageFormat    // .text / .markdown / .templateCard
    let replyToMessageId: String?
    let streaming: Bool
    let streamId: String?
}

enum MessageFormat { case text, markdown, templateCard }
```

---

## ExternalChannel Protocol

```swift
protocol ExternalChannel: AnyObject {
    var channelId: String { get }
    var channelType: ExternalChannelType { get }
    var gatewayState: GatewayState { get }

    /// Channel 收到消息后回调给 AgentHead
    var onMessage: ((InboundMessage) -> Void)? { get set }

    /// AgentHead 发消息出去
    func send(_ message: OutboundMessage)

    /// 连接管理
    func connect()
    func disconnect()
}

enum ExternalChannelType: String {
    case wecom
    // 未来: .slack, .discord, .web
}
```

---

## Command System

AgentHead 层的斜杠命令解析（不在 Channel 层）：

```swift
struct ParsedCommand {
    let command: String      // "idea", "status", "send"...
    let args: String
    let rawMessage: InboundMessage
}

enum CommandParser {
    /// "/idea 做一个登录页" → ParsedCommand(command: "idea", args: "做一个登录页")
    /// "hello" → nil
    static func parse(_ message: InboundMessage) -> ParsedCommand?
}
```

### Supported Commands (Phase 1)

| Command | Action | Response |
|---------|--------|----------|
| `/idea <描述>` | `IdeaStore.shared.add()` | "Idea added: ..." |
| `/status` | 汇总 `AgentHead.allAgents()` | Markdown 表格 |
| `/list` | 列出 agents | project / branch / status |
| `/send <project> <command>` | `AgentHead.sendCommand()` | "Command sent to ..." |
| `/help` | 列出命令 | 命令列表 |
| 非 `/` 开头 | Phase 1: 提示用 /help | Phase 2: LLM 理解 |

---

## AgentHead Changes

```swift
class AgentHead {
    // === 现有不动 ===
    // agents, channels, worktreeIndex, orderedIDs
    // register(), unregister(), updateStatus(), ...

    // === 新增 ===
    private var externalChannels: [String: ExternalChannel] = [:]

    func registerChannel(_ channel: ExternalChannel)
    func unregisterChannel(_ channelId: String)
    func handleInbound(_ message: InboundMessage)
    func pushToChannel(_ channelId: String, message: OutboundMessage)
    func broadcast(_ content: String, format: MessageFormat = .text)
}
```

### Status Notifications

`updateStatus()` 新增钩子：当 agent 状态变为 `.waiting` 或 `.error` 时，通过已注册的外部通道广播通知：

```
[ProjectName] ◐ Waiting: Waiting for input
[ProjectName] ✕ Error: Build failed
```

---

## WeComBotChannel

### WeCom WebSocket Protocol

| Item | Detail |
|------|--------|
| Endpoint | `wss://openws.work.weixin.qq.com` |
| Auth | 连接后发送 `aibot_subscribe` 帧（botId + secret） |
| Heartbeat | 自动 ping/pong |
| Reconnect | 指数退避: 1s → 2s → 4s → ... → 30s max |

### Commands (帧类型)

| Command | Direction | Purpose |
|---------|-----------|---------|
| `aibot_subscribe` | outbound | 认证订阅 |
| `aibot_msg_callback` | inbound | 用户消息 |
| `aibot_event_callback` | inbound | 事件（进入聊天等） |
| `aibot_respond_msg` | outbound | 被动回复（支持流式） |
| `aibot_send_msg` | outbound | 主动推送 |

### Frame Format

```json
{
  "cmd": "aibot_msg_callback",
  "headers": { "req_id": "xxx" },
  "body": {
    "msgid": "...",
    "aibotid": "...",
    "chatid": "...",
    "chattype": "group",
    "from": { "userid": "..." },
    "msgtype": "text",
    "text": { "content": "@Bot hello" }
  }
}
```

### WeComFrameParser (Pure Functions)

```swift
enum WeComFrameParser {
    static func parse(_ data: Data) -> WeComFrame?
    static func toInboundMessage(_ frame: WeComFrame, channelId: String) -> InboundMessage?
    static func toSendFrame(_ message: OutboundMessage, botId: String) -> Data?
    static func toRespondFrame(_ message: OutboundMessage, reqId: String) -> Data?
    static func subscribeFrame(botId: String, secret: String) -> Data?
}

struct WeComFrame {
    let cmd: String
    let reqId: String
    let body: [String: Any]
}
```

### WeComBotConfig

```swift
struct WeComBotConfig: Codable, Equatable {
    let botId: String                          // "aib-xxx"
    let secret: String                         // 认证密钥
    var name: String?                          // 显示名称
    var autoConnect: Bool?                     // 默认 true
    var maxReconnectInterval: TimeInterval?    // 默认 30s
}
```

### Connection Lifecycle

Uses `GatewayStateMachine` from RemoteChannel plan (Task 1):

```
disconnected → connecting → connected → error
                    ↑                      │
                    └──────────────────────┘
                        (reconnect)
```

---

## Config Persistence

`Config.swift` 新增 `wecomBot` 字段：

```json
{
  "wecomBot": {
    "botId": "aib-xxxx",
    "secret": "your-secret",
    "name": "AMUX助手",
    "autoConnect": true
  }
}
```

Uses `decodeIfPresent` for backward compatibility.

---

## File Structure

| Action | File | Responsibility |
|--------|------|----------------|
| Reuse | `Sources/Core/GatewayState.swift` | Connection state machine (from RemoteChannel Task 1) |
| Create | `Sources/Core/ExternalChannel.swift` | `ExternalChannel` protocol + `InboundMessage` / `OutboundMessage` / `ChatType` / `MessageFormat` |
| Create | `Sources/Core/CommandParser.swift` | Slash command parsing |
| Create | `Sources/Core/WeComBotChannel.swift` | WeCom WebSocket client, conforms to `ExternalChannel` |
| Create | `Sources/Core/WeComBotConfig.swift` | WeCom bot configuration |
| Create | `Sources/Core/WeComFrameParser.swift` | WeCom frame parsing (pure functions) |
| Modify | `Sources/Core/AgentHead.swift` | Add externalChannels, handleInbound(), status notifications |
| Modify | `Sources/Core/Config.swift` | Add `wecomBot` field |
| Create | `tests/WeComFrameParserTests.swift` | Frame parsing tests |
| Create | `tests/CommandParserTests.swift` | Command parsing tests |
| Create | `tests/AgentHeadExternalTests.swift` | External channel integration tests |

---

## Out of Scope

- LLM intent understanding (Phase 2)
- Media messages (image/file/voice) — text only for now
- Template card interactions — markdown replies only
- Streaming responses — simple full-message replies first
- Other external channels (Slack, Discord, Web)
- Welcome message on `enter_chat` event
