# Phase 1 — Streaming Terminal

> 参考：design.md §8 重构阶段、§6.2 PTY Streaming、§12 待定事项与决策
> **TDD**: 先写失败测试再实现。**Subagent-driven**: 可使用 `subagent-driven-development` skill 将 Task 委托给子 agent 并行实施。

**目标**：用 pipe-pane 流式输出替代 capture-pane 轮询，删除 terminal polling，保证 vim/TUI 正常（ANSI、alternate screen、光标、双宽字符）。

**预估**：2~3 天

---

## 前置条件

- tmux 已安装
- 现有 control mode 或 capture-pane 可运行

---

## Task 1: 创建 pty_bridge 模块

**Files:**
- Create: `src/runtime/mod.rs`
- Create: `src/runtime/pty_bridge.rs`
- Modify: `src/lib.rs`

**Step 1: 写失败测试（TDD）**

- 在 `src/runtime/pty_bridge.rs` 或 `mod.rs` 中写 `#[cfg(test)] mod tests`
- 测试：`PtyBridge::new(pane_target)` 存在，`subscribe_output()` 返回 receiver
- `cargo test runtime` 应失败（模块未实现）

**Step 2: 创建 runtime 模块骨架**

`src/runtime/mod.rs`:
```rust
mod pty_bridge;
pub use pty_bridge::PtyBridge;
```

`src/lib.rs` 添加 `pub mod runtime;`

**Step 3: 实现 PtyBridge（tmux pipe-pane 模式）**

`pty_bridge.rs` 核心职责：
- 对给定 tmux pane target 执行 `tmux pipe-pane -o`，将 stdout 作为 RAW BYTE STREAM
- 提供 `subscribe_output() -> impl Stream<Item = Vec<u8>>` 或 channel receiver
- 后台 task 持续读取 pipe 输出，发送到 channel

**Step 4: 验证**

- 单元测试：mock tmux 或集成测试用真实 tmux session
- `cargo test runtime`

---

## Task 2: 接入 alacritty_terminal 流式解析

**Files:**
- Modify: `src/terminal/term_bridge.rs`
- 或 Create: `src/runtime/stream_processor.rs`

**Step 1: 流式 feed**

- 从 `PtyBridge` 接收 `Vec<u8>`
- 调用 `alacritty_terminal::ansi::Processor::advance(&mut term, bytes)`
- 确保 alternate screen、双宽字符、光标由 alacritty 正确解析

**Step 2: 输出 Grid 更新事件**

- 解析后产生 `TerminalEvent { bytes, pane_id, timestamp, event_type }` 或等效结构
- 发送到 channel，供 UI 订阅

**Step 3: 验证**

- 在 pmux 内运行 `vim`，检查颜色、alternate screen、光标
- 运行 `echo -e '\033[31mred\033[0m'` 验证 ANSI

---

## Task 3: 替换 app_root 中的 terminal 数据源

**Files:**
- Modify: `src/ui/app_root.rs`

**Step 1: 移除 capture-pane 轮询**

- 删除或禁用 200ms capture-pane 轮询循环
- 删除 `PMUX_USE_CAPTURE_PANE` fallback 路径中的 capture-pane 逻辑

**Step 2: 接入 PtyBridge 流**

- 在 `start_tmux_session` 中：创建 `PtyBridge` 替代 control mode / capture-pane
- 将 `PtyBridge.subscribe_output()` 的数据喂给 `TermBridge.advance()`
- 通过 channel 或 `cx.notify()` 触发 TerminalView 重绘

**Step 3: 多 pane 支持**

- 每个 pane 一个 `PtyBridge` 实例
- `pane_targets_shared` 改为驱动多个 bridge 的创建/销毁

**Step 4: 验证**

- 切换 worktree、分屏，确认每个 pane 独立渲染
- 无 polling loop（可 grep 确认无 `capture_pane` 调用）

---

## Task 4: 删除 terminal_poller 及相关

**Files:**
- Modify: `src/lib.rs`
- Delete 或 Deprecate: `src/terminal_poller.rs`（若仅含 PaneSnapshot，可迁移到 runtime）

**Step 1: 移除 terminal 轮询**

- 确认无代码再调用 `terminal_poller` 或 `capture_pane` 做 terminal 内容更新
- 从 `lib.rs` 移除 `pub mod terminal_poller`（或标记 `#[deprecated]`）

**Step 2: 清理**

- 删除未使用的 `visible_lines` 调用（若 TerminalView 已改为直接读 Term grid）
- `cargo build` 通过

---

## 验收

- [ ] vim 在 pmux 内颜色、alternate screen、光标正常
- [ ] 无 terminal 内容 polling（grep 无 capture-pane 轮询）
- [ ] 多 pane 分屏各自独立渲染
- [ ] `cargo test` 通过
