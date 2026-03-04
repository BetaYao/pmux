# Eliminate PROMPT_SP send-keys Flash Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Eliminate the flicker of `unsetopt PROMPT_SP 2>/dev/null; clear` when switching worktrees by using tmux's custom shell command instead of send-keys.

**Architecture:** When creating tmux sessions/windows, pass `zsh -o nopromptsp` as the shell command when default-shell is zsh. This disables PROMPT_SP at startup with no command echo. For non-zsh shells, skip PROMPT_SP handling entirely (bash/fish have no equivalent).

**Tech Stack:** Rust, tmux CLI, zsh

---

## Task 1: Add default-shell detection helper

**Files:**
- Modify: `src/runtime/backends/tmux_control_mode.rs` (add impl block for TmuxControlModeRuntime)

**Step 1: Add the helper function**

Add a static helper (or associated function) to detect if tmux's default shell is zsh:

```rust
/// Returns true if tmux's default-shell is zsh. Used to decide whether to pass
/// `zsh -o nopromptsp` as the new-session/new-window command (avoids send-keys echo flash).
fn is_default_shell_zsh() -> bool {
    let out = Command::new("tmux")
        .args(["show", "-g", "default-shell"])
        .output();
    match out {
        Ok(o) if o.status.success() => {
            let s = String::from_utf8_lossy(&o.stdout);
            let path = s.trim().to_lowercase();
            path.ends_with("zsh") || path.contains("/zsh")
        }
        _ => false,
    }
}
```

Place this inside `impl TmuxControlModeRuntime` (or as a free function in the same module if preferred).

**Step 2: Run `cargo check`**

Run: `RUSTUP_TOOLCHAIN=stable cargo check`
Expected: Compiles successfully.

**Step 3: Commit**

```bash
git add src/runtime/backends/tmux_control_mode.rs
git commit -m "feat(tmux): add is_default_shell_zsh helper"
```

---

## Task 2: Replace send-keys with custom shell in new()

**Files:**
- Modify: `src/runtime/backends/tmux_control_mode.rs` (lines 434–490)

**Step 1: Update new-session create_args**

In `new()`, when `!skip_create_and_send_keys`, append the shell command to `create_args` when default is zsh:

```rust
// After: create_args.extend(["-c", &dir_owned]);  (or after building create_args)
if Self::is_default_shell_zsh() {
    create_args.push("zsh".to_string());
    create_args.push("-o".to_string());
    create_args.push("nopromptsp".to_string());
}
```

Note: tmux `new-session` accepts `[command]` as trailing args. So the full call becomes:
`tmux new-session -d -s X -n Y -c /path zsh -o nopromptsp`

**Step 2: Update new-window in duplicate-session path**

In the block where `window_exists` is false (lines 467–473), add the shell command to `win_args`:

```rust
let mut win_args = vec![
    "new-window".to_string(),
    "-d".to_string(),
    "-t".to_string(),
    session_name.to_string(),
    "-n".to_string(),
    window_name.clone(),
];
if let Some(dir) = start_dir.and_then(|p| p.to_str()) {
    win_args.extend(["-c".to_string(), dir.to_string()]);
}
if Self::is_default_shell_zsh() {
    win_args.extend(["zsh".to_string(), "-o".to_string(), "nopromptsp".to_string()]);
}
let args_ref: Vec<&str> = win_args.iter().map(|s| s.as_str()).collect();
let _ = Command::new("tmux").args(&args_ref).output();
```

**Step 3: Remove the send-keys block**

Delete the entire block (lines 485–490):

```rust
if !skip_create_and_send_keys {
    let pane_target = format!("{}:{}", session_name, window_name);
    let _ = Command::new("tmux")
        .args(["send-keys", "-t", &pane_target, " unsetopt PROMPT_SP 2>/dev/null; clear", "Enter"])
        .output();
}
```

**Step 4: Run `cargo check` and tests**

Run: `RUSTUP_TOOLCHAIN=stable cargo check`
Run: `RUSTUP_TOOLCHAIN=stable cargo test tmux_control_mode`
Expected: All pass.

**Step 5: Commit**

```bash
git add src/runtime/backends/tmux_control_mode.rs
git commit -m "feat(tmux): use zsh -o nopromptsp in new(), remove send-keys"
```

---

## Task 3: Replace send-keys with custom shell in switch_window()

**Files:**
- Modify: `src/runtime/backends/tmux_control_mode.rs` (lines 1030–1051)

**Step 1: Update new-window args in switch_window**

In `switch_window()`, when `!window_exists`, add the shell command to `win_args`:

```rust
let mut win_args = vec![
    "new-window".to_string(),
    "-d".to_string(),
    "-t".to_string(),
    self.session_name.clone(),
    "-n".to_string(),
    window_name.to_string(),
];
if let Some(dir) = start_dir.and_then(|p| p.to_str()) {
    win_args.extend(["-c".to_string(), dir.to_string()]);
}
if Self::is_default_shell_zsh() {
    win_args.extend(["zsh".to_string(), "-o".to_string(), "nopromptsp".to_string()]);
}
let args_ref: Vec<&str> = win_args.iter().map(|s| s.as_str()).collect();
let _ = Command::new("tmux").args(&args_ref).output();
```

**Step 2: Remove the send-keys block**

Delete the block (lines 1046–1051):

```rust
// Disable PROMPT_SP in the new window's shell (same reason as in new())
let pane_target = format!("{}:{}", self.session_name, window_name);
let _ = Command::new("tmux")
    .args(["send-keys", "-t", &pane_target, " unsetopt PROMPT_SP 2>/dev/null; clear", "Enter"])
    .output();
```

**Step 3: Run tests**

Run: `RUSTUP_TOOLCHAIN=stable cargo test`
Expected: All pass.

**Step 4: Commit**

```bash
git add src/runtime/backends/tmux_control_mode.rs
git commit -m "feat(tmux): use zsh -o nopromptsp in switch_window(), remove send-keys"
```

---

## Task 4: Update terminal-implementation rule

**Files:**
- Modify: `.cursor/rules/terminal-implementation.mdc` (lines 76–81)

**Step 1: Replace PROMPT_SP Handling section**

Replace:

```markdown
## PROMPT_SP Handling

- pmux 在创建 session 和 new-window 时通过 `send-keys` 发送 ` unsetopt PROMPT_SP 2>/dev/null; clear` 禁用 zsh 的 `%` 指示符并清屏
- 命令前加空格（` unsetopt ...`）避免写入 shell history
- `send-keys` 由 tmux 立即入队，无需 sleep——PTY 建立 + `-CC attach` 的耗时足够 shell 处理完
- 回归测试用 `printf` (无 trailing newline) 验证 PROMPT_SP 已被禁用：capture-pane 不应出现 `MARKER%`
```

With:

```markdown
## PROMPT_SP Handling

- pmux 在创建 session 和 new-window 时，若 tmux 的 default-shell 为 zsh，则传入 `zsh -o nopromptsp` 作为 command，在 shell 启动时禁用 PROMPT_SP，避免 send-keys 回显闪烁
- 若 default-shell 非 zsh（如 bash/fish），则不处理 PROMPT_SP（这些 shell 无此选项）
- 回归测试用 `printf` (无 trailing newline) 验证 PROMPT_SP 已被禁用：capture-pane 不应出现 `MARKER%`
```

**Step 2: Commit**

```bash
git add .cursor/rules/terminal-implementation.mdc
git commit -m "docs: update PROMPT_SP handling rule for zsh -o nopromptsp"
```

---

## Task 5: Manual verification

**Step 1: Start pmux with a repo**

```bash
RUSTUP_TOOLCHAIN=stable cargo run
# Add a workspace, ensure default shell is zsh (tmux show -g default-shell)
```

**Step 2: Switch worktrees**

Create a second worktree, switch to it. Verify:
- No flash of `unsetopt PROMPT_SP 2>/dev/null; clear`
- Terminal shows clean prompt
- `printf "MARKER"` (no newline) does not show `MARKER%` in capture-pane

**Step 3: Run regression tests**

```bash
RUSTUP_TOOLCHAIN=stable cargo test
# If e2e/regression scripts exist for worktree switch, run them
```

---

## Plan index update

Add to `docs/plans/README.md` under an appropriate section (e.g. "Terminal UX" or new section):

```markdown
| [2026-03-04-eliminate-prompt-sp-flash.md](2026-03-04-eliminate-prompt-sp-flash.md) | 消除 worktree 切换时 PROMPT_SP send-keys 闪烁 | ~0.5 天 |
```

---

## Notes

- `zsh -o nopromptsp` disables PROMPT_SP at startup; equivalent to `unsetopt PROMPT_SP` in an interactive session.
- tmux `new-session` and `new-window` accept `[command]` as trailing arguments; when provided, it replaces the default shell for that pane.
- `-c` (start-directory) is applied before the command runs, so the shell starts in the correct worktree path.
