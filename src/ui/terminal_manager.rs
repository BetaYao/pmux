//! TerminalManager - manages terminal buffers, resize, focus, IME, and search.
//!
//! Extracted from AppRoot Phase 4.
//! Observes RuntimeManager for runtime reference.

use crate::agent_status::AgentStatus;
use crate::config::Config;
use crate::runtime::{AgentRuntime, RuntimeError, StatusPublisher};
use crate::shell_integration::ShellPhaseInfo;
use crate::terminal::ContentExtractor;
use crate::terminal::{GhosttyTerminalView, TerminalConfig, TerminalInput, TerminalSession};
use crate::ui::app_root::{coalesce_and_process_output, detect_agent_in_pane, is_pane_shell, MAX_STATUS_CONTENT_LEN};
use crate::ui::terminal_controller::ResizeController;
use crate::ui::terminal_view::TerminalBuffer;
use crate::ui::terminal_area_entity::TerminalAreaEntity;
use futures_util::future::{select, Either};
use futures_util::pin_mut;
use gpui::prelude::*;
use gpui::{App, Context, Entity, FocusHandle};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};

pub struct TerminalManager {
    pub buffers: Arc<Mutex<HashMap<String, TerminalBuffer>>>,
    pub focus: Option<FocusHandle>,
    pub needs_focus: bool,
    pub resize_controller: ResizeController,
    pub preferred_dims: Option<(u16, u16)>,
    pub shared_dims: Arc<Mutex<Option<(u16, u16)>>>,
    pub ime_pending_enter: Arc<AtomicBool>,
    pub area_entity: Option<Entity<TerminalAreaEntity>>,
    // Shared state from other managers
    pub modal_overlay_open: Arc<AtomicBool>,
    pub split_dragging: Arc<AtomicBool>,
    pub status_publisher: Option<StatusPublisher>,
    // Search
    pub search_active: bool,
    pub search_query: String,
    pub search_current_match: usize,
}

impl TerminalManager {
    pub fn new(modal_overlay_open: Arc<AtomicBool>, split_dragging: Arc<AtomicBool>) -> Self {
        Self {
            buffers: Arc::new(Mutex::new(HashMap::new())),
            focus: None,
            needs_focus: false,
            resize_controller: ResizeController::new(),
            preferred_dims: None,
            shared_dims: Arc::new(Mutex::new(None)),
            ime_pending_enter: Arc::new(AtomicBool::new(false)),
            area_entity: None,
            modal_overlay_open,
            split_dragging,
            status_publisher: None,
            search_active: false,
            search_query: String::new(),
            search_current_match: 0,
        }
    }

    pub fn ensure_focus(&mut self, cx: &mut Context<Self>) {
        if self.focus.is_none() {
            self.focus = Some(cx.focus_handle());
        }
    }

    /// Clean up terminal buffers for a workspace prefix (on switch/close).
    pub fn cleanup_buffers_for_prefix(&mut self, prefix: &str) {
        if let Ok(mut buffers) = self.buffers.lock() {
            let colon_prefix = format!("{}:", prefix);
            buffers.retain(|k, _| k != prefix && !k.starts_with(&colon_prefix));
        }
    }

    /// Toggle search bar visibility.
    pub fn toggle_search(&mut self) {
        self.search_active = !self.search_active;
        if !self.search_active {
            self.search_query.clear();
            self.search_current_match = 0;
        }
    }

    /// Activate search mode.
    pub fn start_search(&mut self) {
        self.search_active = true;
        self.search_query.clear();
        self.search_current_match = 0;
    }

    /// Deactivate search mode.
    pub fn stop_search(&mut self) {
        self.search_active = false;
        self.search_query.clear();
        self.search_current_match = 0;
    }

    /// Resolve terminal dimensions: preferred → shared → config → fallback.
    pub fn resolve_terminal_dims(&self) -> (u16, u16) {
        self.preferred_dims
            .or_else(|| {
                if let Ok(dims) = self.shared_dims.lock() {
                    *dims
                } else {
                    None
                }
            })
            .or_else(|| {
                crate::config::Config::load().ok().and_then(|c| match (c.last_terminal_cols, c.last_terminal_rows) {
                    (Some(cols), Some(rows)) => Some((cols, rows)),
                    _ => None,
                })
            })
            .unwrap_or((120, 36))
    }

    // ========================================================================
    // Terminal setup methods (moved from AppRoot)
    // ========================================================================

    pub fn setup_local_terminal(
        &mut self,
        runtime: Arc<dyn AgentRuntime>,
        pane_target: &str,
        status_key: &str,
        _terminal_area_entity: Option<Entity<TerminalAreaEntity>>,
        cx: &mut Context<Self>,
    ) {
        let pane_target_str = pane_target.to_string();
        let fallback_dims = self.resolve_terminal_dims();
        let actual_dims = runtime.get_pane_dimensions(&pane_target_str);
        // Use GPUI/config dims as the authoritative rendering size.
        // Only fall back to tmux query when GPUI dims are unavailable (80x24).
        let (cols, rows) = if fallback_dims != (80, 24) {
            fallback_dims
        } else if actual_dims.0 > 0 && actual_dims.1 > 0 && actual_dims != (80, 24) {
            actual_dims
        } else {
            fallback_dims
        };

        // #region agent log
        crate::debug_log::dbg_session_log(
            "app_root.rs:setup_local_terminal",
            "terminal dims and pane_target",
            &serde_json::json!({
                "pane_target": &pane_target_str,
                "cols": cols, "rows": rows,
                "actual_pane_dims": format!("{}x{}", actual_dims.0, actual_dims.1),
                "fallback_dims": format!("{}x{}", fallback_dims.0, fallback_dims.1),
                "preferred_dims": self.preferred_dims,
            }),
            "H4",
        );
        // #endregion

        // Force the tmux window AND pane to the target size before capture.
        // resize-window bypasses the client-size constraint that limits resize-pane.
        let dims_match = actual_dims == (cols, rows);
        if !dims_match {
            if let Some((session, _)) = runtime.session_info() {
                let wn = runtime.session_info().map(|(_, w)| w).unwrap_or_default();
                let window_target = format!("{}:{}", session, wn);
                let _ = std::process::Command::new("tmux")
                    .args(["resize-window", "-t", &window_target,
                           "-x", &cols.to_string(), "-y", &rows.to_string()])
                    .output();
            }
            let _ = std::process::Command::new("tmux")
                .args(["resize-pane", "-t", &pane_target_str,
                       "-x", &cols.to_string(), "-y", &rows.to_string()])
                .output();
            // Wait for the shell to process SIGWINCH and redraw at the new size.
            // Without this, capture-pane grabs content with stale cursor positions.
            std::thread::sleep(std::time::Duration::from_millis(150));
        }

        // Check pane dims after subprocess resize (or reuse actual_dims when already correct).
        // Avoid calling runtime.resize() when the pane is already at the target size: even a
        // no-op resize-pane sends SIGWINCH to the foreground process, causing it to redraw.
        // That redraw arrives via %output events AFTER the initial capture-pane snapshot,
        // making the terminal flash between old-layout and new-layout content (visible "shake"
        // on every worktree or tab switch). Only fall back to CC resize when subprocess resize
        // failed to achieve the target dimensions.
        let post_subprocess_dims = if dims_match {
            actual_dims
        } else {
            runtime.get_pane_dimensions(&pane_target_str)
        };
        let resize_succeeded = if post_subprocess_dims == (cols, rows) {
            // Pane is already at the correct size — skip runtime.resize() to avoid SIGWINCH.
            true
        } else {
            // Subprocess resize failed or was skipped; use CC resize as a last resort.
            let _ = runtime.resize(&pane_target_str, cols, rows);
            let final_dims = runtime.get_pane_dimensions(&pane_target_str);
            final_dims == (cols, rows)
        };
        let post_resize_dims = if resize_succeeded { (cols, rows) } else { post_subprocess_dims };

        // NOTE: Previously we called runtime.set_skip_initial_capture() when resize failed,
        // to avoid a brief "shake" from dimension-mismatched content. However, this caused
        // a much worse bug: when the tmux window has orphan panes (e.g. a 1-row leftover),
        // the main pane cannot be resized to the target dims, skip_capture fires, and the
        // terminal starts completely blank. A slight layout mismatch is far preferable to
        // showing nothing. The capture will be at the pane's actual dims and any mismatch
        // self-corrects on the next output event.

        // #region agent log
        crate::debug_log::dbg_session_log(
            "app_root.rs:setup_local_terminal",
            "pre-subscribe state",
            &serde_json::json!({
                "dims_match": dims_match,
                "skip_capture": false,  // no longer skipped
                "pane_target": &pane_target_str,
                "post_resize_dims": format!("{}x{}", post_resize_dims.0, post_resize_dims.1),
                "resize_succeeded": resize_succeeded,
            }),
            "H_skip",
        );
        // #endregion

        if let Some(rx) = runtime.subscribe_output(&pane_target_str) {
            use crate::terminal::{Terminal, TerminalSize};

            // #region agent log
            crate::debug_log::dbg_session_log(
                "app_root.rs:setup_local_terminal",
                "initial PTY config",
                &serde_json::json!({"cols": cols, "rows": rows}),
                "H15",
            );
            // #endregion

            let is_tmux = runtime.backend_type().starts_with("tmux");
            let terminal = Arc::new(if is_tmux {
                Terminal::new_tmux(
                    pane_target_str.clone(),
                    TerminalSize {
                        cols: cols as u16,
                        rows: rows as u16,
                        cell_width: 8.0,
                        cell_height: 16.0,
                    },
                )
            } else {
                Terminal::new(
                    pane_target_str.clone(),
                    TerminalSize {
                        cols: cols as u16,
                        rows: rows as u16,
                        cell_width: 8.0,
                        cell_height: 16.0,
                    },
                )
            });

            // Pre-populate the terminal with the initial capture-pane snapshot synchronously
            // so the very first GPUI render frame already shows real content instead of a
            // blank screen with the cursor at position (0,0). subscribe_output() puts the
            // snapshot into the channel before returning; try_recv() drains it immediately
            // without any blocking. The async output task below then receives only the live
            // %output events going forward.
            let mut ext = ContentExtractor::new();
            if let Ok(initial_chunk) = rx.try_recv() {
                terminal.process_output(&initial_chunk);
                ext.feed(&initial_chunk);
            }

            // Forward PTY write-back (terminal sequences like OSC response that need to go back to PTY)
            let pty_write_rx = terminal.pty_write_rx.clone();
            let runtime_for_pty = runtime.clone();
            let pane_for_pty = pane_target_str.clone();
            std::thread::spawn(move || {
                while let Ok(data) = pty_write_rx.recv() {
                    let _ = runtime_for_pty.send_input(&pane_for_pty, &data);
                }
            });

            // Handle OSC 52 clipboard store requests (e.g. from opencode, tmux copy-mode)
            let clipboard_rx = terminal.clipboard_store_rx.clone();
            std::thread::spawn(move || {
                while let Ok(text) = clipboard_rx.recv() {
                    use std::io::Write;
                    if let Ok(mut child) = std::process::Command::new("pbcopy")
                        .stdin(std::process::Stdio::piped())
                        .spawn()
                    {
                        if let Some(stdin) = child.stdin.as_mut() {
                            let _ = stdin.write_all(text.as_bytes());
                        }
                        let _ = child.wait();
                    }
                }
            });

            let runtime_for_resize = runtime.clone();
            let pane_for_resize = pane_target_str.clone();
            let shared_dims_for_resize = Arc::clone(&self.shared_dims);
            let split_dragging_for_resize = self.split_dragging.clone();
            // Throttle PTY resize: execute first resize immediately (critical for shrinking),
            // coalesce rapid subsequent resizes to avoid SIGWINCH flood, and always apply
            // the final size via a trailing-edge timer.
            let pending_resize_dims = Arc::new(std::sync::atomic::AtomicU32::new(0));
            let last_pty_resize_ms = Arc::new(std::sync::atomic::AtomicU64::new(0));
            const RESIZE_THROTTLE_MS: u64 = 32;
            let resize_callback: Arc<dyn Fn(u16, u16) + Send + Sync> = Arc::new(move |cols, rows| {
                // Skip runtime resize during split divider drag to prevent tmux feedback loop
                // (resize-pane redistributes space between panes, fighting the UI ratio).
                if split_dragging_for_resize.load(Ordering::SeqCst) {
                    return;
                }
                // #region agent log
                crate::debug_log::dbg_session_log(
                    "app_root.rs:resize_callback(setup_local)",
                    "PTY resize requested (throttled)",
                    &serde_json::json!({"cols": cols, "rows": rows}),
                    "H15",
                );
                // #endregion
                let packed = ((cols as u32) << 16) | (rows as u32);
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0);
                let last = last_pty_resize_ms.load(Ordering::SeqCst);

                if now.saturating_sub(last) >= RESIZE_THROTTLE_MS {
                    // Throttle window passed: execute immediately
                    last_pty_resize_ms.store(now, Ordering::SeqCst);
                    pending_resize_dims.store(0, Ordering::SeqCst);
                    let _ = runtime_for_resize.resize(&pane_for_resize, cols, rows);
                    if let Ok(mut d) = shared_dims_for_resize.lock() {
                        *d = Some((cols, rows));
                    }
                    if let Ok(mut cfg) = Config::load() {
                        cfg.last_terminal_cols = Some(cols);
                        cfg.last_terminal_rows = Some(rows);
                        let _ = cfg.save();
                    }
                } else {
                    // Within throttle window: store pending, spawn trailing thread if needed
                    let prev = pending_resize_dims.swap(packed, Ordering::SeqCst);
                    if prev == 0 {
                        let pending = pending_resize_dims.clone();
                        let last_ms = last_pty_resize_ms.clone();
                        let rt = runtime_for_resize.clone();
                        let pane = pane_for_resize.clone();
                        let shared = shared_dims_for_resize.clone();
                        std::thread::spawn(move || {
                            std::thread::sleep(std::time::Duration::from_millis(RESIZE_THROTTLE_MS + 20));
                            let dims = pending.swap(0, Ordering::SeqCst);
                            if dims != 0 {
                                let c = (dims >> 16) as u16;
                                let r = dims as u16;
                                last_ms.store(
                                    std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .map(|d| d.as_millis() as u64)
                                        .unwrap_or(0),
                                    Ordering::SeqCst,
                                );
                                let _ = rt.resize(&pane, c, r);
                                if let Ok(mut d) = shared.lock() {
                                    *d = Some((c, r));
                                }
                                if let Ok(mut cfg) = Config::load() {
                                    cfg.last_terminal_cols = Some(c);
                                    cfg.last_terminal_rows = Some(r);
                                    let _ = cfg.save();
                                }
                            }
                        });
                    }
                }
            });

            let focus_handle = self.focus.get_or_insert_with(|| cx.focus_handle()).clone();
            let runtime_for_input = runtime.clone();
            let pane_for_input = pane_target_str.clone();
            let pending_enter = self.ime_pending_enter.clone();
            let modal_open_for_input = self.modal_overlay_open.clone();
            let input_callback: Arc<dyn Fn(&[u8]) + Send + Sync> =
                Arc::new(move |bytes: &[u8]| {
                    // Block input to terminal when a modal (settings/new branch) is open
                    if modal_open_for_input.load(Ordering::Relaxed) {
                        return;
                    }
                    let _ = runtime_for_input.send_input(&pane_for_input, bytes);
                    // IME: first Enter only confirms composition; clear pending so we don't send \r (user must press Enter again to submit)
                    let _ = pending_enter.swap(false, Ordering::SeqCst);
                });
            if let Ok(mut buffers) = self.buffers.lock() {
                buffers.clear();
                buffers.insert(
                    pane_target_str.clone(),
                    TerminalBuffer::Terminal {
                        terminal: terminal.clone(),
                        focus_handle: focus_handle.clone(),
                        resize_callback: Some(resize_callback),
                        input_callback: Some(input_callback),
                    },
                );
            }

            // When capture was skipped (resize failed), send C-l to make
            // the shell clear and redraw at the correct pane dimensions.
            if !resize_succeeded {
                let _ = runtime.send_key(&pane_target_str, "C-l", false);
                // #region agent log
                crate::debug_log::dbg_session_log(
                    "app_root.rs:setup_local_terminal",
                    "sent C-l for redraw (resize failed)",
                    &serde_json::json!({"pane_target": &pane_target_str}),
                    "H_redraw",
                );
                // #endregion
            }

            let status_publisher = self.status_publisher.clone();
            let pane_target_clone = pane_target_str.clone();
            let status_key_clone = status_key.to_string();
            let terminal_for_output = terminal.clone();
            let term_area_entity = self.area_entity.clone();
            let modal_open = self.modal_overlay_open.clone();
            // ext was created and pre-seeded with the initial snapshot above.

            cx.spawn(async move |_entity, cx| {
                use std::time::{Duration, Instant};
                let mut last_status_check = Instant::now();
                let _last_resync = Instant::now();
                let mut last_output_time = Instant::now();

                let mut last_phase = ext.shell_phase();
                let mut last_alt_screen = false;
                let mut agent_override: Option<crate::config::AgentDef> = None;
                let agent_detect: crate::config::AgentDetectConfig = crate::config::Config::load()
                    .map(|c| c.agent_detect)
                    .unwrap_or_else(|_| crate::config::Config::default().agent_detect);
                let status_interval = Duration::from_millis(200);

                // Deferred rendering: don't render mid-frame. Wait for an output
                // gap to detect TUI frame completion. This compensates for tmux
                // stripping CSI ?2026h synchronized-output markers.
                let mut pending_notify = false;
                let mut first_pending_time: Option<Instant> = None;
                // Gap threshold: if no output for this long, consider the frame complete.
                // 16ms = one 60fps frame; gives TUI programs time to complete their
                // frame output before we render.
                const RENDER_GAP: Duration = Duration::from_millis(16);
                // Safety cap: force a render if deferred too long (continuous streaming).
                // In alt-screen mode, the forced render does a capture-pane resync first
                // to avoid showing mid-frame ghosting.
                const MAX_RENDER_DELAY: Duration = Duration::from_millis(200);

                // Shell-path dirty flag: set when data is processed,
                // cleared when render tick fires.
                let mut dirty = false;

                // Initial status check for recovered sessions.
                // capture-pane doesn't include OSC 133 markers, so ext.shell_phase()
                // is Unknown after the initial snapshot. Use detect_agent_in_pane()
                // which checks both pane_current_command and child processes.
                {
                    if let Some(agent_def) = detect_agent_in_pane(&pane_target_clone, &agent_detect) {
                        agent_override = Some(agent_def.clone());
                        let screen_text = terminal_for_output.screen_tail_text(
                            terminal_for_output.size().rows as usize,
                        );
                        if let Some(ref pub_) = status_publisher {
                            let detected = agent_def.detect_status(&screen_text);
                            let _ = pub_.force_status(&status_key_clone, detected, &screen_text, &agent_def.message_skip_patterns);
                        }
                    } else if is_pane_shell(&pane_target_clone) {
                        if let Some(ref pub_) = status_publisher {
                            let _ = pub_.force_status(&status_key_clone, AgentStatus::Idle, "", &[]);
                        }
                    }
                }

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

                            if alt_screen && agent_override.is_some() {
                                // TUI agent (e.g. opencode): use text pattern detection,
                                // not hardcoded ShellPhase::Input which always maps to Idle.
                                let agent_def = agent_override.as_ref().unwrap();
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
                            } else if alt_screen {
                                // Alt screen without agent override (e.g. vim, htop)
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
                                let mut is_selecting = terminal_for_output.selecting().load(std::sync::atomic::Ordering::Relaxed);
                                if is_selecting {
                                    let sel_start = terminal_for_output.selecting_since().load(std::sync::atomic::Ordering::Relaxed);
                                    let now_ms = std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .map(|d| d.as_millis() as u64)
                                        .unwrap_or(0);
                                    if sel_start > 0 && now_ms.saturating_sub(sel_start) >= 5000 {
                                        terminal_for_output.selecting().store(false, std::sync::atomic::Ordering::Relaxed);
                                        terminal_for_output.selecting_since().store(0, std::sync::atomic::Ordering::Relaxed);
                                        is_selecting = false;
                                    }
                                }
                                if dirty {
                                    terminal_for_output.take_dirty();
                                    if !modal_open.load(Ordering::Relaxed) && !is_selecting {
                                        if let Some(ref tae) = term_area_entity {
                                            let _ = cx.update_entity(tae, |_, cx| cx.notify());
                                        }
                                    }
                                    dirty = false;
                                } else {
                                    if terminal_for_output.take_dirty()
                                        && !modal_open.load(Ordering::Relaxed)
                                        && !is_selecting
                                    {
                                        if let Some(ref tae) = term_area_entity {
                                            let _ = cx.update_entity(tae, |_, cx| cx.notify());
                                        }
                                    }
                                }
                            }
                            Either::Right((Either::Right((_, _)), _)) => {
                                // ── idle_tick fired ──
                                let mut is_selecting = terminal_for_output.selecting().load(std::sync::atomic::Ordering::Relaxed);
                                if is_selecting {
                                    let sel_start = terminal_for_output.selecting_since().load(std::sync::atomic::Ordering::Relaxed);
                                    let now_ms = std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .map(|d| d.as_millis() as u64)
                                        .unwrap_or(0);
                                    if sel_start > 0 && now_ms.saturating_sub(sel_start) >= 5000 {
                                        terminal_for_output.selecting().store(false, std::sync::atomic::Ordering::Relaxed);
                                        terminal_for_output.selecting_since().store(0, std::sync::atomic::Ordering::Relaxed);
                                        is_selecting = false;
                                    }
                                }
                                if terminal_for_output.take_dirty()
                                    && !modal_open.load(Ordering::Relaxed)
                                    && !is_selecting
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
            })
            .detach();
        } else {
            if let Ok(mut buffers) = self.buffers.lock() {
                buffers.clear();
                buffers.insert(
                    pane_target_str,
                    TerminalBuffer::Error("Streaming unavailable.".to_string()),
                );
            }
            cx.notify();
        }
    }

    /// Set up terminal output stream for a single pane. Inserts into buffers without clearing.
    /// Used when adding a new split pane or restoring multi-pane layout.
    pub fn setup_pane_terminal_output(
        &mut self,
        runtime: Arc<dyn AgentRuntime>,
        pane_target: &str,
        status_key: &str,
        _terminal_area_entity: Option<Entity<TerminalAreaEntity>>,
        cx: &mut Context<Self>,
    ) {
        let pane_target_str = pane_target.to_string();
        let (cols, rows) = runtime.get_pane_dimensions(&pane_target_str);

        if let Some(rx) = runtime.subscribe_output(&pane_target_str) {
            use crate::terminal::{Terminal, TerminalSize};

            let is_tmux = runtime.backend_type().starts_with("tmux");
            let terminal = Arc::new(if is_tmux {
                Terminal::new_tmux(
                    pane_target_str.clone(),
                    TerminalSize {
                        cols: cols as u16,
                        rows: rows as u16,
                        cell_width: 8.0,
                        cell_height: 16.0,
                    },
                )
            } else {
                Terminal::new(
                    pane_target_str.clone(),
                    TerminalSize {
                        cols: cols as u16,
                        rows: rows as u16,
                        cell_width: 8.0,
                        cell_height: 16.0,
                    },
                )
            });

            let pty_write_rx = terminal.pty_write_rx.clone();
            let runtime_for_pty = runtime.clone();
            let pane_for_pty = pane_target_str.clone();
            std::thread::spawn(move || {
                while let Ok(data) = pty_write_rx.recv() {
                    let _ = runtime_for_pty.send_input(&pane_for_pty, &data);
                }
            });

            // Handle OSC 52 clipboard store requests (e.g. from opencode, tmux copy-mode)
            let clipboard_rx = terminal.clipboard_store_rx.clone();
            std::thread::spawn(move || {
                while let Ok(text) = clipboard_rx.recv() {
                    use std::io::Write;
                    if let Ok(mut child) = std::process::Command::new("pbcopy")
                        .stdin(std::process::Stdio::piped())
                        .spawn()
                    {
                        if let Some(stdin) = child.stdin.as_mut() {
                            let _ = stdin.write_all(text.as_bytes());
                        }
                        let _ = child.wait();
                    }
                }
            });

            let runtime_for_resize = runtime.clone();
            let pane_for_resize = pane_target_str.clone();
            let split_dragging_for_resize2 = self.split_dragging.clone();
            let pending_resize_dims2 = Arc::new(std::sync::atomic::AtomicU32::new(0));
            let last_pty_resize_ms2 = Arc::new(std::sync::atomic::AtomicU64::new(0));
            const RESIZE_THROTTLE_MS: u64 = 32;
            let resize_callback: Arc<dyn Fn(u16, u16) + Send + Sync> =
                Arc::new(move |cols, rows| {
                    // Skip runtime resize during split divider drag to prevent tmux feedback loop
                    if split_dragging_for_resize2.load(Ordering::SeqCst) {
                        return;
                    }
                    let packed = ((cols as u32) << 16) | (rows as u32);
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis() as u64)
                        .unwrap_or(0);
                    let last = last_pty_resize_ms2.load(Ordering::SeqCst);

                    if now.saturating_sub(last) >= RESIZE_THROTTLE_MS {
                        last_pty_resize_ms2.store(now, Ordering::SeqCst);
                        pending_resize_dims2.store(0, Ordering::SeqCst);
                        let _ = runtime_for_resize.resize(&pane_for_resize, cols, rows);
                    } else {
                        let prev = pending_resize_dims2.swap(packed, Ordering::SeqCst);
                        if prev == 0 {
                            let pending = pending_resize_dims2.clone();
                            let last_ms = last_pty_resize_ms2.clone();
                            let rt = runtime_for_resize.clone();
                            let pane = pane_for_resize.clone();
                            std::thread::spawn(move || {
                                std::thread::sleep(std::time::Duration::from_millis(RESIZE_THROTTLE_MS + 20));
                                let dims = pending.swap(0, Ordering::SeqCst);
                                if dims != 0 {
                                    let c = (dims >> 16) as u16;
                                    let r = dims as u16;
                                    last_ms.store(
                                        std::time::SystemTime::now()
                                            .duration_since(std::time::UNIX_EPOCH)
                                            .map(|d| d.as_millis() as u64)
                                            .unwrap_or(0),
                                        Ordering::SeqCst,
                                    );
                                    let _ = rt.resize(&pane, c, r);
                                }
                            });
                        }
                    }
                });

            let focus_handle = self.focus.get_or_insert_with(|| cx.focus_handle()).clone();
            let runtime_for_input = runtime.clone();
            let pane_for_input = pane_target_str.clone();
            let pending_enter = self.ime_pending_enter.clone();
            let modal_open_for_input = self.modal_overlay_open.clone();
            let input_callback: Arc<dyn Fn(&[u8]) + Send + Sync> =
                Arc::new(move |bytes: &[u8]| {
                    // Block input to terminal when a modal (settings/new branch) is open
                    if modal_open_for_input.load(Ordering::Relaxed) {
                        return;
                    }
                    let _ = runtime_for_input.send_input(&pane_for_input, bytes);
                    // IME: first Enter only confirms composition; clear pending so we don't send \r (user must press Enter again to submit)
                    let _ = pending_enter.swap(false, Ordering::SeqCst);
                });
            if let Ok(mut buffers) = self.buffers.lock() {
                buffers.insert(
                    pane_target_str.clone(),
                    TerminalBuffer::Terminal {
                        terminal: terminal.clone(),
                        focus_handle: focus_handle.clone(),
                        resize_callback: Some(resize_callback),
                        input_callback: Some(input_callback),
                    },
                );
            }

            let status_publisher = self.status_publisher.clone();
            let pane_target_clone = pane_target_str.clone();
            let status_key_clone = status_key.to_string();
            let terminal_for_output = terminal.clone();
            let term_area_entity = self.area_entity.clone();
            let modal_open = self.modal_overlay_open.clone();
            let mut ext = ContentExtractor::new();

            cx.spawn(async move |_entity, cx| {
                use std::time::{Duration, Instant};
                let mut last_status_check = Instant::now();
                let _last_resync = Instant::now();
                let mut last_output_time = Instant::now();

                let mut last_phase = ext.shell_phase();
                let mut last_alt_screen = false;
                let mut agent_override: Option<crate::config::AgentDef> = None;
                let agent_detect: crate::config::AgentDetectConfig = crate::config::Config::load()
                    .map(|c| c.agent_detect)
                    .unwrap_or_else(|_| crate::config::Config::default().agent_detect);
                let status_interval = Duration::from_millis(200);

                // Deferred rendering (same as local terminal loop).
                let mut pending_notify = false;
                let mut first_pending_time: Option<Instant> = None;
                const RENDER_GAP: Duration = Duration::from_millis(16);
                const MAX_RENDER_DELAY: Duration = Duration::from_millis(200);

                // Shell-path dirty flag: set when data is processed,
                // cleared when render tick fires.
                let mut dirty = false;

                // Initial status check for recovered sessions (same as setup_local_terminal).
                {
                    if let Some(agent_def) = detect_agent_in_pane(&pane_target_clone, &agent_detect) {
                        agent_override = Some(agent_def.clone());
                        let screen_text = terminal_for_output.screen_tail_text(
                            terminal_for_output.size().rows as usize,
                        );
                        if let Some(ref pub_) = status_publisher {
                            let detected = agent_def.detect_status(&screen_text);
                            let _ = pub_.force_status(&status_key_clone, detected, &screen_text, &agent_def.message_skip_patterns);
                        }
                    } else if is_pane_shell(&pane_target_clone) {
                        if let Some(ref pub_) = status_publisher {
                            let _ = pub_.force_status(&status_key_clone, AgentStatus::Idle, "", &[]);
                        }
                    }
                }

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

                            if alt_screen && agent_override.is_some() {
                                // TUI agent (e.g. opencode): use text pattern detection,
                                // not hardcoded ShellPhase::Input which always maps to Idle.
                                let agent_def = agent_override.as_ref().unwrap();
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
                            } else if alt_screen {
                                // Alt screen without agent override (e.g. vim, htop)
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
                                let mut is_selecting = terminal_for_output.selecting().load(std::sync::atomic::Ordering::Relaxed);
                                if is_selecting {
                                    let sel_start = terminal_for_output.selecting_since().load(std::sync::atomic::Ordering::Relaxed);
                                    let now_ms = std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .map(|d| d.as_millis() as u64)
                                        .unwrap_or(0);
                                    if sel_start > 0 && now_ms.saturating_sub(sel_start) >= 5000 {
                                        terminal_for_output.selecting().store(false, std::sync::atomic::Ordering::Relaxed);
                                        terminal_for_output.selecting_since().store(0, std::sync::atomic::Ordering::Relaxed);
                                        is_selecting = false;
                                    }
                                }
                                if dirty {
                                    terminal_for_output.take_dirty();
                                    if !modal_open.load(Ordering::Relaxed) && !is_selecting {
                                        if let Some(ref tae) = term_area_entity {
                                            let _ = cx.update_entity(tae, |_, cx| cx.notify());
                                        }
                                    }
                                    dirty = false;
                                } else {
                                    if terminal_for_output.take_dirty()
                                        && !modal_open.load(Ordering::Relaxed)
                                        && !is_selecting
                                    {
                                        if let Some(ref tae) = term_area_entity {
                                            let _ = cx.update_entity(tae, |_, cx| cx.notify());
                                        }
                                    }
                                }
                            }
                            Either::Right((Either::Right((_, _)), _)) => {
                                // ── idle_tick fired ──
                                let mut is_selecting = terminal_for_output.selecting().load(std::sync::atomic::Ordering::Relaxed);
                                if is_selecting {
                                    let sel_start = terminal_for_output.selecting_since().load(std::sync::atomic::Ordering::Relaxed);
                                    let now_ms = std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .map(|d| d.as_millis() as u64)
                                        .unwrap_or(0);
                                    if sel_start > 0 && now_ms.saturating_sub(sel_start) >= 5000 {
                                        terminal_for_output.selecting().store(false, std::sync::atomic::Ordering::Relaxed);
                                        terminal_for_output.selecting_since().store(0, std::sync::atomic::Ordering::Relaxed);
                                        is_selecting = false;
                                    }
                                }
                                if terminal_for_output.take_dirty()
                                    && !modal_open.load(Ordering::Relaxed)
                                    && !is_selecting
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
            })
            .detach();
        } else {
            if let Ok(mut buffers) = self.buffers.lock() {
                buffers.insert(
                    pane_target_str,
                    TerminalBuffer::Error("Streaming unavailable.".to_string()),
                );
            }
            cx.notify();
        }
    }

    // ========================================================================
    // Ghostty terminal setup (new engine)
    // ========================================================================

    /// Create a gpui-ghostty backed terminal pane.
    ///
    /// Similar to `setup_local_terminal()` but uses the ghostty VT engine
    /// instead of the alacritty-based Terminal. Keeps the same output pump
    /// and content extractor pipeline for agent status detection.
    pub fn setup_ghostty_terminal_pane(
        &mut self,
        runtime: &Arc<dyn AgentRuntime>,
        pane_id: &str,
        cols: u16,
        rows: u16,
        cx: &mut Context<Self>,
    ) -> Result<(), RuntimeError> {
        // 1. Subscribe to output
        let output_rx = runtime
            .subscribe_output(&pane_id.to_string())
            .ok_or_else(|| RuntimeError::PaneNotFound(pane_id.to_string()))?;

        // 2. Create ghostty session
        let config = TerminalConfig {
            cols,
            rows,
            default_fg: ghostty_vt::Rgb {
                r: 0xcc,
                g: 0xcc,
                b: 0xcc,
            },
            default_bg: ghostty_vt::Rgb {
                r: 0x1e,
                g: 0x1e,
                b: 0x1e,
            },
            update_window_title: true,
        };
        let session = TerminalSession::new(config)
            .map_err(|e| RuntimeError::Backend(format!("TerminalSession::new failed: {:?}", e)))?;

        // 3. Input callback — routes keystrokes to the runtime
        let runtime_clone = runtime.clone();
        let pane_id_clone = pane_id.to_string();
        let modal_open = self.modal_overlay_open.clone();
        let input = TerminalInput::new(move |bytes| {
            if !modal_open.load(Ordering::Relaxed) {
                let _ = runtime_clone.send_input(&pane_id_clone, bytes);
            }
        });

        // 4. Create gpui-ghostty TerminalView entity
        let focus = cx.focus_handle();
        let view = cx.new(|_cx| GhosttyTerminalView::new_with_input(session, focus.clone(), input));

        // Pre-populate with initial capture (if available) synchronously
        if let Ok(initial_chunk) = output_rx.try_recv() {
            view.update(cx, |v, cx| {
                v.queue_output_bytes(&initial_chunk, cx);
            });
        }

        // 5. Output pump (async) — feeds bytes to ghostty view + content extractor
        let view_clone = view.clone();
        let status_publisher = self.status_publisher.clone();
        let status_key = pane_id.to_string();
        let pane_id_for_detect = pane_id.to_string();
        let term_area_entity = self.area_entity.clone();

        cx.spawn(async move |_this, cx| {
            let mut extractor = ContentExtractor::new();
            let mut last_phase = extractor.shell_phase();
            let agent_detect: crate::config::AgentDetectConfig = crate::config::Config::load()
                .map(|c| c.agent_detect)
                .unwrap_or_else(|_| crate::config::Config::default().agent_detect);
            let mut agent_override: Option<crate::config::AgentDef> = None;
            let status_interval = std::time::Duration::from_millis(200);
            let mut last_status_check = std::time::Instant::now();

            loop {
                match output_rx.recv_async().await {
                    Ok(bytes) => {
                        // Drain any additional buffered chunks
                        let mut all_bytes = bytes;
                        while let Ok(next) = output_rx.try_recv() {
                            all_bytes.extend_from_slice(&next);
                        }

                        // Feed to ghostty view
                        let _ = cx.update_entity(&view_clone, |v, cx| {
                            v.queue_output_bytes(&all_bytes, cx);
                        });

                        // Feed to content extractor for status detection
                        extractor.feed(&all_bytes);

                        // Notify terminal area entity for redraw
                        if let Some(ref tae) = term_area_entity {
                            let _ = cx.update_entity(tae, |_, cx| cx.notify());
                        }

                        // Status detection (throttled)
                        let now = std::time::Instant::now();
                        let phase = extractor.shell_phase();
                        if phase != last_phase
                            || now.duration_since(last_status_check) >= status_interval
                        {
                            last_status_check = now;

                            // Agent detection
                            if agent_override.is_none()
                                && matches!(
                                    phase,
                                    crate::shell_integration::ShellPhase::Running
                                        | crate::shell_integration::ShellPhase::Unknown
                                )
                            {
                                agent_override =
                                    detect_agent_in_pane(&pane_id_for_detect, &agent_detect);
                            } else if matches!(
                                phase,
                                crate::shell_integration::ShellPhase::Input
                                    | crate::shell_integration::ShellPhase::Prompt
                                    | crate::shell_integration::ShellPhase::Output
                            ) {
                                agent_override = None;
                            }
                            last_phase = phase;

                            if let Some(ref pub_) = status_publisher {
                                if let Some(ref agent_def) = agent_override {
                                    let content_str =
                                        extractor.content_for_status(MAX_STATUS_CONTENT_LEN);
                                    let detected = agent_def.detect_status(&content_str);
                                    let _ = pub_.force_status(
                                        &status_key,
                                        detected,
                                        &content_str,
                                        &agent_def.message_skip_patterns,
                                    );
                                } else {
                                    let content_str =
                                        extractor.content_for_status(MAX_STATUS_CONTENT_LEN);
                                    let shell_info = ShellPhaseInfo {
                                        phase,
                                        last_post_exec_exit_code: extractor.last_exit_code(),
                                    };
                                    let _ = pub_.check_status(
                                        &status_key,
                                        crate::status_detector::ProcessStatus::Running,
                                        Some(shell_info),
                                        &content_str,
                                        &[],
                                    );
                                }
                            }
                        }
                    }
                    Err(_) => break, // Channel closed
                }
            }
        })
        .detach();

        // 6. Store buffer
        if let Ok(mut buffers) = self.buffers.lock() {
            buffers.insert(
                pane_id.to_string(),
                TerminalBuffer::GhosttyTerminal {
                    view: view.clone(),
                },
            );
        }

        // Update focus
        self.focus = Some(focus);

        Ok(())
    }

    /// Resize a ghostty terminal pane (both runtime and view).
    pub fn resize_ghostty_pane(
        &self,
        pane_id: &str,
        runtime: &Arc<dyn AgentRuntime>,
        cols: u16,
        rows: u16,
        cx: &mut App,
    ) {
        let _ = runtime.resize(&pane_id.to_string(), cols, rows);
        if let Some(TerminalBuffer::GhosttyTerminal { view }) =
            self.buffers.lock().ok().and_then(|b| b.get(pane_id).cloned())
        {
            view.update(cx, |v, cx| {
                v.resize_terminal(cols, rows, cx);
            });
        }
    }
}
