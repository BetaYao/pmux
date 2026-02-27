# pmux — AI Agent 多分支开发工作台

## 1. 背景与目标

pmux 是 AI Agent 多分支开发工作台：管理多个 AI Agent 并行工作（每 agent 一个 git worktree），实时监控状态，主动通知，快速 Review Diff。

**重构目标**：从「tmux 的 GUI 前端」升级为「AI Agent Runtime + 多终端工作台」。

---

## 2. UI 操作大方向（不变）

重构**不改变**以下用户流程与能力，仅调整底层实现：

| # | 能力 | 说明 |
|---|------|------|
| 1 | 启动页依赖检测 | 启动时检查 tmux 等依赖，失败则提示；实现方式可优化（如通过 Runtime 检测） |
| 2 | 启动页打开 workspace | 选择 git 目录 → 打开 workspace |
| 3 | 多 workspace 支持 | 多个仓库 Tab 切换，每个 workspace = 一个 session |
| 4 | 多 worktree 支持 | 可新增 worktree 分支（[+ New Branch]），每个 worktree 一个 agent |
| 5 | 独立 terminal + 多 pane | 每个 worktree 有独立终端，支持 ⌘D/⌘⇧D 分屏，多 pane 并行 |
| 6 | 关闭 GUI 后台不关 | 关闭 pmux 窗口后，agent 继续在后台运行，重开可恢复 |
| 7 | Agent 进展通知 | 每个 worktree 的 agent 状态变化（Waiting/Error 等）有通知（面板 + 系统） |

---

## 3. 核心原则

| 原则 | 说明 |
|------|------|
| **Terminal = Stream** | 处理 RAW PTY BYTES，而非 screen text snapshot |
| **UI 不知道 tmux** | UI 只依赖 AgentRuntime API |
| **Agent 是一等公民** | Agent 封装 lifecycle、tty、状态机；tmux 只是 backend 之一 |

---

## 4. 目标架构

```
pmux
├── UI (GPUI)           ← 渲染、交互、通知
├── Agent Runtime       ← Event Bus、PTY Engine、State Machine
└── Backends            ← tmux / local pty / docker / ssh
```

**依赖方向**：`UI → Runtime API → Backend Adapter`。UI 禁止直接调用 `tmux::*`。

```
                    ┌─────────────────────────────────────┐
                    │           AgentRuntime API           │
                    │  send_input / resize / subscribe_*   │
                    └─────────────────┬───────────────────┘
                                      │
         ┌────────────────────────────┼────────────────────────────┐
         ▼                            ▼                            ▼
   ┌───────────┐              ┌──────────────┐              ┌──────────────┐
   │   tmux    │              │  local pty   │              │ docker/ssh   │
   │  adapter  │              │   adapter    │              │   adapter    │
   └───────────┘              └──────────────┘              └──────────────┘
```

UI 只依赖 Runtime API；Backend 可插拔，接入新 backend 无需改 UI。

---

## 5. 技术栈

### 保留

| crate | 用途 |
|-------|------|
| gpui / gpui_platform | UI 框架 |
| alacritty_terminal | 终端仿真（改为 streaming 模式） |
| tokio / blocking | 异步与 IO |
| serde / serde_json | 配置与序列化 |
| notify-rust | 系统通知 |
| rfd | 文件选择 |
| thiserror | 错误处理 |
| git_utils | Git 操作 |

### 调整

| 现有 | 改为 |
|------|------|
| terminal_poller / status_poller / capture-pane | PTY stream + Event Bus |
| tmux send-keys | xterm escape → PTY write |
| tmux 直接调用 | 通过 Runtime API，tmux 作为 backend adapter |

### 新增

- `src/runtime/`：`agent_runtime.rs`、`pty_bridge.rs`
- Event Bus：Agent 状态、Terminal 输出、通知

---

## 6. 数据流（目标）

### 6.1 Event Bus

事件类型（便于实现）：

| 事件类型 | 说明 | 订阅方 |
|----------|------|--------|
| `AgentStateChange` | Agent 状态变化（Running / Waiting / Error 等） | Sidebar、StatusBar |
| `TerminalOutput` | 终端字节流 / Grid 更新，携带 `pane_id` | TerminalView |
| `Notification` | 需提醒用户（等待输入、错误等） | Notification 面板、系统通知 |

实现：tokio `broadcast`（多订阅者）或 `mpsc`。Runtime 发布，UI 订阅。

### 6.2 PTY Streaming

**模式**：tmux backend 使用 `tmux pipe-pane -o` 输出 RAW BYTE STREAM；local pty backend 直接读 PTY master。

**必须正确处理**（保证 vim / TUI 正常）：
- **alternate screen**：vim 等全屏 TUI 切换主/备屏
- **双宽字符**：CJK 等宽字符
- **光标位置**：由 alacritty_terminal 解析 VT 序列维护

```
RAW BYTE STREAM (pipe-pane / PTY)
  → alacritty_terminal::Processor  ← 解析 ANSI、alternate screen、光标
  → Terminal Grid
  → UI Render
```

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

### 7.2 UI 数据获取模式

```
Event Bus (subscribe)
       │
       ├─ AgentStateChange  → Sidebar 状态图标、StatusBar 聚合
       ├─ TerminalOutput    → TerminalView 渲染
       └─ Notification     → 通知面板、系统 notify
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

| Phase | 内容 | 预估 |
|-------|------|------|
| **1** | Streaming Terminal：pty_bridge、pipe-pane 流式、删除 capture-pane / polling | 2~3 天 |
| **2** | Runtime 抽离：UI → Runtime API，tmux + local PTY adapter | 3~5 天 |
| **3** | Agent Runtime：Agent 模型、Event Bus、PtyHandle trait、移除 pane_status_tracker | 1 周 |
| **4** | 输入重写：xterm escape → PTY write，支持鼠标 / TUI | 2 天 |

**暂停**：Runtime 稳定前暂停 UI polish、diff overlay、sidebar 优化、notification 扩展。

---

## 9. 成功标志

- [ ] 关闭 pmux UI，agent 继续运行
- [ ] vim / TUI 在 pmux 内完全正常
- [ ] 无 polling loop
- [ ] UI 不包含 tmux 调用
- [ ] 新 backend 可在不改 UI 情况下接入

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
| **Control mode vs Pipe-pane** | Phase 1 统一用 pipe-pane 流式模式；control mode 作为 fallback 或用于复杂 TUI。 |
| **Event Bus 与 GPUI 线程** | tokio mpsc channel + spawn；Event Bus 在 async runtime，UI main thread 通过 channel 拉取事件后 `cx.notify()`，避免 UI 直接 await IO。 |
| **Stream 类型** | `subscribe_output` 返回 `futures::Stream`，可组合 `async_stream` 实现。`TerminalEvent` 字段：`bytes`, `pane_id`, `timestamp`, `event_type`。 |
| **PtyHandle 抽象** | 定义 `trait PtyHandle { write(&[u8]); resize(u16,u16); subscribe_output() }`。各 backend 实现该 trait。 |

### Backend 与配置

| 待定项 | 决策 |
|--------|------|
| **Backend 选择** | 通过 config.json 或环境变量指定；UI 可显示 backend 选项（非必需）。Phase 1 默认 tmux adapter。 |
| **其他 backend 优先级** | Phase 1–2 先做 tmux + local PTY；docker/ssh 作为 Phase 3–4 可选扩展。 |
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
