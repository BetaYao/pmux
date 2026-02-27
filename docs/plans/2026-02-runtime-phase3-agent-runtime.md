# Phase 3 — Agent Runtime

> 参考：design.md §6.1 Event Bus、§6.5 Agent 状态、§10 核心数据模型、§12 待定事项与决策
> **TDD**: 先写失败测试再实现。**Subagent-driven**: 可使用 `subagent-driven-development` skill 将 Task 委托给子 agent 并行实施。

**目标**：Agent 作为一等公民，引入 Event Bus，状态来源从 terminal 文本解析改为 process lifecycle。移除 `pane_status_tracker`、`status_detector`、`status_poller`。

**预估**：1 周

---

## 前置条件

- Phase 2 完成（Runtime API、tmux adapter 已接入）
- 现有 status 轮询可运行

---

## Task 1: 实现 Event Bus

**Files:**
- Create: `src/runtime/event_bus.rs`
- Modify: `src/runtime/mod.rs`

**Step 1: 定义事件类型**

```rust
pub enum RuntimeEvent {
    AgentStateChange(AgentStateChange),
    TerminalOutput(TerminalOutput),
    Notification(Notification),
}

pub struct AgentStateChange {
    pub agent_id: AgentId,
    pub pane_id: Option<PaneId>,
    pub state: AgentState,
}

pub struct TerminalOutput {
    pub pane_id: PaneId,
    pub bytes: Vec<u8>,
    pub timestamp: Instant,
}

pub struct Notification {
    pub agent_id: AgentId,
    pub message: String,
    pub notif_type: NotificationType,
}
```

**Step 2: 实现 Event Bus**

- 使用 `tokio::sync::broadcast` 或 `mpsc`
- `pub fn publish(&self, event: RuntimeEvent)`
- `pub fn subscribe(&self) -> impl Stream<Item = RuntimeEvent>`
- 支持按事件类型过滤（可选）

**Step 3: GPUI 线程桥接**

- Event Bus 在 tokio runtime 中
- 后台 task 从 subscribe 拉取事件，通过 `std::sync::mpsc` 发到 main thread
- AppRoot 在 `update` 或定时检查 channel，收到事件后 `cx.notify()`

---

## Task 2: Agent 模型与状态机

**Files:**
- Create: `src/runtime/agent.rs`
- Modify: `src/runtime/agent_runtime.rs`

**Step 1: Agent 结构**

```rust
pub struct Agent {
    pub id: AgentId,
    pub worktree: PathBuf,
    pub state: AgentState,
    pub panes: Vec<PaneHandle>,
}

pub enum AgentState {
    Starting,
    Running,
    WaitingInput,
    Error { message: String },
    Exited { code: Option<i32> },
}
```

**Step 2: 状态来源**

- **主来源**：process lifecycle
  - tmux backend：pane 存活 + 进程 exit code（若可获取）
  - local_pty：子进程 `wait()`、exit code、stderr
- **WaitingInput**：PTY blocking 或 Agent 内部状态机（可选：保留轻量文本解析作 fallback）
- **Error**：exit code != 0 或 stderr 捕获

**Step 3: 状态变化发布**

- Runtime 内部检测到状态变化 → `event_bus.publish(AgentStateChange)`
- 移除对 `status_detector` 的依赖

---

## Task 3: 移除 status_poller / pane_status_tracker / status_detector

**Files:**
- Modify: `src/lib.rs`
- Modify: `src/ui/app_root.rs`
- Delete 或 Deprecate: `src/status_poller.rs`, `src/pane_status_tracker.rs`, `src/status_detector.rs`

**Step 1: 移除 StatusPoller**

- 从 app_root 删除 `status_poller` 字段及相关逻辑
- 删除 500ms 状态轮询线程
- 删除 `on_status_change` callback 注册

**Step 2: 替换状态来源**

- AppRoot 订阅 `Event Bus (AgentStateChange)`
- 收到事件后更新 `pane_statuses` HashMap
- 调用 `cx.notify()` 触发 Sidebar、StatusBar 重绘

**Step 3: 移除 pane_status_tracker**

- 状态由 Runtime 管理，通过 Event Bus 推送
- 删除 `PaneStatusTracker`、`DebouncedStatusTracker`

**Step 4: 移除或保留 status_detector**

- 若完全用 process lifecycle，可删除
- 若保留文本解析作 fallback，迁移到 Runtime 内部，不暴露给 UI

**Step 5: 清理**

- 从 `lib.rs` 移除 `status_poller`、`pane_status_tracker`、`status_detector` 模块
- `cargo build` 通过

---

## Task 4: state.json 与 recover 映射

**Files:**
- Modify: `src/app_state.rs` 或 `src/window_state.rs`
- Modify: `src/runtime/` 中 recover 逻辑

**Step 1: 新 schema**

```json
{
  "workspaces": [
    {
      "path": "/path/to/repo",
      "worktrees": [
        {
          "branch": "feat-x",
          "path": "/path/to/repo-feat-x",
          "agent_id": "agent-xxx",
          "pane_ids": ["pane-0"],
          "backend_session_id": "sdlc-repo",
          "backend_window_id": "@0"
        }
      ]
    }
  ]
}
```

**Step 2: recover() 实现**

- 读取 state.json
- 对每个 workspace/worktree，按 `backend_session_id` 调用 tmux `has-session`
- 存在则 attach，不存在则 spawn 新 session
- 发布 `AgentStateChange` 到 Event Bus

**Step 3: 迁移工具**

- 若旧 state.json 格式不同，提供 `migrate_state()` 或兼容解析

---

## Task 5: 通知集成

**Files:**
- Modify: `src/ui/app_root.rs`
- Modify: `src/notification_manager.rs`

**Step 1: 从 Event Bus 接收 Notification**

- Runtime 在检测到 Waiting/Error 时发布 `Notification` 事件
- AppRoot 订阅后调用 `NotificationManager.add()`、`system_notifier`

**Step 2: 保持 UI 操作不变**

- 通知面板、系统通知、Sidebar 红点逻辑不变
- 仅数据来源从 poller 改为 Event Bus

---

## 验收

- [ ] 无 status polling（grep 无 `status_poller`、`capture_pane` 做状态检测）
- [ ] Agent 状态由 process lifecycle 驱动，Event Bus 推送
- [ ] 关闭 pmux 重开，recover 正确恢复 session
- [ ] 通知（Waiting/Error）正常触发
- [ ] `cargo test` 通过
