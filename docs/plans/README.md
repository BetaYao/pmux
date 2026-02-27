# pmux 实施计划

本目录存放分阶段实施计划，对应 `design.md` 中的重构阶段。

**Test-Driven Development**：实施时遵循 TDD（Red-Green-Refactor），先写失败测试再实现。参见 `test-driven-development` skill。

**Subagent-Driven Development**：实施时可使用 `subagent-driven-development` skill，通过 mcp_task 将任务委托给 explore/shell/generalPurpose 子 agent 并行执行，加速实施。

## Runtime 重构计划（2026-02）

| Phase | 文件 | 目标 | 预估 |
|-------|------|------|------|
| **1** | [2026-02-runtime-phase1-streaming-terminal.md](2026-02-runtime-phase1-streaming-terminal.md) | Streaming Terminal：pipe-pane 流式，删除 capture-pane 轮询 | 2~3 天 |
| **2** | [2026-02-runtime-phase2-runtime-abstraction.md](2026-02-runtime-phase2-runtime-abstraction.md) | Runtime 抽离：UI → AgentRuntime API，tmux + local PTY adapter | 3~5 天 |
| **3** | [2026-02-runtime-phase3-agent-runtime.md](2026-02-runtime-phase3-agent-runtime.md) | Agent Runtime：Event Bus、状态机、移除 status_poller | 1 周 |
| **4** | [2026-02-runtime-phase4-input-rewrite.md](2026-02-runtime-phase4-input-rewrite.md) | 输入重写：xterm escape → PTY write，支持鼠标/TUI | 2 天 |

**依赖顺序**：Phase 1 → 2 → 3 → 4（Phase 2 与 3 部分可并行）

## 历史计划

- [2026-02-25-spec2-implementation.md](2026-02-25-spec2-implementation.md) — Sidebar + Terminal 实现
- [2026-02-25-spec2-sidebar-terminal-design.md](2026-02-25-spec2-sidebar-terminal-design.md) — 设计文档
