# P0: Session 命名统一 + UI 与 Backend 解耦

> 实施清单。设计见 `docs/plans/2026-02-28-p0-session-naming-and-ui-decouple.md`

## 1. Session 命名工具函数

- [x] 1.1 在 `src/runtime/backends/mod.rs` 添加 `session_name_for_worktree(worktree_path)`
- [x] 1.2 添加 `MAIN_WINDOW` 常量和 `main_window_target(worktree_path)`
- [x] 1.3 修改 `create_runtime_from_env` 使用 `session_name_for_worktree`
- [x] 1.4 运行 `cargo test runtime::backends` 通过

## 2. AgentRuntime session_info() API

- [x] 2.1 在 `agent_runtime.rs` 添加 `session_info() -> Option<(String, String)>` 到 trait
- [x] 2.2 TmuxRuntime 实现 `session_info()` 返回 `Some((session, window))`
- [x] 2.3 LocalPtyRuntime 实现 `session_info()` 返回 `None`
- [x] 2.4 LocalPtyAgent 实现 `session_info()` 返回 `None`
- [x] 2.5 运行 `cargo test` 通过

## 3. save_runtime_state 解耦

- [x] 3.1 移除 app_root 对 TmuxRuntime 的 downcast
- [x] 3.2 用 `rt.session_info()` 获取 backend_session_id、backend_window_id
- [x] 3.3 运行 `cargo build` 确认无 TmuxRuntime 引用

## 4. confirm_delete_worktree 修正

- [x] 4.1 添加 `use crate::runtime::backends::main_window_target`
- [x] 4.2 用 `main_window_target(&worktree.path)` 替代 sdlc-{repo} 推导
- [x] 4.3 运行 `cargo test` 通过

## 5. diff_overlay 存储与使用 session

- [x] 5.1 扩展 `diff_overlay_open` 为 `(branch, window_name, session, pane_target)` 或等价结构
- [x] 5.2 `open_diff_overlay` 用 `rt.session_info()` 获取并存储 session
- [x] 5.3 `close_diff_overlay` 用存储的 session 调用 `kill_window`
- [x] 5.4 更新 render 中所有访问 `diff_overlay_open` 的代码
- [x] 5.5 运行 `cargo test` 通过

## 6. 注释与测试更新

- [x] 6.1 `new_branch_dialog.rs` 注释改为 pmux-{worktree_folder} 模型
- [x] 6.2 `workspace_state.rs` tmux_session 改为 `pmux-{repo_name}`
- [x] 6.3 `runtime/state.rs` 测试中 sdlc-repo → pmux-*
- [x] 6.4 `runtime/backends/tmux.rs` 测试中 sdlc-test → pmux-test
- [x] 6.5 运行 `rg "sdlc-" src/` 确认无残留

## 7. 验收

- [x] 7.1 `cargo test` 通过
- [x] 7.2 `rg "sdlc-|downcast_ref.*TmuxRuntime" src/` 无结果
- [x] 7.3 `rg "TmuxRuntime" src/ui/` 无结果
