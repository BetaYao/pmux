# AgentHead 统一终端任务管理设计

## 背景

当前 `AgentHead` 只管理 AI coding agent（Claude Code、Codex、Gemini CLI 等）。但用户在 pmux 终端里也会运行传统 Shell 命令（`brew install`、`btop`、`make build`、长脚本等），这些同样是需要监控的长时任务。

本设计将 AgentHead 的定位从"AI Agent 管理器"扩展为"统一终端任务管理器"——AI agent 和 Shell 命令在同一个数据模型下管理，共享相同的状态查询和展示机制。

## 设计决策

### 1. 主键从 worktree path 改为 terminal ID

**现状：** `AgentHead` 用 worktree path 作为每个 agent 的唯一标识。

**问题：** 终端是实际被监控的实体，worktree path 只是终端的上下文。当前的模型把两者耦合在一起。

**决策：** terminal ID（TerminalSurface 上的 UUID）作为主键，worktree path 降级为普通属性。

```
旧模型: worktreePath → AgentInfo → surface (weak ref)
新模型: terminalID   → AgentInfo → (surface, worktreePath, ...)
```

注：当前 pmux 中每个终端都关联一个 worktree，`worktreePath` 始终有值，不需要变为 optional。

**迁移影响：** 这是一个破坏性的主键变更。当前 `AgentInfo.id` 就是 worktree path，所有消费者通过 `agent.id` 获取路径。改为 terminal ID 后，所有读取 `agent.id` 并期望得到路径的地方都会静默破坏。需要逐一排查以下消费者中的 `agent.id` 引用：

- `MainWindowController`（surfaces 字典、repoVCs 字典、所有 worktree path 传递）
- `StatusPublisher`（5 个字典全部按 worktree path 索引：`trackers`、`surfaces`、`lastMessages`、`runningStartTimes`、`lastViewportHashes`）
- `DashboardViewController`（`AgentDisplayInfo.id` 传递）
- `WebhookStatusProvider`（worktree path 匹配）
- `NotificationManager` / `NotificationHistory`

`StatusPublisher` 改动最大——其内部 5 个字典需要全部从 worktree path 键改为 terminal ID 键，或者在 poll 时通过 surface 对象获取对应的 terminal ID。

### 2. 统一类型枚举

**决策：** AI agent 和 shell 命令在 `AgentType` 中平级共存。

```swift
enum AgentType: String, Codable, CaseIterable {
    // AI Agents
    case claudeCode, codex, openCode, gemini, cline, goose, amp, aider, cursor, kiro
    // Shell tasks
    case brew, btop, top, htop, docker, npm, yarn, make, cargo, go, python, pip
    case shellCommand   // 通用 shell 命令兜底
    case unknown

    var isAIAgent: Bool {
        switch self {
        case .claudeCode, .codex, .openCode, .gemini, .cline, .goose, .amp, .aider, .cursor, .kiro:
            return true
        default:
            return false
        }
    }

    var isShellTask: Bool {
        !isAIAgent && self != .unknown
    }
}
```

### 3. 一个终端 = 一个任务槽位

一个终端始终对应一个任务槽位。当用户执行新命令时，该终端的类型、状态、commandLine 随之更新，不保留历史记录。这与现有 AI agent 模型一致。

### 4. 检测方式：OSC 133 + 文本匹配

**Shell 命令的状态和命令识别策略：**

- **命令文本获取：** 优先从 OSC 133 Phase C 的 `cmdline` / `cmdline_url` 参数获取。如果不可用（shell integration 未发送 cmdline），fallback 到从终端文本匹配 prompt 行之后的内容。
- **运行状态：** 通过 OSC 133 Phase 判断（C = running, D = finished, A/B = idle）。
- **退出码：** 从 OSC 133 Phase D 的 `exitcode` 参数获取。

注：Ghostty 底层解析器完整支持 `cmdline` 和 `cmdline_url`，但当前 Ghostty 的 shell integration 脚本（zsh/fish/bash）在 preexec 中只发送 `\e]133;C\a`，不附带 cmdline。因此 fallback 到文本匹配在短期内是必要的。

## 数据模型

### TerminalSurface 新增

```swift
class TerminalSurface {
    let id: String = UUID().uuidString   // 终端唯一标识
    // ... 现有属性不变
}
```

### AgentInfo 改造

```swift
struct AgentInfo {
    let id: String                     // terminal ID（主键，来自 TerminalSurface.id）
    let worktreePath: String           // 关联的 worktree 路径
    var agentType: AgentType           // 统一类型（AI agent 或 shell 命令）
    let project: String                // repo 显示名
    let branch: String                 // git 分支
    var status: AgentStatus            // 当前状态
    var lastMessage: String            // 最近消息
    var commandLine: String?           // 当前执行的命令（来自 OSC 133 cmdline 或文本匹配）
    var roundDuration: TimeInterval    // 当前 running 轮次的时长
    let startedAt: Date?               // 用于计算 totalDuration
    weak var surface: TerminalSurface? // 弱引用，MainWindowController 持有强引用
    var channel: AgentChannel?         // 通信 channel（强引用由 AgentHead 持有）
    var taskProgress: TaskProgress     // 任务进度

    var totalDuration: TimeInterval {
        guard let startedAt else { return 0 }
        return Date().timeIntervalSince(startedAt)
    }
}
```

### OSC133Parser 扩展

```swift
struct ParsedMarker {
    let kind: MarkerKind
    let exitCode: UInt8?
    let commandLine: String?           // 新增：从 cmdline 或 cmdline_url 参数解析
}
```

`ShellState` 新增 `lastCommandLine: String?` 属性，在收到 Phase C 且包含 cmdline 时记录。

注：当前 `OSC133Parser` 的 OSC buffer 上限为 256 字节。`cmdline` 参数可能包含较长的命令（如 `docker run` 带大量 flags），需要将上限提高到 1024 字节。

## API 变更

### AgentHead

```swift
class AgentHead {
    // 注册：以 surface 为核心
    func register(surface: TerminalSurface, worktreePath: String, branch: String,
                  project: String, startedAt: Date?, tmuxSessionName: String?)

    // 注销
    func unregister(terminalID: String)

    // 状态更新
    func updateStatus(terminalID: String, status: AgentStatus,
                      lastMessage: String, roundDuration: TimeInterval)
    /// 原子更新命令行和类型（避免两次独立调用之间的竞态窗口）
    func updateDetection(terminalID: String, commandLine: String?, agentType: AgentType)
    func updateTaskProgress(terminalID: String, totalTasks: Int,
                            completedTasks: Int, currentTask: String?)

    // 查询
    func agent(for terminalID: String) -> AgentInfo?
    func agent(forWorktree path: String) -> AgentInfo?      // 便捷方法（当前一个 worktree 对应一个终端）
    func allAgents() -> [AgentInfo]
    func agentsForProject(_ project: String) -> [AgentInfo]

    // 排序（接受 worktree paths，因为排序来自 config 持久化，terminal ID 是运行时生成的 UUID）
    func reorder(paths: [String])

    // Channel 通信
    func sendCommand(to terminalID: String, command: String)
    func readOutput(from terminalID: String, lines: Int) -> String?
    func channel(for terminalID: String) -> AgentChannel?

    // Webhook 路由（保持按 worktree path 匹配，因为 webhook event 携带 cwd）
    func handleWebhookEvent(_ event: WebhookEvent)
}
```

Webhook 路由内部从 `agents` 字典中按 `worktreePath` 查找匹配的 terminal。为避免 O(n) 遍历，维护一个反向索引 `worktreeIndex: [String: String]`（worktreePath → terminalID），在 register/unregister 时同步更新。

## 检测策略

### 统一优先级表

| 信息 | AI Agent 优先源 | Shell 任务优先源 | Fallback |
|------|-----------------|-----------------|----------|
| 运行状态 | Webhook/Hooks | OSC 133 Phase | 文本匹配 |
| 命令/类型 | 终端文本匹配（agent 名） | OSC 133 cmdline | 终端文本匹配（命令名） |
| lastMessage | Webhook message | 终端末行文本 | — |
| 退出码 | OSC 133 Phase D | OSC 133 Phase D | 进程状态 |
| 任务进度 | Hooks taskProgress | — | — |

### Shell 命令类型检测

从命令文本（无论来源）中提取第一个 token 做匹配：

```swift
extension AgentType {
    /// 从命令行文本检测 shell 任务类型
    static func detect(fromCommand command: String) -> AgentType {
        let first = command.split(separator: " ").first
            .map { String($0) }?.lowercased() ?? ""
        switch first {
        case "brew":            return .brew
        case "btop":            return .btop
        case "top":             return .top
        case "htop":            return .htop
        case "docker":          return .docker
        case "npm", "npx":      return .npm
        case "yarn":            return .yarn
        case "make":            return .make
        case "cargo":           return .cargo
        case "go":              return .go
        case "python", "python3": return .python
        case "pip", "pip3":     return .pip
        default:                return .shellCommand
        }
    }
}
```

与现有 `detect(fromLowercased:)` 共存。统一检测流程：

1. 如果有 OSC 133 cmdline → 用 `detect(fromCommand:)` 识别类型
2. 如果终端文本匹配到 AI agent 模式（"claude"、"codex" 等）→ 现有 `detect(fromLowercased:)`
3. 如果终端文本能提取命令行 → 用 `detect(fromCommand:)` 作为 fallback
4. 都不匹配 → `.unknown`

### OSC 133 状态映射

| Shell Phase | AgentStatus |
|-------------|-------------|
| `.running` (Phase C→D) | `.running` |
| `.prompt` / `.input` (Phase A/B) | `.idle` |
| `.output` + exitCode 0 | `.idle` |
| `.output` + exitCode != 0 | `.error` |

## 对现有消费者的影响

| 消费者 | 改动 |
|--------|------|
| `MainWindowController` | `register()` 改传 surface，内部用 `surface.id` 做 key。所有 `worktreePath` 引用处需要通过 surface 查找或改用 terminal ID |
| `StatusPublisher` | poll 时将 OSC 133 的 commandLine 信息传给 AgentHead；类型检测逻辑增加 shell 命令分支 |
| `OSC133Parser` | `ParsedMarker` 增加 `commandLine` 字段，解析 `cmdline` / `cmdline_url` 参数 |
| `ShellState` | 增加 `lastCommandLine` 属性 |
| `DashboardViewController` | 无直接改动（通过 `AgentDisplayInfo` 间接消费） |
| `AgentCardView` / `MiniCardView` | 可选展示 `commandLine`（如 "brew install ffmpeg"） |
| `WebhookServer` | 路由逻辑从直接按 worktree path 匹配改为遍历 agents 找 worktreePath 匹配的 terminal |
| `AgentHeadTests` | 更新所有测试用 terminal ID 作为 key |

## 不变的部分

- `AgentChannel` 协议及 `TmuxChannel` / `HooksChannel` 实现不变
- `AgentStatus` 枚举不变
- `TaskProgress` 结构不变
- Dashboard 布局和排序逻辑不变
- Delegate 回调模式不变（`AgentHeadDelegate.agentDidUpdate` 仍然是主要通知机制）
- config.json 中的 `agentDetect` 配置结构不变

## 边界情况

1. **AI Agent 启动 shell 命令**：比如 Claude Code 内部调用 `npm install`。这种情况下 AI agent 的 Hooks 会报告 tool_use，不需要 OSC 133 介入。类型保持 `.claudeCode`。

2. **用户先跑 shell 命令再启动 AI agent**：类型会从 `.shellCommand` / `.brew` 等更新为 `.claudeCode`。类型更新规则：
   - 当前类型为 `.unknown` → 允许更新为任何类型
   - 当前类型为 shell 任务（`isShellTask`）→ 允许更新为任何类型（shell 命令结束后启动 AI agent）
   - 当前类型为 AI agent（`isAIAgent`）→ 仅允许更新为另一个 AI agent 类型（防误判），不允许降级为 shell 类型

3. **快速连续命令**：用户快速执行多个短命令。OSC 133 Phase D → Phase C 切换很快，中间可能只有一次 poll 采样。这是可接受的——我们只展示当前状态，不追踪历史。

4. **无 OSC 133 支持的 shell**：如果 shell 没有集成 OSC 133（如某些旧版 bash），完全回退到文本匹配。此时 shell 命令的状态检测与现有 AI agent 检测方式一致。

## 测试策略

### AgentHeadTests 更新

- 所有现有测试从 worktree path 切换到 terminal ID 作为 key
- 新增：注册时验证 `surface.id` 成为主键
- 新增：`agent(forWorktree:)` 便捷查询测试
- 新增：`updateDetection` 原子更新测试

### AgentType 检测测试

- `detect(fromCommand:)` 基本匹配：`"brew install ffmpeg"` → `.brew`
- 带路径的命令：`"/usr/local/bin/brew install ffmpeg"` → 需要处理路径前缀（取 basename）
- 带环境变量前缀：`"ENV=val make build"` → 跳过 `KEY=val` 前缀
- 带管道的命令：`"npm run build | tee log"` → 取第一个 token `.npm`
- 空命令 / 空白字符 → `.unknown`

### OSC133Parser cmdline 测试

- `133;C;cmdline=brew install ffmpeg` → commandLine = `"brew install ffmpeg"`
- `133;C;cmdline_url=brew%20install%20ffmpeg` → commandLine = `"brew install ffmpeg"`
- `133;C` (无 cmdline) → commandLine = `nil`
- 超长命令（验证 buffer 上限 1024 足够）

### OSC 133 状态映射测试

- Phase C → `.running`
- Phase D + exitCode 0 → `.idle`
- Phase D + exitCode 1 → `.error`
- Phase A → `.idle`
- 已在现有 `StatusDetectorTests` 中覆盖，需验证 shell 命令场景下行为一致
