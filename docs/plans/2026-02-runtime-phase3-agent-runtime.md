# Phase 3 — Agent Runtime

> 参考：design.md §6.1 Event Bus、§6.5 Agent 状态、§10 核心数据模型、§12 待定事项与决策
> **TDD**: 先写失败测试再实现。**Subagent-driven**: 可使用 `subagent-driven-development` skill 将任务委托给子 agent 并行实施。

**目标**：Agent 作为一等公民，引入 Event Bus，状态来源从 terminal 文本解析改为 process lifecycle。移除 `pane_status_tracker`、`status_detector`、`status_poller`。

**预估**：1 周

---

## 前置条件

- [ ] Phase 2 完成（Runtime API、tmux adapter 已接入，UI 不直接调用 tmux）
- 现有 status 轮询可运行（StatusPoller、PaneStatusTracker、StatusDetector）
- 当前状态流：StatusPoller (500ms) → PaneStatusTracker → StatusDetector → pane_statuses HashMap → Sidebar/StatusBar

---

## 实施顺序与依赖

```
Task 1 (Event Bus) ──┬──> Task 2 (Agent 模型) ──> Task 3 (移除 status_poller)
                     │
                     └──> Task 5 (通知集成)
                     
Task 4 (state.json) 可独立进行，与 Task 3 并行
```

**建议顺序**：Task 1 → Task 2 → Task 3 → Task 5。Task 4 可与 Task 3 并行。

---

## Task 1: 实现 Event Bus

**Files:**
- Create: `src/runtime/event_bus.rs`
- Modify: `src/runtime/mod.rs`

**Step 1: 写失败测试（TDD）**

- 测试：`EventBus::new()` 存在，`publish` / `subscribe` 可收发事件
- `cargo test runtime::event_bus` 应失败

**Step 2: 定义事件类型**

```rust
// src/runtime/event_bus.rs
use std::time::Instant;

use crate::agent_status::AgentStatus;
use crate::runtime::agent_runtime::{AgentId, PaneId};

#[derive(Clone, Debug)]
pub enum RuntimeEvent {
    AgentStateChange(AgentStateChange),
    TerminalOutput(TerminalOutput),
    Notification(Notification),
}

#[derive(Clone, Debug)]
pub struct AgentStateChange {
    pub agent_id: AgentId,
    pub pane_id: Option<PaneId>,
    pub state: AgentStatus,
}

#[derive(Clone, Debug)]
pub struct TerminalOutput {
    pub pane_id: PaneId,
    pub bytes: Vec<u8>,
    pub timestamp: Instant,
}

#[derive(Clone, Debug)]
pub struct Notification {
    pub agent_id: AgentId,
    pub message: String,
    pub notif_type: NotificationType,
}

#[derive(Clone, Debug)]
pub enum NotificationType {
    WaitingInput,
    Error,
    Info,
}
```

**Step 3: 实现 Event Bus**

- 使用 `tokio::sync::broadcast`（多订阅者）或 `flume`（若需 Sync）
- `pub fn publish(&self, event: RuntimeEvent)`
- `pub fn subscribe(&self) -> flume::Receiver<RuntimeEvent>` 或 `impl Stream`
- 容量建议 64~256，避免慢消费者阻塞

**Step 4: GPUI 线程桥接**

- Event Bus 在 tokio/blocking 线程中
- 后台 task：`subscribe().recv()` → `std::sync::mpsc::Sender` 发到 main thread
- AppRoot 在 `update` 或 `cx.on_frame` 检查 channel，收到后更新 state 并 `cx.notify()`

**Step 5: 验证**

- `cargo test runtime::event_bus` 通过
- 集成测试：publish 后 subscribe 能收到

---

## Task 2: Agent 模型与状态机

**Files:**
- Create: `src/runtime/agent.rs`
- Modify: `src/runtime/mod.rs`
- Modify: `src/runtime/backends/tmux.rs`（发布 AgentStateChange）

**Step 1: Agent 结构**

```rust
// src/runtime/agent.rs
use std::path::PathBuf;
use crate::agent_status::AgentStatus;

pub type AgentId = String;

#[derive(Clone, Debug)]
pub struct Agent {
    pub id: AgentId,
    pub worktree: PathBuf,
    pub state: AgentStatus,
    pub panes: Vec<String>,  // PaneId list
}
```

**Step 2: 状态来源（替代 status_detector 文本解析）**

| 状态 | 主来源 |
|------|--------|
| Running | tmux pane 存活 + 进程在运行 |
| WaitingInput | PTY blocking（可选）或保留轻量文本 fallback |
| Error | process exit code != 0 或 stderr 捕获 |
| Exited | process 已退出 |
| Unknown | 无法检测时 |

**tmux backend 实现**：
- 检测 pane 存活：`tmux list-panes -t <target>` 有输出
- 进程 exit：tmux 内进程退出时，可通过 `tmux display-message` 或 control mode 获取（若支持）
- 简化方案：先保留「pane 存活 = Running」，exit 通过 tmux 事件或轮询 pane 是否存在（低频，如 2s）

**Step 3: 状态变化发布**

- TmuxRuntime 或 Agent 管理逻辑检测到状态变化
- 调用 `event_bus.publish(RuntimeEvent::AgentStateChange(...))`
- 移除对 `StatusDetector` 的调用

**Step 4: 验证**

- Agent 结构可序列化/反序列化（若需 state.json）
- 状态变化能通过 Event Bus 发布

---

## Task 3: 移除 status_poller / pane_status_tracker / status_detector

**Files:**
- Modify: `src/lib.rs`
- Modify: `src/ui/app_root.rs`
- Delete: `src/status_poller.rs`, `src/pane_status_tracker.rs`
- Delete 或迁移: `src/status_detector.rs`

**Step 1: 移除 StatusPoller**

- 从 AppRoot 删除 `status_poller: Option<Arc<Mutex<StatusPoller>>>` 字段
- 删除 500ms 状态轮询 spawn 逻辑
- 删除 `on_status_change` callback 注册

**Step 2: 替换状态来源**

- AppRoot 订阅 `EventBus` 的 `AgentStateChange`
- 收到事件后更新 `pane_statuses: HashMap<String, AgentStatus>`
- 调用 `update_status_counts()` 和 `cx.notify()`

**Step 3: 移除 pane_status_tracker**

- 删除 `PaneStatusTracker`、`DebouncedStatusTracker`
- 状态由 Event Bus 推送，直接写入 `pane_statuses`

**Step 4: 移除或保留 status_detector**

- 若完全用 process lifecycle，删除 `status_detector.rs`
- 若保留文本解析作 fallback，迁移到 `src/runtime/` 内部，不暴露给 UI

**Step 5: 清理**

- `lib.rs` 移除 `pub mod status_poller`、`pub mod pane_status_tracker`、`pub mod status_detector`
- `cargo build` 通过
- `rg "status_poller" src/` 应无结果
- `rg "pane_status_tracker" src/` 应无结果
- `rg "status_detector" src/` 应无结果（或仅在 runtime 内）

**Step 6: 验证**

- Sidebar 状态图标、StatusBar 聚合仍正常
- 无 500ms 轮询（可 `grep -r "500" src/` 或 `grep "status_poller"` 确认）

---

## Task 4: state.json 与 recover 映射

**Files:**
- Modify: `src/app_state.rs` 或新建 `src/runtime/state.rs`
- Modify: `src/window_state.rs`（若存在）
- Modify: `src/runtime/backends/tmux.rs`（recover 逻辑）

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
          "pane_ids": ["%0"],
          "backend": "tmux",
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
- 对每个 workspace/worktree，按 `backend_session_id` 调用 `tmux has-session`
- 存在则 attach（恢复 Runtime、subscribe_output）
- 不存在则 spawn 新 session
- 发布 `AgentStateChange` 到 Event Bus

**Step 3: 迁移工具**

- 若旧 state 格式不同，提供 `migrate_state()` 或兼容解析

**Step 4: 验证**

- 关闭 pmux 重开，recover 正确恢复 session
- 多 worktree 状态正确恢复

---

## Task 5: 通知集成

**Files:**
- Modify: `src/ui/app_root.rs`
- Modify: `src/notification_manager.rs`（若有）

**Step 1: 从 Event Bus 接收 Notification**

- Runtime 在检测到 Waiting/Error 时发布 `RuntimeEvent::Notification`
- AppRoot 订阅后调用 `NotificationManager.add()`、`system_notifier`

**Step 2: 保持 UI 操作不变**

- 通知面板、系统通知、Sidebar 红点逻辑不变
- 仅数据来源从 StatusPoller 改为 Event Bus

**Step 3: 验证**

- Waiting/Error 状态变化时，通知正常触发

---

## 验收

- [ ] 无 status polling（`grep` 无 `status_poller`、无 `capture_pane` 做状态检测）
- [ ] Agent 状态由 process lifecycle 驱动，Event Bus 推送
- [ ] 关闭 pmux 重开，recover 正确恢复 session
- [ ] 通知（Waiting/Error）正常触发
- [ ] Sidebar、StatusBar 状态显示正常
- [ ] `cargo run` 正常
- [ ] `cargo test` 通过（SIGBUS 除外）
