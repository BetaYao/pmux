# Phase 2 + Phase 3 实施路线图

> 本文档提供 Phase 2 与 Phase 3 的总体视图与实施建议，便于「开始 plan」时快速定位。

## 总览

| Phase | 核心目标 | 关键产出 | 预估 |
|-------|----------|----------|------|
| **2** | UI 不直接调用 tmux | AgentRuntime trait、TmuxRuntime、app_root 重构 | 3~5 天 |
| **3** | Agent 一等公民、无轮询 | Event Bus、移除 status_poller | 1 周 |

## 依赖关系

```
Phase 1 (完成)
    │
    ▼
Phase 2: Task 1 → Task 2 → Task 4 → Task 5
    │         └── Task 3 (local_pty, 可选)
    │
    ▼
Phase 3: Task 1 → Task 2 → Task 3 → Task 5
              └── Task 4 (state.json, 可并行)
```

## Phase 2 快速入口

1. **先做**：`docs/plans/2026-02-runtime-phase2-runtime-abstraction.md` Task 1
2. **定义**：`AgentRuntime` trait、`TerminalEvent`、`RuntimeError`
3. **实现**：`TmuxRuntime` 封装现有 `crate::tmux::*`
4. **重构**：`app_root.rs` 全部调用改为 `runtime.xxx()`
5. **验证**：`rg "tmux" src/ui/` 无结果

## Phase 3 快速入口

1. **先做**：`docs/plans/2026-02-runtime-phase3-agent-runtime.md` Task 1
2. **定义**：`EventBus`、`RuntimeEvent`（AgentStateChange、TerminalOutput、Notification）
3. **桥接**：Event Bus → main thread → `cx.notify()`
4. **移除**：`status_poller`、`pane_status_tracker`、`status_detector`
5. **验证**：`grep status_poller` 无结果，状态由 Event Bus 推送

## 可并行任务

- Phase 2 Task 3（local_pty）可与 Task 4 并行或延后
- Phase 3 Task 4（state.json）可与 Task 3 并行

## 建议实施节奏

1. **Phase 2 完整**：确保 UI 不再依赖 tmux 直接调用
2. **Phase 3 完整**：引入 Event Bus，移除所有 status 轮询
3. **验收**：`cargo run`、多 workspace/worktree、通知、recover 均正常
