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

**Phase 2+3 路线图**：[2026-02-phase2-phase3-roadmap.md](2026-02-phase2-phase3-roadmap.md) — 快速入口与实施建议

## Recover / Attach 重构

| 计划 | 目标 | 预估 |
|------|------|------|
| [2026-02-28-extract-attach-runtime-recover.md](2026-02-28-extract-attach-runtime-recover.md) | 抽出 `attach_runtime` 共享逻辑，实现 `try_recover_then_switch` / `try_recover_then_start`，接入 `recover_runtime` | ~0.5 天 |

## Config 与 Backend (P1)

| 计划 | 目标 | 预估 |
|------|------|------|
| [2026-02-28-p1-config-backend.md](2026-02-28-p1-config-backend.md) | config.json 支持 backend；env 覆盖 config；StatusBar 展示；加载时校验 | ~1 天 |

## Remote Channels

| 计划 | 目标 | 预估 |
|------|------|------|
| [2026-02-28-remote-channels.md](2026-02-28-remote-channels.md) | Discord/KOOK 推送 Agent 状态与告警；secrets.json；遥控命令（Phase C） | Phase A 1~2 天 |

## UI 性能

| 计划 | 目标 | 预估 |
|------|------|------|
| [2026-02-28-ui-performance-ultimate.md](2026-02-28-ui-performance-ultimate.md) | 界面性能终极方案：worktree 缓存、切换异步化、细粒度重绘、渲染瘦身 | 4~6 天 |
| [2026-02-28-terminal-element-implementation.md](2026-02-28-terminal-element-implementation.md) | TerminalElement 完全重构（Zed 参考）：paint_quad + shape_line，cursor/resize，完整测试 | 8~12 天 |
| [2026-02-28-route-b-entity-split-implementation.md](2026-02-28-route-b-entity-split-implementation.md) | **路线 B**：Zed 风格 Entity 拆分，TopBar/Sidebar 等独立 Model+Entity，细粒度 notify | ~3 天 |

**依赖顺序**：Phase 1（worktree 缓存）→ Phase 2（切换异步化）→ Phase 3~5

**TerminalElement**：参考 [2026-02-28-terminal-element-brainstorm.md](2026-02-28-terminal-element-brainstorm.md)

**路线 B**：参考 [2026-02-28-route-b-zed-style-entity-split-brainstorm.md](2026-02-28-route-b-zed-style-entity-split-brainstorm.md)

## Local PTY Default + tmux Control Mode

| 计划 | 目标 | 预估 |
|------|------|------|
| [2026-03-01-local-pty-default-and-control-mode.md](2026-03-01-local-pty-default-and-control-mode.md) | Phase 1: local PTY 默认（消除 pipe-pane/capture-pane 问题）；Phase 2: tmux -CC control mode 持久化 | Phase 1: 3-5 天, Phase 2: 2-3 周 |

## gpui-terminal 重构

| 计划 | 目标 | 预估 |
|------|------|------|
| [2026-03-01-gpui-terminal-refactor-default-tmux.md](2026-03-01-gpui-terminal-refactor-default-tmux.md) | 用 gpui-terminal 替换自建终端管线；默认 backend 改为 tmux；Stream 适配器 (RuntimeReader/Writer/TeePipe)；ContentExtractor | 5~8 天 |

**参考:** [2026-03-gpui-terminal-refactor-default-tmux.md](2026-03-gpui-terminal-refactor-default-tmux.md)（brainstorming 方案）

## Terminal UX

| 计划 | 目标 | 预估 |
|------|------|------|
| [2026-03-04-eliminate-prompt-sp-flash.md](2026-03-04-eliminate-prompt-sp-flash.md) | 消除 worktree 切换时 PROMPT_SP send-keys 闪烁 | ~0.5 天 |

## Terminal 性能优化

| 计划 | 目标 | 预估 |
|------|------|------|
| [2026-03-02-tmux-input-perf-direct-pty-write.md](2026-03-02-tmux-input-perf-direct-pty-write.md) | Direct PTY write 绕过 send-keys；BatchedTextRun 替代逐字符 shape_line；清理 debug 日志；缓存 cell metrics | ~2 小时 |

## Sidebar 状态修复

| 计划 | 目标 |
|------|------|
| [2026-03-04-sidebar-multi-pane-status-fix.md](2026-03-04-sidebar-multi-pane-status-fix.md) | 修复 sidebar 只显示 primary pane 状态的 bug；添加 `highest_priority_for_prefix` 聚合多 pane；对齐 TopBar StatusCounts 语义 |

## 历史计划

- [2026-02-25-spec2-implementation.md](2026-02-25-spec2-implementation.md) — Sidebar + Terminal 实现
- [2026-02-25-spec2-sidebar-terminal-design.md](2026-02-25-spec2-sidebar-terminal-design.md) — 设计文档
