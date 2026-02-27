# Phase 2 — Runtime Layer 抽离

> 参考：design.md §4 目标架构、§10 核心数据模型、§12 待定事项与决策
> **TDD**: 先写失败测试再实现。**Subagent-driven**: 可使用 `subagent-driven-development` skill 将 Task 委托给子 agent 并行实施。

**目标**：UI 不再直接调用 `tmux::*`，所有操作通过 `AgentRuntime` API。tmux 降级为 backend adapter，实现 local PTY adapter 作为可选。

**预估**：3~5 天

---

## 前置条件

- Phase 1 完成（streaming terminal 已接入）
- 现有 tmux session、pane、window 逻辑可运行

---

## Task 1: 定义 AgentRuntime trait 与类型

**Files:**
- Create: `src/runtime/agent_runtime.rs`
- Modify: `src/runtime/mod.rs`

**Step 1: 定义核心类型**

```rust
// agent_runtime.rs
pub type AgentId = String;
pub type PaneId = String;

pub struct TerminalEvent {
    pub bytes: Vec<u8>,
    pub pane_id: PaneId,
    pub timestamp: std::time::Instant,
    pub event_type: TerminalEventType,
}

pub struct AgentStateChange {
    pub agent_id: AgentId,
    pub state: AgentState,
}

pub trait AgentRuntime: Send + Sync {
    fn send_input(&self, pane_id: &PaneId, bytes: &[u8]) -> Result<()>;
    fn resize(&self, pane_id: &PaneId, cols: u16, rows: u16) -> Result<()>;
    fn subscribe_output(&self, pane_id: &PaneId) -> impl Stream<Item = TerminalEvent>;
    fn subscribe_state(&self) -> impl Stream<Item = AgentStateChange>;
    fn list_panes(&self, agent_id: &AgentId) -> Vec<PaneId>;
    fn open_diff(&self, worktree: &Path, pane_id: Option<&PaneId>) -> Result<()>;
    fn open_review(&self, worktree: &Path) -> Result<()>;
    fn restart(&self, agent_id: &AgentId) -> Result<()>;
    fn recover(&self, agent_ids: Option<Vec<AgentId>>) -> Result<()>;
}
```

**Step 2: 定义 PtyHandle trait**

```rust
pub trait PtyHandle: Send + Sync {
    fn write(&self, bytes: &[u8]) -> Result<()>;
    fn resize(&self, cols: u16, rows: u16) -> Result<()>;
    fn subscribe_output(&self) -> impl Stream<Item = TerminalEvent>;
}
```

---

## Task 2: 实现 tmux_adapter

**Files:**
- Create: `src/backends/mod.rs` 或 `src/runtime/backends/tmux.rs`
- 封装现有 `tmux::session`、`tmux::pane`、`tmux::window`

**Step 1: TmuxRuntime 实现 AgentRuntime**

- `send_input` → 内部调用 `tmux send-keys`（Phase 4 前）或 PTY write
- `resize` → `tmux resize-pane`
- `subscribe_output` → 使用 Phase 1 的 PtyBridge（pipe-pane）
- `list_panes` → `tmux list-panes`
- `open_diff` / `open_review` → 封装 `tmux new-window` + `send-keys` nvim 命令
- `restart` → 重启 agent 进程（tmux 内）
- `recover` → `tmux has-session` + attach

**Step 2: 隐藏 tmux 模块**

- UI 不 `use crate::tmux::*`
- 所有 tmux 调用仅在 `tmux_adapter` 内部

---

## Task 3: 实现 local PTY adapter（可选，优先）

**Files:**
- Create: `src/runtime/backends/local_pty.rs`

**Step 1: LocalPtyRuntime**

- 使用 `nix::pty` 或 `portable-pty` 创建 PTY
- 直接 spawn 进程到 PTY，读取 master 输出
- 实现 `AgentRuntime` trait

**Step 2: Backend 选择**

- config.json 或环境变量 `PMUX_BACKEND=tmux|local_pty`
- 默认 `tmux`

---

## Task 4: 重构 app_root 移除 tmux 依赖

**Files:**
- Modify: `src/ui/app_root.rs`

**Step 1: 替换依赖**

- 删除 `use crate::tmux::{session, pane, window, control_mode_attach}`
- 添加 `use crate::runtime::AgentRuntime` 或持有 `Arc<dyn AgentRuntime>`

**Step 2: 替换调用**

| 原调用 | 替换为 |
|--------|--------|
| `Session::new()` | `runtime.create_session()` 或等价 |
| `tmux_pane::capture_pane` | 已由 Phase 1 移除 |
| `tmux_pane::list_panes_for_window` | `runtime.list_panes(agent_id)` |
| `tmux_pane::get_pane_dimensions` | `runtime.resize` 或等价 |
| `tmux_pane::select_pane` | `runtime.focus_pane(pane_id)` 或等价 |
| `tmux_pane::split_pane_*` | `runtime.split_pane(pane_id, vertical)` |
| `tmux_window::create_window_with_command` | `runtime.open_diff` / `open_review` |
| `InputHandler.send_key_to_target` | `runtime.send_input(pane_id, bytes)` |

**Step 3: 验证**

- `rg "tmux::" src/ui/` 应无结果
- `rg "crate::tmux" src/ui/` 应无结果

---

## Task 5: 依赖检测按 backend

**Files:**
- Modify: `src/deps.rs` 或启动页逻辑

**Step 1: 分 backend 检测**

- tmux backend：检查 `tmux -V`
- local_pty backend：检查 PTY 功能（可用 `nix::pty::openpty` 或等价）

**Step 2: 启动页根据 backend 显示**

- 若选择 tmux 且未安装，提示安装 tmux
- 若选择 local_pty，提示系统要求

---

## 验收

- [ ] UI 不包含 `tmux::` 或 `crate::tmux` 调用
- [ ] 通过 config 可切换 tmux / local_pty 后端
- [ ] 多 workspace、多 worktree、多 pane 流程正常
- [ ] `cargo test` 通过
