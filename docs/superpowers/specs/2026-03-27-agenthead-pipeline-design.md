# AgentHead 管道架构设计

## 背景

amux 当前的 `AgentHead` 是一个被动的状态注册中心——收集各 terminal 的 agent 状态，暴露给 UI 展示。它不具备记忆能力，不能接受人类指令，也无法驱动 agent 执行任务。

之前的协调 Agent 设计（`2026-03-21-coordinator-agent-design.md`）提出了一个中心化的协调者来统领所有子 Agent。本设计在此基础上做了关键演进：

**决策权下放到每个 Project 的 main agent，AgentHead 退化为信息管道和能力供给层。**

## 核心理念

```
AgentHead = 管道（不做决策）
Project main agent = 大脑（做决策）
```

每个 Project 的 main 分支上运行一个 Claude Code（或其他 AI agent），它拥有完整的项目上下文——代码、git 历史、PR——天然就是这个项目最好的决策者。AgentHead 不需要重新造一个 LLM 决策引擎，而是把信息和能力输送给 main agent，让它自己判断。

## 架构总览

```
                    外部 Channels
     ┌────────┬────────┬──────────┬──────────┐
     │GitHub  │MQTT    │WebSocket │ 人类 UI  │
     │(HTTP)  │(手机)  │(飞书/企微)│(amux)   │
     └───┬────┴───┬────┴────┬─────┴────┬─────┘
         └────────┴─────────┴──────────┘
                       |
               ┌───────v────────┐
               │   AgentHead    │
               │                │
               │ 1. 状态维护     │
               │ 2. main 桥梁   │
               │ 3. 外部桥梁    │
               │ 4. 目标同步     │
               │ 5. Channel 适配│
               │ 6. 定期汇总     │
               └──┬─────────┬───┘
            ┌─────v──┐  ┌───v─────┐
            │Proj A  │  │Proj B   │
            │main<-->│  │main<-->│
            │ wt1,wt2│  │ wt1    │
            └────────┘  └────────┘
```

## AgentHead 六大管线

### 管线 1：实时状态维护

维护所有 worktree-pane 的实时状态。这是现有能力的延续和增强。

**数据流：**

```
TerminalSurface (每个 pane)
  --> StatusPublisher (2s 轮询)
  --> StatusDetector (优先级判定)
  --> WorktreeStatusAggregator (pane -> worktree 聚合)
  --> AgentHead (状态存储 + 记忆持久化)
```

**当前已有：**
- 每个 pane 的 AgentStatus / AgentType / lastMessage / taskProgress / roundDuration
- pane -> worktree 的聚合映射

**需要新增：**
- 状态变化历史持久化到记忆层（不只是当前快照，要有时间线）
- 状态变化时生成事件，驱动其他管线（比如通知 main agent、触发汇总）
- worktree 生命周期跟踪：创建 -> 运行 -> 完成/失败/放弃

### 管线 2：worktree-main 沟通桥梁

main agent 需要了解各 worktree agent 的进展，也需要能给它们下达指令。AgentHead 是这个双向通信的收口。

**main -> worktree（下行）：**

| 能力 | 说明 |
|---|---|
| `sendCommand(terminalID, command)` | 向 worktree agent 发指令（复用 AgentChannel） |
| `createWorktree(branch, task?)` | 创建新 worktree + 启动 agent 会话 |
| `removeWorktree(path)` | 回收 worktree（清理资源、杀会话） |

**worktree -> main（上行）：**

| 数据 | 说明 |
|---|---|
| 状态变化事件 | worktree agent 从 Running->Idle/Error 时通知 main |
| 进展摘要 | 压缩的文本描述（"运行 15 分钟，创建了 PR #42"） |
| 求助信号 | worktree agent 卡住时的 escalation |

**关键设计：** AgentHead 不转发原始终端输出，而是把状态变化 + webhook 事件压缩为结构化摘要后传给 main agent。避免信息过载。

### 管线 3：main 与外部世界的桥梁

main agent 通过 AgentHead 与外部系统交互。AgentHead 统一适配，main agent 不需要关心底层协议。

#### 3.1 GitHub Issues

| 能力 | 说明 |
|---|---|
| `fetchIssues(repo, filters?)` | 拉取 issues 列表 |
| `getIssue(repo, number)` | 获取单个 issue 详情 |
| `updateIssue(repo, number, body?)` | 更新 issue 状态/评论 |
| `createPR(repo, branch, title, body)` | 创建 Pull Request |

#### 3.2 人类指令

用户通过 amux UI 输入的内容：
- 项目目标设定/更新
- 临时任务（"停掉 feat-x，优先做 fix-y"）
- 对特定 worktree 的干预（"这个分支放弃吧"）
- 优先级调整

AgentHead 接收后路由到对应 Project 的 main agent。

#### 3.3 Idea 收集箱

比 GitHub Issue 更轻量的输入通道。人类随时随地丢进来一句话、一个灵感、一个模糊的想法，不需要格式化。

**输入来源：**
- amux UI（快捷输入框）
- 飞书/企微消息（随手发一句）
- 手机 App（通勤时的灵感）

**存储：**

```
~/.config/amux/memory/projects/<repo>/ideas.jsonl
```

```jsonl
{"ts":"2026-03-27T08:30:00Z","source":"wechat","text":"登录页能不能加个记住密码","tags":["ui","login"]}
{"ts":"2026-03-27T12:15:00Z","source":"amux-ui","text":"性能好像变差了，首屏加载要3秒","tags":["perf"]}
{"ts":"2026-03-27T22:00:00Z","source":"mqtt","text":"考虑支持 dark mode 的自动切换","tags":[]}
```

**AgentHead 不处理 idea 的语义，只做收集和存储。** Main agent 在规划阶段可以查询 idea 池，决定是否将某个 idea 升级为正式 TODO 项：

```
Main → AgentHead: readIdeas("my-repo", since: "2026-03-26")
Main: 分析 ideas，将有价值的纳入 Top 5 规划
Main → AgentHead: writeTodo(...) // idea 升级为正式任务
```

**与 TODO 的区别：**

| | Idea | TODO |
|---|---|---|
| 格式 | 自由文本，一句话 | 结构化，有 status/approval |
| 来源 | 人类随时输入 | Main agent 规划生成 |
| 生命周期 | 长期积累，不清理 | 随规划周期轮转 |
| 审批 | 不需要 | 需要人类审批 |
| 执行 | 不直接执行 | 对应 worktree |

**能力接口：**

| 能力 | 说明 |
|---|---|
| `addIdea(projectPath, text, source?, tags?)` | 记录一个 idea |
| `readIdeas(projectPath, since?, tags?)` | 查询 idea 列表 |

#### 3.3 记忆与 TODO 读写

| 能力 | 说明 |
|---|---|
| `readMemory(projectPath, key?)` | 读取 project 记忆 |
| `writeMemory(projectPath, key, value)` | 写入 project 记忆 |
| `readGlobalMemory(key?)` | 读取全局记忆 |
| `getDecisionHistory(projectPath, limit?)` | 获取历史决策记录 |
| `writeTodo(projectPath, items)` | 写入/更新 TODO List |
| `readTodo(projectPath)` | 读取当前 TODO List |

### 管线 4：项目目标双向同步

每个 Project 有一个目标描述，由人类设定、main agent 参考和更新。

**人类 -> 目标：** 通过 UI 设定/修改项目目标
**目标 -> main：** main agent 每次决策时查询目标
**main -> 目标：** main agent 可以更新目标（比如"v1.0 已完成，转向 v1.1"）
**跨 Project：** main agent 可查询其他 project 的目标和状态汇总

存储：

```
~/.config/amux/memory/
|- global.json                  # 全局优先级、人类偏好
'- projects/
   |- <repo-name>/
   |  |- goal.md                # 项目目标（人类/main agent 共同维护）
   |  |- memory.json            # project 级记忆（main agent 自由读写的 KV）
   |  |- todo.json              # TODO List（共享状态机，详见 TODO List 设计章节）
   |  |- ideas.jsonl            # Idea 收集箱（人类随手记，append-only）
   |  |- decisions.jsonl        # 决策历史（append-only log）
   |  '- worktrees/
   |     '- <branch>.json       # worktree 生命周期记录
   '- ...
```

**goal.md 示例：**

```markdown
# amux-swift

## 当前目标
完成 split pane 功能的稳定化，准备 v0.3.0 release。

## 优先级
1. 修复 split pane 的 resize crash
2. 实现 event-driven redraw 替代 timer-based polling
3. 完善 AgentHead 管道架构

## 约束
- 不引入 SwiftUI，保持纯 AppKit
- macOS 14.0+ 最低支持
```

**decisions.jsonl 示例：**

```jsonl
{"ts":"2026-03-27T10:30:00Z","action":"createWorktree","branch":"feat-payment","issue":"#42","reason":"main agent 判断支付功能优先级最高"}
{"ts":"2026-03-27T11:15:00Z","action":"removeWorktree","branch":"feat-old","reason":"已合并到 main"}
{"ts":"2026-03-27T11:20:00Z","action":"sendCommand","target":"feat-payment","command":"继续实现，参考 PR #38 的模式"}
```

### 管线 5：外部 Channel 适配

AgentHead 作为统一的 Channel 适配层，对内提供一致的消息接口，对外对接不同协议。

```
AgentHead
  |- ChannelAdapter (protocol)
  |  |- GitHubAdapter       (HTTP REST / Webhooks)
  |  |- MQTTAdapter         (手机 App 双向通信)
  |  |- WebSocketAdapter    (飞书 / 企业微信 bot)
  |  |- PMuxUIAdapter       (本地 UI 输入输出)
  |  |- WebhookAdapter      (现有的 Claude Code hooks，已实现)
```

**统一消息模型：**

每个 adapter 将外部协议翻译为统一的内部消息格式：

```swift
struct PipelineMessage {
    let source: ChannelType          // .github, .mqtt, .websocket, .ui, .webhook
    let target: MessageTarget        // .project(path), .worktree(path), .global
    let type: MessageType            // .command, .status, .issue, .goal, .report
    let payload: [String: Any]
    let timestamp: Date
}
```

**优先级：** 先实现 GitHubAdapter 和 PMuxUIAdapter，其他 adapter 后续按需添加。MQTT 和 WebSocket adapter 的具体协议格式取决于手机 App 和飞书/企微 bot 的设计，此处只定义适配层接口。

### 管线 6：定期汇总 Report

AgentHead 定期生成全局状态报告，推送给人类和各 main agent。

**触发方式：**
- 定时（可配置间隔，默认 30 分钟）
- 事件驱动（重大状态变化时：worktree 完成、agent 连续错误）

**Report 内容：**

```
=== amux 状态汇总 2026-03-27 14:00 ===

[amux-swift] 目标：完成 split pane 稳定化
  main: idle, 最近决策: 15 分钟前分派了 feat-resize
  feat-resize: running (12m), Claude Code, "正在修复 NSView constraints"
  feat-redraw: idle, PR #20 已创建，等待 review

[payment-api] 目标：v2.0 支付模块重构
  main: idle
  feat-payment: error (3次), "测试失败: Docker 未启动"
  ⚠️ 需要人类关注

全局：2 个 project, 3 个活跃 worktree, 1 个需要关注
```

**推送目标：**
- amux UI（通知面板）
- MQTT（手机 App）
- WebSocket（飞书/企微）
- 各 main agent（可选，避免打断正在工作的 agent）

## TODO List 设计

TODO List 是整个协调循环的**共享状态机**。只有一份，存在 AgentHead 的记忆层里，所有参与者（Main agent、人类、AgentHead）通过同一份数据协作。

### 为什么只维护一份

- 如果 Main 自己维护一份、AgentHead 也维护一份，就有同步问题
- AgentHead 是管道，天然就是信息的中转站，由它持有存储层合理
- Main 不需要自己记 TODO——每次被唤醒时，从 AgentHead 查当前 TODO 状态即可
- 人类的审批结果也写到同一份 TODO 里，不需要额外的审批队列

### 数据模型

```
~/.config/amux/memory/projects/<repo>/todo.json
```

```json
{
  "planCycle": 3,
  "plannedAt": "2026-03-27T10:00:00Z",
  "items": [
    {
      "id": 1,
      "task": "支付模块重构",
      "issue": "#42",
      "status": "running",
      "approval": "approved",
      "approvedBy": "human",
      "worktree": "feat-payment",
      "progress": "Claude Code 运行中，已创建 3 个文件"
    },
    {
      "id": 2,
      "task": "修复登录 bug",
      "issue": "#38",
      "status": "pending",
      "approval": "approved",
      "approvedBy": "human",
      "worktree": null,
      "progress": null
    },
    {
      "id": 3,
      "task": "升级 Swift 依赖到 5.11",
      "issue": null,
      "status": "skipped",
      "approval": "rejected",
      "approvedBy": "human",
      "worktree": null,
      "progress": null
    }
  ]
}
```

### 两个写入者，各管各的字段

| 字段 | 谁写 | 何时写 |
|---|---|---|
| `task`, `issue`, `id` | Main agent | 规划阶段生成 |
| `approval`, `approvedBy` | AgentHead（转发人类决定） | 人类审批后 |
| `status` | AgentHead | worktree 状态变化时 |
| `worktree` | Main agent | 创建 worktree 时 |
| `progress` | AgentHead | 从状态检测管线汇总 |

### 状态流转

```
pending_approval → approved / rejected
                      ↓
                   pending (等待 Main 创建 worktree)
                      ↓
                   running (worktree 已启动)
                      ↓
              completed / failed / abandoned
```

## 典型 Case：规划-审批-执行循环

以下是一个完整的协调循环，展示各组件如何协作。

### 步骤 1：AgentHead 检测到 Main idle，唤醒它

AgentHead 通过管线 1（状态维护）发现 Project Main 的 agent 处于 idle 状态，且没有活跃的 TODO 项。

```
AgentHead → Main: "请做一次规划"
```

### 步骤 2：Main 收集数据并规划

Main 被唤醒后，通过 AgentHead 的能力接口收集多个数据源：

```
Main → AgentHead: fetchIssues("my-repo", open)          // GitHub Issues
Main → AgentHead: readIdeas("my-repo", since: yesterday) // 人类随手记的 idea
Main → AgentHead: readMemory("my-repo", "goal")          // 项目目标
Main → AgentHead: getDecisionHistory("my-repo", 10)      // 最近决策
Main: (本地) 扫描代码架构问题
Main: (本地) 获取过去一天的日志、监控数据
```

Main 综合所有信息（Issues + Ideas + 目标 + 监控），生成 Top 5 actions，写入 AgentHead 的 TODO List。某些 idea 可能被直接采纳为 TODO 项：

```
Main → AgentHead: writeTodo("my-repo", [
  {task: "支付模块重构", issue: "#42", status: "pending_approval"},
  {task: "修复登录 bug", issue: "#38", status: "pending_approval"},
  {task: "升级 Swift 依赖", status: "pending_approval"},
  {task: "补充 API 文档", status: "pending_approval"},
  {task: "清理废弃代码", status: "pending_approval"}
])
```

### 步骤 3：AgentHead 推送给人类审批

AgentHead 检测到新的 TODO 写入（状态为 pending_approval），通过管线 5（Channel 适配）推送给人类：

```
AgentHead → 飞书/企微:
  "Project my-repo 规划了 5 个任务：
   1. 支付模块重构 (#42)
   2. 修复登录 bug (#38)
   3. 升级 Swift 依赖
   4. 补充 API 文档
   5. 清理废弃代码
   请回复要执行的编号（如：1,2,3）"
```

### 步骤 4：人类审批

人类回复："做 1 和 2，3 先不做，4 和 5 下次再说"

AgentHead 更新 TODO List：
- #1: approval=approved
- #2: approval=approved
- #3: approval=rejected
- #4: approval=deferred
- #5: approval=deferred

AgentHead 通知 Main："人类批准了 #1 和 #2"

### 步骤 5：Main 创建 Worktree 执行任务

Main 收到通知，开始执行：

```
Main → AgentHead: createWorktree("feat-payment", task: #1)
```

AgentHead 执行创建（git worktree add + 启动 agent 会话），更新 TODO：
- #1: status=running, worktree="feat-payment"

### 步骤 6：AgentHead 持续跟踪

```
AgentHead 管线 1（状态维护）持续监控 feat-payment 的状态：
  - Running → "正在修复 NSView constraints"
  - Running → "测试通过，准备提 PR"
  - Idle → "PR #45 已创建"

每次状态变化，AgentHead 更新 TODO #1 的 progress 字段。

当 feat-payment 变为 idle 且检测到 PR 创建：
  AgentHead → Main: "任务 #1 完成，PR #45 已创建"
  AgentHead 更新 TODO #1: status=completed

Main 收到通知，决定：
  - 启动 #2: createWorktree("fix-login", task: #2)
  - 或者先 review PR #45 再继续
```

### 未来演进：自动审批

当信任度建立后，AgentHead 或 Main 可以跳过人类审批环节：

```
步骤 3 变为：
  AgentHead 检查 auto-approve 规则：
  - 低风险任务（文档、依赖升级）→ 自动 approved
  - 高风险任务（架构变更、数据迁移）→ 仍需人类审批
```

## Main Agent 的角色

main agent 不是 AgentHead 创建的特殊实体，而是**用户在 main 分支上启动的普通 Claude Code 会话**，只是它通过 AgentHead 提供的能力获得了协调者的权限。

amux 不控制 Main agent 的决策循环——它是 Main agent 自己的行为。amux 只确保 Main agent 有足够的信息和工具来完成这个循环。

### Main Agent 如何获得 AgentHead 能力（待定）

具体接口机制后续设计。候选：

1. **MCP Server** -- AgentHead 暴露为 MCP server，main agent 通过 MCP tools 调用
2. **Claude Code Hooks** -- AgentHead 在事件时向 main agent 推送
3. **混合** -- MCP 做主动查询，Hooks 做被动推送
4. **文件系统协议** -- agent 读写约定格式的文件来通信

## 与现有架构的关系

### 保留不变

- **TerminalSurface / GhosttyBridge / SurfaceRegistry** -- 终端渲染和管理
- **StatusPublisher / StatusDetector** -- 状态检测管线
- **WorktreeStatusAggregator** -- 聚合逻辑
- **AgentChannel (Tmux/Zmx/Hooks)** -- 终端通信通道
- **WebhookServer / WebhookStatusProvider** -- webhook 接收（作为 WebhookAdapter 的基础）
- **Config** -- 配置系统

### 需要扩展

| 组件 | 变更 |
|---|---|
| `AgentHead` | 从状态注册中心扩展为六管线管道 |
| `AgentInfo` | 新增 `role` 字段区分 main / worktree agent |
| `MainWindowController` | 新增 worktree 创建/回收的程序化接口 |
| UI | 人类指令输入 + Report 展示 |

### 新增组件

| 组件 | 职责 |
|---|---|
| `MemoryStore` | 记忆层的读写封装（文件系统 JSON/JSONL） |
| `ChannelAdapter` (protocol) | 外部 Channel 统一适配接口 |
| `GitHubAdapter` | GitHub Issues/PR 交互 |
| `MQTTAdapter` | 手机 App 双向通信（后续） |
| `WebSocketAdapter` | 飞书/企微 bot（后续） |
| `ReportGenerator` | 定期汇总生成 |

## 不做什么

- **不做跨 Project 自动协调** -- 各 main agent 独立决策
- **不做 LLM 调用** -- 决策由 main agent 完成
- **不做实时双向对话** -- agent 间不直接对话，经 AgentHead 管道
- **不做自动启动 main agent** -- 人类决定何时启动

## 与之前设计的关系

| 之前的设计 | 本设计的演进 |
|---|---|
| 协调 Agent 设计 (03-21) | 决策权从中心化协调 Agent 下放到各 Project main agent |
| AgentHead 统一终端任务设计 (03-21) | 完全兼容，terminal ID 主键和统一类型枚举继续沿用 |
| Webhook 状态检测设计 (03-20) | 完全兼容，webhook 管线纳入为 WebhookAdapter |
