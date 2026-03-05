# pmux — AI Agent 多分支开发工作台

## 1. 背景与目标

pmux 是 AI Agent 多分支开发工作台：管理多个 AI Agent 并行工作（每 agent 一个 git worktree），实时监控状态，主动通知，快速 Review Diff。

**重构目标**：从「tmux 的 GUI 前端」升级为「AI Agent Runtime + 多终端工作台」。

---

## 2. UI 操作大方向（不变）

重构**不改变**以下用户流程与能力，仅调整底层实现：

| # | 能力 | 说明 |
|---|------|------|
| 1 | 启动页依赖检测 | 默认 tmux（control mode）：检查 tmux binary，不可用时自动 fallback 到 local pty |
| 2 | 启动页打开 workspace | 选择 git 目录 → 打开 workspace |
| 3 | 多 workspace 支持 | 多个仓库 Tab 切换，每个 workspace = 一个 session |
| 4 | 多 worktree 支持 | 可新增 worktree 分支（[+ New Branch]），每个 worktree 一个 agent |
| 5 | 独立 terminal + 多 pane | 每个 worktree 有独立终端，支持 ⌘D/⌘⇧D 分屏，多 pane 并行 |
| 6 | 关闭 GUI 后台不关 | `tmux-cc` 模式：关闭窗口后 agent 在 tmux session 中继续运行，重开自动 attach 恢复；`local` 模式：关闭即停止 |
| 7 | Agent 进展通知 | 每个 worktree 的 agent 状态变化（Waiting/Error 等）有通知（面板 + 系统） |

---

## 3. 核心原则

| 原则 | 说明 |
|------|------|
| **Terminal = Stream** | 处理 RAW PTY BYTES（local pty 直读 master / tmux-cc `%output` 路由），而非 screen text snapshot |
| **UI 不知道 backend** | UI 只依赖 AgentRuntime API，不直接调用 tmux/pty |
| **Agent 是一等公民** | Agent 封装 lifecycle、tty、状态机；tmux-cc / local pty 只是 backend 之一 |

---

## 4. 目标架构

```
pmux
├── UI (GPUI)           ← 渲染、交互、通知
├── Agent Runtime       ← Event Bus、PTY Engine、State Machine
└── Backends            ← tmux (默认, control mode 持久化) / local pty (fallback) / docker / ssh
```

**依赖方向**：`UI → Runtime API → Backend Adapter`。UI 禁止直接调用 `tmux::*`。

```
                    ┌─────────────────────────────────────┐
                    │           AgentRuntime API           │
                    │  send_input / resize / subscribe_*   │
                    └─────────────────┬───────────────────┘
                                      │
    ┌──────────────┬─────────────────┼────────────────┬──────────────┐
    ▼              ▼                 ▼                ▼              ▼
┌──────────┐ ┌──────────┐   ┌──────────────┐  ┌──────────┐  ┌──────────┐
│ local pty│ │ tmux-cc  │   │  tmux (pipe  │  │ docker   │  │   ssh    │
│ (默认)   │ │ -CC ctrl │   │  -pane,leg.) │  │ (future) │  │ (future) │
└──────────┘ └──────────┘   └──────────────┘  └──────────┘  └──────────┘
```

**Backend 策略**：
- **local pty**（默认）：直接 spawn shell，gpui-terminal 获得干净 PTY 流，无 tmux 依赖，零配置即用
- **tmux-cc**（持久化）：`tmux -CC attach` control mode，结构化 `%output` 事件路由到 local PTY pair，关闭 GUI agent 继续运行
- **tmux**（legacy）：旧版 `pipe-pane` + `capture-pane`，已 deprecated，保留兼容

UI 只依赖 Runtime API；Backend 可插拔，接入新 backend 无需改 UI。通过 `PMUX_BACKEND` 环境变量或 `config.json` 的 `backend` 字段切换。

---

## 5. 技术栈

### 保留

| crate | 用途 |
|-------|------|
| gpui / gpui_platform | UI 框架 |
| gpui-terminal | 嵌入式终端组件（内部使用 alacritty_terminal 做 VTE 解析） |
| alacritty_terminal | gpui-terminal 内部使用；pmux 不再直接依赖 |
| tokio / blocking | 异步与 IO |
| serde / serde_json | 配置与序列化 |
| notify-rust | 系统通知 |
| rfd | 文件选择 |
| thiserror | 错误处理 |
| git_utils | Git 操作 |

### 调整

| 现有 | 改为 | 状态 |
|------|------|------|
| terminal_poller / status_poller / capture-pane | PTY stream (local pty / tmux-cc %output) + Event Bus | ✅ 已实现 |
| terminal_rendering / term_bridge / TerminalEngine | gpui-terminal + RuntimeReader/Writer + ContentExtractor | ✅ 已实现 |
| tmux send-keys | xterm escape → Runtime.send_input → PTY write | ✅ 已实现 |
| tmux 直接调用 | 通过 Runtime API，tmux 作为 backend adapter | ✅ 已实现 |
| 默认 tmux backend | 默认 local pty，tmux-cc 用于持久化 | ✅ 已实现 |
| pipe-pane 流式模式 | local pty 直读 PTY master；tmux-cc 用 %output 事件 | ✅ 已实现 |

### 新增

- `src/runtime/`：`agent_runtime.rs`、`pty_bridge.rs`
- `src/runtime/backends/tmux_control_mode.rs`：tmux -CC control mode parser + runtime
- Event Bus：Agent 状态、Terminal 输出、通知
- `src/remotes/`：Remote Channels（Discord、KOOK、飞书），见 §13

---

## 6. 数据流（目标）

### 6.1 Event Bus

事件类型（便于实现）：

| 事件类型 | 说明 | 订阅方 |
|----------|------|--------|
| `AgentStateChange` | Agent 状态变化（Running / Waiting / Error 等） | Sidebar、StatusBar |
| `TerminalOutput` | 终端字节流 / Grid 更新，携带 `pane_id` | TerminalView |
| `Notification` | 需提醒用户（等待输入、错误等） | Notification 面板、系统通知、RemoteChannels |

实现：tokio `broadcast`（多订阅者）或 `mpsc`。Runtime 发布，UI 订阅。

### 6.2 PTY Streaming

**模式**：local pty backend 直接读 PTY master（默认）；tmux-cc backend 通过 `tmux -CC` control mode 的 `%output` 事件获取结构化输出并路由到 local PTY pair。**默认 backend 为 local**，可通过 `PMUX_BACKEND=tmux-cc` 或 config 启用持久化模式。旧版 tmux backend（pipe-pane）已 deprecated。

**数据流**：

```
subscribe_output(pane_id) → TeePipe
       │
       ├─ rx1 → RuntimeReader → gpui_terminal::TerminalView（渲染）
       └─ rx2 → ContentExtractor → StatusPublisher（Agent 状态检测）

输入：gpui_terminal 键盘事件 → RuntimeWriter → runtime.send_input(pane_id, bytes)
```

**必须正确处理**（保证 vim / TUI 正常）：
- **alternate screen**：vim 等全屏 TUI 切换主/备屏
- **双宽字符**：CJK 等宽字符
- **光标位置**：由 gpui-terminal（内部 alacritty_terminal）解析 VT 序列维护

### 6.3 输出订阅链路

```
AgentRuntime.subscribe_output(pane_id)
         │
         ▼
   Event Bus (TerminalOutput { pane_id, bytes, ... })
         │
    ┌────┴────┬─────────────┐
    ▼         ▼             ▼
TerminalView  (其他订阅者)   (可选：日志/录制)
    │
    └→ GPUI 重绘（按 pane_id 路由）
```

### 6.4 用户输入

```
keyboard / mouse → xterm escape 序列 → Runtime.send_input(bytes) → PTY write
```

### 6.5 Agent 状态

```
process lifecycle → Event Bus (AgentStateChange) → Sidebar / StatusBar / Notification
```
（不再依赖 terminal 文本解析）

---

## 7. UI 布局与数据获取

### 7.1 布局

两栏：Sidebar（左）| Content（右）

- **Sidebar**：Worktree 列表、状态图标、[+ New Branch]
- **Content**：Workspace 标签栏、Pane 标签栏、终端主区域
- **Diff**：⌘⇧D 打开 nvim diffview

### 7.2 UI 原型图（Ghostty 风格）

借鉴 [Ghostty](https://ghostty.org) 的设计理念：**平台原生、极简、功能优先**。使用原生 UI 组件，无自定义绘制控件；布局一上来即分为左 | 右 | 底，无统一 title bar。

**可交互原型**：[`docs/pmux-prototype.html`](docs/pmux-prototype.html)（浏览器打开即可预览）

**截图**：![pmux UI 原型](docs/pmux-prototype.png)

```
╭────────────────────────────┬─────────────────────────────────────────────────────────────────╮
│  [●][○][○]  [📁][🔔][+]   │  repo-a    repo-b    repo-c    +                                 │  ← 原生 Tab
│  ─────────────────────    │  ─────────────────────────────────────────────────────────────  │
│                            │                                                                 │
│  📎 hq                     │  $ agent run --task fix-bug                                     │
│     Claude is waiting...   │  > Running...                                                    │
│     ~/fun/cmuxterm-hq      │                                                                 │
│                            │  ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─  │  ← 原生 Split
│  ▓ cmux cli/unix socket ▓  │  $ git status                                                   │
│     PR: ...Implemented     │  modified: src/lib.rs                                            │
│     ~/fun/cmux             │                                                                 │
│                            │                                                                 │
│  * ssh                     │  (非焦点 Pane 可淡出，突出当前焦点)                              │
│     branch: feat-ssh        │                                                                 │
│     ~/fun/cmux              │                                                                 │
│  ⋮                         │                                                                 │
│  ─────────────────────    │                                                                 │
│  + New Branch              │                                                                 │
│                            │                                                                 │
├────────────────────────────┴─────────────────────────────────────────────────────────────────┤
│  ⌘B  ⌘N  ⌘⇧N  ⌘1-8  ⌘W  ⌘⇧D  ⌘⇧R                                                    │ 提示   │
╰──────────────────────────────────────────────────────────────────────────────────────────────╯
```

**Ghostty 风格要点**
- **平台原生**：macOS 用原生 traffic lights、原生 Tab、原生 Split，符合系统预期
- **极简**：无多余装饰，Status Bar 仅展示快捷键，可悬停展开说明
- **视觉层次**：选中项高亮（▓），非焦点 Pane 可淡出（unfocused split fading）
- **布局**：左 | 右 | 底 三块，无统一 title bar；Sidebar header 内嵌图标

**图例**：📎 主 worktree  * 其他  ▓ 选中  ⋮ 可滚动

**左侧 Sidebar**
- **Header**（在 Sidebar 内）：macOS 系统按钮（红黄绿）+ workspace 图标 + 通知 icon + 添加 workspace icon
- **Worktree list**（可滚动）：
  - 每项两行：状态图标 | **分支名称**（粗体）| 最后更新时间（右对齐）/ 最后一条消息或状态提示
  - 示例：`● feature/login  5m` / `  Waiting for input`
  - 选中项蓝色背景高亮
- **[+ New Branch]**：固定在 list 下方

**右侧 Content**
- **Workspace tab bar**：当前 workspace 与多 tab 切换
- **主区域**：多 Pane terminal（⌘D/⌘⇧D 分屏），每个 Pane 可对应同一 worktree 的不同任务
- **数据流**：输出通过 Runtime API `subscribe_output` 订阅，输入通过 Runtime API `send_input` 发送

**Status Bar**
- 展示常用快捷键（⌘B、⌘N、⌘⇧N、⌘1-8、⌘W、⌘⇧D、⌘⇧R）
- 支持 UI 内实时提示用户

### 7.3 UI 数据获取模式

```
Event Bus (subscribe)
       │
       ├─ AgentStateChange  → Sidebar 状态图标、StatusBar 聚合
       ├─ TerminalOutput    → TerminalView 渲染
       └─ Notification     → 通知面板、系统 notify、RemoteChannels
       │
用户操作（快捷键 / 点击）
       │
       ├─ 应用级（⌘B / ⌘N 等）→ UI 内部处理
       ├─ Diff（⌘⇧D）       → Runtime.open_diff(worktree, pane_id)
       ├─ Review（⌘⇧R）     → Runtime.open_review(worktree)
       └─ 透传终端           → Runtime.send_input(pane_id, bytes)
```

UI 不轮询，只订阅 Event Bus 并响应；所有 tmux/backend 操作通过 Runtime API。

---

## 8. 重构阶段

| Phase | 内容 | 预估 | 状态 |
|-------|------|------|------|
| **1** | Local PTY backend + LocalPtyAgent 多 pane 支持，deprecate 旧 tmux pipe-pane 路径 | 1 周 | ✅ 已完成 |
| **2** | tmux Control Mode 持久化：ControlModeParser，TmuxControlModeRuntime，注册 tmux-cc backend + recover 支持。**默认 backend 改为 tmux**（control mode），"tmux"/"tmux-cc" 合并，tmux 不可用时 fallback 到 local | 2 周 | ✅ 已完成 |
| **3** | Agent Runtime 完善：Agent 模型、Event Bus、PtyHandle trait、状态机 | 1 周 | 待开始 |
| **4** | UI / 通知 / Remote Channels | 2 周 | 待开始 |

**实现计划详情**：见 `docs/plans/2026-03-01-local-pty-default-and-control-mode.md`

**E2E 测试**：
- Phase 1：`tests/e2e/local_pty_e2e.sh` — 截图+OCR+阶梯检测+vim 兼容性
- Phase 2：`tests/e2e/tmux_cc_e2e.sh` — 持久化+恢复+截图断言

---

## 9. 成功标志

- [x] 默认 local pty backend 无 tmux 依赖，零配置即用
- [x] vim / TUI 在 pmux 内完全正常（local pty 提供干净 PTY 流）
- [x] 无阶梯 prompt、无光标错位（E2E 截图断言验证）
- [x] tmux-cc backend 实现：关闭 pmux UI，agent 继续运行（`tmux -CC` 持久化）
- [x] tmux-cc 恢复：重开 pmux 自动 attach，OCR 可见上次内容
- [x] 新 backend 可在不改 UI 情况下接入（AgentRuntime trait 抽象）
- [ ] 无 polling loop（部分完成，status 检测仍有 poll）
- [ ] UI 不包含 tmux 调用（进行中，个别路径仍有直接调用）

---

## 10. 核心数据模型（目标）

```rust
struct Agent {
    id: AgentId,
    worktree: PathBuf,
    state: AgentState,   // Starting | Running | WaitingInput | Error | Exited
    panes: Vec<PaneHandle>,  // 每个 Agent 内可管理多个 Pane
}

trait PtyHandle {
    fn write(&self, bytes: &[u8]);
    fn resize(&self, cols: u16, rows: u16);
    fn subscribe_output(&self) -> impl Stream<Item = TerminalEvent>;
}

struct TerminalEvent {
    bytes: Vec<u8>,
    pane_id: PaneId,
    timestamp: Instant,
    event_type: TerminalEventType,
}

trait AgentRuntime {
    fn send_input(&self, pane_id: PaneId, bytes: &[u8]);
    fn resize(&self, pane_id: PaneId, cols: u16, rows: u16);
    fn subscribe_output(&self, pane_id: PaneId) -> impl Stream<Item = TerminalEvent>;
    fn subscribe_state(&self) -> impl Stream<Item = AgentStateChange>;
    fn list_panes(&self, agent_id: AgentId) -> Vec<PaneId>;

    fn open_diff(&self, worktree: &Path, pane_id: Option<PaneId>) -> Result<()>;
    fn open_review(&self, worktree: &Path) -> Result<()>;   // ⌘⇧R

    fn restart(&self, agent_id: AgentId) -> Result<()>;
    fn recover(&self, agent_ids: Option<Vec<AgentId>>) -> Result<()>;  // None = 全部
}
```

**Recovery 场景**：pmux 重开 → `recover()` 按 state.json 映射 attach/spawn，无需重建 agent。

---

## 11. 快捷键（保留）

| 快捷键 | 功能 |
|--------|------|
| ⌘B | 收缩/展开侧边栏 |
| ⌘N | 新建 workspace |
| ⌘⇧N | 新建 branch + worktree |
| ⌘1-8 | 切换 Workspace tab |
| ⌘W | 关闭当前 tab |
| ⌘⇧D | 打开 Diff 视图（只读） |
| ⌘⇧R | 打开 Review（可提交/comment/approve） |

非应用级快捷键透传至终端（通过 Runtime API 的 send_input）。

---

## 12. 待定事项与决策

### 架构与模型

| 待定项 | 决策 |
|--------|------|
| **Agent 与 Pane 映射** | 一个 Agent 对应一个 worktree；每个 Agent 内可管理多个 Pane。`subscribe_output` 支持 per-Pane，Event 携带 `pane_id`，便于 UI 精确渲染和输入路由。 |
| **Agent 状态来源** | 主来源：process lifecycle（启动/运行/exit/crash）。WaitingInput 通过 PTY blocking 或 Agent 内部状态机判断。Error 由 process exit code + stderr 捕获。文本解析仅作 fallback。 |
| **Diff 归属** | Runtime 提供 `open_diff(worktree, [pane_id?])` API，UI 调用。Runtime 内部封装 nvim/diffview，UI 不直接处理 diff 逻辑。 |

### 技术实现

| 待定项 | 决策 |
|--------|------|
| **Control mode vs Pipe-pane** | **已决策**：local pty 为默认（无 tmux 依赖）；tmux-cc（control mode）用于持久化场景；旧版 pipe-pane (`TmuxRuntime`) 已 deprecated。 |
| **Event Bus 与 GPUI 线程** | tokio mpsc channel + spawn；Event Bus 在 async runtime，UI main thread 通过 channel 拉取事件后 `cx.notify()`，避免 UI 直接 await IO。 |
| **Stream 类型** | `subscribe_output` 返回 `futures::Stream`，可组合 `async_stream` 实现。`TerminalEvent` 字段：`bytes`, `pane_id`, `timestamp`, `event_type`。 |
| **PtyHandle 抽象** | 定义 `trait PtyHandle { write(&[u8]); resize(u16,u16); subscribe_output() }`。各 backend 实现该 trait。 |

### Backend 与配置

| 待定项 | 决策 |
|--------|------|
| **Backend 选择** | 通过 `PMUX_BACKEND` 环境变量或 config.json `backend` 字段指定。有效值：`tmux`（默认，control mode 持久化）、`tmux-cc`（alias）、`local`（direct PTY）。无效值 fallback 到 `tmux`。tmux 不可用时自动 fallback 到 `local`。 |
| **其他 backend 优先级** | Phase 1–2 已完成 local PTY + tmux-cc；docker/ssh 作为 Phase 3–4 可选扩展。 |
| **恢复时的 session 映射** | state.json 保存：`workspace → worktree → AgentId → pane_id → backend session/window id`。`recover()` 按此映射 attach/spawn。 |

### 错误与恢复

| 待定项 | 决策 |
|--------|------|
| **restart / recover 粒度** | 默认 per-Agent。`recover()` 可 restart 所有 Agent 或指定 Agent，避免影响其他并行 Agent。 |
| **依赖检测范围** | 按 backend 检测：tmux backend 检查 tmux binary + version；local PTY 检查 PTY 功能。各 backend 独立检测。 |

### 边界与兼容

| 待定项 | 决策 |
|--------|------|
| **多 Pane 的 Runtime 抽象** | 每个 Pane 由 Agent 内部 Pane handle 管理，输出/输入都带 `pane_id`。UI 通过 Agent API 获取 pane 列表和状态。 |
| **配置迁移** | config/state 新 schema 支持上述映射；提供迁移工具向后兼容老版本。 |
| **⌘⇧D 与 ⌘⇧R** | ⌘⇧D：打开 Diff 视图（只读）；⌘⇧R：触发 Review（可提交/comment/approve）。Runtime 提供对应 API，UI 调用。 |

---

## 13. 远程通知通道（Remote Channels）

### 13.1 目标

通过现有 IM 平台实现：手机/异地查看 agent 进度、接收告警通知、遥控命令。不建独立 App 或网站。

### 13.2 实现策略

- **搭架子时机**：提前搭架子，在 Runtime 稳定前可先建 `src/remotes/` 骨架，订阅接口占位
- **平台优先级**：Discord、KOOK、飞书均采用 Bot 方案；飞书发送已实现，接收命令待实现

### 13.3 支持优先级

| 优先级 | 平台 | 说明 |
|--------|------|------|
| 1 | Discord | Bot API 发消息 + Gateway 收命令 |
| 2 | KOOK | 国内可直连，Bot API 发消息 + Gateway 收命令 |
| 3 | 飞书 | Bot API 发消息（app_id + app_secret → tenant_access_token）；接收命令暂未实现 |

### 13.4 架构

```
Event Bus (AgentStateChange, Notification)
         │
         ├─→ 桌面系统通知 (notify-rust)
         ├─→ UI 通知面板
         └─→ RemoteChannelPublisher
                    │
                    ├─→ Discord Adapter
                    ├─→ KOOK Adapter
                    └─→ 飞书 Adapter（发送已实现）
```

统一抽象：`RemoteChannel` trait，配置驱动；各 Adapter 将平台无关消息格式化为平台格式（文本/卡片/Embed）并发送。

### 13.5 推送内容（第一阶段）

- Agent 状态变化：`workspace / worktree: Running → WaitingInput`
- 需确认的告警：`workspace / worktree: Error`
- 消息必须包含 **workspace**、**worktree** 标识，避免多实例/多 workspace 混淆
- 后续可选：简要进度汇总（防刷屏，可节流）

### 13.6 接收命令（遥控）

支持 Bot 接收命令，调用 Runtime API。Discord 斜杠命令、KOOK 消息解析等：

- `status`：查询所有 agent 状态
- `restart <worktree>`：重启指定 worktree
- `send <worktree> <text>`：向指定 worktree 发送输入（需谨慎，可配置白名单）

### 13.7 配置与敏感信息

- **配置共享**：Remote 配置跨 pmux 实例共享，所有推送消息需明确 workspace、worktree
- **敏感信息**：`bot_token` 等存于 `~/.config/pmux/secrets.json`，不纳入 config.json
- **Discord、KOOK 仅 Bot**：不使用 Webhook，统一用 Bot API 发消息 + Gateway 收命令
- **.gitignore**：`secrets.json` 加入忽略

### 13.8 配置示例

**config.json**（不含敏感信息）：

```json
{
  "remote_channels": {
    "discord": { "enabled": true, "channel_id": "123456789" },
    "kook": { "enabled": true, "channel_id": "xxx" },
    "feishu": { "enabled": true, "chat_id": "oc_xxx" }
  }
}
```

**~/.config/pmux/secrets.json**（git 忽略）：

```json
{
  "remote_channels": {
    "discord": {
      "bot_token": "YOUR_BOT_TOKEN"
    },
    "kook": {
      "bot_token": "xxx"
    },
    "feishu": {
      "app_id": "cli_xxx",
      "app_secret": "xxx"
    }
  }
}
```

### 13.9 实现位置

- 新增：`src/remotes/`（`channel.rs`、`publisher.rs`、`discord.rs`、`kook.rs`、`feishu.rs`）
- 依赖：`reqwest` 发 HTTP
- 订阅 Event Bus，与 Runtime 解耦；接收命令时调用 Runtime API

---

## 14. 语音输入/输出（Voice I/O）

### 14.1 目标

通过语音驱动 agent 实现代码：用户说话下达任务，系统朗读 agent 状态/告警。

### 14.2 语音输入目标

| 层级 | 说明 |
|------|------|
| **焦点 Pane** | 语音任务发送给当前获取焦点的 pane |
| **多 Pane 时** | 若一个 worktree 有多个 pane，发送给**第一个** pane |
| **无 Agent 时** | 若第一个 pane 未启动 agent，自动启动**默认 agent** |

### 14.3 默认 Agent 配置层级

默认 agent（如 `claude`、`opencode`）按**由全局到具体**顺序读取，后者覆盖前者（worktree 最优先）：

| 优先级 | 作用域 | 配置路径示例 | 说明 |
|--------|--------|--------------|------|
| 1 | 全局 | `config.default_agent` | 基础默认 |
| 2 | Workspace | `workspaces[].default_agent` | 覆盖全局 |
| 3 | Worktree | `worktrees[].default_agent` | 覆盖 workspace |

解析时：worktree → workspace → global，取第一个有值的。未配置时 fallback 到固定默认（如 `claude`）。

### 14.4 语音输出

- 订阅 Event Bus（`AgentStateChange`、`Notification`）
- 关键事件触发 TTS：WaitingInput、Error、Exited 等
- 支持中英双语

### 14.5 语言支持

STT（语音转文字）和 TTS（文字转语音）均支持**中文 + 英文**，可按用户配置或系统语言选择。

### 14.6 配置示例

```json
{
  "default_agent": "claude",
  "workspaces": [
    {
      "path": "~/projects/repo-a",
      "default_agent": "opencode"
    }
  ],
  "worktrees": {
    "feat-auth": {
      "default_agent": "claude"
    }
  },
  "voice": {
    "enabled": true,
    "stt_provider": "whisper",
    "tts_provider": "system",
    "language": "auto"
  }
}
```

### 14.7 实现位置

- 新增：`src/voice/`（`stt.rs`、`tts.rs`、`input_handler.rs`）
- 与现有 `Runtime.send_input`、Event Bus 对接
- 需实现：`Runtime.spawn_default_agent(worktree)` 或等价 API

---

## 15. Agent 状态检测系统

Agent 状态检测是 pmux 的核心能力之一。整个系统由 **检测 → 发布 → 分发 → 展示** 四个阶段组成。

### 15.1 状态枚举

```rust
// src/agent_status.rs
enum AgentStatus {
    Running,         // 绿 ●  — Agent 正在执行
    Waiting,         // 黄 ◐  — 等待用户输入
    WaitingConfirm,  // 橙 ▲  — 需要人工确认（如权限审批）
    Idle,            // 灰 ○  — 空闲，无任务
    Error,           // 红 ✕  — 发生错误
    Exited,          // 蓝 ✓  — 进程已退出
    Unknown,         // 紫 ?  — 状态无法判断
}
```

**优先级**（高→低）：Error(6) > Exited(5) = WaitingConfirm(5) > Waiting(4) > Running(3) > Idle(2) > Unknown(1)

多 pane 场景（split panes）：Sidebar 按 worktree 聚合，取所有 pane 中优先级最高的状态显示（`AgentStatus::highest_priority_for_prefix()`）。

### 15.2 检测链路总览

```
PTY output bytes
       │
       ▼
ContentExtractor.feed(bytes)
       │
       ├─ Osc133Parser → ShellPhaseInfo { phase, exit_code }
       └─ text_buf → take_content() → visible text
       │
       ▼
StatusPublisher.check_status(pane_id, process_status, shell_info, content)
       │
       ├─ StatusDetector.detect(process, osc133, text) → AgentStatus
       ├─ DebouncedStatusTracker → 去抖 → changed?
       └─ extract_last_line(content, 80) → last_line
       │
       ▼ (if changed)
Event Bus
       │
       ├─ RuntimeEvent::AgentStateChange { state, prev_state, last_line, pane_id }
       └─ RuntimeEvent::Notification { message, notif_type }  (条件触发)
       │
       ▼
AppRoot event subscription
       │
       ├─ StatusCountsModel  → TopBar 聚合计数
       ├─ PaneSummaryModel   → Sidebar (last_line, status_since, status)
       ├─ NotificationManager → 通知面板
       ├─ system_notifier     → macOS 系统通知
       └─ RemoteChannels      → Discord / KOOK / 飞书
```

### 15.3 检测优先级（StatusDetector）

```
文件：src/status_detector.rs

Priority 1: Process lifecycle (最高)
  ProcessStatus::Exited  → Exited
  ProcessStatus::Error   → Error

Priority 2: OSC 133 shell markers
  ShellPhase::Running → Running
  ShellPhase::Input / Prompt → Waiting
  ShellPhase::Output + exit_code ≠ 0 → Error
  ShellPhase::Output + exit_code = 0 → Idle（命令成功完成）

Priority 3: Text patterns (fallback)
  检查最后 15 行（check_line_count），先去 ANSI 转义
  先检查最后 3 行是否匹配 Idle prompt 模式（覆盖旧 Running 文本）
  再依次匹配：Confirm > Error > Waiting > Running > Idle > Unknown
  内置 regex pattern 组，支持 custom_* 扩展
```

**内置文本模式**：

| 类别 | 示例 pattern |
|------|-------------|
| Idle (prompt) | `❯`, `$`, `%`, `git:(main) ✗`, `user@host:~ $`（最后 3 行优先匹配） |
| Running | `thinking`, `analyzing`, `writing`, `esc to interrupt` |
| Waiting | `? `, `> `, `human:`, `press enter`, `waiting for` |
| WaitingConfirm | `requires approval`, `Accept/Reject`, `Always allow` |
| Error | `error`, `panic`, `traceback`, `exit code [1-9]` |

### 15.4 去抖机制（DebouncedStatusTracker）

```
文件：src/status_detector.rs

- 普通状态变化：需连续 2 次检测到相同新状态才提交（debounce_threshold=2）
- 紧急状态（Error, Exited, WaitingConfirm）：跳过去抖，立即提交
- 避免终端输出中瞬间出现的关键词导致误判
```

### 15.5 状态发布（StatusPublisher）

```
文件：src/runtime/status_publisher.rs

触发：终端有新输出时调用 check_status()（事件驱动，非轮询）

输入：pane_id, ProcessStatus, Option<ShellPhaseInfo>, content: &str

流程：
  1. 记录 prev_status = tracker.current_status()
  2. StatusDetector.detect() → 新状态
  3. DebouncedStatusTracker.update_with_status() → changed?
  4. extract_last_line(content, 80) → last_line（最后一行有意义的终端输出）
  5. 发布 AgentStateChange { state, prev_state, last_line, pane_id }
  6. 条件触发 Notification（见 §15.6）
```

### 15.6 通知触发规则

```
文件：src/runtime/status_publisher.rs

状态转换触发通知：
  (Running → Idle)    → Info    "pmux — Done"     ★ agent 完成任务
  (* → Waiting)       → WaitingInput              等待输入
  (* → WaitingConfirm)→ WaitingConfirm            需要确认
  (* → Error)         → Error                     错误
  (* → Exited)        → Info                      进程退出

通知消息内容 = last_line（真实终端输出），last_line 为空时 fallback 到 status.display_text()

通知分发：
  NotificationManager → UI 通知面板（30s 合并窗口去重）
  system_notifier     → macOS 系统通知（notify-rust）
  RemoteChannels      → Discord / KOOK / 飞书 bot
```

### 15.7 Sidebar 状态展示

```
文件：src/ui/sidebar.rs

布局（每个 worktree 两行）：
  [◓] feature-auth                          3m     ← 动画图标 + status_since
      Writing authentication middleware...          ← last_line（真实终端输出）

数据来源：
  - PaneSummaryModel（Entity）：per-pane { status, last_line, status_since }
  - pane_statuses（HashMap）：per-pane AgentStatus（用于聚合）
  - running_animation_frame：Running 状态动画帧

Running 动画：
  帧序列 ◐ → ◓ → ● → ◑，250ms/帧
  由 AppRoot.manage_running_animation() 管理 timer 生命周期：
    - has_running=true + 无 timer → 启动
    - has_running=false + 有 timer → 停止
  动画帧通过 cx.notify() 驱动 Sidebar 重绘

status_since 显示：
  < 1 min → "Just now"
  < 1 hour → "Nm"
  < 1 day → "Nh"
  ≥ 1 day → "Nd"
```

### 15.8 数据模型与文件

| 模块 | 文件 | 职责 |
|------|------|------|
| AgentStatus | `src/agent_status.rs` | 状态枚举、优先级、StatusCounts 聚合 |
| ShellIntegration | `src/shell_integration.rs` | OSC 133 解析器、ShellPhase/ShellState |
| ContentExtractor | `src/terminal/content_extractor.rs` | 终端输出解析：OSC 133 + 可见文本提取 + `extract_last_line()` |
| StatusDetector | `src/status_detector.rs` | 三级优先级状态检测 + DebouncedStatusTracker 去抖 |
| StatusPublisher | `src/runtime/status_publisher.rs` | 事件驱动发布：状态变化 → EventBus（含 last_line、prev_state） |
| EventBus | `src/runtime/event_bus.rs` | 发布/订阅：AgentStateChange、Notification、TerminalOutput |
| PaneSummaryModel | `src/ui/models/pane_summary_model.rs` | GPUI Entity：per-pane { status, last_line, status_since } |
| StatusCountsModel | `src/ui/models/status_counts_model.rs` | GPUI Entity：per-worktree 聚合计数（TopBar 用） |
| NotificationManager | `src/notification_manager.rs` | 通知存储、合并去重、已读/未读 |
| SystemNotifier | `src/system_notifier.rs` | macOS 桌面通知（notify-rust） |

### 15.9 已知限制与改进方向

| 问题 | 现状 | 改进方向 |
|------|------|----------|
| 文本检测误报 | 终端输出中含 "error"/"waiting" 等关键词可能误判 | 增加上下文权重、agent 特定模式（claude/cursor/opencode） |
| Idle prompt 检测 | 内置 `❯$%#>` + git prompt + user@host 模式，最后 3 行匹配即 Idle | 支持 `add_idle_pattern()` 自定义 prompt 模式 |
| 检测窗口 | 最后 15 行（check_line_count=15），旧文本快速滑出检测范围 | 可调节 `with_line_count()` |
| OSC 133 依赖 | 需要用户 shell 配置 OSC 133 才有精确的 phase 信息 | 提供 `docs/shell-integration.md` 配置引导 |
| 去抖延迟 | 普通状态变化需 2 次检测才提交，存在 ~500ms 延迟 | 可调节 debounce_threshold |
| last_line 截断 | `extract_last_line` 按字节截断，多字节 UTF-8 可能断裂 | 改用 char boundary 截断 |
| 多 agent 类型 | 当前 pattern 偏向 claude/opencode | 支持 per-agent-type pattern 配置 |
