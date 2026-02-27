# Phase 4 — 输入系统重写

> 参考：design.md §6.4 用户输入、§12 待定事项与决策
> **TDD**: 先写失败测试再实现。**Subagent-driven**: 可使用 `subagent-driven-development` skill 将 Task 委托给子 agent 并行实施。

**目标**：替换 `tmux send-keys` 为 xterm escape 序列直接写入 PTY。支持鼠标、vim/fzf 完整可用、TUI 稳定。

**预估**：2 天

---

## 前置条件

- Phase 2 完成（Runtime API、tmux adapter）
- Phase 3 完成（Event Bus、Agent 模型）
- 现有 `InputHandler` + `key_to_tmux` 可运行

---

## Task 1: 实现 key_to_xterm_escape

**Files:**
- Create: `src/input/xterm_escape.rs`
- 或 Modify: `src/input_handler.rs` 重命名为 `src/input/mod.rs`

**Step 1: 键盘映射**

- 普通字符：直接 UTF-8 bytes
- 功能键：映射到 xterm 序列
  - Enter → `\r` 或 `\n`
  - Backspace → `\x7f`
  - Tab → `\t`
  - Escape → `\x1b`
  - 方向键 → `\x1b[A` / `\x1b[B` / `\x1b[C` / `\x1b[D`
  - Home/End/PgUp/PgDn → `\x1bOH` / `\x1bOF` / `\x1b[5~` / `\x1b[6~`
  - F1–F12 → `\x1bOP` … `\x1b[24~`
  - Ctrl+ 组合 → 对应 control 字符（Ctrl+A = `\x01` 等）

**Step 2: 修饰键**

- Shift、Alt、Ctrl、Cmd 组合
- 参考 xterm 的 modifyOtherKeys、modifyCursorKeys

**Step 3: 单元测试**

- 覆盖常用键、组合键
- 与 `key_to_tmux` 行为对比（在 tmux 下）

---

## Task 2: 实现鼠标协议（可选，优先基础）

**Files:**
- Modify: `src/input/xterm_escape.rs`

**Step 1: 基础鼠标**

- SGR 1006 模式：`\x1b[<0;x;y;M` / `m`（press/release）
- 支持 click、drag

**Step 2: 启用**

- 启动时向 PTY 写入 `\x1b[?1000h` 等启用序列
- 根据终端能力决定是否启用

---

## Task 3: Runtime.send_input 改为 PTY write

**Files:**
- Modify: `src/runtime/backends/tmux.rs` 或 adapter
- Modify: `src/input_handler.rs`

**Step 1: tmux adapter**

- 若 tmux 支持 pipe-pane 双向，可从 pipe 写入
- 否则需通过 `tmux send-keys` 发送**已编码的 xterm 序列**（即把 bytes 当作 literal 发送）
- 目标：UI 不再调用 `tmux send-keys`，改为 `runtime.send_input(pane_id, xterm_bytes)`

**Step 2: local_pty adapter**

- 直接 `pty_master.write_all(bytes)`

**Step 3: InputHandler 重构**

- 删除 `send_key_to_target` 的 tmux 调用
- 新逻辑：`key_event → key_to_xterm_escape() → runtime.send_input(pane_id, bytes)`
- 保留 `key_to_tmux` 仅用于 fallback 或测试

---

## Task 4: AppRoot 键盘事件路径

**Files:**
- Modify: `src/ui/app_root.rs`

**Step 1: 替换调用**

- 原：`input_handler.send_key_to_target(target, tmux_key)`
- 新：`let bytes = key_to_xterm_escape(key, modifiers); runtime.send_input(&active_pane_id, &bytes)`

**Step 2: 鼠标事件**

- 若实现鼠标协议，在 TerminalView 或 AppRoot 处理 mouse down/move/up
- 转换为 xterm mouse 序列，调用 `runtime.send_input`

---

## Task 5: 验证 TUI

**Step 1: vim**

- 在 pmux 内运行 vim
- 测试：方向键、Ctrl+ 组合、Esc、鼠标点击、resize

**Step 2: fzf**

- 运行 `fzf`，测试方向键、Tab、Enter

**Step 3: Claude Code / opencode**

- 若可用，验证无异常

---

## 验收

- [ ] 输入路径无 `tmux send-keys`（或仅 adapter 内部使用）
- [ ] vim、fzf 在 pmux 内完整可用
- [ ] 鼠标支持（若已实现）
- [ ] `cargo test` 通过
