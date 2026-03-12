# Fix: Recovery 后 Agent Status 显示 Unknown

## Problem

重启 pmux 后（recover 已存在的 tmux session），所有 worktree 的 agent 状态显示 Unknown，即使终端里 claude 正在运行且显示 "esc to interrupt"。

### Root Cause 分析

有 **三个问题** 共同导致了这个 bug：

**问题 1：初始快照无 OSC 133 标记**
- `capture-pane -p -e` 返回纯文本 + SGR 颜色码，**不包含 OSC 133 转义序列**
- `ext.feed(&initial_chunk)` 处理后，`phase` 始终为 `Unknown`

**问题 2：Agent 检测条件要求 `phase == Running`**
- `app_root.rs:1131`: `if phase == ShellPhase::Running && ...`
- `phase` 是 `Unknown` → 条件为 false → 永远不会查询 tmux `pane_current_command`
- `agent_override` 始终为 None → 走不到 text pattern 检测路径

**问题 3：Agent 空闲时事件循环不执行**
- `setup_local_terminal` 中，初始快照在 `cx.spawn` 之前被 `rx.try_recv()` 消费
- `cx.spawn` 内的 loop 等待 `rx.recv_async()` 获取新输出
- Agent 空闲（如 "esc to interrupt"）时无新输出 → loop 永远不执行 → 状态检测永远不跑

## Fix

三处改动，应用于 `setup_local_terminal` 和 `setup_pane_terminal_output` 两个函数：

### Change A：Loop 前增加初始检测（两个函数）

在 `cx.spawn` 内、`loop` 前，增加一次性的 agent 检测 + 状态发布：

**`setup_local_terminal`** (line ~1083, 在 `let status_interval = ...` 之后、`loop {` 之前):

```rust
// --- Initial agent detection for recovery ---
// After reattaching to an existing tmux session, the agent may already be
// running but produce no new output. The event loop below only triggers on
// new output, so we run one detection pass on the pre-seeded snapshot.
{
    let initial_content = ext.take_content().0;
    if !initial_content.is_empty() {
        if let Ok(out) = std::process::Command::new("tmux")
            .args(["display-message", "-t", &pane_target_clone, "-p", "#{pane_current_command}"])
            .output()
        {
            let cmd = String::from_utf8_lossy(&out.stdout).trim().to_string();
            agent_override = agent_detect.find_agent(&cmd).cloned();
        }
        let content_for_status: &str = if initial_content.len() > MAX_STATUS_CONTENT_LEN {
            let start = initial_content.len() - MAX_STATUS_CONTENT_LEN;
            let start = initial_content.ceil_char_boundary(start);
            &initial_content[start..]
        } else {
            &initial_content
        };
        if let Some(ref agent_def) = agent_override {
            let detected = agent_def.detect_status(content_for_status);
            if let Some(ref pub_) = status_publisher {
                let _ = pub_.force_status(&status_key_clone, detected, content_for_status);
            }
        } else {
            let shell_info = ShellPhaseInfo {
                phase: ext.shell_phase(),
                last_post_exec_exit_code: ext.last_exit_code(),
            };
            if let Some(ref pub_) = status_publisher {
                let _ = pub_.check_status(
                    &status_key_clone,
                    crate::status_detector::ProcessStatus::Running,
                    Some(shell_info),
                    content_for_status,
                );
            }
        }
    }
}
```

**`setup_pane_terminal_output`** (line ~1368, 同样在 `loop` 前): 相同逻辑，但因为没有 pre-seed，第一次 `ext.take_content()` 会是空的。不过仍然需要加，因为后续 Change C 确保第一个 chunk 也会触发检测。

### Change B：扩展 phase 条件（两个 loop 内）

**Agent 检测触发条件** — 将 `Running` 扩展到也包含 `Unknown`：

```rust
// BEFORE:
if phase == crate::shell_integration::ShellPhase::Running
    && !alt_screen
    && agent_override.is_none()

// AFTER:
if (phase == crate::shell_integration::ShellPhase::Running
    || phase == crate::shell_integration::ShellPhase::Unknown)
    && !alt_screen
    && agent_override.is_none()
```

**Agent 重置条件** — 只有明确进入 shell prompt 时才清除：

```rust
// BEFORE:
} else if phase != crate::shell_integration::ShellPhase::Running {
    agent_override = None;
}

// AFTER:
} else if phase != crate::shell_integration::ShellPhase::Running
    && phase != crate::shell_integration::ShellPhase::Unknown
{
    agent_override = None;
}
```

**安全性分析**：
- 用户在普通 shell prompt（未运行 agent）+ phase Unknown → 查询 pane_current_command 得到 "zsh"/"bash" → find_agent 不匹配 → agent_override 仍为 None → 走 else 分支 → 和之前行为一致
- 用户有 OSC 133 + 正在运行 agent → phase 为 Running → 原有逻辑不变
- OSC 133 + 返回 shell prompt → phase 为 Input/Prompt → `!= Running && != Unknown` → agent_override 被清除 ✓

### Change C：强制首次状态检测（两个 loop 内）

```rust
// BEFORE:
let mut last_status_check = Instant::now();

// AFTER:
let mut last_status_check = Instant::now() - status_interval;
```

这确保 loop 收到第一个 chunk 时，`now.duration_since(last_status_check) >= status_interval` 必然为 true，状态检测一定会执行。

## 涉及文件

| 文件 | 改动 |
|------|------|
| `src/ui/app_root.rs` | `setup_local_terminal` (~line 1083): Change A + B + C |
| `src/ui/app_root.rs` | `setup_pane_terminal_output` (~line 1368): Change B + C |

## 验证场景

1. 打开 pmux → 输入 `claude` → 关闭 pmux → 重新打开 → 状态应为 Running（匹配 "to interrupt"）
2. 打开 pmux → 输入 `claude` → claude 等待输入（显示 "?"）→ 关闭 → 重新打开 → 状态应为 Waiting
3. 正常使用（非 recovery）→ 行为不变
4. 未配置 OSC 133 → agent text pattern 检测也能工作
