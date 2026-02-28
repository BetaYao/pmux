# Change: P0 Session 命名统一 + UI 与 Backend 解耦

## Why

当前存在两处架构违规：
1. UI 层通过 `downcast_ref::<TmuxRuntime>()` 直接依赖 backend 实现
2. Session 命名混用 `sdlc-*` 与 `pmux-*`，导致 delete worktree、close diff 等操作 target 错误

## What Changes

- 在 `AgentRuntime` 中增加 `session_info() -> Option<(String, String)>` API
- 添加 `session_name_for_worktree`、`main_window_target` 工具函数
- 统一使用 `pmux-*` 命名（一 worktree 一 session）
- 移除 app_root 对 TmuxRuntime 的 downcast
- 修正 confirm_delete_worktree、open/close_diff_overlay 的 target 推导

## Impact

**Affected Code:**
- `src/runtime/agent_runtime.rs`: 新增 session_info()
- `src/runtime/backends/*.rs`: 实现 session_info，添加工具函数
- `src/ui/app_root.rs`: 用 trait API 替代 downcast，修正 session/window target
- `src/new_branch_dialog.rs`, `src/workspace_state.rs`, `src/runtime/state.rs`: 注释与测试 sdlc → pmux
