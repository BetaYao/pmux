# P0: Session 命名统一 + UI 与 Backend 解耦

> **Brainstorming** - 设计决策与实施清单

## 1. 目标

1. 在 `AgentRuntime` 中增加 `session_info()` API，消除 UI 对 `TmuxRuntime` 的 downcast
2. 统一使用 `pmux-*` 作为 session 命名，替换所有 `sdlc-*`

---

## 2. 当前 Session 模型（统一为 pmux-* 后）

**create_runtime_from_env (tmux)** 已采用：
- `session_name` = `pmux-{worktree_path.file_name()}`，例如 `pmux-feature-x`
- `window_name` = `"main"`（主终端窗口）
- 一个 worktree 对应一个 tmux session

**open_diff / open_review**：
- 在**当前 worktree 的 session** 中新建窗口
- `window_name` = `review-{branch}`，例如 `review-feature-x`

---

## 3. session_info() API 设计

### 3.1 Trait 签名

```rust
// agent_runtime.rs
trait AgentRuntime {
    // ... existing methods ...

    /// Returns (session_id, window_id) for backends that support session persistence.
    /// - tmux: Some((session_name, window_name)) e.g. Some(("pmux-feature-x", "main"))
    /// - local_pty: None (no session to recover)
    fn session_info(&self) -> Option<(String, String)>;
}
```

### 3.2 各 Backend 实现

| Backend | 返回值 |
|---------|--------|
| TmuxRuntime | `Some((session_name.clone(), window_name.clone()))` |
| LocalPtyRuntime | `None` |

### 3.3 使用场景

- **save_runtime_state**：用 `rt.session_info()` 替代 downcast 获取 `backend_session_id`、`backend_window_id`
- **open_diff_overlay**：用 `rt.session_info()` 获取 session，构造 `pane_target = "{session}:{window}.0"`
- **close_diff_overlay**：需要 session 来构造 kill target。若 `diff_overlay_open` 存了 `pane_target`，可从中解析 `session:window`，或扩展 tuple 显式存 session

---

## 4. Session 命名工具函数

为避免多处重复推导 session 名称，在 `runtime/backends/mod.rs` 或新建 `runtime/session_naming.rs` 中提供：

```rust
/// Session naming for tmux backend. One worktree = one session.
/// Example: /foo/repo/feature-x -> "pmux-feature-x"
pub fn session_name_for_worktree(worktree_path: &Path) -> String {
    format!("pmux-{}", worktree_path
        .file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or_else(|| "default".into()))
}

/// Default main window name for a worktree session.
pub const MAIN_WINDOW: &str = "main";

/// Window target for killing the main worktree session.
/// Used when deleting a worktree (kill its session's main window).
pub fn main_window_target(worktree_path: &Path) -> String {
    format!("{}:{}", session_name_for_worktree(worktree_path), MAIN_WINDOW)
}
```

### 4.1 调用点

| 位置 | 当前 | 改为 |
|------|------|------|
| create_runtime_from_env | 内联 `pmux-{file_name}` | `session_name_for_worktree(worktree_path)` |
| confirm_delete_worktree | `sdlc-{repo}`, `{branch}` 作为 window | `main_window_target(&worktree.path)` |
| open_diff_overlay | `active_pane_target.split(':')` / `sdlc-workspace` | `rt.session_info().map(|(s,_)| s)` |
| close_diff_overlay | 同上 | 从 `diff_overlay_open` 的 session 或 pane_target 解析 |

---

## 5. confirm_delete_worktree 的修正

**当前错误逻辑**：
```rust
let session_name = format!("sdlc-{}", repo_name...);  // 错误：按 repo
let window_name = branch.replace('/', "-");           // 错误：branch 作 window
```

**正确逻辑**（pmux 模型：一 worktree 一 session）：
- 被删 worktree 的 path：`worktree.path`
- session = `pmux-{worktree.path.file_name()}`
- window = `"main"`
- target = `"{session}:main"`

**注意**：`confirm_delete_worktree` 删除的可能是**非当前** worktree，不能依赖 `self.runtime`，必须从 `worktree.path` 推导 target。因此需要 `main_window_target(worktree_path)` 工具函数。

---

## 6. open_diff_overlay / close_diff_overlay 的修正

### 6.1 open_diff_overlay

**当前**：从 `active_pane_target` 解析 session，local backend 时为 `"local"`，fallback 为 `"sdlc-workspace"`。

**修改**：
- 有 `self.runtime` 时，用 `rt.session_info()` 取 session
- 若 `session_info() == None`（local_pty），diff 行为不同：local 的 open_review 是发命令到当前 pane，不新建 tmux 窗口。此时 overlay 的 pane_target 可能需要不同处理（或 local 下暂不支持 diff overlay 的独立 pane）
- 为简单起见，可先处理 tmux：`if let Some((session, _)) = rt.session_info() { pane_target = format!("{}:{}", session, window_name) } else { /* local: 暂不建 overlay 或复用当前 pane */ }`

### 6.2 close_diff_overlay

**当前**：同样从 `active_pane_target` 取 session，可能已切换 worktree，导致 session 错误。

**修改**：在 **open** 时把 session 存进 `diff_overlay_open`：
- 将 `(branch, window_name, pane_target)` 扩展为 `(branch, window_name, session, pane_target)`，或
- 从 `pane_target` 格式 `"{session}:{window}.0"` 可解析出 session，避免改 tuple 结构

选择：存 `(branch, window_name, session, pane_target)` 更清晰，close 时直接 `format!("{}:{}", session, window_name)` 调用 `kill_window`。local backend 下 session 为空时，`kill_window` 为 no-op（local 的 kill_window 已实现为空操作）。

---

## 7. 需要更新的文件

| 文件 | 变更 |
|------|------|
| `src/runtime/agent_runtime.rs` | 添加 `session_info() -> Option<(String, String)>` |
| `src/runtime/backends/tmux.rs` | 实现 `session_info()`；测试中用 `pmux-*` |
| `src/runtime/backends/local_pty.rs` | 实现 `session_info()` 返回 `None` |
| `src/runtime/backends/mod.rs` | 添加 `session_name_for_worktree`, `main_window_target`；create_runtime_from_env 使用前者 |
| `src/ui/app_root.rs` | save_runtime_state 用 `session_info()` 替代 downcast；confirm_delete_worktree 用 `main_window_target`；open/close_diff_overlay 用 `session_info()` 或存储的 session |
| `src/new_branch_dialog.rs` | 注释改为 `pmux-{worktree_folder}` 模型 |
| `src/workspace_state.rs` | `tmux_session` 改为 `pmux-{repo_name}` 或考虑弃用（若未被使用） |
| `src/runtime/state.rs` | 测试中的 `sdlc-repo` 改为 `pmux-*` 示例 |

---

## 8. 边界情况

### 8.1 local_pty 下的 diff

- `open_review` 在 local 下发命令到 pane，不创建新窗口
- overlay 显示的是否应为「当前 pane 的 buffer」？若 UI 假定每个 overlay 有独立 pane_target，local 下可能需要特殊逻辑
- **建议**：P0 先保证 tmux 路径正确；local 的 overlay 可后续单独处理

### 8.2 删除 worktree 时 runtime 可能是 local

- `confirm_delete_worktree` 中 `rt.kill_window(&target)`：local 的 `kill_window` 已是 no-op
- 若当前 backend 为 local，传入的 target 不会被使用，无影响
- 若未来支持「关闭 pmux 后 agent 继续跑」，则可能是 tmux；删除时 `self.runtime` 可能是当前 worktree 的 runtime，而被删的是另一个 worktree。此时仍需从 `worktree.path` 推导 target，不能依赖 `self.runtime`

### 8.3 diff 打开后切换 worktree

- `diff_overlay_open` 存的是打开 diff 时的 session
- 切换 worktree 后，close 时仍应用当初的 session 来 kill_window，这是正确的

---

## 9. 实施顺序建议

1. **添加工具函数**：`session_name_for_worktree`, `main_window_target`
2. **扩展 AgentRuntime**：添加 `session_info()`，各 backend 实现
3. **修改 save_runtime_state**：用 `session_info()` 替代 downcast
4. **修改 confirm_delete_worktree**：用 `main_window_target(&worktree.path)`
5. **修改 open_diff_overlay**：用 `rt.session_info()`，扩展存储 session
6. **修改 close_diff_overlay**：用存储的 session
7. **更新注释与测试**：sdlc → pmux，调整 state/workspace_state 中的示例

---

## 10. 验证

- [ ] `cargo test` 通过
- [ ] tmux backend：创建 worktree、开 diff、关 diff、删 worktree，session/window 命名正确
- [ ] `rg "sdlc-|downcast_ref.*TmuxRuntime" src/` 无结果
- [ ] `rg "pmux-" src/` 仅出现于命名逻辑和测试

---

## 11. 详细实施任务 (Task-by-Task)

> **For Claude:** 按顺序逐项执行，每项完成后运行 `cargo test` 验证。

### Task 1: 添加 session 命名工具函数

**文件**: `src/runtime/backends/mod.rs`

**步骤**:
1. 在 `use` 后、`PMUX_BACKEND_ENV` 前添加：

```rust
use std::path::Path;

/// Session naming for tmux backend. One worktree = one session.
pub fn session_name_for_worktree(worktree_path: &Path) -> String {
    format!("pmux-{}", worktree_path
        .file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or_else(|| "default".into()))
}

/// Default main window name for a worktree session.
pub const MAIN_WINDOW: &str = "main";

/// Window target for killing the main worktree session.
pub fn main_window_target(worktree_path: &Path) -> String {
    format!("{}:{}", session_name_for_worktree(worktree_path), MAIN_WINDOW)
}
```

2. 修改 `create_runtime_from_env` 中 tmux 分支，将内联的 `format!("pmux-{}", ...)` 改为 `session_name_for_worktree(worktree_path)`
3. 运行 `cargo test runtime::backends` 通过

---

### Task 2: AgentRuntime 添加 session_info()

**文件**: `src/runtime/agent_runtime.rs`

**步骤**:
1. 在 `kill_window` 之后添加：

```rust
    /// Returns (session_id, window_id) for backends that support session persistence.
    fn session_info(&self) -> Option<(String, String)>;
```

2. 运行 `cargo build` 确认编译失败（backends 未实现）
3. 在 `src/runtime/backends/tmux.rs` 的 impl AgentRuntime 中实现：`Some((self.session_name.clone(), self.window_name.clone()))`
4. 在 `src/runtime/backends/local_pty.rs` 的 impl AgentRuntime（LocalPtyRuntime 和 LocalPtyAgent）中实现：`None`
5. 运行 `cargo test` 通过

---

### Task 3: save_runtime_state 用 session_info() 替代 downcast

**文件**: `src/ui/app_root.rs`

**步骤**:
1. 移除对 `crate::runtime::backends::TmuxRuntime` 的导入（若有显式 import）
2. 将 `save_runtime_state` 中 616–628 行替换为：

```rust
        let (backend_session_id, backend_window_id) = rt.session_info()
            .map(|(s, w)| (s, w))
            .unwrap_or_else(|| (
                worktree_path.to_string_lossy().to_string(),
                branch_name.to_string(),
            ));
```

3. 运行 `cargo build`，确认不再有 `downcast_ref` 或 `TmuxRuntime` 引用
4. 运行 `cargo test` 通过

---

### Task 4: confirm_delete_worktree 用 main_window_target

**文件**: `src/ui/app_root.rs`

**步骤**:
1. 添加 `use crate::runtime::backends::main_window_target;`
2. 在 `confirm_delete_worktree` 中，删除 `repo_name`、`session_name`、`window_name` 的旧推导逻辑
3. 替换为：`let target = main_window_target(&worktree.path);`
4. 删除 `if let Some(rt)` 中构造 target 的逻辑，直接 `rt.kill_window(&target)`
5. 运行 `cargo test` 通过

---

### Task 5: diff_overlay_open 扩展存储 session

**文件**: `src/ui/app_root.rs`

**步骤**:
1. 将 `diff_overlay_open: Option<(String, String, String)>` 改为 `Option<(String, String, Option<String>, String)>`，即 `(branch, window_name, session, pane_target)`
2. 更新所有构造和 destructure 该字段的代码
3. 在 `open_diff_overlay` 中：`let session = self.runtime.as_ref().and_then(|rt| rt.session_info()).map(|(s,_)| s);`，传入 session
4. 在 `close_diff_overlay` 中：用存储的 session，`let target = session.as_ref().map(|s| format!("{}:{}", s, window_name)); if let (Some(rt), Some(t)) = (&self.runtime, target) { let _ = rt.kill_window(&t); }`
5. 更新 `render` 中访问 `diff_overlay_open` 的代码
6. 运行 `cargo test` 通过

---

### Task 6: open_diff_overlay 用 session_info 构造 pane_target

**文件**: `src/ui/app_root.rs`

**步骤**:
1. 在 `open_diff_overlay` 开头，从 `self.runtime.session_info()` 获取 session
2. 若 `session.is_none()`（local_pty），可保持现有 fallback 或跳过 overlay 逻辑（按当前 local 行为决定）
3. 用 `format!("{}:{}.0", session, window_name)` 构造 pane_target
4. 运行 `cargo test` 通过

---

### Task 7: 更新注释与测试中的 sdlc → pmux

**文件**:
- `src/new_branch_dialog.rs`: 注释改为 `pmux-{worktree_folder}` 模型
- `src/workspace_state.rs`: `tmux_session` 改为 `format!("pmux-{}", repo_name)`
- `src/runtime/state.rs`: 测试中的 `sdlc-repo` 改为 `pmux-repo` 或 `pmux-test`
- `src/runtime/backends/tmux.rs`: 测试中的 `sdlc-test` 改为 `pmux-test`

**步骤**:
1. 逐文件修改
2. 运行 `cargo test` 通过
3. 运行 `rg "sdlc-" src/` 确认无残留

---

### Task 8: 最终验证

**命令**:
```bash
cargo test
rg "sdlc-|downcast_ref.*TmuxRuntime" src/
rg "TmuxRuntime" src/ui/
```

**预期**:
- 所有测试通过
- `sdlc-` 和 `downcast_ref` 无结果
- `src/ui/` 中无 `TmuxRuntime` 引用（仅通过 trait 使用）
