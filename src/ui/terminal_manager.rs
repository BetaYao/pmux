//! TerminalManager - manages terminal buffers, resize, focus, IME, and search.
//!
//! Extracted from AppRoot Phase 4.
//! Observes RuntimeManager for runtime reference.

use crate::runtime::{AgentRuntime, RuntimeError, StatusPublisher};
use crate::shell_integration::ShellPhaseInfo;
use crate::terminal::ContentExtractor;
use crate::terminal::{GhosttyTerminalView, TerminalConfig, TerminalInput, TerminalSession};
use crate::ui::terminal_controller::ResizeController;
use crate::ui::terminal_view::TerminalBuffer;
use crate::ui::terminal_area_entity::TerminalAreaEntity;
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
                    focus_handle: focus.clone(),
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
        if let Some(TerminalBuffer::GhosttyTerminal { view, .. }) =
            self.buffers.lock().ok().and_then(|b| b.get(pane_id).cloned())
        {
            view.update(cx, |v, cx| {
                v.resize_terminal(cols, rows, cx);
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers moved from app_root.rs (used exclusively by TerminalManager pipelines)
// ---------------------------------------------------------------------------

/// Max terminal content length (chars) passed to status detection. Capping avoids O(n) regex
/// work on huge buffers in large/active panes (e.g. big monorepos), keeping input responsive.
const MAX_STATUS_CONTENT_LEN: usize = 32_768;

/// Detect which agent is running in a tmux pane.
///
/// First checks `pane_current_command` (fast). If that doesn't match a known agent,
/// falls back to checking child processes of the pane shell. This handles cases where
/// tmux reports the binary filename instead of the symlink name (e.g. Claude CLI's
/// binary is `2.1.72` but the symlink is `claude`).
fn detect_agent_in_pane(
    pane_target: &str,
    agent_detect: &crate::config::AgentDetectConfig,
) -> Option<crate::config::AgentDef> {
    // Fast path: check pane_current_command directly
    if let Ok(out) = std::process::Command::new("tmux")
        .args(["display-message", "-t", pane_target, "-p", "#{pane_current_command}"])
        .output()
    {
        let cmd = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if let Some(agent) = agent_detect.find_agent(&cmd) {
            return Some(agent.clone());
        }
    }

    // Slow path: check child processes of the pane's shell.
    // tmux may report a version-named binary (e.g. "2.1.72" for Claude CLI)
    // instead of the symlink name ("claude"). Walk the process tree to find
    // the real command.
    if let Ok(out) = std::process::Command::new("tmux")
        .args(["display-message", "-t", pane_target, "-p", "#{pane_pid}"])
        .output()
    {
        let pane_pid = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if let Ok(pid) = pane_pid.parse::<u32>() {
            // pgrep -P <pid> lists direct children
            if let Ok(children) = std::process::Command::new("pgrep")
                .args(["-P", &pid.to_string()])
                .output()
            {
                let child_pids = String::from_utf8_lossy(&children.stdout);
                for child_pid in child_pids.lines().map(str::trim).filter(|s| !s.is_empty()) {
                    // Get the command name of each child process
                    if let Ok(ps_out) = std::process::Command::new("ps")
                        .args(["-o", "comm=", "-p", child_pid])
                        .output()
                    {
                        let child_cmd = String::from_utf8_lossy(&ps_out.stdout).trim().to_string();
                        if let Some(agent) = agent_detect.find_agent(&child_cmd) {
                            return Some(agent.clone());
                        }
                    }
                }
            }
        }
    }

    None
}

