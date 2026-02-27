# Phase 2 — Runtime Layer 抽离

> 参考：design.md §4 目标架构、§10 核心数据模型、§12 待定事项与决策
> **TDD**: 先写失败测试再实现。**Subagent-driven**: 可使用 `subagent-driven-development` skill 将 Task 委托给子 agent 并行实施。

**目标**：UI 不再直接调用 `tmux::*`，所有操作通过 `AgentRuntime` API。tmux 降级为 backend adapter，实现 local PTY adapter 作为可选。

**预估**：3~5 天

---

## 前置条件

- [x] Phase 1 完成（streaming terminal、PtyBridge、pipe-pane 已接入）
- 现有 tmux session、pane、window 逻辑可运行
- `app_root.rs` 当前直接调用：`Session`、`tmux_pane::*`、`tmux_window::*`、`control_mode_attach`、`InputHandler`

---

## 实施顺序与依赖

```
Task 1 (AgentRuntime trait) ──┬──> Task 2 (tmux_adapter) ──> Task 4 (app_root 重构)
                              │
                              └──> Task 3 (local_pty, 可选)
```

**建议顺序**：Task 1 → Task 2 → Task 4 → Task 5。Task 3 可与 Task 4 并行或延后。

---

## Task 1: 定义 AgentRuntime trait 与类型

**Files:**
- Create: `src/runtime/agent_runtime.rs`
- Modify: `src/runtime/mod.rs`

**Step 1: 写失败测试（TDD）**

- 在 `src/runtime/agent_runtime.rs` 中写 `#[cfg(test)] mod tests`
- 测试：`AgentRuntime` trait 存在，`TmuxRuntime` 实现该 trait（占位实现）
- `cargo test runtime::agent_runtime` 应失败（模块未实现）

**Step 2: 定义核心类型**

```rust
// src/runtime/agent_runtime.rs
use std::path::Path;

pub type AgentId = String;
pub type PaneId = String;

#[derive(Clone, Debug)]
pub struct TerminalEvent {
    pub bytes: Vec<u8>,
    pub pane_id: PaneId,
    pub timestamp: std::time::Instant,
}

#[derive(Clone, Debug)]
pub struct AgentStateChange {
    pub agent_id: AgentId,
    pub state: crate::agent_status::AgentStatus,
}

pub trait AgentRuntime: Send + Sync {
    fn send_input(&self, pane_id: &PaneId, bytes: &[u8]) -> Result<(), RuntimeError>;
    fn resize(&self, pane_id: &PaneId, cols: u16, rows: u16) -> Result<(), RuntimeError>;
    fn subscribe_output(&self, pane_id: &PaneId) -> Option<flume::Receiver<Vec<u8>>>;
    fn list_panes(&self, agent_id: &AgentId) -> Vec<PaneId>;
    fn focus_pane(&self, pane_id: &PaneId) -> Result<(), RuntimeError>;
    fn split_pane(&self, pane_id: &PaneId, vertical: bool) -> Result<PaneId, RuntimeError>;
    fn open_diff(&self, worktree: &Path, pane_id: Option<&PaneId>) -> Result<(), RuntimeError>;
    fn open_review(&self, worktree: &Path) -> Result<(), RuntimeError>;
    fn kill_window(&self, window_target: &str) -> Result<(), RuntimeError>;
}

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("backend error: {0}")]
    Backend(String),
    #[error("pane not found: {0}")]
    PaneNotFound(String),
}
```

**注意**：`subscribe_output` 先返回 `flume::Receiver`（与 Phase 1 PtyBridge 一致），Phase 3 再统一为 Event Bus Stream。

**Step 3: 定义 PtyHandle trait（可选，供 backend 内部用）**

```rust
pub trait PtyHandle: Send + Sync {
    fn write(&self, bytes: &[u8]) -> Result<(), RuntimeError>;
    fn resize(&self, cols: u16, rows: u16) -> Result<(), RuntimeError>;
    fn subscribe_output(&self) -> Option<flume::Receiver<Vec<u8>>>;
}
```

**Step 4: 验证**

- `cargo build` 通过
- `cargo test runtime` 通过

---

## Task 2: 实现 tmux_adapter

**Files:**
- Create: `src/runtime/backends/mod.rs`
- Create: `src/runtime/backends/tmux.rs`
- Modify: `src/runtime/mod.rs`

**Step 1: 创建 backends 模块**

`src/runtime/backends/mod.rs`:
```rust
mod tmux;
pub use tmux::TmuxRuntime;
```

**Step 2: TmuxRuntime 实现 AgentRuntime**

封装现有 `crate::tmux::*` 调用，不改变 tmux 逻辑，仅做转发：

| AgentRuntime 方法 | 内部实现 |
|-------------------|----------|
| `send_input` | `InputHandler::send_key_to_target_with_literal` 或 `tmux send-keys` |
| `resize` | `tmux_pane::resize_pane` |
| `subscribe_output` | `PtyBridge::new` + `subscribe_output`，或 control_mode fallback |
| `list_panes` | `tmux_pane::list_panes_for_window` |
| `focus_pane` | `tmux_pane::select_pane` |
| `split_pane` | `tmux_pane::split_pane_vertical` / `split_pane_horizontal` |
| `open_diff` | `tmux_window::create_window_with_command` + nvim diffview |
| `open_review` | `tmux_window::create_window_with_command` + nvim review |
| `kill_window` | `tmux_window::kill_window` |

**Step 3: TmuxRuntime 构造**

- 持有 `session_name: String`、`window_name: String`、`InputHandler`、`PtyBridge` 或 control_mode handle
- 提供 `TmuxRuntime::new(session_name, window_name, ...)` 或 `from_session(session: &Session)`

**Step 4: 验证**

- `cargo test runtime::backends` 通过
- 单元测试：mock 或 `#[ignore]` 集成测试

---

## Task 3: 实现 local PTY adapter（可选，可延后）

**Files:**
- Create: `src/runtime/backends/local_pty.rs`
- Modify: `src/runtime/backends/mod.rs`

**Step 1: 添加依赖**

`Cargo.toml`:
```toml
portable-pty = "0.8"  # 或 nix = { version = "0.27", features = ["pty"] }
```

**Step 2: LocalPtyRuntime**

- 使用 `portable_pty::native_pty_system()` 创建 PTY
- `Command::new("shell").spawn()` 到 PTY
- 读取 PTY master 输出 → `flume::Sender`
- 实现 `AgentRuntime` trait

**Step 3: Backend 选择**

- `config.json`: `"backend": "tmux" | "local_pty"`
- 或环境变量 `PMUX_BACKEND=tmux|local_pty`
- 默认 `tmux`

**Step 4: 验证**

- `PMUX_BACKEND=local_pty cargo run` 可启动（若实现）

---

## Task 4: 重构 app_root 移除 tmux 依赖

**Files:**
- Modify: `src/ui/app_root.rs`
- Modify: `src/input_handler.rs`（可选：移入 runtime，或通过 Runtime 调用）

**Step 1: 引入 Runtime**

- 删除 `use crate::tmux::{session, pane, window, control_mode_attach}`
- 添加 `use crate::runtime::{AgentRuntime, backends::TmuxRuntime}`
- AppRoot 持有 `runtime: Arc<dyn AgentRuntime>` 或 `Option<Arc<TmuxRuntime>>`

**Step 2: 替换调用映射**

| 原调用 | 替换为 |
|--------|--------|
| `Session::new(name)` | `TmuxRuntime::new` 或 `runtime.create_session`（若 trait 扩展） |
| `session.ensure_in(path)` | 封装在 TmuxRuntime 构造中 |
| `tmux_pane::list_panes_for_window` | `runtime.list_panes(agent_id)` |
| `tmux_pane::get_pane_dimensions` | `runtime.resize` 调用时传入，或 trait 增加 `get_dimensions` |
| `tmux_pane::select_pane` | `runtime.focus_pane(pane_id)` |
| `tmux_pane::split_pane_vertical/horizontal` | `runtime.split_pane(pane_id, vertical)` |
| `tmux_window::create_window_with_command` | `runtime.open_diff` / `runtime.open_review` |
| `tmux_window::kill_window` | `runtime.kill_window(target)` |
| `InputHandler.send_key_to_target` | `runtime.send_input(pane_id, bytes)` |
| `PtyBridge::new` / control_mode | `runtime.subscribe_output(pane_id)` |

**Step 3: 保持 UI 流程不变**

- `start_tmux_session` → 创建 TmuxRuntime，调用 `runtime.subscribe_output` 等
- `switch_to_worktree` → 同上
- `handle_key_down` → `runtime.send_input`
- `split_pane` 回调 → `runtime.split_pane`
- Diff/Review 打开 → `runtime.open_diff` / `runtime.open_review`

**Step 4: 移除或隐藏 tmux 模块**

- UI 不 `use crate::tmux::*`
- 所有 tmux 调用仅在 `src/runtime/backends/tmux.rs` 内部

**Step 5: 验证**

- `rg "tmux::" src/ui/` 应无结果
- `rg "crate::tmux" src/ui/` 应无结果
- `rg "tmux_pane" src/ui/` 应无结果
- `rg "tmux_window" src/ui/` 应无结果
- `cargo run` 正常启动，多 workspace、多 worktree、多 pane 流程正常

---

## Task 5: 依赖检测按 backend

**Files:**
- Modify: `src/deps.rs`
- Modify: 启动页 / `loading_state.rs`（若有）

**Step 1: 分 backend 检测**

- tmux backend：检查 `tmux -V`
- local_pty backend：检查 PTY 功能（`portable_pty` 或 `nix::pty::openpty`）

**Step 2: 启动页根据 backend 显示**

- 若选择 tmux 且未安装，提示安装 tmux
- 若选择 local_pty，提示系统要求

**Step 3: 验证**

- `cargo run` 启动时依赖检测正常

---

## 验收

- [ ] UI 不包含 `tmux::`、`crate::tmux`、`tmux_pane`、`tmux_window` 调用
- [ ] 通过 config 可切换 tmux / local_pty 后端（若实现 Task 3）
- [ ] 多 workspace、多 worktree、多 pane 流程正常
- [ ] `cargo run` 正常
- [ ] `cargo test` 通过（SIGBUS 除外）
