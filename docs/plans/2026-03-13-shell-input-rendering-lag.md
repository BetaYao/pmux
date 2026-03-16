# Shell Input Rendering Lag Fix — Implementation Plan

> **For Claude:** This is a single-file refactor. No new modules. No TDD (GPUI async loops are not unit-testable). Implement sequentially. Manual verification after each task.

**Goal:** Decouple terminal output processing from rendering in shell mode to eliminate mid-state flicker during fast input.
**Architecture:** Split the output loop into alt-screen (TUI) and normal-shell paths. Shell path uses zero-wait processing + 16ms fixed-rate rendering via nested `futures_util::future::select`.
**Tech Stack:** Rust, GPUI, futures_util
**Spec:** `docs/superpowers/specs/2026-03-13-shell-input-rendering-lag-design.md`

---

### Task 1: Refactor first output loop (`setup_local_terminal`)

**Files:**
- Modify: `src/ui/app_root.rs` (lines 1382–1571)

**Step 1: Add `dirty` flag before loop**

In the `cx.spawn(async move |_entity, cx| { ... })` block, after the `const MAX_RENDER_DELAY` line (~line 1361) and before the `// Initial status check` block (~line 1363), add:

```rust
                // Shell-path dirty flag: set when data is processed,
                // cleared when render tick fires.
                let mut dirty = false;
```

**Step 2: Wrap existing loop body in `if alt_screen` + add Shell `else` branch**

Replace the entire `loop { ... }` block (lines 1384–1571) with the new structure. The key changes are:

1. Move `alt_screen` check to loop top
2. Wrap existing body inside `if alt_screen { ... }`
3. Add `else { ... }` with the new Shell path

Here is the complete replacement for lines 1384–1571:

```rust
                loop {
                    let alt_screen = terminal_for_output.mode()
                        .contains(alacritty_terminal::term::TermMode::ALT_SCREEN);

                    if alt_screen {
                        // ── TUI path: existing logic, unchanged ──
                        // Reset shell state on entry
                        dirty = false;

                        let idle_timeout = if pending_notify {
                            RENDER_GAP
                        } else if Instant::now().duration_since(last_output_time) < Duration::from_secs(2) {
                            Duration::from_millis(300)
                        } else {
                            Duration::from_secs(2)
                        };

                        let got_output = match coalesce_and_process_output(
                            &rx,
                            &terminal_for_output,
                            &mut ext,
                            idle_timeout,
                            &cx,
                        ).await {
                            Ok(got) => got,
                            Err(_) => break,
                        };

                        if !got_output {
                            if pending_notify {
                                pending_notify = false;
                                first_pending_time = None;
                                if !modal_open.load(Ordering::Relaxed)
                                    && !terminal_for_output.is_synchronized_output()
                                {
                                    if let Some(ref tae) = term_area_entity {
                                        let _ = cx.update_entity(tae, |_, cx| cx.notify());
                                    }
                                }
                                continue;
                            }
                            if terminal_for_output.take_dirty()
                                && !modal_open.load(Ordering::Relaxed)
                            {
                                if let Some(ref tae) = term_area_entity {
                                    let _ = cx.update_entity(tae, |_, cx| cx.notify());
                                }
                            }
                            if let Some(ref agent_def) = agent_override {
                                let screen_text = terminal_for_output.screen_tail_text(
                                    terminal_for_output.size().rows as usize,
                                );
                                let detected = agent_def.detect_status(&screen_text);
                                if let Some(ref pub_) = status_publisher {
                                    let _ = pub_.force_status(
                                        &status_key_clone,
                                        detected,
                                        &screen_text,
                                        &agent_def.message_skip_patterns,
                                    );
                                }
                            }
                            continue;
                        }
                        last_output_time = Instant::now();

                        let now = Instant::now();
                        let phase = ext.shell_phase();
                        if phase != last_phase || alt_screen != last_alt_screen
                            || now.duration_since(last_status_check) >= status_interval
                        {
                            last_status_check = now;
                            if !alt_screen && agent_override.is_none()
                                && matches!(phase,
                                    crate::shell_integration::ShellPhase::Running
                                    | crate::shell_integration::ShellPhase::Unknown)
                            {
                                agent_override = detect_agent_in_pane(&pane_target_clone, &agent_detect);
                            } else if matches!(phase,
                                crate::shell_integration::ShellPhase::Input
                                | crate::shell_integration::ShellPhase::Prompt
                                | crate::shell_integration::ShellPhase::Output)
                            {
                                agent_override = None;
                            }
                            last_phase = phase;
                            last_alt_screen = alt_screen;

                            if alt_screen {
                                let content_str = ext.content_for_status(MAX_STATUS_CONTENT_LEN);
                                let shell_info = ShellPhaseInfo {
                                    phase: crate::shell_integration::ShellPhase::Input,
                                    last_post_exec_exit_code: ext.last_exit_code(),
                                };
                                if let Some(ref pub_) = status_publisher {
                                    let _ = pub_.check_status(
                                        &status_key_clone,
                                        crate::status_detector::ProcessStatus::Running,
                                        Some(shell_info),
                                        &content_str,
                                        &[],
                                    );
                                }
                            } else if let Some(ref agent_def) = agent_override {
                                let screen_text = terminal_for_output.screen_tail_text(
                                    terminal_for_output.size().rows as usize,
                                );
                                let detected = agent_def.detect_status(&screen_text);
                                if let Some(ref pub_) = status_publisher {
                                    let _ = pub_.force_status(
                                        &status_key_clone,
                                        detected,
                                        &screen_text,
                                        &agent_def.message_skip_patterns,
                                    );
                                }
                            } else {
                                let content_str = ext.content_for_status(MAX_STATUS_CONTENT_LEN);
                                let shell_info = ShellPhaseInfo {
                                    phase,
                                    last_post_exec_exit_code: ext.last_exit_code(),
                                };
                                if let Some(ref pub_) = status_publisher {
                                    let _ = pub_.check_status(
                                        &status_key_clone,
                                        crate::status_detector::ProcessStatus::Running,
                                        Some(shell_info),
                                        &content_str,
                                        &[],
                                    );
                                }
                            }
                        }

                        // TUI rendering: deferred, wait for output gap.
                        // Dead else-shell branch removed (we're inside if alt_screen).
                        if modal_open.load(Ordering::Relaxed) {
                            // skip while modal open
                        } else {
                            if !pending_notify {
                                first_pending_time = Some(Instant::now());
                            }
                            pending_notify = true;

                            if let Some(start) = first_pending_time {
                                if start.elapsed() >= MAX_RENDER_DELAY {
                                    if !terminal_for_output.is_synchronized_output() {
                                        if let Some(ref tae) = term_area_entity {
                                            let _ = cx.update_entity(tae, |_, cx| cx.notify());
                                        }
                                    }
                                    pending_notify = false;
                                    first_pending_time = None;
                                }
                            }
                        }

                    } else {
                        // ── Shell path: zero-wait processing + 16ms render tick ──
                        // Reset TUI state on entry
                        pending_notify = false;
                        first_pending_time = None;

                        // One-shot timers, recreated each iteration
                        let render_tick = cx.background_executor().timer(Duration::from_millis(16));
                        let idle_dur = if Instant::now().duration_since(last_output_time) < Duration::from_secs(2) {
                            Duration::from_millis(300)
                        } else {
                            Duration::from_secs(2)
                        };
                        let idle_tick = cx.background_executor().timer(idle_dur);

                        // Three-way select via nested select:
                        //   Either::Left            = recv (data or channel closed)
                        //   Either::Right(Left)     = render_tick (16ms)
                        //   Either::Right(Right)    = idle_tick
                        let recv = rx.recv_async();
                        let timers = select(render_tick, idle_tick);
                        pin_mut!(recv);
                        pin_mut!(timers);

                        match select(recv, timers).await {
                            Either::Left((Ok(chunk), _)) => {
                                // ── Data arrived: process immediately ──
                                terminal_for_output.process_output(&chunk);
                                ext.feed(&chunk);
                                // Drain all buffered chunks
                                while let Ok(next) = rx.try_recv() {
                                    terminal_for_output.process_output(&next);
                                    ext.feed(&next);
                                }
                                dirty = true;
                                last_output_time = Instant::now();

                                // Status detection (same throttle as TUI path,
                                // but omit alt_screen != last_alt_screen since
                                // alt_screen is always false in this branch)
                                let now = Instant::now();
                                let phase = ext.shell_phase();
                                if phase != last_phase
                                    || now.duration_since(last_status_check) >= status_interval
                                {
                                    last_status_check = now;
                                    if agent_override.is_none()
                                        && matches!(phase,
                                            crate::shell_integration::ShellPhase::Running
                                            | crate::shell_integration::ShellPhase::Unknown)
                                    {
                                        agent_override = detect_agent_in_pane(&pane_target_clone, &agent_detect);
                                    } else if matches!(phase,
                                        crate::shell_integration::ShellPhase::Input
                                        | crate::shell_integration::ShellPhase::Prompt
                                        | crate::shell_integration::ShellPhase::Output)
                                    {
                                        agent_override = None;
                                    }
                                    last_phase = phase;
                                    last_alt_screen = false; // always false in shell path

                                    if let Some(ref agent_def) = agent_override {
                                        let screen_text = terminal_for_output.screen_tail_text(
                                            terminal_for_output.size().rows as usize,
                                        );
                                        let detected = agent_def.detect_status(&screen_text);
                                        if let Some(ref pub_) = status_publisher {
                                            let _ = pub_.force_status(
                                                &status_key_clone,
                                                detected,
                                                &screen_text,
                                                &agent_def.message_skip_patterns,
                                            );
                                        }
                                    } else {
                                        let content_str = ext.content_for_status(MAX_STATUS_CONTENT_LEN);
                                        let shell_info = ShellPhaseInfo {
                                            phase,
                                            last_post_exec_exit_code: ext.last_exit_code(),
                                        };
                                        if let Some(ref pub_) = status_publisher {
                                            let _ = pub_.check_status(
                                                &status_key_clone,
                                                crate::status_detector::ProcessStatus::Running,
                                                Some(shell_info),
                                                &content_str,
                                                &[],
                                            );
                                        }
                                    }
                                }

                                // Recheck: data may have switched to alt screen
                                let recheck = terminal_for_output.mode()
                                    .contains(alacritty_terminal::term::TermMode::ALT_SCREEN);
                                if recheck {
                                    dirty = false;
                                    continue;
                                }
                            }
                            Either::Left((Err(_), _)) => {
                                // Channel closed
                                break;
                            }
                            Either::Right((Either::Left((_, _)), _)) => {
                                // ── render_tick fired ──
                                if dirty {
                                    terminal_for_output.take_dirty();
                                    if !modal_open.load(Ordering::Relaxed) {
                                        if let Some(ref tae) = term_area_entity {
                                            let _ = cx.update_entity(tae, |_, cx| cx.notify());
                                        }
                                    }
                                    dirty = false;
                                } else {
                                    if terminal_for_output.take_dirty()
                                        && !modal_open.load(Ordering::Relaxed)
                                    {
                                        if let Some(ref tae) = term_area_entity {
                                            let _ = cx.update_entity(tae, |_, cx| cx.notify());
                                        }
                                    }
                                }
                            }
                            Either::Right((Either::Right((_, _)), _)) => {
                                // ── idle_tick fired ──
                                if terminal_for_output.take_dirty()
                                    && !modal_open.load(Ordering::Relaxed)
                                {
                                    if let Some(ref tae) = term_area_entity {
                                        let _ = cx.update_entity(tae, |_, cx| cx.notify());
                                    }
                                }
                                if let Some(ref agent_def) = agent_override {
                                    let screen_text = terminal_for_output.screen_tail_text(
                                        terminal_for_output.size().rows as usize,
                                    );
                                    let detected = agent_def.detect_status(&screen_text);
                                    if let Some(ref pub_) = status_publisher {
                                        let _ = pub_.force_status(
                                            &status_key_clone,
                                            detected,
                                            &screen_text,
                                            &agent_def.message_skip_patterns,
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
```

**Step 3: Build to verify compilation**

```
Run: RUSTUP_TOOLCHAIN=stable cargo check 2>&1 | head -30
Expected: no errors (warnings OK)
```

**Step 4: Commit**

```bash
git add src/ui/app_root.rs
git commit -m "refactor: split first output loop into TUI/Shell paths

Shell path uses zero-wait processing + 16ms render tick to eliminate
mid-state flicker during fast typing. TUI path unchanged (indented
into if-block)."
```

---

### Task 2: Refactor second output loop (`setup_pane_terminal_output`)

**Files:**
- Modify: `src/ui/app_root.rs` (lines 1772–1932)

**Step 1: Add `dirty` flag before loop**

After the `const MAX_RENDER_DELAY` line (~line 1754), before `// Initial status check`, add:

```rust
                let mut dirty = false;
```

**Step 2: Replace loop body**

Replace the `loop { ... }` block (lines 1774–1932) with the **identical structure** from Task 1. The code is byte-for-byte the same as Task 1's replacement — the two loops have the same variable names (`rx`, `terminal_for_output`, `ext`, `modal_open`, `term_area_entity`, `status_publisher`, `status_key_clone`, `pane_target_clone`, `agent_detect`, `agent_override`, `last_phase`, `last_alt_screen`, `last_status_check`, `status_interval`, `last_output_time`, `pending_notify`, `first_pending_time`, `RENDER_GAP`, `MAX_RENDER_DELAY`, `dirty`).

Copy the exact same `loop { ... }` block from Task 1 Step 2.

**Step 3: Build to verify compilation**

```
Run: RUSTUP_TOOLCHAIN=stable cargo check 2>&1 | head -30
Expected: no errors (warnings OK)
```

**Step 4: Commit**

```bash
git add src/ui/app_root.rs
git commit -m "refactor: split second output loop into TUI/Shell paths

Same structure as first loop. Both output loops now use zero-wait
processing + 16ms render tick for shell mode."
```

---

### Task 3: Run tests + manual verification

**Step 1: Run full test suite**

```
Run: RUSTUP_TOOLCHAIN=stable cargo test 2>&1 | tail -20
Expected: all existing tests pass (this change only affects async runtime loops, not testable code)
```

**Step 2: Build and launch**

```
Run: RUSTUP_TOOLCHAIN=stable cargo run
```

**Step 3: Manual verification checklist**

Run these tests in the launched pmux application:

1. Fast input: type `echo abcdefghijklmnop` quickly — no blank character flashes
2. Fast delete: type a long command, hold backspace — cursor doesn't race ahead of deletion
3. TUI mode: run `vim` or `claude` — TUI renders normally, no ghosting regression
4. Mode switch: exit vim back to shell, re-enter vim — smooth transitions both ways
5. High throughput: `seq 1 100000` — screen scrolls continuously (not frozen then jump)
6. Agent status: run a command — sidebar shows Idle→Running→Idle transitions
7. Close tab: close a workspace tab — no hang or panic

**Step 4: Commit verification note**

```bash
git add -A
git commit -m "verify: shell input rendering lag fix tested manually

Verified: fast input, fast delete, TUI mode, mode switching,
high throughput, agent status, tab close."
```

---

## Summary

| Task | What | Est. Lines |
|------|------|-----------|
| 1 | Refactor first output loop (setup_local_terminal) | ~265 touched |
| 2 | Refactor second output loop (setup_pane_terminal_output) | ~265 touched |
| 3 | Test suite + manual verification | 0 code |

Total: ~530 lines touched in `src/ui/app_root.rs` (170 new, 360 re-indented).
