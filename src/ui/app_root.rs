// ui/app_root.rs - Root component for pmux GUI
use crate::agent_status::{StatusCounts, AgentStatus};
use crate::config::Config;
use crate::remotes::{RemoteChannelPublisher, spawn_remote_gateways};
use crate::remotes::secrets::Secrets;
use crate::deps::{self, DependencyCheckResult};
use crate::file_selector::show_folder_picker_async;
use crate::git_utils::{is_git_repository, get_git_error_message, GitError};
use crate::notification::NotificationType;
use crate::notification_manager::NotificationManager;
use crate::system_notifier;
use crate::shell_integration::ShellPhaseInfo;
use crate::terminal::ContentExtractor;
use crate::runtime::{AgentRuntime, EventBus, RuntimeEvent, StatusPublisher};
use crate::runtime::backends::{create_runtime_from_env, kill_tmux_window, legacy_window_name_for_worktree, list_tmux_windows, migrate_tmux_window_name, recover_runtime, resolve_backend, session_name_for_workspace, window_name_for_worktree, window_target};
use crate::runtime::{RuntimeState, WorktreeState};
use crate::ui::{AppState, sidebar::Sidebar, workspace_tabbar::WorkspaceTabBar, terminal_controller::ResizeController, terminal_view::TerminalBuffer, terminal_area_entity::TerminalAreaEntity, notification_panel_entity::NotificationPanelEntity, new_branch_dialog_entity::NewBranchDialogEntity, close_tab_dialog_ui::CloseTabDialogUi, delete_worktree_dialog_ui::DeleteWorktreeDialogUi, split_pane_container::SplitPaneContainer, diff_view::DiffViewOverlay, status_bar::StatusBar, models::{StatusCountsModel, NotificationPanelModel, NewBranchDialogModel, PaneSummaryModel}, topbar_entity::TopBarEntity};
use crate::split_tree::SplitNode;
use crate::workspace_manager::WorkspaceManager;
use crate::input::{key_to_xterm_escape, KeyModifiers};
use futures_util::future::{select, Either};
use futures_util::pin_mut;
use crate::window_state::PersistentAppState;
use crate::new_branch_orchestrator::{NewBranchOrchestrator, CreationResult, NotificationSender};
use crate::notification::Notification;
use gpui::prelude::FluentBuilder;
use gpui::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

// Terminal clipboard actions — dispatched via GPUI key bindings (cmd-v, cmd-c)
// so they work correctly even when TerminalInputHandler is active.
actions!(pmux_terminal, [TerminalPaste, TerminalCopy]);

/// When true, AppRoot will set show_settings=true and clear this flag at start of render.
/// Used by menu action (open_settings) to open Settings from main.rs without window access.
pub static OPEN_SETTINGS_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Max terminal content length (chars) passed to status detection. Capping avoids O(n) regex
/// work on huge buffers in large/active panes (e.g. big monorepos), keeping input responsive.
const MAX_STATUS_CONTENT_LEN: usize = 32_768;

// ---------------------------------------------------------------------------
// Clipboard paste helpers (image / file / text)
// ---------------------------------------------------------------------------

/// Map GPUI ImageFormat to file extension.
fn image_format_extension(format: ImageFormat) -> &'static str {
    match format {
        ImageFormat::Png => "png",
        ImageFormat::Jpeg => "jpeg",
        ImageFormat::Webp => "webp",
        ImageFormat::Gif => "gif",
        ImageFormat::Svg => "svg",
        ImageFormat::Bmp => "bmp",
        ImageFormat::Tiff => "tiff",
        ImageFormat::Ico => "ico",
    }
}

/// Save clipboard image bytes to a temp file under `$TMPDIR/pmux-images/`.
/// Returns the absolute path on success, `None` on failure.
fn save_clipboard_image_to_temp(image: &Image) -> Option<String> {
    let dir = std::env::temp_dir().join("pmux-images");
    let _ = std::fs::create_dir_all(&dir);
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let ext = image_format_extension(image.format);
    let filename = format!("pmux-paste-{}-{:x}.{}", ts, image.id, ext);
    let path = dir.join(&filename);
    match std::fs::write(&path, &image.bytes) {
        Ok(()) => Some(path.to_string_lossy().into_owned()),
        Err(e) => {
            eprintln!("pmux: failed to save pasted image: {}", e);
            None
        }
    }
}

/// Remove pmux paste image files older than 24 hours.
/// Errors are silently ignored (directory may not exist yet).
fn cleanup_old_paste_images() {
    let dir = std::env::temp_dir().join("pmux-images");
    let Ok(entries) = std::fs::read_dir(&dir) else { return };
    let cutoff = std::time::SystemTime::now()
        - std::time::Duration::from_secs(24 * 60 * 60);
    for entry in entries.flatten() {
        if let Ok(meta) = entry.metadata() {
            if let Ok(modified) = meta.modified() {
                if modified < cutoff {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
    }
}

/// Shell-quote a path if it contains special characters (spaces, quotes, etc.).
fn shell_quote_path(path: &str) -> String {
    let needs_quoting = path.chars().any(|c| matches!(c,
        ' ' | '\'' | '"' | '(' | ')' | '&' | '|' | ';' | '$'
        | '`' | '!' | '#' | '*' | '?' | '[' | ']' | '{' | '}'
    ));
    if needs_quoting {
        format!("'{}'", path.replace('\'', "'\\''"))
    } else {
        path.to_string()
    }
}

/// Build paste text from all clipboard entries.
/// - `String` entries: concatenated as-is (preserves existing text-paste behaviour).
/// - `Image` entries: saved to a temp file, path pasted (enables Cmd+V image paste).
/// - `ExternalPaths` entries: file paths pasted with shell quoting.
fn build_paste_text_from_clipboard(clipboard: &ClipboardItem) -> String {
    let mut result = String::new();
    let mut did_cleanup = false;
    for entry in clipboard.entries() {
        match entry {
            ClipboardEntry::String(cs) => {
                result.push_str(&cs.text);
            }
            ClipboardEntry::Image(image) => {
                if !did_cleanup {
                    cleanup_old_paste_images();
                    did_cleanup = true;
                }
                if let Some(path) = save_clipboard_image_to_temp(image) {
                    if !result.is_empty() { result.push(' '); }
                    result.push_str(&shell_quote_path(&path));
                }
            }
            ClipboardEntry::ExternalPaths(paths) => {
                for p in paths.paths() {
                    if !result.is_empty() { result.push(' '); }
                    result.push_str(&shell_quote_path(&p.display().to_string()));
                }
            }
        }
    }
    result
}

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

/// Check if the tmux pane's foreground process is a shell (zsh, bash, fish, etc.)
fn is_pane_shell(pane_target: &str) -> bool {
    if let Ok(out) = std::process::Command::new("tmux")
        .args(["display-message", "-t", pane_target, "-p", "#{pane_current_command}"])
        .output()
    {
        let cmd = String::from_utf8_lossy(&out.stdout).trim().to_string();
        matches!(cmd.as_str(), "zsh" | "bash" | "fish" | "sh" | "dash" | "ksh" | "tcsh" | "csh" | "nu" | "elvish" | "pwsh")
    } else {
        false
    }
}


/// Notification sender that forwards to AppRoot's NotificationManager
struct AppNotificationSender {
    manager: Arc<Mutex<NotificationManager>>,
}

impl NotificationSender for AppNotificationSender {
    fn send(&self, notification: Notification) {
        if let Ok(mut mgr) = self.manager.lock() {
            mgr.add(notification.pane_id(), notification.notif_type(), notification.message());
        }
    }
}

/// Coalesce terminal output chunks into a single buffer and process once.
/// Returns `Ok(true)` if output data was received, `Ok(false)` for idle timeout.
/// Returns `Err` if the channel is closed.
async fn coalesce_and_process_output(
    rx: &flume::Receiver<Vec<u8>>,
    terminal: &crate::terminal::Terminal,
    ext: &mut ContentExtractor,
    idle_timeout: std::time::Duration,
    cx: &AsyncApp,
) -> Result<bool, flume::RecvError> {
    use std::time::Duration;

    // Step 1: Wait for first chunk OR idle timeout.
    let first_chunk: Vec<u8>;
    {
        let timer = cx.background_executor().timer(idle_timeout);
        let recv = rx.recv_async();
        pin_mut!(timer);
        pin_mut!(recv);
        match select(recv, timer).await {
            Either::Left((Ok(chunk), _)) => {
                first_chunk = chunk;
            }
            Either::Left((Err(e), _)) => return Err(e),
            Either::Right((_, _)) => return Ok(false),
        }
    }

    // Step 2: Adaptive coalescing window.
    // Alt-screen (TUI programs): use 50ms to capture more of the TUI frame in
    // one batch. tmux strips CSI 2026, so we can't detect frame boundaries;
    // a wider window reduces mid-frame renders that cause ghosting.
    // Normal shell: 4ms for responsive keystroke echo.
    let coalesce_ms: u64 = if terminal.is_alt_screen() { 50 } else { 4 };

    let mut coalesce_buf = first_chunk;

    // Wait the FULL coalescing window, then drain everything that arrived.
    // This ensures all %output events for a single TUI frame are collected
    // before processing. In tmux mode, CSI 2026 is consumed by tmux so
    // this coalescing window is the primary defense against ghosting.
    cx.background_executor()
        .timer(Duration::from_millis(coalesce_ms))
        .await;

    // Drain all chunks that arrived during the coalescing window.
    while let Ok(next) = rx.try_recv() {
        coalesce_buf.extend_from_slice(&next);
    }

    // Step 3: Single-shot processing of the entire coalesced buffer.
    terminal.process_output(&coalesce_buf);
    ext.feed(&coalesce_buf);

    // Step 4: Signal that output was received.
    Ok(true)
}

/// Main application root component
pub struct AppRoot {
    state: AppState,
    workspace_manager: WorkspaceManager,
    status_counts: StatusCounts,
    notification_manager: Arc<Mutex<NotificationManager>>,
    sidebar_visible: bool,
    /// Per-pane terminal buffers (Term = pipe-pane/control mode streaming; Legacy = error placeholder only)
    terminal_buffers: Arc<Mutex<HashMap<String, TerminalBuffer>>>,
    /// Split layout tree (single Pane or Vertical/Horizontal with children)
    split_tree: SplitNode,
    /// Index of focused pane in flatten() order
    focused_pane_index: usize,
    /// When dragging a divider: (path, start_pos, start_ratio, is_vertical)
    split_divider_drag: Option<(Vec<bool>, f32, f32, bool)>,
    /// Active pane target (e.g. "local:/path/to/worktree")
    active_pane_target: Option<String>,
    /// Shared target for input routing (updated when switching panes)
    active_pane_target_shared: Arc<Mutex<String>>,
    /// List of pane targets (for multi-pane split layout)
    pane_targets_shared: Arc<Mutex<Vec<String>>>,
    /// Runtime for terminal/backend operations (local PTY)
    runtime: Option<Arc<dyn AgentRuntime>>,
    /// Real-time agent status per pane ID
    pane_statuses: Arc<Mutex<HashMap<String, AgentStatus>>>,
    /// Event Bus for status/notification events
    event_bus: Arc<EventBus>,
    /// Status publisher (publishes to EventBus, replaces StatusPoller)
    status_publisher: Option<StatusPublisher>,
    /// JSONL session scanner for supplementary agent status detection
    session_scanner: Option<crate::session_scanner::SessionScanner>,
    /// Status key base for current worktree (e.g. "local:/path/to/worktree")
    status_key_base: Option<String>,
    /// Whether EventBus subscription has been started (spawn once)
    event_bus_subscription_started: bool,
    /// NewBranchDialogModel + Entity - dialog state; Entity observes, re-renders only when model notifies
    new_branch_dialog_model: Option<Entity<NewBranchDialogModel>>,
    new_branch_dialog_entity: Option<Entity<NewBranchDialogEntity>>,
    /// Focus handle for new branch dialog input (focus on open)
    dialog_input_focus: Option<FocusHandle>,
    /// Delete worktree confirmation dialog
    delete_worktree_dialog: DeleteWorktreeDialogUi,
    /// Close tab confirmation dialog (with tmux session cleanup option)
    close_tab_dialog: CloseTabDialogUi,
    /// Pending worktree selection to be processed on next render
    pending_worktree_selection: Option<usize>,
    /// When Some(idx): switching to worktree idx, show loading in terminal area
    worktree_switch_loading: Option<usize>,
    /// Current active worktree index (synced with Sidebar/TabBar)
    active_worktree_index: Option<usize>,
    /// Cached worktrees for active repo. Refreshed on workspace change, branch create/delete, explicit refresh.
    /// Avoids calling discover_worktrees in render path.
    cached_worktrees: Vec<crate::worktree::WorktreeInfo>,
    /// Repo path for which cached_worktrees is valid
    cached_worktrees_repo: Option<PathBuf>,
    /// Cached tmux window names for the current repo; filled once when opening repo to avoid repeated list-windows calls.
    cached_tmux_windows: Option<(PathBuf, Vec<String>)>,
    /// Maps worktree path → repo path (workspace tab path). Built incrementally
    /// when worktrees are discovered for each repo. Used for per-tab agent counts.
    worktree_to_repo_map: HashMap<PathBuf, PathBuf>,
    /// Sidebar context menu: which worktree index has menu open, and mouse (x, y) position
    sidebar_context_menu: Option<(usize, f32, f32)>,
    /// Terminal context menu: mouse (x, y) position when right-clicked
    terminal_context_menu: Option<(f32, f32)>,
    /// Built-in diff view entity (replaces nvim+diffview overlay)
    diff_view_entity: Option<Entity<DiffViewOverlay>>,
    /// Sidebar width in pixels (persisted to state.json)
    sidebar_width: u32,
    /// When Some, dependency check failed - show self-check page
    dependency_check: Option<DependencyCheckResult>,
    /// When true, focus terminal area on next frame (keyboard input without clicking first)
    terminal_needs_focus: bool,
    /// Stable focus handle for terminal area (must persist across renders for key events)
    terminal_focus: Option<FocusHandle>,
    /// ResizeController: debounced window bounds → (cols, rows) for runtime resize.
    /// Resize is driven here; gpui-terminal uses with_resize_callback.
    resize_controller: ResizeController,
    /// Last (cols, rows) we resized to. Used to initialize new engines at full size (avoids flash).
    preferred_terminal_dims: Option<(u16, u16)>,
    /// Shared dims updated by resize callback (callable from paint phase without cx).
    shared_terminal_dims: Arc<std::sync::Mutex<Option<(u16, u16)>>>,
    /// When true, show the Settings modal overlay
    show_settings: bool,
    /// Draft config when Settings is open; None when closed. Updated on open and by toggles.
    settings_draft: Option<Config>,
    /// Draft secrets when Settings is open; None when closed.
    settings_secrets_draft: Option<Secrets>,
    /// Which channel config panel is open: "discord", "kook", "feishu"
    settings_configuring_channel: Option<String>,
    /// Which agent is being edited in the Agent Detect settings (index into agent_detect.agents)
    settings_editing_agent: Option<usize>,
    /// Active settings tab: "channels" or "agent_detect"
    settings_tab: String,
    /// Focus handle for the settings modal (steals focus from terminal when open)
    settings_focus: Option<FocusHandle>,
    /// Which settings text field is focused: "agent-name-{idx}", "rule-patterns-{agent_idx}-{rule_idx}"
    settings_focused_field: Option<String>,
    /// StatusCountsModel - TopBar/StatusBar observe this for entity-scoped re-render (Phase 0 spike)
    status_counts_model: Option<Entity<StatusCountsModel>>,
    /// TopBar Entity - observes StatusCountsModel, re-renders only when status changes
    topbar_entity: Option<Entity<TopBarEntity>>,
    /// NotificationPanelModel - show_panel, unread_count; Panel + bell observe this
    notification_panel_model: Option<Entity<NotificationPanelModel>>,
    /// NotificationPanel Entity - observes model, re-renders only when panel state changes
    notification_panel_entity: Option<Entity<NotificationPanelEntity>>,
    /// Terminal area Entity - when content changes, notify this instead of AppRoot (Phase 4)
    terminal_area_entity: Option<Entity<TerminalAreaEntity>>,
    /// When true, new branch (or other modal) dialog is open; terminal output loop skips
    /// notifying terminal area so the main thread stays responsive for dialog input (e.g. in large repos).
    modal_overlay_open: Arc<AtomicBool>,
    /// When true, a split divider is being dragged; resize callbacks skip runtime.resize()
    /// to prevent tmux feedback loop (resize-pane redistributes space, fighting the UI ratio).
    split_dragging: Arc<AtomicBool>,
    /// IME: set on Enter (no Cmd/Alt); cleared when replace_text_in_range runs or after 50ms timeout. Ensures "commit + Enter" sends text then \\r (no extra newline).
    ime_pending_enter: Arc<AtomicBool>,
    /// When true, search bar is visible and keyboard input appends to search_query
    search_active: bool,
    /// Current search query (when search_active)
    search_query: String,
    /// Index of current match when cycling (Enter/Cmd+G)
    search_current_match: usize,
    /// PaneSummaryModel - per-pane last_line + status_since for Sidebar
    pane_summary_model: Option<Entity<PaneSummaryModel>>,
    /// Running animation frame index (cycles through RUNNING_FRAMES)
    running_animation_frame: usize,
    /// Running animation timer task (250ms tick)
    running_animation_task: Option<gpui::Task<()>>,
    /// Whether the pmux window is focused (shared with event loop for notification suppression)
    window_focused_shared: Arc<AtomicBool>,
    /// Timestamp of last user keyboard input (shared with event loop for notification suppression)
    last_input_time: Arc<Mutex<std::time::Instant>>,
    /// Pending notification jump target: (pane_id, timestamp). Set when a system notification is
    /// sent so that clicking the notification (which activates the window) auto-focuses the pane.
    pending_notification_jump: Arc<Mutex<Option<(String, std::time::Instant)>>>,
    /// Previous window focus state, used to detect unfocused→focused transitions for notification click-to-focus.
    was_window_focused: bool,
    /// Available update info (set by background check)
    update_available: Option<crate::updater::UpdateInfo>,
    /// Whether an update download is in progress
    update_downloading: bool,
}

const RUNNING_ANIMATION_INTERVAL_MS: u64 = 250;

/// Derive a human-readable source label from a pane_id.
/// Format: "repo / worktree" or "repo / worktree / pane N"
/// Handles "local:/path/to/worktree" and "local:/path/to/worktree:N" formats.
fn pane_id_to_source_label(pane_id: &str) -> String {
    let path_str = if let Some(s) = pane_id.strip_prefix("local:") {
        s
    } else {
        // For tmux or other backends, return as-is
        return pane_id.to_string();
    };

    // Split off optional pane index suffix (e.g. ":1")
    let (path_part, pane_num) = {
        // Find the last ':' that is followed only by digits
        if let Some(colon_pos) = path_str.rfind(':') {
            let suffix = &path_str[colon_pos + 1..];
            if !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit()) {
                let idx: usize = suffix.parse().unwrap_or(0);
                (&path_str[..colon_pos], idx + 1)
            } else {
                (path_str, 1usize)
            }
        } else {
            (path_str, 1usize)
        }
    };

    let path = std::path::Path::new(path_part);
    let components: Vec<_> = path.components().collect();
    let n = components.len();

    let label = if n >= 2 {
        let parent = components[n - 2].as_os_str().to_string_lossy();
        let child = components[n - 1].as_os_str().to_string_lossy();
        format!("{} / {}", parent, child)
    } else {
        path.file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| path_part.to_string())
    };

    if pane_num > 1 {
        format!("{} / pane {}", label, pane_num)
    } else {
        label
    }
}

/// Extract the worktree filesystem path from a pane_id.
/// Handles: "local:/path/to/worktree" → Some("/path/to/worktree")
///          "local:/path/to/worktree:1" → Some("/path/to/worktree")
///          "%0" (tmux) → None
fn extract_worktree_path_from_pane_id(pane_id: &str) -> Option<PathBuf> {
    let path_str = pane_id.strip_prefix("local:")?;
    if let Some(colon_pos) = path_str.rfind(':') {
        let suffix = &path_str[colon_pos + 1..];
        if !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit()) {
            return Some(PathBuf::from(&path_str[..colon_pos]));
        }
    }
    Some(PathBuf::from(path_str))
}

impl AppRoot {
    /// Get sidebar width for persistence (clamped 200-400)
    pub fn sidebar_width(&self) -> u32 {
        self.sidebar_width.clamp(200, 400)
    }

    /// Save workspace state to Config (paths and active tab index only; worktree selection follows tmux window name).
    pub fn save_config(&self) {
        let mut config = Config::load().unwrap_or_default();
        let paths = self.workspace_manager.workspace_paths();
        config.save_workspaces(&paths, self.workspace_manager.active_tab_index().unwrap_or(0));
        let _ = config.save();
    }

    pub fn new() -> Self {
        let config = Config::load().unwrap_or_default();
        let mut workspace_manager = WorkspaceManager::new();

        let workspace_paths = config.get_workspace_paths();
        for path in workspace_paths {
            if is_git_repository(&path) {
                workspace_manager.add_workspace(path);
            } else {
                eprintln!("AppRoot: Saved workspace is not a valid git repository: {:?}", path);
            }
        }

        let active_idx = config.active_workspace_index.min(workspace_manager.tab_count().saturating_sub(1));
        if workspace_manager.tab_count() > 0 && active_idx < workspace_manager.tab_count() {
            workspace_manager.switch_to_tab(active_idx);
        }

        let paths = workspace_manager.workspace_paths();
        if paths.len() != config.workspace_paths.len() {
            let mut config = Config::load().unwrap_or_default();
            config.save_workspaces(&paths, workspace_manager.active_tab_index().unwrap_or(0));
            let _ = config.save();
        }

        // Load sidebar width from PersistentAppState (clamp 200-400)
        let sidebar_width = PersistentAppState::load()
            .map(|s| s.sidebar_width.clamp(200, 400))
            .unwrap_or(280);

        // Run dependency check; store result only when deps are missing
        let dependency_check = {
            let result = deps::check_dependencies_detailed();
            if result.is_ok() {
                None
            } else {
                Some(result)
            }
        };

        Self {
            state: AppState {
                workspace_path: None,
                error_message: None,
            },
            workspace_manager,
            status_counts: StatusCounts::new(),
            notification_manager: Arc::new(Mutex::new(NotificationManager::new())),
            sidebar_visible: true,
            terminal_buffers: Arc::new(Mutex::new(HashMap::new())),
            split_tree: SplitNode::pane(""),
            focused_pane_index: 0,
            split_divider_drag: None,
            active_pane_target: None,
            active_pane_target_shared: Arc::new(Mutex::new(String::new())),
            pane_targets_shared: Arc::new(Mutex::new(Vec::new())),
            runtime: None,
            pane_statuses: Arc::new(Mutex::new(HashMap::new())),
            event_bus: Arc::new(EventBus::default()),
            status_publisher: None,
            session_scanner: None,
            status_key_base: None,
        event_bus_subscription_started: false,
        new_branch_dialog_model: None,
        new_branch_dialog_entity: None,
        dialog_input_focus: None,
        delete_worktree_dialog: DeleteWorktreeDialogUi::new(),
        close_tab_dialog: CloseTabDialogUi::new(),
            pending_worktree_selection: None,
            worktree_switch_loading: None,
            active_worktree_index: None,
            cached_worktrees: Vec::new(),
            cached_worktrees_repo: None,
            cached_tmux_windows: None,
            worktree_to_repo_map: HashMap::new(),
            sidebar_context_menu: None,
            terminal_context_menu: None,
            diff_view_entity: None,
            sidebar_width,
            dependency_check,
            terminal_needs_focus: false,
            terminal_focus: None,
            resize_controller: ResizeController::new(),
            preferred_terminal_dims: None,
            shared_terminal_dims: Arc::new(std::sync::Mutex::new(None)),
            show_settings: false,
            settings_draft: None,
            settings_secrets_draft: None,
            settings_configuring_channel: None,
            settings_editing_agent: None,
            settings_tab: "channels".to_string(),
            settings_focus: None,
            settings_focused_field: None,
            status_counts_model: None,
            topbar_entity: None,
            notification_panel_model: None,
            notification_panel_entity: None,
            terminal_area_entity: None,
            modal_overlay_open: Arc::new(AtomicBool::new(false)),
            split_dragging: Arc::new(AtomicBool::new(false)),
            ime_pending_enter: Arc::new(AtomicBool::new(false)),
            search_active: false,
            search_query: String::new(),
            search_current_match: 0,
            pane_summary_model: None,
            running_animation_frame: 0,
            running_animation_task: None,
            window_focused_shared: Arc::new(AtomicBool::new(true)),
            last_input_time: Arc::new(Mutex::new(std::time::Instant::now())),
            pending_notification_jump: Arc::new(Mutex::new(None)),
            was_window_focused: true,
            update_available: None,
            update_downloading: false,
        }
    }

    /// Create StatusCountsModel and TopBarEntity when has_workspaces (Phase 0 spike).
    /// Called from init_workspace_restoration before attach_runtime so EventBus handler can use model.
    fn ensure_entities(&mut self, cx: &mut Context<Self>) {
        if self.dialog_input_focus.is_none() {
            self.dialog_input_focus = Some(cx.focus_handle());
        }
        if self.settings_focus.is_none() {
            self.settings_focus = Some(cx.focus_handle());
        }
        if !self.has_workspaces() {
            return;
        }
        if self.status_counts_model.is_none() {
            let pane_statuses = Arc::clone(&self.pane_statuses);
            let model = cx.new(move |_cx| StatusCountsModel::new(pane_statuses));
            self.status_counts_model = Some(model);
        }
        if self.pane_summary_model.is_none() {
            let model = cx.new(|_cx| PaneSummaryModel::new());
            self.pane_summary_model = Some(model);
        }
        if self.topbar_entity.is_none() {
            if let Some(ref model) = self.status_counts_model {
                let workspace_manager = self.workspace_manager.clone();
                let app_root_entity = cx.entity();
                let app_root_entity_select = app_root_entity.clone();
                let on_select = Arc::new(move |idx: usize, _w: &mut Window, cx: &mut App| {
                    let _ = cx.update_entity(&app_root_entity_select, |this: &mut AppRoot, cx| {
                        this.handle_workspace_tab_switch(idx, cx);
                        let counts = this.compute_per_tab_active_counts();
                        if let Some(ref e) = this.topbar_entity {
                            let _ = cx.update_entity(e, |t: &mut TopBarEntity, cx| {
                                t.set_workspace_manager(this.workspace_manager.clone());
                                t.set_per_tab_active_counts(counts);
                                cx.notify();
                            });
                        }
                    });
                });
                let app_root_entity_close = app_root_entity.clone();
                let on_close = Arc::new(move |idx: usize, _w: &mut Window, cx: &mut App| {
                    let _ = cx.update_entity(&app_root_entity_close, |this: &mut AppRoot, cx| {
                        if let Some(tab) = this.workspace_manager.get_tab(idx) {
                            let path = tab.path.clone();
                            let name = tab.display_name.clone();
                            this.close_tab_dialog.open(idx, path, name);
                        }
                        cx.notify();
                    });
                });
                let topbar = cx.new(move |cx| {
                    TopBarEntity::new(model.clone(), workspace_manager, on_select, on_close, cx)
                });
                self.topbar_entity = Some(topbar);
            }
        }
        if self.notification_panel_model.is_none() {
            let model = cx.new(|_cx| NotificationPanelModel::new());
            self.notification_panel_model = Some(model);
        }
        if self.notification_panel_entity.is_none() {
            if let Some(ref model) = self.notification_panel_model {
                let model = model.clone();
                let notif_mgr = Arc::clone(&self.notification_manager);
                let app_root_entity = cx.entity();
                let on_close = {
                    let model = model.clone();
                    Arc::new(move |_window: &mut Window, cx: &mut App| {
                        let _ = cx.update_entity(&model, |m: &mut NotificationPanelModel, cx| {
                            m.set_show_panel(false);
                            cx.notify();
                        });
                    })
                };
                let on_mark_read = {
                    let model = model.clone();
                    let mgr = notif_mgr.clone();
                    Arc::new(move |id: uuid::Uuid, _window: &mut Window, cx: &mut App| {
                        if let Ok(mut m) = mgr.lock() {
                            m.mark_read(id);
                            let count = m.unread_count();
                            drop(m);
                            let _ = cx.update_entity(&model, |m: &mut NotificationPanelModel, cx| {
                                m.set_unread_count(count);
                                cx.notify();
                            });
                        }
                    })
                };
                let on_clear_all = {
                    let model = model.clone();
                    let mgr = notif_mgr.clone();
                    Arc::new(move |_window: &mut Window, cx: &mut App| {
                        if let Ok(mut m) = mgr.lock() {
                            m.clear_all();
                            drop(m);
                            let _ = cx.update_entity(&model, |m: &mut NotificationPanelModel, cx| {
                                m.set_unread_count(0);
                                cx.notify();
                            });
                        }
                    })
                };
                let on_jump_to_pane = {
                    let entity = app_root_entity.clone();
                    Arc::new(move |pane_id: &str, _window: &mut Window, cx: &mut App| {
                        let _ = cx.update_entity(&entity, |this: &mut AppRoot, cx| {
                            if let Some(idx) = this.split_tree.flatten().into_iter().position(|(t, _)| t == pane_id) {
                                if this.focused_pane_index != idx {
                                    this.focused_pane_index = idx;
                                    this.active_pane_target = Some(pane_id.to_string());
                                    if let Ok(mut guard) = this.active_pane_target_shared.lock() {
                                        *guard = pane_id.to_string();
                                    }
                                    if let Some(ref rt) = this.runtime {
                                        let _ = rt.focus_pane(&pane_id.to_string());
                                    }
                                    this.terminal_needs_focus = true;
                                }
                            }
                            cx.notify();
                        });
                    })
                };
                let on_dismiss_and_jump = {
                    let entity = app_root_entity.clone();
                    let mgr = notif_mgr.clone();
                    let np_model = model.clone();
                    Arc::new(move |uuid: uuid::Uuid, pane_id: &str, _window: &mut Window, cx: &mut App| {
                        // Clear the notification
                        let unread_after = if let Ok(mut m) = mgr.lock() {
                            m.clear(uuid);
                            m.unread_count()
                        } else { 0 };
                        // Update unread count in model
                        let _ = cx.update_entity(&np_model, |m: &mut crate::ui::models::NotificationPanelModel, cx| {
                            m.set_unread_count(unread_after);
                            cx.notify();
                        });
                        // Jump to the source pane
                        let pane_id = pane_id.to_string();
                        let _ = cx.update_entity(&entity, |this: &mut AppRoot, cx| {
                            if let Some(idx) = this.split_tree.flatten().into_iter().position(|(t, _)| t == pane_id) {
                                this.focused_pane_index = idx;
                                this.active_pane_target = Some(pane_id.clone());
                                if let Ok(mut guard) = this.active_pane_target_shared.lock() {
                                    *guard = pane_id.clone();
                                }
                                if let Some(ref rt) = this.runtime {
                                    let _ = rt.focus_pane(&pane_id);
                                }
                                this.terminal_needs_focus = true;
                            }
                            cx.notify();
                        });
                    })
                };
                let entity = cx.new(move |cx| {
                    NotificationPanelEntity::new(
                        model,
                        notif_mgr,
                        on_close,
                        on_mark_read,
                        on_clear_all,
                        on_jump_to_pane,
                        on_dismiss_and_jump,
                        cx,
                    )
                });
                self.notification_panel_entity = Some(entity);
            }
        }
        if self.new_branch_dialog_model.is_none() {
            let model = cx.new(|_cx| NewBranchDialogModel::new());
            self.new_branch_dialog_model = Some(model);
        }
        if self.new_branch_dialog_entity.is_none() {
            if let (Some(ref model), Some(ref focus)) =
                (&self.new_branch_dialog_model, &self.dialog_input_focus)
            {
                let model = model.clone();
                let focus = focus.clone();
                let app_root_entity = cx.entity();
                let app_root_for_close = app_root_entity.clone();
                let entity_holder: std::sync::Arc<parking_lot::Mutex<Option<Entity<NewBranchDialogEntity>>>> =
                    Arc::new(parking_lot::Mutex::new(None));
                let on_create = {
                    let model = model.clone();
                    let entity_holder = Arc::clone(&entity_holder);
                    Arc::new(move |_window: &mut Window, cx: &mut App| {
                        let branch_name = entity_holder
                            .lock()
                            .as_ref()
                            .map(|e| e.update(cx, |this, _| this.branch_name().to_string()))
                            .unwrap_or_else(|| model.read(cx).branch_name.clone());
                        if branch_name.trim().is_empty() {
                            return;
                        }
                        let _ = cx.update_entity(&model, |m: &mut NewBranchDialogModel, cx| {
                            m.set_branch_name(&branch_name);
                            m.start_creating();
                            cx.notify();
                        });
                        let _ = cx.update_entity(&app_root_entity, |this: &mut AppRoot, cx| {
                            this.create_branch_from_model(cx);
                        });
                    }) as Arc<dyn Fn(&mut Window, &mut App) + Send + Sync>
                };
                let on_close = {
                    let model = model.clone();
                    Arc::new(move |_window: &mut Window, cx: &mut App| {
                        let _ = cx.update_entity(&model, |m: &mut NewBranchDialogModel, cx| {
                            m.close();
                            cx.notify();
                        });
                        let _ = cx.update_entity(&app_root_for_close, |this: &mut AppRoot, cx| {
                            this.modal_overlay_open.store(false, Ordering::Relaxed);
                            this.terminal_needs_focus = true;
                            cx.notify();
                        });
                    }) as Arc<dyn Fn(&mut Window, &mut App) + Send + Sync>
                };
                let on_branch_name_change = {
                    let entity_holder = Arc::clone(&entity_holder);
                    Arc::new(move |new_value: String, _window: &mut Window, cx: &mut App| {
                        if let Some(ref entity) = *entity_holder.lock() {
                            let _ = entity.update(cx, |this, cx| {
                                this.set_branch_name(new_value);
                                cx.notify();
                            });
                        }
                    }) as Arc<dyn Fn(String, &mut Window, &mut App) + Send + Sync>
                };
                let entity = cx.new(move |cx| {
                    NewBranchDialogEntity::new(
                        model,
                        focus,
                        on_create,
                        on_close,
                        on_branch_name_change,
                        cx,
                    )
                });
                *entity_holder.lock() = Some(entity.clone());
                self.new_branch_dialog_entity = Some(entity);
            }
        }
    }

    /// Initialize workspace restoration (call after AppRoot is created).
    /// Attaches to session; current worktree is derived from tmux window name (no persist).
    pub fn init_workspace_restoration(&mut self, cx: &mut Context<Self>) {
        self.ensure_entities(cx);
        if self.terminal_focus.is_none() {
            self.terminal_focus = Some(cx.focus_handle());
        }

        // Start background update check
        self.start_update_check(cx);

        let repo_path = self.workspace_manager.active_tab().map(|t| t.path.clone());

        if let Some(path) = repo_path {
            self.refresh_worktrees_for_repo(&path);

            if self.try_recover_then_switch(&path, cx) {
                return;
            }

            self.active_worktree_index = None;
            let worktrees = &self.cached_worktrees;
            if !worktrees.is_empty() {
                self.active_worktree_index = Some(0);
                let wt = &worktrees[0];
                let wt_path = wt.path.clone();
                let branch = wt.short_branch_name().to_string();
                self.schedule_switch_to_worktree_async(&path, &wt_path, &branch, 0, cx);
                return;
            }
            self.schedule_start_main_session(&path, cx);
        }
    }

    /// Spawn background update check (runs once per session, respects interval and config).
    fn start_update_check(&mut self, cx: &mut Context<Self>) {
        let config = Config::load().unwrap_or_default();
        if !config.auto_update.enabled {
            return;
        }

        // Check if enough time has elapsed since last check
        if let Some(last_ts) = config.auto_update.last_check_timestamp {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let interval_secs = config.auto_update.check_interval_hours * 3600;
            if now.saturating_sub(last_ts) < interval_secs {
                return;
            }
        }

        let skipped = config.auto_update.skipped_version.clone();
        let notification_manager = Arc::clone(&self.notification_manager);

        cx.spawn(async move |entity, cx| {
            // Delay 5 seconds to not compete with startup
            blocking::unblock(|| std::thread::sleep(std::time::Duration::from_secs(5))).await;

            let result = blocking::unblock(move || {
                crate::updater::check_for_update(skipped.as_deref())
            })
            .await;

            // Update last check timestamp
            if let Ok(mut cfg) = Config::load() {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                cfg.auto_update.last_check_timestamp = Some(now);
                let _ = cfg.save();
            }

            match result {
                Ok(crate::updater::UpdateCheckResult::UpdateAvailable(info)) => {
                    let version = info.latest_version.display();
                    if let Ok(mut mgr) = notification_manager.lock() {
                        mgr.add(
                            "__updater__",
                            crate::notification::NotificationType::Info,
                            &format!("New version available: {}", version),
                        );
                    }
                    let _ = entity.update(cx, |this: &mut AppRoot, cx| {
                        this.update_available = Some(info);
                        cx.notify();
                    });
                }
                Ok(_) => {}
                Err(e) => {
                    eprintln!("pmux: update check failed: {}", e);
                }
            }
        })
        .detach();
    }

    /// Download and install the available update, then relaunch.
    fn trigger_update(&mut self, cx: &mut Context<Self>) {
        let info = match &self.update_available {
            Some(info) => info.clone(),
            None => return,
        };
        self.update_downloading = true;
        cx.notify();

        cx.spawn(async move |entity, cx| {
            let result = blocking::unblock(move || {
                crate::updater::download_and_install(&info)
            })
            .await;

            match result {
                Ok(app_path) => {
                    // Save state before relaunch
                    let _ = entity.update(cx, |this: &mut AppRoot, _cx| {
                        this.save_config();
                    });
                    crate::updater::relaunch(&app_path);
                }
                Err(e) => {
                    let _ = entity.update(cx, |this: &mut AppRoot, cx| {
                        this.update_downloading = false;
                        if let Ok(mut mgr) = this.notification_manager.lock() {
                            mgr.add(
                                "__updater__",
                                crate::notification::NotificationType::Error,
                                &format!("Update failed: {}", e),
                            );
                        }
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }

    /// Skip the currently available update version.
    fn skip_update_version(&mut self) {
        if let Some(ref info) = self.update_available {
            let version_tag = info.latest_version.display();
            if let Ok(mut config) = Config::load() {
                config.auto_update.skipped_version = Some(version_tag);
                let _ = config.save();
            }
            self.update_available = None;
        }
    }

    fn setup_local_terminal(
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
                "preferred_dims": self.preferred_terminal_dims,
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
            let shared_dims_for_resize = Arc::clone(&self.shared_terminal_dims);
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

            let focus_handle = self.terminal_focus.get_or_insert_with(|| cx.focus_handle()).clone();
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
            if let Ok(mut buffers) = self.terminal_buffers.lock() {
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
            let term_area_entity = self.terminal_area_entity.clone();
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
                    // When a render is pending, use a short gap timeout to detect
                    // when the TUI frame is complete. Otherwise use longer timeouts
                    // for resync / idle status checks.
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
                        Err(_) => break, // channel closed
                    };

                    if !got_output {
                        // Output gap or true idle.
                        if pending_notify {
                            // Gap detected after output — TUI frame is likely complete.
                            pending_notify = false;
                            first_pending_time = None;
                            if !modal_open.load(Ordering::Relaxed)
                                && !terminal_for_output.is_synchronized_output()
                            {
                                if let Some(ref tae) = term_area_entity {
                                    let _ = cx.update_entity(tae, |_, cx| cx.notify());
                                }
                            }
                            // Don't fall through to resync — we just rendered fresh output.
                            continue;
                        }
                        // Idle resync is now handled by the background resync
                        // thread in Terminal::new_tmux() — no subprocess calls here.
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

                    let alt_screen = terminal_for_output.mode()
                        .contains(alacritty_terminal::term::TermMode::ALT_SCREEN);

                    // Throttle status detection: only run on phase change or every 200ms
                    let now = Instant::now();
                    let phase = ext.shell_phase();
                    if phase != last_phase || alt_screen != last_alt_screen
                        || now.duration_since(last_status_check) >= status_interval
                    {
                        last_status_check = now;
                        // Agent detection: query tmux when we need to identify the agent.
                        // - Running/Unknown + no agent: check process tree (handles version-named binaries)
                        // - Input/Prompt/Output: shell is back at prompt, clear agent override
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
                            // Alt screen TUI (vim, htop) → force Idle
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
                            // Known agent CLI → detect sub-status from visible screen content.
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
                            // Normal shell command → use OSC 133 phase
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

                    // Rendering strategy depends on terminal mode:
                    // - Alt-screen (TUI): defer rendering and wait for output gap to
                    //   detect TUI frame completion. This prevents ghosting caused by
                    //   tmux stripping CSI ?2026h synchronized-output markers.
                    // - Normal shell: render immediately after coalescing for responsive
                    //   keystroke echo and command output.
                    if modal_open.load(Ordering::Relaxed) {
                        // skip while modal open
                    } else if alt_screen {
                        // Alt-screen: deferred rendering.
                        if !pending_notify {
                            first_pending_time = Some(Instant::now());
                        }
                        pending_notify = true;

                        // Safety: force render if deferred too long (continuous streaming).
                        if let Some(start) = first_pending_time {
                            if start.elapsed() >= MAX_RENDER_DELAY {
                                // Resync is handled by background thread in
                                // Terminal::new_tmux() — no subprocess calls here.
                                if !terminal_for_output.is_synchronized_output() {
                                    if let Some(ref tae) = term_area_entity {
                                        let _ = cx.update_entity(tae, |_, cx| cx.notify());
                                    }
                                }
                                pending_notify = false;
                                first_pending_time = None;
                            }
                        }
                    } else {
                        // Normal shell: render immediately after coalescing.
                        if let Some(ref tae) = term_area_entity {
                            let _ = cx.update_entity(tae, |_, cx| cx.notify());
                        }
                    }

                }
            })
            .detach();
        } else {
            if let Ok(mut buffers) = self.terminal_buffers.lock() {
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
    fn setup_pane_terminal_output(
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

            let focus_handle = self.terminal_focus.get_or_insert_with(|| cx.focus_handle()).clone();
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
            if let Ok(mut buffers) = self.terminal_buffers.lock() {
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
            let term_area_entity = self.terminal_area_entity.clone();
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
                        // Idle resync is now handled by the background resync
                        // thread in Terminal::new_tmux() — no subprocess calls here.
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

                    // Throttle status detection: only run on phase change or every 200ms
                    let now = Instant::now();
                    let phase = ext.shell_phase();
                    let alt_screen = terminal_for_output.mode()
                        .contains(alacritty_terminal::term::TermMode::ALT_SCREEN);
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

                    // Rendering strategy (same as local terminal loop).
                    if modal_open.load(Ordering::Relaxed) {
                        // skip while modal open
                    } else if alt_screen {
                        // Alt-screen: deferred rendering.
                        if !pending_notify {
                            first_pending_time = Some(Instant::now());
                        }
                        pending_notify = true;

                        if let Some(start) = first_pending_time {
                            if start.elapsed() >= MAX_RENDER_DELAY {
                                // Resync handled by background thread.
                                if !terminal_for_output.is_synchronized_output() {
                                    if let Some(ref tae) = term_area_entity {
                                        let _ = cx.update_entity(tae, |_, cx| cx.notify());
                                    }
                                }
                                pending_notify = false;
                                first_pending_time = None;
                            }
                        }
                    } else {
                        // Normal shell: render immediately after coalescing.
                        if let Some(ref tae) = term_area_entity {
                            let _ = cx.update_entity(tae, |_, cx| cx.notify());
                        }
                    }
                }
            })
            .detach();
        } else {
            if let Ok(mut buffers) = self.terminal_buffers.lock() {
                buffers.insert(
                    pane_target_str,
                    TerminalBuffer::Error("Streaming unavailable.".to_string()),
                );
            }
            cx.notify();
        }
    }

    /// Attach an existing runtime: wire UI state, terminal, status publisher.
    /// Used by start_local_session, switch_to_worktree, and try_recover_*.
    /// When `saved_split_tree` is Some (multi-pane recovery), restores the full layout.
    fn attach_runtime(
        &mut self,
        runtime: Arc<dyn AgentRuntime>,
        pane_target: String,
        worktree_path: &Path,
        branch_name: &str,
        cx: &mut Context<Self>,
        saved_split_tree: Option<SplitNode>,
    ) {
        // #region agent log
        crate::debug_log::dbg_session_log(
            "app_root.rs:attach_runtime",
            "attaching runtime",
            &serde_json::json!({
                "backend_type": runtime.backend_type(),
                "pane_target": &pane_target,
                "worktree_path": worktree_path.to_string_lossy(),
                "branch_name": branch_name,
            }),
            "H_backend",
        );
        // #endregion
        self.runtime = Some(runtime.clone());

        // Validate saved_split_tree against actual tmux pane count.
        // If tmux was killed and restarted (kill-server), there is only 1 pane even if
        // the persisted state recorded multiple. Using a stale split tree would try to
        // subscribe to pane IDs that don't exist, leaving phantom terminal areas.
        let all_panes = runtime.list_panes(&pane_target);
        let actual_pane_count = all_panes.len().max(1);
        let (split_tree, pane_targets): (SplitNode, Vec<String>) = match saved_split_tree {
            Some(tree) if tree.pane_count() > 1 && tree.pane_count() <= actual_pane_count => {
                let targets: Vec<String> = tree.flatten().into_iter().map(|(t, _)| t).collect();
                (tree, targets)
            }
            _ => {
                // Kill orphan panes: when we expect a single pane but the tmux window
                // has extras (e.g. leftover 1-row panes from diff view), they steal rows
                // and prevent the main pane from resizing to target dimensions.
                if actual_pane_count > 1 {
                    for orphan in &all_panes {
                        if orphan != &pane_target {
                            let _ = runtime.kill_pane(orphan);
                        }
                    }
                }
                let _ = runtime.focus_pane(&pane_target);
                (SplitNode::pane(&pane_target), vec![pane_target.clone()])
            }
        };

        self.split_tree = split_tree;
        self.active_pane_target = Some(pane_targets[0].clone());
        self.focused_pane_index = 0;
        if let Ok(mut guard) = self.active_pane_target_shared.lock() {
            *guard = pane_targets[0].clone();
        }
        if let Ok(mut guard) = self.pane_targets_shared.lock() {
            *guard = pane_targets.clone();
        }
        self.terminal_needs_focus = true;

        self.ensure_event_bus_subscription(cx);

        // Use "local:{worktree_path}" as the status key so sidebar can look up status
        // by worktree path regardless of backend (tmux pane IDs vs local PTY IDs).
        let status_key_base = format!("local:{}", worktree_path.display());
        self.status_key_base = Some(status_key_base.clone());
        let status_publisher = StatusPublisher::new(Arc::clone(&self.event_bus));
        for (i, _pt) in pane_targets.iter().enumerate() {
            let sk = if i == 0 { status_key_base.clone() } else { format!("{}:{}", status_key_base, i) };
            status_publisher.register_pane(&sk);
        }
        self.status_publisher = Some(status_publisher);

        // Start JSONL session scanner for supplementary status detection
        let mut scanner = crate::session_scanner::SessionScanner::new(Arc::clone(&self.event_bus));
        scanner.start_watching(&status_key_base, worktree_path);
        self.session_scanner = Some(scanner);

        // Phase 4: Create TerminalAreaEntity for scoped notify (only terminal re-renders on content change)
        let repo_name = self.workspace_manager.active_tab().map(|t| t.name.clone()).unwrap_or_else(|| "workspace".to_string());
        let app_root_entity = cx.entity();
        let app_root_for_drag = app_root_entity.clone();
        let app_root_for_drag_end = app_root_entity.clone();
        let app_root_for_pane = app_root_entity.clone();
        let term_entity_holder: Arc<Mutex<Option<Entity<TerminalAreaEntity>>>> = Arc::new(Mutex::new(None));
        let term_entity_holder_for_ratio = term_entity_holder.clone();
        let term_entity_holder_for_drag = term_entity_holder.clone();
        let term_entity_holder_for_drag_end = term_entity_holder.clone();
        let term_entity_holder_for_pane = term_entity_holder.clone();
        let on_ratio = Arc::new(move |path: Vec<bool>, ratio: f32, _w: &mut Window, cx: &mut App| {
            let _ = cx.update_entity(&app_root_entity, |this: &mut AppRoot, cx| {
                this.split_tree.update_ratio(&path, ratio);
                if let Ok(guard) = term_entity_holder_for_ratio.lock() {
                    if let Some(ref e) = *guard {
                        let _ = cx.update_entity(e, |ent: &mut TerminalAreaEntity, cx| {
                            ent.set_split_tree(this.split_tree.clone());
                            cx.notify();
                        });
                    }
                }
                cx.notify();
            });
        }) as Arc<dyn Fn(Vec<bool>, f32, &mut Window, &mut App)>;
        let split_dragging_for_start = self.split_dragging.clone();
        let split_dragging_for_end = self.split_dragging.clone();
        let on_drag_start = Arc::new(move |path: Vec<bool>, pos: f32, ratio: f32, vert: bool, _w: &mut Window, cx: &mut App| {
            split_dragging_for_start.store(true, Ordering::SeqCst);
            let _ = cx.update_entity(&app_root_for_drag, |this: &mut AppRoot, cx| {
                this.split_divider_drag = Some((path.clone(), pos, ratio, vert));
                if let Ok(guard) = term_entity_holder_for_drag.lock() {
                    if let Some(ref e) = *guard {
                        let _ = cx.update_entity(e, |ent: &mut TerminalAreaEntity, cx| {
                            ent.set_split_divider_drag(Some((path, pos, ratio, vert)));
                            cx.notify();
                        });
                    }
                }
                cx.notify();
            });
        }) as Arc<dyn Fn(Vec<bool>, f32, f32, bool, &mut Window, &mut App)>;
        let on_drag_end = Arc::new(move |_w: &mut Window, cx: &mut App| {
            split_dragging_for_end.store(false, Ordering::SeqCst);
            let _ = cx.update_entity(&app_root_for_drag_end, |this: &mut AppRoot, cx| {
                this.split_divider_drag = None;
                // Force resize all terminals: during drag, runtime.resize() was suppressed
                // so the terminal process doesn't know about the new dimensions.
                // The VTE grid was already resized locally — now sync to runtime.
                if let Some(ref rt) = this.runtime {
                    if let Ok(buffers) = this.terminal_buffers.lock() {
                        for (pane_id, buf) in buffers.iter() {
                            if let TerminalBuffer::Terminal { terminal, .. } = buf {
                                let sz = terminal.size();
                                if sz.cols > 0 && sz.rows > 0 {
                                    let _ = rt.resize(pane_id, sz.cols, sz.rows);
                                }
                            }
                        }
                    }
                }
                if let Ok(guard) = term_entity_holder_for_drag_end.lock() {
                    if let Some(ref e) = *guard {
                        let _ = cx.update_entity(e, |ent: &mut TerminalAreaEntity, cx| {
                            ent.set_split_divider_drag(None);
                            cx.notify();
                        });
                    }
                }
                cx.notify();
            });
        }) as Arc<dyn Fn(&mut Window, &mut App)>;
        let terminal_focus = self.terminal_focus.clone();
        let on_pane = Arc::new(move |pane_idx: usize, window: &mut Window, cx: &mut App| {
            let _ = cx.update_entity(&app_root_for_pane, |this: &mut AppRoot, cx| {
                this.focused_pane_index = pane_idx;
                if let Some(target) = this.split_tree.focus_index_to_pane_target(pane_idx) {
                    if let Some(ref rt) = this.runtime {
                        let _ = rt.focus_pane(&target);
                    }
                    this.active_pane_target = Some(target.clone());
                    if let Ok(mut guard) = this.active_pane_target_shared.lock() {
                        *guard = target.clone();
                    }
                    this.terminal_needs_focus = false;
                    if let Ok(buffers) = this.terminal_buffers.lock() {
                        if let Some(TerminalBuffer::Terminal { focus_handle, .. }) = buffers.get(&target) {
                            window.focus(focus_handle, cx);
                        } else {
                            drop(buffers);
                            if let Some(ref focus) = terminal_focus {
                                window.focus(focus, cx);
                            }
                        }
                    } else if let Some(ref focus) = terminal_focus {
                        window.focus(focus, cx);
                    }
                } else {
                    this.terminal_needs_focus = true;
                    if let Some(ref focus) = terminal_focus {
                        window.focus(focus, cx);
                    }
                }
                if let Ok(guard) = term_entity_holder_for_pane.lock() {
                    if let Some(ref e) = *guard {
                        let _ = cx.update_entity(e, |entity: &mut TerminalAreaEntity, cx| {
                            entity.set_focused_pane_index(pane_idx);
                            cx.notify();
                        });
                    }
                }
                cx.notify();
            });
        }) as Arc<dyn Fn(usize, &mut Window, &mut App)>;

        let app_root_for_ctx_menu = cx.entity();
        let on_ctx_menu = Arc::new(move |x: f32, y: f32, _w: &mut Window, cx: &mut App| {
            let _ = cx.update_entity(&app_root_for_ctx_menu, |this: &mut AppRoot, cx| {
                this.terminal_context_menu = Some((x, y));
                cx.notify();
            });
        }) as Arc<dyn Fn(f32, f32, &mut Window, &mut App)>;

        let term_entity = cx.new(|_cx| {
            TerminalAreaEntity::new(
                self.split_tree.clone(),
                Arc::clone(&self.terminal_buffers),
                self.focused_pane_index,
                repo_name.clone(),
                true,
                self.split_divider_drag.clone(),
                Some(on_ratio),
                Some(on_drag_start),
                Some(on_drag_end),
                Some(on_pane),
                Some(on_ctx_menu),
                if self.search_active {
                    Some(self.search_query.clone())
                } else {
                    None
                },
                self.search_current_match,
            )
        });
        if let Ok(mut guard) = term_entity_holder.lock() {
            *guard = Some(term_entity.clone());
        }
        self.terminal_area_entity = Some(term_entity);

        let term_entity_for_setup = self.terminal_area_entity.clone();
        if pane_targets.len() == 1 {
            self.setup_local_terminal(runtime.clone(), &pane_targets[0], &status_key_base, term_entity_for_setup, cx);
        } else {
            if let Ok(mut buffers) = self.terminal_buffers.lock() {
                buffers.clear();
            }
            for (i, pt) in pane_targets.iter().enumerate() {
                let sk = if i == 0 { status_key_base.clone() } else { format!("{}:{}", status_key_base, i) };
                self.setup_pane_terminal_output(runtime.clone(), pt, &sk, self.terminal_area_entity.clone(), cx);
            }
        }

        if let Some(tab) = self.workspace_manager.active_tab() {
            let wp = tab.path.clone();
            self.save_runtime_state(&wp, worktree_path, branch_name);
        }
        // Sync active_worktree_index from tmux current window (match by name, no persist)
        if let Some((_, window_name)) = runtime.session_info() {
            if self.cached_worktrees_repo.as_deref() == self.workspace_manager.active_tab().map(|t| t.path.as_path()) {
                if let Some(idx) = Self::find_worktree_index_by_window_name(&self.cached_worktrees, &window_name) {
                    self.active_worktree_index = Some(idx);
                }
            }
        }
    }

    /// Start local PTY session for the given repo
    /// Sets up terminal content polling, status polling, and input handling.
    /// Backend is selected via PMUX_BACKEND env var (local or tmux).
    fn start_local_session(&mut self, worktree_path: &Path, branch_name: &str, cx: &mut Context<Self>) {
        let workspace_path = self
            .workspace_manager
            .active_tab()
            .map(|t| t.path.clone())
            .unwrap_or_else(|| worktree_path.to_path_buf());
        let config = Config::load().ok();
        let (init_cols, init_rows) = self.preferred_terminal_dims.unwrap_or_else(|| {
            config.as_ref()
                .and_then(|c| match (c.last_terminal_cols, c.last_terminal_rows) {
                    (Some(cols), Some(rows)) => Some((cols, rows)),
                    _ => None,
                })
                .unwrap_or((80, 24))
        });
        let result = match create_runtime_from_env(&workspace_path, worktree_path, branch_name, init_cols, init_rows, config.as_ref()) {
            Ok(r) => r,
            Err(e) => {
                self.state.error_message = Some(format!("Runtime error: {}", e));
                return;
            }
        };
        if let Some(msg) = &result.fallback_message {
            if let Ok(mut mgr) = self.notification_manager.lock() {
                mgr.add("", crate::notification::NotificationType::Info, msg);
            }
        }
        let runtime = result.runtime;
        let pane_target = runtime
            .primary_pane_id()
            .unwrap_or_else(|| format!("local:{}", worktree_path.display()));
        self.attach_runtime(runtime, pane_target, worktree_path, branch_name, cx, None);
    }

    /// Handle adding a new workspace
    fn handle_add_workspace(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |entity, cx| {
            let selected = show_folder_picker_async().await;
            if let Some(path) = selected {
                entity.update(cx, |this, cx| {
                    if !is_git_repository(&path) {
                        let error = GitError::NotARepository;
                        this.state.error_message = Some(get_git_error_message(&path, &error));
                    } else if this.workspace_manager.is_workspace_open(&path) {
                        if let Some(idx) = this.workspace_manager.find_workspace_index(&path) {
                            this.handle_workspace_tab_switch(idx, cx);
                        }
                    } else {
                        let idx = this.workspace_manager.add_workspace(path.clone());
                        this.workspace_manager.switch_to_tab(idx);
                        this.state.error_message = None;

                        // Save config (multi-repo state)
                        this.save_config();

                        // Start tmux session + polling (use first worktree if any)
                        this.active_worktree_index = None;
                        this.refresh_worktrees_for_repo(&path);
                        let worktrees = &this.cached_worktrees;
                        if !worktrees.is_empty() {
                            this.active_worktree_index = Some(0);
                            let wt = &worktrees[0];
                            let wt_path = wt.path.clone();
                            let branch = wt.short_branch_name().to_string();
                            this.switch_to_worktree(&wt_path, &branch, cx);
                        } else {
                            this.start_local_session(&path, "main", cx);
                        }
                        let counts = this.compute_per_tab_active_counts();
                        if let Some(ref e) = this.topbar_entity {
                            let _ = cx.update_entity(e, |t: &mut TopBarEntity, cx| {
                                t.set_workspace_manager(this.workspace_manager.clone());
                                t.set_per_tab_active_counts(counts);
                                cx.notify();
                            });
                        }
                    }
                    cx.notify();
                }).ok();
            }
        }).detach();
    }

    /// Switch to a workspace tab by index. Saves/restores Sidebar/TabBar state per repo.
    fn handle_workspace_tab_switch(&mut self, idx: usize, cx: &mut Context<Self>) {
        if idx >= self.workspace_manager.tab_count() {
            return;
        }

        // Save current worktree index to the tab we're leaving
        if let Some(current_tab) = self.workspace_manager.active_tab_mut() {
            current_tab.save_worktree_index(self.active_worktree_index);
        }

        self.workspace_manager.switch_to_tab(idx);
        self.save_config();
        self.stop_current_session();

        if let Some(tab) = self.workspace_manager.active_tab() {
            let repo_path = tab.path.clone();
            let saved_wt_index = tab.last_worktree_index();
            self.refresh_worktrees_for_repo(&repo_path);

            if self.try_recover_then_switch(&repo_path, cx) {
                // Recovery attached to the tmux session's current window, but
                // the user may have last selected a different worktree. Override
                // with the saved per-tab index and switch tmux if needed.
                if let Some(saved_idx) = saved_wt_index {
                    if saved_idx < self.cached_worktrees.len()
                        && self.active_worktree_index != Some(saved_idx)
                    {
                        let wt = &self.cached_worktrees[saved_idx];
                        let wt_path = wt.path.clone();
                        let branch = wt.short_branch_name().to_string();
                        self.active_worktree_index = Some(saved_idx);
                        self.schedule_switch_to_worktree_async(
                            &repo_path, &wt_path, &branch, saved_idx, cx,
                        );
                    }
                }
                cx.notify();
                return;
            }
            let worktrees = &self.cached_worktrees;
            let (wt_path, branch, worktree_idx) = if worktrees.is_empty() {
                self.schedule_start_main_session(&repo_path, cx);
                cx.notify();
                return;
            } else {
                // Restore saved worktree index (with bounds check), fallback to 0
                let restored_idx = saved_wt_index
                    .filter(|&i| i < worktrees.len())
                    .unwrap_or(0);
                let wt = &worktrees[restored_idx];
                self.active_worktree_index = Some(restored_idx);
                (wt.path.clone(), wt.short_branch_name().to_string(), restored_idx)
            };
            self.schedule_switch_to_worktree_async(&repo_path, &wt_path, &branch, worktree_idx, cx);
        }
        cx.notify();
    }

    /// Start tmux session for the currently active workspace tab.
    /// Tries recover (match by tmux window name); else uses first worktree.
    fn start_session_for_active_tab(&mut self, cx: &mut Context<Self>) {
        if let Some(tab) = self.workspace_manager.active_tab() {
            let repo_path = tab.path.clone();
            self.refresh_worktrees_for_repo(&repo_path);

            if self.try_recover_then_switch(&repo_path, cx) {
                cx.notify();
                return;
            }
            let worktrees = &self.cached_worktrees;
            if worktrees.is_empty() {
                self.active_worktree_index = None;
                self.schedule_start_main_session(&repo_path, cx);
            } else {
                let wt = &worktrees[0];
                self.active_worktree_index = Some(0);
                let wt_path = wt.path.clone();
                let branch = wt.short_branch_name().to_string();
                self.schedule_switch_to_worktree_async(&repo_path, &wt_path, &branch, 0, cx);
            }
        }
        cx.notify();
    }

    pub fn has_workspaces(&self) -> bool {
        !self.workspace_manager.is_empty()
    }

    #[allow(dead_code)]
    fn effective_backend(&self) -> String {
        crate::runtime::backends::resolve_backend(
            crate::config::Config::load().ok().as_ref(),
        )
    }

    fn resolve_terminal_dims(&self) -> (u16, u16) {
        self.preferred_terminal_dims
            .or_else(|| {
                if let Ok(dims) = self.shared_terminal_dims.lock() {
                    *dims
                } else {
                    None
                }
            })
            .or_else(|| {
                Config::load().ok().and_then(|c| match (c.last_terminal_cols, c.last_terminal_rows) {
                    (Some(cols), Some(rows)) => Some((cols, rows)),
                    _ => None,
                })
            })
            .unwrap_or((120, 36))
    }

    /// Try recover from runtime_state; worktree is derived from tmux current window (match by name).
    /// For local PTY, always returns false.
    #[allow(dead_code)]
    fn try_recover_then_switch(
        &mut self,
        workspace_path: &Path,
        cx: &mut Context<Self>,
    ) -> bool {
        let backend = self.effective_backend();
        if backend != "tmux" && backend != "tmux-cc" {
            return false;
        }
        let state = match RuntimeState::load() {
            Ok(s) => s,
            Err(_) => return false,
        };
        let workspace_path_buf = workspace_path.to_path_buf();
        let workspace = match state.find_workspace(&workspace_path_buf) {
            Some(w) => w,
            None => return false,
        };
        let worktree = match workspace.worktrees.first() {
            Some(w) => w,
            None => return false,
        };

        let (cols, rows) = self.resolve_terminal_dims();
        let runtime = match recover_runtime(
            &worktree.backend,
            worktree,
            Some(Arc::clone(&self.event_bus)),
            cols,
            rows,
        ) {
            Ok(rt) => rt,
            Err(_) => return false,
        };
        let current_window_name = match runtime.session_info() {
            Some((_, wn)) => wn,
            None => return false,
        };
        let worktree = match workspace
            .worktrees
            .iter()
            .find(|w| {
                let new_name = window_name_for_worktree(&w.path, &w.branch);
                let old_name = legacy_window_name_for_worktree(&w.branch);
                new_name == current_window_name || old_name == current_window_name
            })
        {
            Some(w) => w,
            None => workspace.worktrees.first().unwrap(),
        };

        let pane_target = runtime
            .primary_pane_id()
            .or_else(|| worktree.pane_ids.first().cloned())
            .unwrap_or_else(|| format!("local:{}", worktree.path.display()));

        let saved_split_tree = worktree
            .split_tree_json
            .as_deref()
            .and_then(|s| serde_json::from_str::<SplitNode>(s).ok());

        self.attach_runtime(
            runtime,
            pane_target,
            &worktree.path,
            &worktree.branch,
            cx,
            saved_split_tree,
        );
        true
    }

    /// Try recover for repo-only (no worktrees). For local PTY, always returns false.
    #[allow(dead_code)]
    fn try_recover_then_start(
        &mut self,
        workspace_path: &Path,
        _repo_name: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        let backend = self.effective_backend();
        if backend != "tmux" && backend != "tmux-cc" {
            return false;
        }
        let state = match RuntimeState::load() {
            Ok(s) => s,
            Err(_) => return false,
        };
        let workspace_path_buf = workspace_path.to_path_buf();
        let workspace = match state.find_workspace(&workspace_path_buf) {
            Some(w) => w,
            None => return false,
        };
        let worktree = match workspace.worktrees.first() {
            Some(w) => w,
            None => return false,
        };

        let (cols, rows) = self.resolve_terminal_dims();
        let runtime = match recover_runtime(
            &worktree.backend,
            worktree,
            Some(Arc::clone(&self.event_bus)),
            cols,
            rows,
        ) {
            Ok(rt) => rt,
            Err(_) => return false,
        };

        // Always prefer live pane IDs — saved IDs may be stale after session recreation
        let pane_target = runtime
            .primary_pane_id()
            .or_else(|| worktree.pane_ids.first().cloned())
            .unwrap_or_else(|| format!("local:{}", worktree.path.display()));

        let saved_split_tree = worktree
            .split_tree_json
            .as_deref()
            .and_then(|s| serde_json::from_str::<SplitNode>(s).ok());

        self.attach_runtime(
            runtime,
            pane_target,
            &worktree.path,
            &worktree.branch,
            cx,
            saved_split_tree,
        );
        true
    }

    /// Start/stop running animation timer based on whether any pane is Running.
    fn manage_running_animation(&mut self, cx: &mut Context<Self>) {
        let has_running = self.pane_summary_model
            .as_ref()
            .map(|m| m.read(cx).has_running())
            .unwrap_or(false);

        if has_running && self.running_animation_task.is_none() {
            let pane_summary_model = self.pane_summary_model.clone();
            self.running_animation_task = Some(cx.spawn(async move |entity, cx| {
                loop {
                    blocking::unblock(|| std::thread::sleep(std::time::Duration::from_millis(RUNNING_ANIMATION_INTERVAL_MS))).await;
                    let should_continue = entity.update(cx, |this, cx| {
                        this.running_animation_frame = this.running_animation_frame.wrapping_add(1);
                        let still_running = pane_summary_model
                            .as_ref()
                            .map(|m| m.read(cx).has_running())
                            .unwrap_or(false);
                        if still_running {
                            cx.notify();
                            true
                        } else {
                            this.running_animation_frame = 0;
                            this.running_animation_task = None;
                            cx.notify();
                            false
                        }
                    });
                    match should_continue {
                        Ok(true) => continue,
                        _ => break,
                    }
                }
            }));
        } else if !has_running && self.running_animation_task.is_some() {
            self.running_animation_task = None;
            self.running_animation_frame = 0;
        }
    }

    fn ensure_event_bus_subscription(&mut self, cx: &mut Context<Self>) {
        if self.event_bus_subscription_started { return; }
        self.event_bus_subscription_started = true;
        self.ensure_entities(cx);
        let event_bus = Arc::clone(&self.event_bus);
        let remote_rx = event_bus.subscribe();
        let config = Config::load().unwrap_or_default();
        let secrets = crate::remotes::Secrets::load().unwrap_or_default();
        spawn_remote_gateways(&config, &secrets);
        let publisher = RemoteChannelPublisher::from_config(&config, &secrets);
        if publisher.has_channels() {
            publisher.run(remote_rx);
        }
        let pane_statuses = self.pane_statuses.clone();
        let notification_manager = self.notification_manager.clone();
        let status_counts_model = self.status_counts_model.clone();
        let notification_panel_model = self.notification_panel_model.clone();
        let pane_summary_model = self.pane_summary_model.clone();
        let active_pane_shared = Arc::clone(&self.active_pane_target_shared);
        let last_input_time = Arc::clone(&self.last_input_time);
        let pending_notification_jump = Arc::clone(&self.pending_notification_jump);
        cx.spawn(async move |entity, cx| {
            let rx = std::sync::Arc::new(std::sync::Mutex::new(event_bus.subscribe()));
            let mut last_branch_check: HashMap<PathBuf, std::time::Instant> = HashMap::new();
            let branch_check_cooldown = std::time::Duration::from_secs(2);
            loop {
                let rx_clone = rx.clone();
                let ev = blocking::unblock(move || rx_clone.lock().unwrap().recv()).await;
                match ev {
                    Ok(RuntimeEvent::AgentStateChange(e)) => {
                        if let Some(ref pane_id) = e.pane_id {
                            // Update PaneSummaryModel (last_line + status_since)
                            if let Some(ref psm) = pane_summary_model {
                                let _ = cx.update_entity(psm, |m, cx| {
                                    let (changed, _) = m.update(pane_id, e.state, e.last_line.clone());
                                    if changed { cx.notify(); }
                                });
                            }
                            if let Some(ref model) = status_counts_model {
                                let _ = cx.update_entity(model, |m, cx| {
                                    m.update_pane_status(pane_id, e.state);
                                    cx.notify();
                                });
                            } else {
                                let mut updated = false;
                                if let Ok(mut statuses) = pane_statuses.lock() {
                                    let prev = statuses.get(pane_id);
                                    if prev != Some(&e.state) {
                                        statuses.insert(pane_id.clone(), e.state);
                                        updated = true;
                                    }
                                }
                                if updated {
                                    let _ = entity.update(cx, |this, _cx| {
                                        this.update_status_counts();
                                    });
                                }
                            }
                            // Manage animation timer + per-tab counts
                            let _ = entity.update(cx, |this, cx| {
                                this.manage_running_animation(cx);
                                this.update_topbar_per_tab_counts(cx);
                            });

                            // Branch refresh: when command completes, check if git branch changed
                            if matches!(e.state, AgentStatus::Idle | AgentStatus::Waiting) {
                                if let Some(wt_path) = extract_worktree_path_from_pane_id(pane_id) {
                                    let now = std::time::Instant::now();
                                    let should_check = last_branch_check
                                        .get(&wt_path)
                                        .map(|last| now.duration_since(*last) >= branch_check_cooldown)
                                        .unwrap_or(true);
                                    if should_check {
                                        last_branch_check.insert(wt_path.clone(), now);
                                        let wt_path_clone = wt_path.clone();
                                        let branch_result = blocking::unblock(move || {
                                            crate::worktree::get_current_branch(&wt_path_clone)
                                        }).await;
                                        if let Ok(new_branch) = branch_result {
                                            let _ = entity.update(cx, |this, cx| {
                                                for wt in &mut this.cached_worktrees {
                                                    if wt.path == wt_path && wt.branch != new_branch {
                                                        let short = new_branch.strip_prefix("refs/heads/").unwrap_or(&new_branch);
                                                        wt.branch = new_branch.clone();
                                                        wt.is_main = short == "main" || short == "master";
                                                        cx.notify();
                                                        break;
                                                    }
                                                }
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Ok(RuntimeEvent::Notification(n)) => {
                        let pane_id = n.pane_id.as_deref().unwrap_or(&n.agent_id);
                        // Suppress all notifications for the currently focused pane
                        let is_active_pane = active_pane_shared
                            .lock()
                            .ok()
                            .map(|g| !g.is_empty() && g.as_str() == pane_id)
                            .unwrap_or(false);
                        if !is_active_pane {
                            let notif_type = match n.notif_type {
                                crate::runtime::NotificationType::Error => NotificationType::Error,
                                crate::runtime::NotificationType::WaitingInput => NotificationType::Waiting,
                                crate::runtime::NotificationType::Info => NotificationType::Info,
                            };
                            let message = n.message.clone();
                            let source_label = if pane_id.is_empty() { None } else { Some(pane_id_to_source_label(pane_id)) };
                            // Suppress Running→Idle notification if user was recently active
                            let recent_user_input = last_input_time.lock().ok()
                                .map(|t| t.elapsed() < std::time::Duration::from_secs(2))
                                .unwrap_or(false);
                            let is_info_notification = matches!(notif_type, NotificationType::Info);
                            let suppress_system_notif = is_info_notification && recent_user_input;
                            let mut unread_after = 0usize;
                            if let Ok(mut mgr) = notification_manager.lock() {
                                if mgr.add_labeled(pane_id, notif_type, &message, source_label) {
                                    if !suppress_system_notif {
                                        system_notifier::notify("pmux", &message, notif_type);
                                        // Store pending jump target for notification click-to-focus
                                        if let Ok(mut pending) = pending_notification_jump.lock() {
                                            *pending = Some((pane_id.to_string(), std::time::Instant::now()));
                                        }
                                    }
                                }
                                unread_after = mgr.unread_count();
                            }
                            if let Some(ref np_model) = notification_panel_model {
                                let _ = cx.update_entity(np_model, |m, cx| {
                                    m.set_unread_count(unread_after);
                                    cx.notify();
                                });
                            }
                        }
                    }
                    Err(_) => break,
                    _ => {}
                }
            }
        })
        .detach();
    }

    fn save_runtime_state(&mut self, workspace_path: &Path, worktree_path: &Path, branch_name: &str) {
        let Some(rt) = &self.runtime else { return };
        let Some(_tab) = self.workspace_manager.active_tab() else { return };

        let agent_id = rt.primary_pane_id().unwrap_or_else(|| format!("local:{}", worktree_path.display()));
        let panes = rt.list_panes(&agent_id);
        let pane_ids: Vec<String> = panes.iter().cloned().collect();

        let backend = rt.backend_type();
        let (backend_session_id, backend_window_id) = rt
            .session_info()
            .unwrap_or_else(|| {
                (
                    worktree_path.to_string_lossy().to_string(),
                    branch_name.to_string(),
                )
            });

        let split_tree_json = serde_json::to_string(&self.split_tree).ok();

        let wt = WorktreeState {
            branch: branch_name.to_string(),
            path: worktree_path.to_path_buf(),
            agent_id: agent_id.clone(),
            pane_ids: pane_ids.clone(),
            backend: backend.to_string(),
            backend_session_id,
            backend_window_id,
            split_tree_json,
        };
        let mut state = RuntimeState::load().unwrap_or_default();
        state.upsert_worktree(workspace_path.to_path_buf(), wt);
        let _ = state.save();
    }

    /// Switch to a specific worktree (spawn new shell for worktree).
    /// Reuses existing -CC connection when switching within the same tmux session.
    fn switch_to_worktree(&mut self, worktree_path: &Path, branch_name: &str, cx: &mut Context<Self>) {
        let workspace_path = self
            .workspace_manager
            .active_tab()
            .map(|t| t.path.clone())
            .unwrap_or_else(|| worktree_path.to_path_buf());

        // Reuse existing runtime if same tmux session
        if self.current_runtime_matches_session(&workspace_path) {
            let runtime = self.runtime.as_ref().unwrap().clone();
            let window_name = window_name_for_worktree(worktree_path, branch_name);
            let legacy_name = legacy_window_name_for_worktree(branch_name);
            let session_name = session_name_for_workspace(&workspace_path);
            migrate_tmux_window_name(&session_name, &legacy_name, &window_name);
            self.detach_ui_from_runtime();
            if let Err(e) = runtime.switch_window(&window_name, Some(worktree_path)) {
                self.runtime = None;
                self.state.error_message = Some(format!("Window switch error: {}", e));
                return;
            }
            let pane_target = runtime
                .primary_pane_id()
                .unwrap_or_else(|| format!("local:{}", worktree_path.display()));
            self.attach_runtime(runtime, pane_target, worktree_path, branch_name, cx, None);
            return;
        }

        self.stop_current_session();

        let config = Config::load().ok();
        let (init_cols, init_rows) = self.preferred_terminal_dims.unwrap_or_else(|| {
            config.as_ref()
                .and_then(|c| match (c.last_terminal_cols, c.last_terminal_rows) {
                    (Some(cols), Some(rows)) => Some((cols, rows)),
                    _ => None,
                })
                .unwrap_or((80, 24))
        });
        let result = match create_runtime_from_env(&workspace_path, worktree_path, branch_name, init_cols, init_rows, config.as_ref()) {
            Ok(r) => r,
            Err(e) => {
                self.state.error_message = Some(format!(
                    "Runtime error for worktree {}: {}",
                    worktree_path.display(),
                    e
                ));
                return;
            }
        };
        if let Some(msg) = &result.fallback_message {
            if let Ok(mut mgr) = self.notification_manager.lock() {
                mgr.add("", crate::notification::NotificationType::Info, msg);
            }
        }
        let runtime = result.runtime;
        let pane_target = runtime
            .primary_pane_id()
            .unwrap_or_else(|| format!("local:{}", worktree_path.display()));
        self.attach_runtime(runtime, pane_target, worktree_path, branch_name, cx, None);
    }

    /// Check if the current runtime is a tmux session for the given workspace.
    fn current_runtime_matches_session(&self, workspace_path: &std::path::Path) -> bool {
        if let Some(ref rt) = self.runtime {
            if let Some((session, _)) = rt.session_info() {
                return session == session_name_for_workspace(workspace_path);
            }
        }
        false
    }

    /// Process pending worktree selection (called from render context).
    /// Reuses the existing -CC connection when switching worktrees within the same session.
    fn process_pending_worktree_selection(&mut self, cx: &mut Context<Self>) {
        let idx = match self.pending_worktree_selection.take() {
            Some(i) => i,
            None => return,
        };
        // Don't switch worktrees while diff view is open
        if self.diff_view_entity.is_some() {
            return;
        }
        let (repo_path, path, branch) = {
            let tab = match self.workspace_manager.active_tab() {
                Some(t) => t,
                None => return,
            };
            let repo_path = tab.path.clone();
            // Use cached worktrees; no sync git in click path
            let worktree = match self.cached_worktrees.get(idx) {
                Some(w) => w,
                None => return,
            };
            (
                repo_path,
                worktree.path.clone(),
                worktree.short_branch_name().to_string(),
            )
        };

        let workspace_path = self
            .workspace_manager
            .active_tab()
            .map(|t| t.path.clone())
            .unwrap_or_else(|| repo_path.clone());

        // Reuse existing runtime if switching worktrees within the same tmux session.
        // Keep current terminal visible (no loading screen); detach+attach only when switch completes.
        if self.current_runtime_matches_session(&workspace_path) {
            self.save_current_worktree_runtime_state();
            self.active_worktree_index = Some(idx);

            let runtime = self.runtime.as_ref().unwrap().clone();
            let window_name = window_name_for_worktree(&path, &branch);
            let legacy_name = legacy_window_name_for_worktree(&branch);
            let session_name = session_name_for_workspace(&workspace_path);
            migrate_tmux_window_name(&session_name, &legacy_name, &window_name);
            let path_clone = path.clone();
            let branch_clone = branch.clone();
            cx.spawn(async move |entity, cx| {
                let wn = window_name.clone();
                let pc = path_clone.clone();
                let switch_result = blocking::unblock(move || {
                    runtime.switch_window(&wn, Some(&pc))
                }).await;

                let _ = entity.update(cx, |this: &mut AppRoot, cx| {
                    match switch_result {
                        Ok(()) => {
                            let rt = this.runtime.as_ref().unwrap().clone();
                            let pane_target = rt
                                .primary_pane_id()
                                .unwrap_or_else(|| format!("local:{}", path.display()));
                            // Restore saved split tree for this worktree
                            let saved_split_tree = RuntimeState::load()
                                .ok()
                                .and_then(|state| {
                                    let ws_path = this.workspace_manager.active_tab()
                                        .map(|t| t.path.clone())?;
                                    state.workspaces.iter()
                                        .find(|ws| ws.path == ws_path)?
                                        .worktrees.iter()
                                        .find(|w| w.path == path)
                                        .and_then(|w| w.split_tree_json.as_deref()
                                            .and_then(|s| serde_json::from_str::<SplitNode>(s).ok()))
                                });
                            this.detach_ui_from_runtime();
                            this.attach_runtime(rt, pane_target, &path, &branch_clone, cx, saved_split_tree);
                            this.save_config();
                        }
                        Err(e) => {
                            this.state.error_message = Some(format!("Window switch error: {}", e));
                        }
                    }
                    cx.notify();
                });
            }).detach();
            return;
        }

        self.save_current_worktree_runtime_state();
        self.active_worktree_index = Some(idx);
        self.worktree_switch_loading = Some(idx);
        self.stop_current_session();
        cx.notify();

        let config = Config::load().ok();
        let saved_dims = self.preferred_terminal_dims.unwrap_or_else(|| {
            config.as_ref()
                .and_then(|c| match (c.last_terminal_cols, c.last_terminal_rows) {
                    (Some(cols), Some(rows)) => Some((cols, rows)),
                    _ => None,
                })
                .unwrap_or((80, 24))
        });
        cx.spawn(async move |entity, cx| {
            let path_clone = path.clone();
            let branch_clone = branch.clone();
            let (ic, ir) = saved_dims;
            let result = blocking::unblock(move || {
                create_runtime_from_env(&workspace_path, &path_clone, &branch_clone, ic, ir, config.as_ref())
            })
            .await;

            match result {
                Ok(creation) => {
                    let pane_target = creation.runtime
                        .primary_pane_id()
                        .unwrap_or_else(|| format!("local:{}", path.display()));
                    let fallback_msg = creation.fallback_message.clone();
                    let _ = entity.update(cx, |this: &mut AppRoot, cx| {
                        this.worktree_switch_loading = None;
                        if let Some(ref msg) = fallback_msg {
                            if let Ok(mut mgr) = this.notification_manager.lock() {
                                mgr.add("", crate::notification::NotificationType::Info, msg);
                            }
                        }
                        this.attach_runtime(creation.runtime, pane_target, &path, &branch, cx, None);
                        this.save_config();
                        cx.notify();
                    });
                }
                Err(e) => {
                    let _ = entity.update(cx, |this: &mut AppRoot, cx| {
                        this.worktree_switch_loading = None;
                        this.state.error_message = Some(format!("Runtime error: {}", e));
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }

    /// Schedule async switch to worktree (avoids blocking main thread on create_runtime).
    /// Reuses existing -CC connection when switching within the same tmux session.
    fn schedule_switch_to_worktree_async(
        &mut self,
        workspace_path: &Path,
        worktree_path: &Path,
        branch_name: &str,
        worktree_idx: usize,
        cx: &mut Context<Self>,
    ) {
        // Reuse existing runtime if same tmux session: no loading screen, keep current terminal until switch completes.
        if self.current_runtime_matches_session(workspace_path) {
            let runtime = self.runtime.as_ref().unwrap().clone();
            let window_name = window_name_for_worktree(worktree_path, branch_name);
            let legacy_name = legacy_window_name_for_worktree(branch_name);
            let session_name = session_name_for_workspace(workspace_path);
            migrate_tmux_window_name(&session_name, &legacy_name, &window_name);
            let worktree_path = worktree_path.to_path_buf();
            let branch_name = branch_name.to_string();
            cx.spawn(async move |entity, cx| {
                let wn = window_name.clone();
                let pc = worktree_path.clone();
                let switch_result = blocking::unblock(move || {
                    runtime.switch_window(&wn, Some(&pc))
                }).await;

                let _ = entity.update(cx, |this: &mut AppRoot, cx| {
                    match switch_result {
                        Ok(()) => {
                            let rt = this.runtime.as_ref().unwrap().clone();
                            let pane_target = rt
                                .primary_pane_id()
                                .unwrap_or_else(|| format!("local:{}", worktree_path.display()));
                            this.detach_ui_from_runtime();
                            this.attach_runtime(rt, pane_target, &worktree_path, &branch_name, cx, None);
                            this.save_config();
                        }
                        Err(e) => {
                            this.state.error_message = Some(format!("Window switch error: {}", e));
                        }
                    }
                    cx.notify();
                });
            }).detach();
            return;
        }

        self.worktree_switch_loading = Some(worktree_idx);
        cx.notify();

        let workspace_path = workspace_path.to_path_buf();
        let worktree_path = worktree_path.to_path_buf();
        let branch_name = branch_name.to_string();
        let config = Config::load().ok();
        let saved_dims = self.preferred_terminal_dims.unwrap_or_else(|| {
            config.as_ref()
                .and_then(|c| match (c.last_terminal_cols, c.last_terminal_rows) {
                    (Some(cols), Some(rows)) => Some((cols, rows)),
                    _ => None,
                })
                .unwrap_or((80, 24))
        });
        cx.spawn(async move |entity, cx| {
            let path_clone = worktree_path.clone();
            let branch_clone = branch_name.clone();
            let (ic, ir) = saved_dims;
            let result = blocking::unblock(move || {
                create_runtime_from_env(&workspace_path, &path_clone, &branch_clone, ic, ir, config.as_ref())
            })
            .await;

            match result {
                Ok(creation) => {
                    let pane_target = creation.runtime
                        .primary_pane_id()
                        .unwrap_or_else(|| format!("local:{}", worktree_path.display()));
                    let fallback_msg = creation.fallback_message.clone();
                    let _ = entity.update(cx, |this: &mut AppRoot, cx| {
                        this.worktree_switch_loading = None;
                        if let Some(ref msg) = fallback_msg {
                            if let Ok(mut mgr) = this.notification_manager.lock() {
                                mgr.add("", crate::notification::NotificationType::Info, msg);
                            }
                        }
                        this.attach_runtime(creation.runtime, pane_target, &worktree_path, &branch_name, cx, None);
                        this.save_config();
                        cx.notify();
                    });
                }
                Err(e) => {
                    let _ = entity.update(cx, |this: &mut AppRoot, cx| {
                        this.worktree_switch_loading = None;
                        this.state.error_message = Some(format!("Runtime error: {}", e));
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }

    /// Schedule async start of main session (no worktrees, start_local_session).
    fn schedule_start_main_session(&mut self, repo_path: &Path, cx: &mut Context<Self>) {
        self.worktree_switch_loading = Some(0);
        cx.notify();

        let repo_path = repo_path.to_path_buf();
        let repo_path_clone = repo_path.clone();
        let saved_dims = self.preferred_terminal_dims.unwrap_or_else(|| {
            Config::load().ok()
                .and_then(|c| match (c.last_terminal_cols, c.last_terminal_rows) {
                    (Some(cols), Some(rows)) => Some((cols, rows)),
                    _ => None,
                })
                .unwrap_or((80, 24))
        });
        cx.spawn(async move |entity, cx| {
            let (ic, ir) = saved_dims;
            let result = blocking::unblock(move || {
                let config = Config::load().ok();
                create_runtime_from_env(&repo_path, &repo_path, "main", ic, ir, config.as_ref())
            })
            .await;

            match result {
                Ok(creation) => {
                    let pane_target = creation.runtime
                        .primary_pane_id()
                        .unwrap_or_else(|| format!("local:{}", repo_path_clone.display()));
                    let fallback_msg = creation.fallback_message.clone();
                    let _ = entity.update(cx, |this: &mut AppRoot, cx| {
                        this.worktree_switch_loading = None;
                        if let Some(ref msg) = fallback_msg {
                            if let Ok(mut mgr) = this.notification_manager.lock() {
                                mgr.add("", crate::notification::NotificationType::Info, msg);
                            }
                        }
                        this.attach_runtime(creation.runtime, pane_target, &repo_path_clone, "main", cx, None);
                        cx.notify();
                    });
                }
                Err(e) => {
                    let _ = entity.update(cx, |this: &mut AppRoot, cx| {
                        this.worktree_switch_loading = None;
                        this.state.error_message = Some(format!("Runtime error: {}", e));
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }

    /// Refresh worktree cache for the given repo. Call when:
    /// - Switching workspace tab
    /// - After create_branch / delete worktree
    /// - On explicit user refresh (future)
    fn refresh_worktrees_for_repo(&mut self, repo_path: &Path) {
        match crate::worktree::discover_worktrees(repo_path) {
            Ok(wt) => {
                // Populate worktree→repo mapping for per-tab agent counts
                for w in &wt {
                    self.worktree_to_repo_map.insert(w.path.clone(), repo_path.to_path_buf());
                }
                self.cached_worktrees = wt;
                self.cached_worktrees_repo = Some(repo_path.to_path_buf());
                // One-shot: cache tmux window list for this repo to speed up worktree switch and orphan detection
                if self.effective_backend() == "tmux" || self.effective_backend() == "tmux-cc" {
                    let windows = list_tmux_windows(repo_path);
                    self.cached_tmux_windows = Some((repo_path.to_path_buf(), windows));
                } else {
                    self.cached_tmux_windows = None;
                }
            }
            Err(_) => {
                self.cached_worktrees.clear();
                self.cached_worktrees_repo = None;
                self.cached_tmux_windows = None;
            }
        }
    }

    /// Find worktree index whose tmux window name matches (for restore by window name).
    fn find_worktree_index_by_window_name(
        worktrees: &[crate::worktree::WorktreeInfo],
        window_name: &str,
    ) -> Option<usize> {
        worktrees
            .iter()
            .position(|wt| {
                let new_name = window_name_for_worktree(&wt.path, wt.short_branch_name());
                let old_name = legacy_window_name_for_worktree(wt.short_branch_name());
                new_name == window_name || old_name == window_name
            })
    }

    /// Get worktrees for current repo (from cache). Call from render.
    fn worktrees_for_render(&self, repo_path: &Path) -> &[crate::worktree::WorktreeInfo] {
        if self.cached_worktrees_repo.as_deref() == Some(repo_path) {
            &self.cached_worktrees
        } else {
            &[]
        }
    }

    /// Tmux window names that have no corresponding worktree (worktree removed externally). Empty when not tmux backend.
    /// Uses cached_tmux_windows when repo matches to avoid repeated list-windows calls.
    fn orphan_tmux_windows_for_repo(&self, repo_path: &Path) -> Vec<String> {
        let backend = self.effective_backend();
        if backend != "tmux" && backend != "tmux-cc" {
            return Vec::new();
        }
        let all: Vec<String> = if self.cached_tmux_windows.as_ref().map(|(p, _)| p.as_path()) == Some(repo_path) {
            self.cached_tmux_windows
                .as_ref()
                .map(|(_, w)| w.clone())
                .unwrap_or_default()
        } else {
            list_tmux_windows(repo_path)
        };
        if all.is_empty() {
            return Vec::new();
        }
        let valid: std::collections::HashSet<String> = if self.cached_worktrees_repo.as_deref() == Some(repo_path) {
            self.cached_worktrees
                .iter()
                .flat_map(|wt| {
                    let new_name = window_name_for_worktree(&wt.path, wt.short_branch_name());
                    let old_name = legacy_window_name_for_worktree(wt.short_branch_name());
                    vec![new_name, old_name]
                })
                .collect()
        } else {
            std::collections::HashSet::new()
        };
        all.into_iter()
            .filter(|w| !valid.contains(w))
            .collect()
    }

    /// Update status_counts from current pane_statuses
    /// Computes aggregate counts for status display
    fn update_status_counts(&mut self) {
        let mut counts = StatusCounts::new();
        if let Ok(statuses) = self.pane_statuses.lock() {
            for status in statuses.values() {
                counts.increment(status);
            }
        }
        self.status_counts = counts;
    }

    /// Compute per-tab active agent counts from pane_statuses.
    /// Groups panes by worktree, then maps to repo/tab via worktree_to_repo_map.
    /// Active = Running, Error, or Waiting.
    fn compute_per_tab_active_counts(&self) -> Vec<usize> {
        let num_tabs = self.workspace_manager.tab_count();
        let mut counts = vec![0usize; num_tabs];
        if let Ok(statuses) = self.pane_statuses.lock() {
            // Group by worktree path, keep highest-priority status per worktree
            let mut worktree_statuses: HashMap<PathBuf, AgentStatus> = HashMap::new();
            for (pane_id, status) in statuses.iter() {
                if let Some(wt_path) = extract_worktree_path_from_pane_id(pane_id) {
                    let entry = worktree_statuses.entry(wt_path).or_insert(AgentStatus::Unknown);
                    if status.priority() > entry.priority() {
                        *entry = *status;
                    }
                }
            }
            // Count active worktrees per tab
            for (wt_path, status) in &worktree_statuses {
                if matches!(status, AgentStatus::Running | AgentStatus::Error | AgentStatus::Waiting) {
                    if let Some(repo_path) = self.worktree_to_repo_map.get(wt_path) {
                        if let Some(tab_idx) = self.workspace_manager.find_workspace_index(repo_path) {
                            if tab_idx < num_tabs {
                                counts[tab_idx] += 1;
                            }
                        }
                    }
                }
            }
        }
        counts
    }

    /// Push per-tab active agent counts to TopBarEntity.
    fn update_topbar_per_tab_counts(&self, cx: &mut Context<Self>) {
        let counts = self.compute_per_tab_active_counts();
        if let Some(ref topbar) = self.topbar_entity {
            let topbar = topbar.clone();
            let _ = cx.update_entity(&topbar, |t: &mut TopBarEntity, cx| {
                t.set_per_tab_active_counts(counts);
                cx.notify();
            });
        }
    }

    /// Detach UI components from the runtime without dropping it.
    /// Used when switching worktrees within the same session — the -CC
    /// connection stays alive, only the terminal UI is torn down.
    fn detach_ui_from_runtime(&mut self) {
        self.resize_controller.reset_for_new_session();
        self.status_publisher.take();
        self.session_scanner.take(); // Stop JSONL session watcher threads
        self.terminal_area_entity.take();
        if let Ok(mut buffers) = self.terminal_buffers.lock() {
            buffers.clear();
        }

        self.status_counts = StatusCounts::new();
        if let Ok(statuses) = self.pane_statuses.lock() {
            for s in statuses.values() {
                self.status_counts.increment(s);
            }
        }

        self.active_pane_target = None;
    }

    /// Stop current session.
    /// Does NOT clear pane_statuses - preserves last known status for worktrees we're leaving
    /// (avoids flicker: main=Idle, switch to feature/test → main stays Idle, feature/test gets its status)
    fn stop_current_session(&mut self) {
        self.detach_ui_from_runtime();
        self.runtime = None;
    }

    /// Handle Cmd+V paste action (dispatched via GPUI key binding).
    /// GPUI's macOS backend intercepts Cmd+V at the Cocoa input system level,
    /// so on_key_down never fires for it when an InputHandler is active.
    /// Using a GPUI action ensures paste works regardless of focus/InputHandler state.
    fn handle_paste(&mut self, _: &TerminalPaste, _window: &mut Window, cx: &mut Context<Self>) {
        // Don't paste when a modal is open
        if self.show_settings || self.new_branch_dialog_model.as_ref().map_or(false, |e| e.read(cx).is_open) {
            return;
        }
        let Some(clipboard) = cx.read_from_clipboard() else { return };
        let text = build_paste_text_from_clipboard(&clipboard);
        if text.is_empty() { return; }

        if let (Some(runtime), Some(target)) = (&self.runtime, self.active_pane_target.as_ref()) {
            let bracketed = if let Ok(buffers) = self.terminal_buffers.lock() {
                if let Some(TerminalBuffer::Terminal { terminal, .. }) = buffers.get(target) {
                    if terminal.display_offset() > 0 {
                        terminal.scroll_to_bottom();
                    }
                    terminal.mode().contains(alacritty_terminal::term::TermMode::BRACKETED_PASTE)
                } else { false }
            } else { false };

            let mut bytes = Vec::with_capacity(text.len() + 12);
            if bracketed {
                bytes.extend_from_slice(b"\x1b[200~");
            }
            bytes.extend_from_slice(text.replace('\n', "\r").as_bytes());
            if bracketed {
                bytes.extend_from_slice(b"\x1b[201~");
            }
            let _ = runtime.send_input(target, &bytes);
        }
    }

    /// Handle Cmd+C copy action (dispatched via GPUI key binding).
    /// Copies selected text from the terminal to the system clipboard.
    /// If no text is selected, sends Ctrl+C (SIGINT) to the terminal.
    fn handle_copy(&mut self, _: &TerminalCopy, _window: &mut Window, cx: &mut Context<Self>) {
        // Don't copy when a modal is open
        if self.show_settings || self.new_branch_dialog_model.as_ref().map_or(false, |e| e.read(cx).is_open) {
            return;
        }
        if let Some(target) = self.active_pane_target.as_ref() {
            let selected_text = if let Ok(buffers) = self.terminal_buffers.lock() {
                if let Some(TerminalBuffer::Terminal { terminal, .. }) = buffers.get(target) {
                    terminal.selection_text()
                } else { None }
            } else { None };

            if let Some(text) = selected_text {
                if !text.is_empty() {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                    return;
                }
            }
            // No selection → send Ctrl+C (SIGINT) to the terminal
            if let Some(runtime) = &self.runtime {
                let _ = runtime.send_input(target, &[0x03]); // ETX = Ctrl+C
            }
        }
    }

    /// Handle keyboard events
    fn handle_key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        // Track last input time for notification suppression (方案 6b)
        if let Ok(mut t) = self.last_input_time.lock() {
            *t = std::time::Instant::now();
        }
        // Modal: when settings is open, only Escape closes it; block all other keys from reaching terminal
        if self.show_settings {
            if event.keystroke.key.as_str() == "escape" {
                self.show_settings = false;
                self.settings_draft = None;
                self.settings_secrets_draft = None;
                self.settings_configuring_channel = None;
                self.settings_editing_agent = None;
                self.settings_focused_field = None;
                cx.notify();
            }
            return;
        }
        // Modal: when new branch dialog is open, only Escape closes it; block all other keys
        if let Some(ref model_entity) = self.new_branch_dialog_model {
            if model_entity.read(cx).is_open {
                if event.keystroke.key.as_str() == "escape" {
                    self.close_new_branch_dialog(cx);
                }
                return;
            }
        }

        // Check for Alt+Cmd+arrows (pane focus switch)
        if event.keystroke.modifiers.platform && event.keystroke.modifiers.alt {
            let pane_count = self.split_tree.pane_count();
            if pane_count > 1 {
                match event.keystroke.key.as_str() {
                    "left" | "up" => {
                        self.focused_pane_index =
                            (self.focused_pane_index + pane_count - 1) % pane_count;
                        if let Some(target) = self.split_tree.focus_index_to_pane_target(self.focused_pane_index) {
                            let t = target.clone();
                            if let Some(rt) = &self.runtime {
                                let _ = rt.focus_pane(&t);
                            }
                            self.active_pane_target = Some(target);
                            if let Ok(mut guard) = self.active_pane_target_shared.lock() {
                                *guard = t;
                            }
                        }
                        cx.notify();
                        return;
                    }
                    "right" | "down" => {
                        self.focused_pane_index = (self.focused_pane_index + 1) % pane_count;
                        if let Some(target) = self.split_tree.focus_index_to_pane_target(self.focused_pane_index) {
                            let t = target.clone();
                            if let Some(rt) = &self.runtime {
                                let _ = rt.focus_pane(&t);
                            }
                            self.active_pane_target = Some(target);
                            if let Ok(mut guard) = self.active_pane_target_shared.lock() {
                                *guard = t;
                            }
                        }
                        cx.notify();
                        return;
                    }
                    _ => {}
                }
            }
        }

        // When search is active, handle search-specific keys
        if self.search_active {
            match event.keystroke.key.as_str() {
                "escape" => {
                    self.search_active = false;
                    self.search_query.clear();
                    if let Some(ref e) = self.terminal_area_entity {
                        let _ = cx.update_entity(e, |ent: &mut TerminalAreaEntity, cx| {
                            ent.set_search(None, 0);
                            cx.notify();
                        });
                    }
                    cx.notify();
                    return;
                }
                "enter" | "g" if event.keystroke.modifiers.platform => {
                    // Cmd+G or Enter: next match (need terminal to count matches)
                    if let Ok(buffers) = self.terminal_buffers.lock() {
                        if let Some(target) = self.active_pane_target.as_ref() {
                            if let Some(TerminalBuffer::Terminal { terminal, .. }) = buffers.get(target) {
                                let matches = terminal.search(&self.search_query);
                                if !matches.is_empty() {
                                    self.search_current_match =
                                        (self.search_current_match + 1) % matches.len();
                                    if let Some(ref e) = self.terminal_area_entity {
                                        let _ = cx.update_entity(e, |ent: &mut TerminalAreaEntity, cx| {
                                            ent.set_search(
                                                Some(self.search_query.clone()),
                                                self.search_current_match,
                                            );
                                            cx.notify();
                                        });
                                    }
                                }
                            }
                        }
                    }
                    cx.notify();
                    return;
                }
                "backspace" => {
                    self.search_query.pop();
                    if let Some(ref e) = self.terminal_area_entity {
                        let query = self.search_query.clone();
                        let _ = cx.update_entity(e, |ent: &mut TerminalAreaEntity, cx| {
                            ent.set_search(
                                if query.is_empty() { None } else { Some(query) },
                                self.search_current_match,
                            );
                            cx.notify();
                        });
                    }
                    cx.notify();
                    return;
                }
                _ => {
                    // Printable: append to search_query (simplified - only ascii)
                    if event.keystroke.key.len() == 1 {
                        let ch = event.keystroke.key.chars().next().unwrap();
                        if ch.is_ascii_graphic() || ch == ' ' {
                            self.search_query.push(ch);
                            if let Some(ref e) = self.terminal_area_entity {
                                let query = self.search_query.clone();
                                let _ = cx.update_entity(e, |ent: &mut TerminalAreaEntity, cx| {
                                    ent.set_search(Some(query), self.search_current_match);
                                    cx.notify();
                                });
                            }
                            cx.notify();
                            return;
                        }
                    }
                }
            }
        }

        // Check for Cmd+key shortcuts (app shortcuts)
        if event.keystroke.modifiers.platform {
            match event.keystroke.key.as_str() {
                "b" => {
                    self.sidebar_visible = !self.sidebar_visible;
                    let visible = self.sidebar_visible;
                    if let Some(ref e) = self.topbar_entity {
                        let _ = cx.update_entity(e, |t: &mut TopBarEntity, cx| {
                            t.set_sidebar_visible(visible);
                            cx.notify();
                        });
                    }
                    cx.notify();
                }
                "f" => {
                    self.search_active = true;
                    self.search_query.clear();
                    self.search_current_match = 0;
                    if let Some(ref e) = self.terminal_area_entity {
                        let _ = cx.update_entity(e, |ent: &mut TerminalAreaEntity, cx| {
                            ent.set_search(Some(String::new()), 0);
                            cx.notify();
                        });
                    }
                    cx.notify();
                    return;
                }
                "i" => {
                    if let Some(ref model) = self.notification_panel_model {
                        let _ = cx.update_entity(model, |m, cx| {
                            m.toggle_panel();
                            cx.notify();
                        });
                    }
                }
                "d" => {
                    if event.keystroke.modifiers.shift {
                        self.handle_split_pane(false, cx); // horizontal
                    } else {
                        self.handle_split_pane(true, cx); // vertical
                    }
                    return;
                }
                "r" => {
                    self.open_diff_view(cx);
                    return;
                }
                // Note: "v" (paste) is handled via GPUI action (TerminalPaste) registered
                // with cx.bind_keys(), not here. GPUI's macOS backend intercepts Cmd+V
                // at the Cocoa level before on_key_down fires when an InputHandler is active.
                "w" => {
                    if self.diff_view_entity.is_some() {
                        self.diff_view_entity = None;
                        cx.notify();
                    } else {
                        self.handle_close_pane(cx);
                    }
                }
                "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" => {
                    if let Ok(idx) = event.keystroke.key.parse::<usize>() {
                        let idx = idx - 1; // 0-based
                        if idx < self.workspace_manager.tab_count() {
                            self.handle_workspace_tab_switch(idx, cx);
                            let counts = self.compute_per_tab_active_counts();
                            if let Some(ref e) = self.topbar_entity {
                                let topbar = e.clone();
                                let wm = self.workspace_manager.clone();
                                let _ = cx.update_entity(&topbar, |t: &mut TopBarEntity, cx| {
                                    t.set_workspace_manager(wm);
                                    t.set_per_tab_active_counts(counts);
                                    cx.notify();
                                });
                            }
                        }
                    }
                }
                _ => {}
            }
            return; // Don't forward Cmd+key to tmux
        }

        // When diff view is open, handle scroll keys, block everything else
        if let Some(ref diff_entity) = self.diff_view_entity {
            let diff_entity = diff_entity.clone();
            let page_size: i32 = 20; // rows per page
            match event.keystroke.key.as_str() {
                "up" => {
                    let _ = cx.update_entity(&diff_entity, |dv, cx| dv.scroll_diff_by(-1, cx));
                }
                "down" => {
                    let _ = cx.update_entity(&diff_entity, |dv, cx| dv.scroll_diff_by(1, cx));
                }
                "pageup" => {
                    let _ = cx.update_entity(&diff_entity, |dv, cx| dv.scroll_diff_by(-page_size, cx));
                }
                "pagedown" => {
                    let _ = cx.update_entity(&diff_entity, |dv, cx| dv.scroll_diff_by(page_size, cx));
                }
                "home" => {
                    let _ = cx.update_entity(&diff_entity, |dv, cx| dv.scroll_diff_to_top(cx));
                }
                "end" => {
                    let _ = cx.update_entity(&diff_entity, |dv, cx| dv.scroll_diff_to_bottom(cx));
                }
                "j" => {
                    let _ = cx.update_entity(&diff_entity, |dv, cx| dv.scroll_diff_by(1, cx));
                }
                "k" => {
                    let _ = cx.update_entity(&diff_entity, |dv, cx| dv.scroll_diff_by(-1, cx));
                }
                _ => {}
            }
            return;
        }

        // Shift+key scroll shortcuts (no Cmd)
        if event.keystroke.modifiers.shift && !event.keystroke.modifiers.platform {
            let scroll_handled = match event.keystroke.key.as_str() {
                "pageup" | "pagedown" | "home" | "end" => {
                    if let Ok(buffers) = self.terminal_buffers.lock() {
                        if let Some(target) = self.active_pane_target.as_ref() {
                            if let Some(TerminalBuffer::Terminal { terminal, .. }) = buffers.get(target) {
                                match event.keystroke.key.as_str() {
                                    "pageup" => {
                                        let rows = terminal.size().rows;
                                        terminal.scroll_display((rows as i32).saturating_sub(2));
                                    }
                                    "pagedown" => {
                                        let rows = terminal.size().rows;
                                        terminal.scroll_display(-((rows as i32).saturating_sub(2)));
                                    }
                                    "home" => terminal.scroll_display(i32::MAX / 2),
                                    "end" => terminal.scroll_to_bottom(),
                                    _ => {}
                                }
                                true
                            } else { false }
                        } else { false }
                    } else { false }
                }
                _ => false,
            };
            if scroll_handled {
                cx.notify();
                return;
            }
        }

        // Forward all other keys to terminal via Runtime (xterm escape sequences)
        let key_name = event.keystroke.key.clone();
        let modifiers = KeyModifiers {
            platform: event.keystroke.modifiers.platform,
            shift: event.keystroke.modifiers.shift,
            alt: event.keystroke.modifiers.alt,
            ctrl: event.keystroke.modifiers.control,
        };

        match (&self.runtime, self.active_pane_target.as_ref()) {
            (Some(runtime), Some(target)) => {
                // IME: defer Enter so replace_text_in_range can send committed text first; then we send \r (or 50ms timeout sends \r)
                if (key_name == "enter" || key_name == "return" || key_name == "kp_enter")
                    && !modifiers.shift
                    && !modifiers.platform
                    && !modifiers.alt
                {
                    self.ime_pending_enter.store(true, Ordering::SeqCst);
                    let runtime = runtime.clone();
                    let target = target.clone();
                    let pending = self.ime_pending_enter.clone();
                    cx.spawn(async move |_entity, cx| {
                        cx.background_executor()
                            .timer(std::time::Duration::from_millis(50))
                            .await;
                        if pending.swap(false, Ordering::SeqCst) {
                            let _ = runtime.send_input(&target, b"\r");
                        }
                    })
                    .detach();
                    return;
                }

                let bytes_opt = if let Ok(buffers) = self.terminal_buffers.lock() {
                    if let Some(TerminalBuffer::Terminal { terminal, .. }) = buffers.get(target) {
                        crate::terminal::key_to_bytes(&event, terminal.mode())
                    } else {
                        None
                    }
                } else {
                    None
                };

                // Only fall back to xterm_escape for non-text keys.
                // Text characters (key_char present) are handled by InputHandler;
                // using xterm_escape as fallback would double-send them.
                let has_text_char = event.keystroke.key_char.as_ref().is_some_and(|c| !c.is_empty());
                let bytes_opt = if has_text_char {
                    bytes_opt
                } else {
                    bytes_opt.or_else(|| key_to_xterm_escape(&key_name, modifiers))
                };

                if let Some(bytes) = bytes_opt {
                    if let Ok(buffers) = self.terminal_buffers.lock() {
                        if let Some(TerminalBuffer::Terminal { terminal, .. }) = buffers.get(target) {
                            if terminal.display_offset() > 0 {
                                terminal.scroll_to_bottom();
                            }
                        }
                    }
                    let send_result = runtime.send_input(target, &bytes);
                    if let Err(e) = send_result {
                        eprintln!("pmux: send_input failed: {}", e);
                    }
                }
            }
            _ => {
                if !modifiers.platform {
                    eprintln!(
                        "pmux: key '{}' not forwarded (runtime={} target={})",
                        key_name,
                        self.runtime.is_some(),
                        self.active_pane_target.as_deref().unwrap_or("none")
                    );
                }
            }
        }
    }

    /// Handle split pane (⌘D vertical, ⌘⇧D horizontal)
    fn handle_split_pane(&mut self, vertical: bool, cx: &mut Context<Self>) {
        let Some(target) = self.split_tree.focus_index_to_pane_target(self.focused_pane_index) else {
            return;
        };
        let new_target = match &self.runtime {
            Some(rt) => match rt.split_pane(&target, vertical) {
                Ok(t) => t,
                Err(_) => return,
            },
            None => return,
        };
        if let Some(new_tree) = self.split_tree.split_at_focused(
            self.focused_pane_index,
            vertical,
            new_target.clone(),
        ) {
            self.split_tree = new_tree.clone();
            if let Some(ref e) = self.terminal_area_entity {
                let _ = cx.update_entity(e, |ent: &mut TerminalAreaEntity, cx| {
                    ent.set_split_tree(new_tree);
                    cx.notify();
                });
            }
            // Derive status key for new pane from status_key_base + pane index
            let pane_count = self.split_tree.pane_count();
            let new_status_key = match &self.status_key_base {
                Some(base) => if pane_count <= 1 { base.clone() } else { format!("{}:{}", base, pane_count - 1) },
                None => format!("local:{}", new_target),
            };
            if let Some(rt) = &self.runtime {
                self.setup_pane_terminal_output(rt.clone(), &new_target, &new_status_key, self.terminal_area_entity.clone(), cx);
            }
            if let Ok(mut guard) = self.pane_targets_shared.lock() {
                *guard = self.split_tree.flatten().into_iter().map(|(t, _)| t).collect();
            }
            if let Some(ref mut pub_) = self.status_publisher {
                pub_.register_pane(&new_status_key);
            }
            self.save_current_worktree_runtime_state();
            cx.notify();
        }
    }

    /// Handle close focused pane (⌘W). No-op if only one pane remains.
    fn handle_close_pane(&mut self, cx: &mut Context<Self>) {
        if self.split_tree.pane_count() <= 1 {
            return;
        }
        let Some(target) = self.split_tree.focus_index_to_pane_target(self.focused_pane_index) else {
            return;
        };
        if let Some(rt) = &self.runtime {
            let _ = rt.kill_pane(&target);
        }
        let Some(new_tree) = self.split_tree.remove_pane_at_index(self.focused_pane_index) else {
            return;
        };
        if let Ok(mut buffers) = self.terminal_buffers.lock() {
            buffers.remove(&target);
        }
        if let Ok(mut statuses) = self.pane_statuses.lock() {
            statuses.retain(|k, _| !k.contains(&target));
        }
        self.split_tree = new_tree.clone();
        if let Some(ref e) = self.terminal_area_entity {
            let _ = cx.update_entity(e, |ent: &mut TerminalAreaEntity, cx| {
                ent.set_split_tree(new_tree);
                cx.notify();
            });
        }
        let pane_count = self.split_tree.pane_count();
        if self.focused_pane_index >= pane_count {
            self.focused_pane_index = pane_count.saturating_sub(1);
        }
        self.active_pane_target = self.split_tree.focus_index_to_pane_target(self.focused_pane_index);
        if let Ok(mut guard) = self.pane_targets_shared.lock() {
            *guard = self.split_tree.flatten().into_iter().map(|(t, _)| t).collect();
        }
        if let (Some(rt), Some(ref target)) = (&self.runtime, &self.active_pane_target) {
            let _ = rt.focus_pane(target);
        }
        self.save_current_worktree_runtime_state();
        cx.notify();
    }

    /// Save runtime state for the current active worktree. No-op if no tab or worktree.
    /// Called on window close and when pane focus changes so the selected worktree restores correctly.
    pub fn save_current_worktree_runtime_state(&mut self) {
        let (workspace_path, worktree_path, branch_name) = {
            let Some(tab) = self.workspace_manager.active_tab() else { return };
            let repo_path = tab.path.clone();
            let Some(awi) = self.active_worktree_index else { return };
            self.refresh_worktrees_for_repo(&repo_path);
            let Some(wt) = self.cached_worktrees.get(awi) else { return };
            (
                repo_path,
                wt.path.clone(),
                wt.short_branch_name().to_string(),
            )
        };
        self.save_runtime_state(&workspace_path, &worktree_path, &branch_name);
    }

    /// Opens diff view for the given worktree index (or current if None)
    fn open_diff_view(&mut self, cx: &mut Context<Self>) {
        self.open_diff_view_for_worktree(self.active_worktree_index, cx);
    }

    /// Opens diff view for a specific worktree index
    fn open_diff_view_for_worktree(&mut self, worktree_idx: Option<usize>, cx: &mut Context<Self>) {
        let repo_path = self.workspace_manager.active_tab()
            .map(|t| t.path.clone())
            .unwrap_or_else(|| PathBuf::from("."));

        self.refresh_worktrees_for_repo(&repo_path);
        let idx = worktree_idx.unwrap_or(0);
        self.open_diff_view_for_worktree_with_cache(idx, cx);
    }

    /// Opens diff view using cached worktrees (no refresh). Call after cache is populated.
    fn open_diff_view_for_worktree_with_cache(&mut self, idx: usize, cx: &mut Context<Self>) {
        let worktrees = &self.cached_worktrees;
        let worktree = match worktrees.get(idx) {
            Some(w) => w,
            None => return,
        };

        let branch = worktree.short_branch_name().to_string();
        let worktree_path = worktree.path.clone();

        let app_entity = cx.entity();
        let entity = cx.new(|cx| {
            let mut overlay = DiffViewOverlay::new(worktree_path, branch);
            overlay.set_on_close(Arc::new(move |_window, cx| {
                let _ = cx.update_entity(&app_entity, |this: &mut AppRoot, cx| {
                    this.diff_view_entity = None;
                    cx.notify();
                });
            }));
            overlay.start_loading(cx);
            overlay
        });
        self.diff_view_entity = Some(entity);
        cx.notify();
    }

    /// Opens the new branch dialog
    fn open_new_branch_dialog(&mut self, cx: &mut Context<Self>) {
        self.ensure_entities(cx);
        self.modal_overlay_open.store(true, Ordering::Relaxed);
        if let Some(ref model) = self.new_branch_dialog_model {
            let _ = cx.update_entity(model, |m, cx| {
                m.open();
                cx.notify();
            });
        }
    }

    /// Closes the new branch dialog
    #[allow(dead_code)]
    fn close_new_branch_dialog(&mut self, cx: &mut Context<Self>) {
        self.modal_overlay_open.store(false, Ordering::Relaxed);
        if let Some(ref model) = self.new_branch_dialog_model {
            let _ = cx.update_entity(model, |m, cx| {
                m.close();
                cx.notify();
            });
        }
        self.terminal_needs_focus = true;
        cx.notify();
    }

    /// Creates a new branch and worktree (called from NewBranchDialogEntity's on_create).
    /// Reads branch_name from model; spawn updates model on completion.
    fn create_branch_from_model(&mut self, cx: &mut Context<Self>) {
        let (branch_name, repo_path) = {
            let model = self.new_branch_dialog_model.as_ref().and_then(|m| Some(m.read(cx).branch_name.clone()));
            let branch = model.unwrap_or_default();
            if branch.trim().is_empty() {
                return;
            }
            let repo = self.workspace_manager.active_tab()
                .map(|t| t.path.clone())
                .unwrap_or_else(|| std::path::PathBuf::from("."));
            (branch, repo)
        };

        let notification_manager = self.notification_manager.clone();
        let model = self.new_branch_dialog_model.clone();
        let app_root_entity = cx.entity();
        let repo_path_clone = repo_path.clone();
        let branch_name_clone = branch_name.clone();

        cx.spawn(async move |_entity, cx| {
            let sender = Arc::new(Mutex::new(AppNotificationSender {
                manager: notification_manager,
            }));
            let orchestrator = NewBranchOrchestrator::new(repo_path_clone.clone())
                .with_notification_sender(sender);
            let result = orchestrator.create_branch_async(&branch_name_clone).await;

            if let Some(ref m) = model {
                let _ = cx.update_entity(m, |modl: &mut NewBranchDialogModel, cx| {
                    match &result {
                        CreationResult::Success { worktree_path, branch_name: _ } => {
                            modl.complete_creating(true);
                            println!("Successfully created worktree at: {:?}", worktree_path);
                        }
                        CreationResult::ValidationFailed { error } => {
                            modl.set_error(error);
                            modl.complete_creating(false);
                        }
                        CreationResult::BranchExists { branch_name } => {
                            modl.set_error(&format!("Branch '{}' already exists", branch_name));
                            modl.complete_creating(false);
                        }
                        CreationResult::GitFailed { error } => {
                            modl.set_error(&format!("Git error: {}", error));
                            modl.complete_creating(false);
                        }
                        CreationResult::TmuxFailed { worktree_path: _, branch_name: _, error } => {
                            modl.set_error(&format!("Tmux error: {}", error));
                            modl.complete_creating(false);
                        }
                    }
                    cx.notify();
                });
            }
            if matches!(result, CreationResult::Success { .. }) {
                let _ = app_root_entity.update(cx, |this: &mut AppRoot, cx| {
                    this.close_new_branch_dialog(cx);
                    if let Some(repo_path) = this.workspace_manager.active_tab().map(|t| t.path.clone()) {
                        this.refresh_worktrees_for_repo(&repo_path);
                    }
                    this.refresh_sidebar(cx);
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// Refreshes the sidebar to show updated worktrees
    fn refresh_sidebar(&mut self, cx: &mut Context<Self>) {
        // The sidebar will refresh on next render
        cx.notify();
    }

    /// Closes the close-tab confirmation dialog
    fn close_close_tab_dialog(&mut self, cx: &mut Context<Self>) {
        self.close_tab_dialog.close();
        self.terminal_needs_focus = true;
        cx.notify();
    }

    /// Toggles the kill_tmux checkbox in the close-tab dialog
    fn toggle_close_tab_kill_tmux(&mut self, cx: &mut Context<Self>) {
        self.close_tab_dialog.toggle_kill_tmux();
        cx.notify();
    }

    /// Confirms tab close: removes tab, stops session, optionally kills tmux session
    fn confirm_close_tab(&mut self, tab_index: usize, kill_tmux: bool, cx: &mut Context<Self>) {
        let closed_path = self.workspace_manager.get_tab(tab_index).map(|t| t.path.clone());
        self.workspace_manager.close_tab(tab_index);

        if self.workspace_manager.is_empty() {
            self.stop_current_session();
        } else {
            self.stop_current_session();
            self.start_session_for_active_tab(cx);
        }

        // Kill tmux session if requested
        if kill_tmux {
            if let Some(ref path) = closed_path {
                let _ = crate::runtime::backends::kill_tmux_session(path);
            }
        }

        self.save_config();
        let counts = self.compute_per_tab_active_counts();
        if let Some(ref e) = self.topbar_entity {
            let _ = cx.update_entity(e, |t: &mut TopBarEntity, cx| {
                t.set_workspace_manager(self.workspace_manager.clone());
                t.set_per_tab_active_counts(counts);
                cx.notify();
            });
        }
        self.close_tab_dialog.close();
        cx.notify();
    }

    /// Closes the delete worktree dialog
    fn close_delete_dialog(&mut self, cx: &mut Context<Self>) {
        self.delete_worktree_dialog.close();
        self.terminal_needs_focus = true;
        cx.notify();
    }

    /// Confirms worktree deletion (tmux kill-window + git worktree remove)
    fn confirm_delete_worktree(&mut self, worktree: crate::worktree::WorktreeInfo, cx: &mut Context<Self>) {
        let repo_path = self.workspace_manager.active_tab()
            .map(|t| t.path.clone())
            .unwrap_or_else(|| PathBuf::from("."));
        let worktree_path = worktree.path.clone();
        let branch = worktree.short_branch_name().to_string();

        let win_name = window_name_for_worktree(&worktree.path, &branch);
        let legacy_name = legacy_window_name_for_worktree(&branch);
        let target = window_target(&repo_path, &win_name);
        let legacy_target = window_target(&repo_path, &legacy_name);
        if let Some(rt) = &self.runtime {
            // Kill both new and legacy window names (one may exist depending on migration state)
            if let Err(e) = rt.kill_window(&target) {
                eprintln!("tmux kill-window failed (best-effort): {}", e);
            }
            if win_name != legacy_name {
                let _ = rt.kill_window(&legacy_target);
            }
        }

        // Git worktree remove
        let mgr = crate::worktree_manager::WorktreeManager::new(repo_path);
        match mgr.remove_worktree(&worktree_path) {
            Ok(()) => {
                self.delete_worktree_dialog.close();
                self.refresh_sidebar(cx);
                let repo_path = self.workspace_manager.active_tab()
                    .map(|t| t.path.clone())
                    .unwrap_or_else(|| PathBuf::from("."));
                self.refresh_worktrees_for_repo(&repo_path);
                let worktrees = &self.cached_worktrees;
                if worktrees.is_empty() {
                    self.active_worktree_index = None;
                    self.stop_current_session();
                } else {
                    let wt = worktrees.first().unwrap();
                    let wt_path = wt.path.clone();
                    let branch = wt.short_branch_name().to_string();
                    self.active_worktree_index = Some(0);
                    self.schedule_switch_to_worktree_async(&repo_path, &wt_path, &branch, 0, cx);
                }
            }
            Err(e) => {
                self.delete_worktree_dialog.set_error(&e.to_string());
            }
        }
        cx.notify();
    }

    fn settings_channel_card_el<F>(
        name: &str,
        channel_key: &str,
        status: &str,
        enabled: bool,
        entity: Entity<Self>,
        on_toggle: F,
    ) -> impl IntoElement
    where
        F: Fn(&mut Config) + Send + 'static,
    {
        let name_owned = name.to_string();
        let status_owned = status.to_string();
        let name_ss = SharedString::from(name_owned.clone());
        let status_ss = SharedString::from(status_owned.clone());
        let entity_for_toggle = entity.clone();
        let entity_for_config = entity.clone();
        let toggle = div()
            .id(format!("settings-toggle-{}", name_owned))
            .w(px(40.))
            .h(px(22.))
            .rounded(px(11.))
            .flex()
            .items_center()
            .px(px(2.))
            .cursor_pointer()
            .bg(if enabled { rgb(0x0066cc) } else { rgb(0x4a4a4a) })
            .on_click(move |_event, _window, cx| {
                let _ = cx.update_entity(&entity_for_toggle, |this: &mut AppRoot, cx| {
                    if let Some(ref mut draft) = this.settings_draft {
                        on_toggle(draft);
                    }
                    cx.notify();
                });
            })
            .child(
                div()
                    .w(px(18.))
                    .h(px(18.))
                    .rounded(px(9.))
                    .bg(rgb(0xffffff))
                    .ml(if enabled { px(18.) } else { px(0.) })
            );
        let channel_key_owned = channel_key.to_string();
        let config_btn = div()
            .id(format!("settings-config-{}", name_owned))
            .px(px(12.))
            .py(px(6.))
            .rounded(px(4.))
            .bg(rgb(0x3d3d3d))
            .text_color(rgb(0xcccccc))
            .text_size(px(12.))
            .font_weight(FontWeight::MEDIUM)
            .cursor_pointer()
            .hover(|style: StyleRefinement| style.bg(rgb(0x4d4d4d)))
            .on_click(move |_event, _window, cx| {
                let _ = cx.update_entity(&entity_for_config, |this: &mut AppRoot, cx| {
                    this.settings_configuring_channel = Some(channel_key_owned.clone());
                    cx.notify();
                });
            })
            .child("配置");
        div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap(px(12.))
            .p(px(12.))
            .rounded(px(6.))
            .bg(rgb(0x1e1e1e))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(0xffffff))
                            .child(name_ss)
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(0x888888))
                            .child(status_ss)
                    )
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(8.))
                    .child(toggle)
                    .child(config_btn)
            )
    }

    fn render_settings_modal(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let app_root_entity = cx.entity();
        let app_root_entity_for_close = app_root_entity.clone();
        // (app_root_entity_save removed — save is now per-agent form)
        let settings_focus = self.settings_focus.get_or_insert_with(|| cx.focus_handle()).clone();
        let active_tab = self.settings_tab.clone();

        // ── Tab bar ──
        let app_root_entity_tab_channels = app_root_entity.clone();
        let app_root_entity_tab_agent = app_root_entity.clone();
        let is_channels = active_tab == "channels";
        let tab_channels = div()
            .id("settings-tab-channels")
            .px(px(16.))
            .py(px(6.))
            .rounded_t(px(6.))
            .cursor_pointer()
            .text_size(px(13.))
            .font_weight(if is_channels { FontWeight::SEMIBOLD } else { FontWeight::NORMAL })
            .text_color(if is_channels { rgb(0xffffff) } else { rgb(0x999999) })
            .bg(if is_channels { rgb(0x3d3d3d) } else { rgb(0x2d2d2d) })
            .hover(|style: StyleRefinement| style.bg(rgb(0x454545)))
            .on_click(move |_event, _window, cx| {
                let _ = cx.update_entity(&app_root_entity_tab_channels, |this: &mut AppRoot, cx| {
                    this.settings_tab = "channels".to_string();
                    cx.notify();
                });
            })
            .child("Channels");
        let tab_agent = div()
            .id("settings-tab-agent-detect")
            .px(px(16.))
            .py(px(6.))
            .rounded_t(px(6.))
            .cursor_pointer()
            .text_size(px(13.))
            .font_weight(if !is_channels { FontWeight::SEMIBOLD } else { FontWeight::NORMAL })
            .text_color(if !is_channels { rgb(0xffffff) } else { rgb(0x999999) })
            .bg(if !is_channels { rgb(0x3d3d3d) } else { rgb(0x2d2d2d) })
            .hover(|style: StyleRefinement| style.bg(rgb(0x454545)))
            .on_click(move |_event, _window, cx| {
                let _ = cx.update_entity(&app_root_entity_tab_agent, |this: &mut AppRoot, cx| {
                    this.settings_tab = "agent_detect".to_string();
                    cx.notify();
                });
            })
            .child("Agent Detect");
        let tab_bar = div()
            .flex()
            .flex_row()
            .gap(px(2.))
            .pb(px(8.))
            .mb(px(4.))
            .border_b_1()
            .border_color(rgb(0x3d3d3d))
            .child(tab_channels)
            .child(tab_agent);

        // ── Tab body ──
        let tab_body = if is_channels {
            self.render_settings_channels_tab(cx)
        } else {
            self.render_settings_agent_detect_tab(cx)
        };

        // (Save button removed — each agent form has its own Save button)

        // ── Layout ──
        // Header row: title + close (fixed)
        let header_row = div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .child(
                div()
                    .text_size(px(18.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(0xffffff))
                    .child("Settings")
            )
            .child(
                div()
                    .id("settings-close-btn")
                    .px(px(12.))
                    .py(px(6.))
                    .rounded(px(4.))
                    .bg(rgb(0x3d3d3d))
                    .text_color(rgb(0xcccccc))
                    .text_size(px(14.))
                    .font_weight(FontWeight::MEDIUM)
                    .cursor_pointer()
                    .hover(|style: StyleRefinement| style.bg(rgb(0x4d4d4d)))
                    .on_click(move |_event, _window, cx| {
                        let _ = cx.update_entity(&app_root_entity_for_close, |this: &mut AppRoot, cx| {
                            this.show_settings = false;
                            this.settings_draft = None;
                            this.settings_secrets_draft = None;
                            this.settings_configuring_channel = None;
                            this.settings_editing_agent = None;
                            cx.notify();
                        });
                    })
                    .child("×")
            );

        // Scrollable tab body
        let scrollable_body = div()
            .id("settings-content-scroll")
            .flex_grow()
            .overflow_y_scroll()
            .child(tab_body);

        let settings_content = div()
            .flex()
            .flex_col()
            .gap(px(16.))
            .max_h(px(600.))
            // Fixed header
            .child(header_row)
            // Fixed tab bar
            .child(tab_bar)
            // Scrollable body (takes remaining space)
            .child(scrollable_body);
        let settings_card = div()
            .id("settings-dialog-card")
            .max_w(px(560.))
            .w_full()
            .flex()
            .flex_col()
            .gap(px(20.))
            .px(px(24.))
            .py(px(24.))
            .rounded(px(8.))
            .bg(rgb(0x2d2d2d))
            .shadow_lg()
            .on_mouse_down(gpui::MouseButton::Left, {
                let app_root_entity_card = app_root_entity.clone();
                move |_event, _window, cx| {
                    // Clicking the card background unfocuses any text field
                    let _ = cx.update_entity(&app_root_entity_card, |this: &mut AppRoot, cx| {
                        if this.settings_focused_field.is_some() {
                            this.settings_focused_field = None;
                            cx.notify();
                        }
                    });
                    cx.stop_propagation();
                }
            })
            .child(settings_content);
        let app_root_entity_for_esc = app_root_entity.clone();
        div()
            .id("settings-modal-overlay")
            .absolute()
            .inset(px(0.))
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x00000099u32))
            .cursor_pointer()
            .focusable()
            .track_focus(&settings_focus)
            .on_key_down(move |event: &KeyDownEvent, _window, cx| {
                if event.keystroke.key.as_str() == "escape" {
                    let _ = cx.update_entity(&app_root_entity_for_esc, |this: &mut AppRoot, cx| {
                        // If a field is focused, just unfocus it; otherwise close settings
                        if this.settings_focused_field.is_some() {
                            this.settings_focused_field = None;
                        } else {
                            this.show_settings = false;
                            this.settings_draft = None;
                            this.settings_secrets_draft = None;
                            this.settings_configuring_channel = None;
                            this.settings_editing_agent = None;
                            this.settings_focused_field = None;
                        }
                        cx.notify();
                    });
                } else {
                    // Route text input to the focused settings field
                    let clipboard_text = if event.keystroke.modifiers.platform && event.keystroke.key.as_str() == "v" {
                        cx.read_from_clipboard().and_then(|c| {
                            let t = c.text().unwrap_or_default();
                            if t.is_empty() { None } else { Some(t) }
                        })
                    } else {
                        None
                    };
                    let is_select_all = event.keystroke.modifiers.platform && event.keystroke.key.as_str() == "a";
                    let _ = cx.update_entity(&app_root_entity_for_esc, |this: &mut AppRoot, cx| {
                        if let Some(ref field_id) = this.settings_focused_field.clone() {
                            // Skip modifier key combos (except paste handled above)
                            if event.keystroke.modifiers.platform && clipboard_text.is_none() && !is_select_all {
                                return;
                            }
                            let key = event.keystroke.key.as_str();
                            let draft = this.settings_draft.get_or_insert_with(|| Config::load().unwrap_or_default());
                            if field_id.starts_with("agent-name-") {
                                if let Ok(idx) = field_id.strip_prefix("agent-name-").unwrap().parse::<usize>() {
                                    if idx < draft.agent_detect.agents.len() {
                                        let name = &mut draft.agent_detect.agents[idx].name;
                                        if let Some(ref paste) = clipboard_text {
                                            let filtered: String = paste.chars().filter(|c| !c.is_control()).collect();
                                            name.push_str(&filtered);
                                        } else if is_select_all {
                                            // no-op for now (select-all not supported in custom inputs)
                                        } else {
                                            match key {
                                                "backspace" => { name.pop(); }
                                                "space" => { name.push(' '); }
                                                "tab" | "enter" => { this.settings_focused_field = None; }
                                                _ => {
                                                    // Use key_char if available, fall back to key name for single chars
                                                    let ch_text = event.keystroke.key_char.as_deref()
                                                        .or_else(|| {
                                                            let k = event.keystroke.key.as_str();
                                                            if k.chars().count() == 1 { Some(k) } else { None }
                                                        });
                                                    if let Some(ch) = ch_text {
                                                        let filtered: String = ch.chars()
                                                            .filter(|c| !c.is_control())
                                                            .collect();
                                                        name.push_str(&filtered);
                                                    }
                                                }
                                            }
                                        }
                                        cx.notify();
                                    }
                                }
                            } else if field_id.starts_with("rule-patterns-") {
                                let parts: Vec<&str> = field_id.strip_prefix("rule-patterns-").unwrap().split('-').collect();
                                if parts.len() == 2 {
                                    if let (Ok(ai), Ok(ri)) = (parts[0].parse::<usize>(), parts[1].parse::<usize>()) {
                                        if ai < draft.agent_detect.agents.len() && ri < draft.agent_detect.agents[ai].rules.len() {
                                            let patterns = &mut draft.agent_detect.agents[ai].rules[ri].patterns;
                                            // Edit patterns as a comma-separated string
                                            let mut text = patterns.join(", ");
                                            if let Some(ref paste) = clipboard_text {
                                                let filtered: String = paste.chars().filter(|c| !c.is_control()).collect();
                                                text.push_str(&filtered);
                                            } else if is_select_all {
                                                // no-op
                                            } else {
                                                match key {
                                                    "backspace" => { text.pop(); }
                                                    "space" => { text.push(' '); }
                                                    "tab" | "enter" => { this.settings_focused_field = None; }
                                                    _ => {
                                                        // Use key_char if available, fall back to key name for single chars
                                                        let ch_text = event.keystroke.key_char.as_deref()
                                                            .or_else(|| {
                                                                let k = event.keystroke.key.as_str();
                                                                if k.chars().count() == 1 { Some(k) } else { None }
                                                            });
                                                        if let Some(ch) = ch_text {
                                                            let filtered: String = ch.chars()
                                                                .filter(|c| !c.is_control())
                                                                .collect();
                                                            text.push_str(&filtered);
                                                        }
                                                    }
                                                }
                                            }
                                            // Use trim_start (not trim) so trailing spaces survive during typing;
                                            // full trim happens on save/defocus.
                                            *patterns = text.split(',').map(|s| s.trim_start().to_string()).filter(|s| !s.is_empty()).collect();
                                            cx.notify();
                                        }
                                    }
                                }
                            } else if field_id.starts_with("agent-skip-patterns-") {
                                if let Ok(idx) = field_id.strip_prefix("agent-skip-patterns-").unwrap().parse::<usize>() {
                                    if idx < draft.agent_detect.agents.len() {
                                        let patterns = &mut draft.agent_detect.agents[idx].message_skip_patterns;
                                        let mut text = patterns.join(", ");
                                        if let Some(ref paste) = clipboard_text {
                                            let filtered: String = paste.chars().filter(|c| !c.is_control()).collect();
                                            text.push_str(&filtered);
                                        } else if is_select_all {
                                            // no-op
                                        } else {
                                            match key {
                                                "backspace" => { text.pop(); }
                                                "space" => { text.push(' '); }
                                                "tab" | "enter" => { this.settings_focused_field = None; }
                                                _ => {
                                                    let ch_text = event.keystroke.key_char.as_deref()
                                                        .or_else(|| {
                                                            let k = event.keystroke.key.as_str();
                                                            if k.chars().count() == 1 { Some(k) } else { None }
                                                        });
                                                    if let Some(ch) = ch_text {
                                                        let filtered: String = ch.chars()
                                                            .filter(|c| !c.is_control())
                                                            .collect();
                                                        text.push_str(&filtered);
                                                    }
                                                }
                                            }
                                        }
                                        // Use trim_start (not trim) so trailing spaces survive during typing;
                                        // full trim happens on save/defocus.
                                        *patterns = text.split(',').map(|s| s.trim_start().to_string()).filter(|s| !s.is_empty()).collect();
                                        cx.notify();
                                    }
                                }
                            }
                        }
                    });
                }
                // Block all keys from reaching terminal
                cx.stop_propagation();
            })
            .on_scroll_wheel(|_event, _window, cx| {
                cx.stop_propagation();
            })
            .on_mouse_down(gpui::MouseButton::Left, move |_event, _window, cx| {
                let _ = cx.update_entity(&app_root_entity, |this: &mut AppRoot, cx| {
                    this.show_settings = false;
                    this.settings_draft = None;
                    this.settings_secrets_draft = None;
                    this.settings_configuring_channel = None;
                    this.settings_editing_agent = None;
                    cx.notify();
                });
            })
            .child(settings_card)
    }

    /// Render the Channels tab content in Settings.
    fn render_settings_channels_tab(&mut self, cx: &mut Context<Self>) -> Div {
        let app_root_entity = cx.entity();
        let config = self.settings_draft.clone().unwrap_or_else(|| Config::load().unwrap_or_default());
        let secrets = self.settings_secrets_draft.clone().unwrap_or_else(|| Secrets::load().unwrap_or_default());
        let discord_configured = config.remote_channels.discord.channel_id.as_ref().map_or(false, |s: &String| !s.is_empty())
            && secrets.remote_channels.discord.bot_token.as_ref().map_or(false, |s: &String| !s.is_empty());
        let kook_configured = config.remote_channels.kook.channel_id.as_ref().map_or(false, |s: &String| !s.is_empty())
            && secrets.remote_channels.kook.bot_token.as_ref().map_or(false, |s: &String| !s.is_empty());
        let feishu_configured = config.remote_channels.feishu.chat_id.as_ref().map_or(false, |s: &String| !s.is_empty())
            && secrets.remote_channels.feishu.app_id.as_ref().map_or(false, |s: &String| !s.is_empty())
            && secrets.remote_channels.feishu.app_secret.as_ref().map_or(false, |s: &String| !s.is_empty());
        let discord_enabled = config.remote_channels.discord.enabled;
        let kook_enabled = config.remote_channels.kook.enabled;
        let feishu_enabled = config.remote_channels.feishu.enabled;
        let app_root_entity_discord = app_root_entity.clone();
        let app_root_entity_kook = app_root_entity.clone();
        let app_root_entity_feishu = app_root_entity.clone();
        let discord_status = if discord_configured { "已配置" } else { "未配置" };
        let kook_status = if kook_configured { "已配置" } else { "未配置" };
        let feishu_status = if feishu_configured { "已配置" } else { "未配置" };
        let channel_cards = div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(Self::settings_channel_card_el(
                "Discord", "discord", discord_status, discord_enabled, app_root_entity_discord,
                |draft| { draft.remote_channels.discord.enabled = !draft.remote_channels.discord.enabled; },
            ))
            .child(Self::settings_channel_card_el(
                "KOOK", "kook", kook_status, kook_enabled, app_root_entity_kook,
                |draft| { draft.remote_channels.kook.enabled = !draft.remote_channels.kook.enabled; },
            ))
            .child(Self::settings_channel_card_el(
                "飞书", "feishu", feishu_status, feishu_enabled, app_root_entity_feishu,
                |draft| { draft.remote_channels.feishu.enabled = !draft.remote_channels.feishu.enabled; },
            ));
        let config_guide = self.render_settings_config_guide(&app_root_entity);
        let mut body = div().flex().flex_col().gap(px(16.)).child(channel_cards);
        if let Some(guide) = config_guide {
            body = body.child(guide);
        }
        body
    }

    /// Render the Agent Detect tab content in Settings.
    fn render_settings_agent_detect_tab(&self, cx: &mut Context<Self>) -> Div {
        self.render_agent_detect_section(cx)
    }

    /// Render the Agent Detect section in Settings.
    fn render_agent_detect_section(&self, cx: &mut Context<Self>) -> Div {
        let app_root_entity = cx.entity();
        let config = self.settings_draft.clone().unwrap_or_else(|| Config::load().unwrap_or_default());
        let agents = config.agent_detect.agents.clone();
        let editing_idx = self.settings_editing_agent;

        let app_root_entity_add = app_root_entity.clone();
        let add_button = div()
            .id("agent-detect-add-btn")
            .px(px(10.))
            .py(px(4.))
            .rounded(px(4.))
            .bg(rgb(0x3d3d3d))
            .text_color(rgb(0xcccccc))
            .text_size(px(12.))
            .cursor_pointer()
            .hover(|style: StyleRefinement| style.bg(rgb(0x4d4d4d)))
            .on_click(move |_event, _window, cx| {
                let _ = cx.update_entity(&app_root_entity_add, |this: &mut AppRoot, cx| {
                    let draft = this.settings_draft.get_or_insert_with(|| Config::load().unwrap_or_default());
                    draft.agent_detect.agents.insert(0, crate::config::AgentDef {
                        name: String::new(),
                        rules: vec![],
                        default_status: "Idle".to_string(),
                        message_skip_patterns: vec![],
                    });
                    // Shift editing index if one was open
                    if let Some(ref mut idx) = this.settings_editing_agent {
                        *idx += 1;
                    }
                    this.settings_editing_agent = Some(0);
                    cx.notify();
                });
            })
            .child("+ 添加");

        let header = div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .child(
                div()
                    .text_size(px(15.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(0xdddddd))
                    .child("Agent Detect"),
            )
            .child(add_button);

        let mut agent_cards = div().flex().flex_col().gap(px(8.));
        for (i, agent) in agents.iter().enumerate() {
            let is_editing = editing_idx == Some(i);
            if is_editing {
                agent_cards = agent_cards.child(self.render_agent_edit_card(i, agent, cx));
            } else {
                agent_cards = agent_cards.child(self.render_agent_summary_card(i, agent, cx));
            }
        }

        div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(header)
            .child(agent_cards)
    }

    /// Render a read-only summary card for an agent in Settings.
    fn render_agent_summary_card(
        &self,
        index: usize,
        agent: &crate::config::AgentDef,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let app_root_entity = cx.entity();
        let app_root_entity_del = app_root_entity.clone();
        let name = agent.name.clone();
        let default_status = agent.default_status.clone();

        // Build rules summary text
        let mut rules_els: Vec<Div> = Vec::new();
        for rule in &agent.rules {
            let patterns_str = rule.patterns.iter().map(|p| format!("\"{}\"", p)).collect::<Vec<_>>().join(", ");
            rules_els.push(
                div()
                    .text_size(px(12.))
                    .text_color(rgb(0x999999))
                    .child(format!("{}: {}", rule.status, patterns_str)),
            );
        }
        rules_els.push(
            div()
                .text_size(px(12.))
                .text_color(rgb(0x777777))
                .child(format!("默认: {}", default_status)),
        );

        let edit_btn = div()
            .id(SharedString::from(format!("agent-edit-{}", index)))
            .px(px(8.))
            .py(px(2.))
            .rounded(px(4.))
            .bg(rgb(0x3d3d3d))
            .text_color(rgb(0xcccccc))
            .text_size(px(11.))
            .cursor_pointer()
            .hover(|style: StyleRefinement| style.bg(rgb(0x4d4d4d)))
            .on_click(move |_event, _window, cx| {
                let _ = cx.update_entity(&app_root_entity, |this: &mut AppRoot, cx| {
                    this.settings_editing_agent = Some(index);
                    cx.notify();
                });
            })
            .child("编辑");

        let del_btn = div()
            .id(SharedString::from(format!("agent-del-{}", index)))
            .px(px(8.))
            .py(px(2.))
            .rounded(px(4.))
            .bg(rgb(0x3d3d3d))
            .text_color(rgb(0xcc6666))
            .text_size(px(11.))
            .cursor_pointer()
            .hover(|style: StyleRefinement| style.bg(rgb(0x4d4d4d)))
            .on_click(move |_event, _window, cx| {
                let _ = cx.update_entity(&app_root_entity_del, |this: &mut AppRoot, cx| {
                    let draft = this.settings_draft.get_or_insert_with(|| Config::load().unwrap_or_default());
                    if index < draft.agent_detect.agents.len() {
                        draft.agent_detect.agents.remove(index);
                    }
                    this.settings_editing_agent = None;
                    cx.notify();
                });
            })
            .child("删除");

        let top_row = div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .child(
                div()
                    .text_size(px(14.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(0xffffff))
                    .child(if name.is_empty() { "(unnamed)".to_string() } else { name }),
            )
            .child(
                div().flex().flex_row().gap(px(6.)).child(edit_btn).child(del_btn),
            );

        let mut card = div()
            .p(px(12.))
            .rounded(px(6.))
            .bg(rgb(0x353535))
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(top_row);
        for el in rules_els {
            card = card.child(el);
        }
        card
    }

    /// Render an editable card for an agent in Settings.
    fn render_agent_edit_card(
        &self,
        index: usize,
        agent: &crate::config::AgentDef,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let app_root_entity = cx.entity();
        let agent_name = agent.name.clone();
        let agent_default = agent.default_status.clone();

        // Name input (real text field)
        let name_field_id = format!("agent-name-{}", index);
        let name_is_focused = self.settings_focused_field.as_deref() == Some(&name_field_id);
        let app_root_entity_name = app_root_entity.clone();
        let name_field_id_for_click = name_field_id.clone();
        let settings_focus_for_name = self.settings_focus.clone();
        let name_display = if agent_name.is_empty() && !name_is_focused {
            div().text_color(rgb(0x666666)).text_size(px(13.)).child("点击输入名称")
        } else {
            let mut row = div().flex().flex_row().items_center();
            if !agent_name.is_empty() {
                row = row.child(div().text_size(px(13.)).text_color(rgb(0xeeeeee)).child(SharedString::from(agent_name.clone())));
            }
            if name_is_focused {
                row = row.child(div().w(px(1.5)).h(px(15.)).bg(rgb(0xffffff)).flex_shrink_0());
            }
            row
        };
        let name_input = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.))
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(rgb(0x999999))
                    .w(px(60.))
                    .child("名称:"),
            )
            .child(
                div()
                    .id(SharedString::from(format!("agent-name-input-{}", index)))
                    .flex_1()
                    .px(px(8.))
                    .py(px(4.))
                    .rounded(px(4.))
                    .bg(rgb(0x2a2a2a))
                    .when(name_is_focused, |el| el.border_1().border_color(rgb(0x0066cc)))
                    .cursor(gpui::CursorStyle::IBeam)
                    .on_click(move |_event, window, cx| {
                        let _ = cx.update_entity(&app_root_entity_name, |this: &mut AppRoot, cx| {
                            this.settings_focused_field = Some(name_field_id_for_click.clone());
                            cx.notify();
                        });
                        // Re-focus overlay so on_key_down continues to fire
                        if let Some(ref focus) = settings_focus_for_name {
                            window.focus(focus, cx);
                        }
                        cx.stop_propagation();
                    })
                    .child(name_display),
            );

        // Default status selector
        let app_root_entity_default = app_root_entity.clone();
        let default_selector = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.))
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(rgb(0x999999))
                    .w(px(60.))
                    .child("默认:"),
            )
            .child(
                div()
                    .id(SharedString::from(format!("agent-default-{}", index)))
                    .px(px(8.))
                    .py(px(4.))
                    .rounded(px(4.))
                    .bg(rgb(0x2a2a2a))
                    .text_size(px(13.))
                    .text_color(rgb(0xeeeeee))
                    .cursor_pointer()
                    .child(agent_default.clone())
                    .on_click(move |_event, _window, cx| {
                        let _ = cx.update_entity(&app_root_entity_default, |this: &mut AppRoot, cx| {
                            let draft = this.settings_draft.get_or_insert_with(|| Config::load().unwrap_or_default());
                            if index < draft.agent_detect.agents.len() {
                                let current = &draft.agent_detect.agents[index].default_status;
                                let options = ["Idle", "Running", "Waiting", "Error"];
                                let next = options.iter()
                                    .position(|&o| o == current.as_str())
                                    .map(|i| (i + 1) % options.len())
                                    .unwrap_or(0);
                                draft.agent_detect.agents[index].default_status = options[next].to_string();
                            }
                            cx.notify();
                        });
                    }),
            );

        // Rules list
        let mut rules_container = div().flex().flex_col().gap(px(4.));
        for (ri, rule) in agent.rules.iter().enumerate() {
            let rule_status = rule.status.clone();
            let patterns_str = rule.patterns.join(", ");
            let app_root_entity_status = app_root_entity.clone();
            let app_root_entity_del_rule = app_root_entity.clone();

            let status_btn = div()
                .id(SharedString::from(format!("rule-status-{}-{}", index, ri)))
                .px(px(6.))
                .py(px(2.))
                .rounded(px(3.))
                .bg(rgb(0x2a2a2a))
                .text_size(px(12.))
                .text_color(rgb(0xeeeeee))
                .w(px(70.))
                .cursor_pointer()
                .child(rule_status.clone())
                .on_click(move |_event, _window, cx| {
                    let _ = cx.update_entity(&app_root_entity_status, |this: &mut AppRoot, cx| {
                        let draft = this.settings_draft.get_or_insert_with(|| Config::load().unwrap_or_default());
                        if index < draft.agent_detect.agents.len() && ri < draft.agent_detect.agents[index].rules.len() {
                            let current = &draft.agent_detect.agents[index].rules[ri].status;
                            let options = ["Running", "Waiting", "Error", "Idle"];
                            let next = options.iter()
                                .position(|&o| o == current.as_str())
                                .map(|i| (i + 1) % options.len())
                                .unwrap_or(0);
                            draft.agent_detect.agents[index].rules[ri].status = options[next].to_string();
                        }
                        cx.notify();
                    });
                });

            let pat_field_id = format!("rule-patterns-{}-{}", index, ri);
            let pat_is_focused = self.settings_focused_field.as_deref() == Some(&pat_field_id);
            let app_root_entity_pat = app_root_entity.clone();
            let pat_field_id_for_click = pat_field_id.clone();
            let settings_focus_for_pat = self.settings_focus.clone();
            let pat_inner = if patterns_str.is_empty() && !pat_is_focused {
                div().text_color(rgb(0x666666)).text_size(px(12.)).child("(no patterns)")
            } else {
                let mut row = div().flex().flex_row().items_center();
                if !patterns_str.is_empty() {
                    row = row.child(div().text_size(px(12.)).text_color(rgb(0xbbbbbb)).child(SharedString::from(patterns_str)));
                }
                if pat_is_focused {
                    row = row.child(div().w(px(1.5)).h(px(13.)).bg(rgb(0xffffff)).flex_shrink_0());
                }
                row
            };
            let patterns_display = div()
                .id(SharedString::from(format!("rule-pat-input-{}-{}", index, ri)))
                .flex_1()
                .px(px(6.))
                .py(px(2.))
                .rounded(px(3.))
                .bg(rgb(0x2a2a2a))
                .when(pat_is_focused, |el| el.border_1().border_color(rgb(0x0066cc)))
                .cursor(gpui::CursorStyle::IBeam)
                .on_click(move |_event, window, cx| {
                    let _ = cx.update_entity(&app_root_entity_pat, |this: &mut AppRoot, cx| {
                        this.settings_focused_field = Some(pat_field_id_for_click.clone());
                        cx.notify();
                    });
                    // Re-focus overlay so on_key_down continues to fire
                    if let Some(ref focus) = settings_focus_for_pat {
                        window.focus(focus, cx);
                    }
                    cx.stop_propagation();
                })
                .child(pat_inner);

            let del_rule_btn = div()
                .id(SharedString::from(format!("rule-del-{}-{}", index, ri)))
                .px(px(6.))
                .py(px(2.))
                .rounded(px(3.))
                .text_size(px(12.))
                .text_color(rgb(0xcc6666))
                .cursor_pointer()
                .hover(|style: StyleRefinement| style.bg(rgb(0x4a3333)))
                .child("×")
                .on_click(move |_event, _window, cx| {
                    let _ = cx.update_entity(&app_root_entity_del_rule, |this: &mut AppRoot, cx| {
                        let draft = this.settings_draft.get_or_insert_with(|| Config::load().unwrap_or_default());
                        if index < draft.agent_detect.agents.len() && ri < draft.agent_detect.agents[index].rules.len() {
                            draft.agent_detect.agents[index].rules.remove(ri);
                        }
                        cx.notify();
                    });
                });

            let rule_row = div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(6.))
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(0x666666))
                        .w(px(16.))
                        .child(format!("{}.", ri + 1)),
                )
                .child(status_btn)
                .child(patterns_display)
                .child(del_rule_btn);

            rules_container = rules_container.child(rule_row);
        }

        // Add rule button
        let app_root_entity_add_rule = app_root_entity.clone();
        let add_rule_btn = div()
            .id(SharedString::from(format!("agent-add-rule-{}", index)))
            .px(px(8.))
            .py(px(4.))
            .rounded(px(4.))
            .bg(rgb(0x2a2a2a))
            .text_color(rgb(0xcccccc))
            .text_size(px(11.))
            .cursor_pointer()
            .hover(|style: StyleRefinement| style.bg(rgb(0x3a3a3a)))
            .on_click(move |_event, _window, cx| {
                let _ = cx.update_entity(&app_root_entity_add_rule, |this: &mut AppRoot, cx| {
                    let draft = this.settings_draft.get_or_insert_with(|| Config::load().unwrap_or_default());
                    if index < draft.agent_detect.agents.len() {
                        draft.agent_detect.agents[index].rules.push(crate::config::AgentRule {
                            status: "Running".to_string(),
                            patterns: vec!["pattern".to_string()],
                        });
                    }
                    cx.notify();
                });
            })
            .child("+ 添加规则");

        // Message skip patterns input
        let skip_patterns_str = agent.message_skip_patterns.join(", ");
        let skip_field_id = format!("agent-skip-patterns-{}", index);
        let skip_is_focused = self.settings_focused_field.as_deref() == Some(&skip_field_id);
        let app_root_entity_skip = app_root_entity.clone();
        let skip_field_id_for_click = skip_field_id.clone();
        let settings_focus_for_skip = self.settings_focus.clone();
        let skip_inner = if skip_patterns_str.is_empty() && !skip_is_focused {
            div().text_color(rgb(0x666666)).text_size(px(12.)).child("(无，逗号分隔)")
        } else {
            let mut row = div().flex().flex_row().items_center();
            if !skip_patterns_str.is_empty() {
                row = row.child(div().text_size(px(12.)).text_color(rgb(0xbbbbbb)).child(SharedString::from(skip_patterns_str)));
            }
            if skip_is_focused {
                row = row.child(div().w(px(1.5)).h(px(13.)).bg(rgb(0xffffff)).flex_shrink_0());
            }
            row
        };
        let skip_patterns_input = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.))
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(rgb(0x999999))
                    .w(px(60.))
                    .child("跳过:"),
            )
            .child(
                div()
                    .id(SharedString::from(format!("agent-skip-input-{}", index)))
                    .flex_1()
                    .px(px(8.))
                    .py(px(4.))
                    .rounded(px(4.))
                    .bg(rgb(0x2a2a2a))
                    .when(skip_is_focused, |el| el.border_1().border_color(rgb(0x0066cc)))
                    .cursor(gpui::CursorStyle::IBeam)
                    .on_click(move |_event, window, cx| {
                        let _ = cx.update_entity(&app_root_entity_skip, |this: &mut AppRoot, cx| {
                            this.settings_focused_field = Some(skip_field_id_for_click.clone());
                            cx.notify();
                        });
                        if let Some(ref focus) = settings_focus_for_skip {
                            window.focus(focus, cx);
                        }
                        cx.stop_propagation();
                    })
                    .child(skip_inner),
            );

        // Save button — saves agent config directly and closes editing form
        let app_root_entity_done = app_root_entity.clone();
        let save_btn = div()
            .id(SharedString::from(format!("agent-save-{}", index)))
            .py(px(6.))
            .rounded(px(4.))
            .bg(rgb(0x0066cc))
            .text_color(rgb(0xffffff))
            .text_size(px(13.))
            .font_weight(FontWeight::MEDIUM)
            .cursor_pointer()
            .hover(|style: StyleRefinement| style.bg(rgb(0x0077dd)))
            .flex()
            .items_center()
            .justify_center()
            .on_click(move |_event, _window, cx| {
                let _ = cx.update_entity(&app_root_entity_done, |this: &mut AppRoot, cx| {
                    // Trim trailing spaces from all patterns before saving
                    if let Some(ref mut draft) = this.settings_draft {
                        for agent in &mut draft.agent_detect.agents {
                            for rule in &mut agent.rules {
                                for p in &mut rule.patterns {
                                    *p = p.trim().to_string();
                                }
                                rule.patterns.retain(|p| !p.is_empty());
                            }
                            for p in &mut agent.message_skip_patterns {
                                *p = p.trim().to_string();
                            }
                            agent.message_skip_patterns.retain(|p| !p.is_empty());
                        }
                    }
                    // Save to config file
                    if let Some(ref draft) = this.settings_draft {
                        let mut current = Config::load().unwrap_or_default();
                        current.agent_detect = draft.agent_detect.clone();
                        current.remote_channels = draft.remote_channels.clone();
                        match current.save() {
                            Ok(()) => eprintln!("[pmux] Agent config saved ({} agents)", current.agent_detect.agents.len()),
                            Err(e) => eprintln!("[pmux] Agent config save FAILED: {}", e),
                        }
                    }
                    this.settings_editing_agent = None;
                    cx.notify();
                });
            })
            .child("Save");

        let rules_header = div()
            .text_size(px(11.))
            .text_color(rgb(0x888888))
            .child("检测规则（按顺序匹配，第一个命中的生效）：");

        let skip_header = div()
            .text_size(px(11.))
            .text_color(rgb(0x888888))
            .child("消息跳过模式（提取最后一条消息时跳过包含这些文本的行）：");

        div()
            .p(px(12.))
            .rounded(px(6.))
            .bg(rgb(0x353535))
            .border_1()
            .border_color(rgb(0x0066cc))
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(name_input)
            .child(default_selector)
            .child(rules_header)
            .child(rules_container)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(8.))
                    .child(add_rule_btn),
            )
            .child(skip_header)
            .child(skip_patterns_input)
            .child(save_btn)
    }

    fn render_settings_config_guide(&self, app_root_entity: &Entity<AppRoot>) -> Option<impl IntoElement> {
        let channel = self.settings_configuring_channel.as_ref()?.clone();
        let (title, steps, url) = match channel.as_str() {
            "discord" => (
                "Discord 配置指南",
                "1. 创建应用并添加 Bot\n2. 复制 Bot Token 到 secrets.json 的 discord.bot_token\n3. 邀请 Bot 到服务器\n4. 开启开发者模式，右键频道复制 Channel ID 到 config.json",
                "https://discord.com/developers/applications",
            ),
            "kook" => (
                "KOOK 配置指南",
                "1. 创建应用并添加机器人\n2. 复制 Token 到 secrets.json 的 kook.bot_token\n3. 邀请机器人到服务器\n4. 获取频道 ID 填入 config.json 的 kook.channel_id",
                "https://developer.kookapp.cn/",
            ),
            "feishu" => (
                "飞书配置指南",
                "1. 创建企业自建应用\n2. 记录 App ID、App Secret 填入 secrets.json\n3. 开通「获取与发送群消息」权限\n4. 将 chat_id 填入 config.json 的 feishu.chat_id",
                "https://open.feishu.cn/",
            ),
            _ => ("配置", "", ""),
        };
        let app_root_entity_config = app_root_entity.clone();
        let url_owned = url.to_string();
        let open_btn = div()
            .px(px(12.))
            .py(px(8.))
            .rounded(px(6.))
            .bg(rgb(0x0066cc))
            .text_color(rgb(0xffffff))
            .text_size(px(12.))
            .font_weight(FontWeight::MEDIUM)
            .cursor_pointer()
            .hover(|s: StyleRefinement| s.bg(rgb(0x0077dd)))
            .on_mouse_down(gpui::MouseButton::Left, move |_event, _window, _cx| {
                let _ = open::that(&url_owned);
            })
            .child("在浏览器中打开");
        let done_btn = div()
            .px(px(12.))
            .py(px(8.))
            .rounded(px(6.))
            .bg(rgb(0x3d3d3d))
            .text_color(rgb(0xcccccc))
            .text_size(px(12.))
            .font_weight(FontWeight::MEDIUM)
            .cursor_pointer()
            .hover(|s: StyleRefinement| s.bg(rgb(0x4d4d4d)))
            .on_mouse_down(gpui::MouseButton::Left, move |_event, _window, cx| {
                let _ = cx.update_entity(&app_root_entity_config, |this: &mut AppRoot, cx| {
                    this.settings_configuring_channel = None;
                    cx.notify();
                });
            })
            .child("完成");
        Some(div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .p(px(16.))
            .rounded(px(6.))
            .bg(rgb(0x1e1e1e))
            .child(
                div()
                    .text_size(px(14.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(0xffffff))
                    .child(title)
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(rgb(0xaaaaaa))
                    .whitespace_normal()
                    .child(steps)
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(8.))
                    .child(open_btn)
                    .child(done_btn)
            ))
    }

    fn render_dependency_check_page(
        &self,
        deps: &DependencyCheckResult,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let missing: Vec<String> = deps.missing.clone();

        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(24.))
            .bg(rgb(0x1e1e1e))
            .child(
                div()
                    .text_size(px(24.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(0xffffff))
                    .child("Dependency Check")
            )
            .child(
                div()
                    .text_size(px(14.))
                    .text_color(rgb(0x999999))
                    .child("pmux requires the following dependencies. Please install any missing items:")
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(12.))
                    .max_w(px(480.))
                    .children(missing.into_iter().map(|cmd| {
                        let install = deps::installation_instructions(&cmd);
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .px(px(16.))
                            .py(px(12.))
                            .rounded(px(6.))
                            .bg(rgb(0x2a2a2a))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.))
                                    .child(
                                        div()
                                            .text_color(rgb(0xff6666))
                                            .child("✗ ")
                                    )
                                    .child(
                                        div()
                                            .text_color(rgb(0xffffff))
                                            .font_weight(FontWeight::MEDIUM)
                                            .child(cmd.clone())
                                    )
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(rgb(0xaaaaaa))
                                    .font_family("ui-monospace")
                                    .child(install)
                            )
                    }))
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(rgb(0x888888))
                    .child("After installing, click the button below to recheck")
            )
            .child(
                div()
                    .id("recheck-deps-btn")
                    .px(px(24.))
                    .py(px(12.))
                    .rounded(px(6.))
                    .bg(rgb(0x0066cc))
                    .text_color(rgb(0xffffff))
                    .text_size(px(15.))
                    .font_weight(FontWeight::MEDIUM)
                    .cursor_pointer()
                    .hover(|style: StyleRefinement| style.bg(rgb(0x0077dd)))
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        let result = deps::check_dependencies_detailed();
                        if result.is_ok() {
                            this.dependency_check = None;
                        } else {
                            this.dependency_check = Some(result);
                        }
                        cx.notify();
                    }))
                    .child("Recheck")
            )
    }

    fn render_startup_page(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let has_error = self.state.error_message.is_some();
        let error_msg = self.state.error_message.clone();

        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(20.))
            .bg(rgb(0x1e1e1e))
            .child(
                div()
                    .text_size(px(28.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(0xffffff))
                    .child("Welcome to pmux")
            )
            .child(
                div()
                    .text_size(px(14.))
                    .text_color(rgb(0x999999))
                    .child("Select a Git repository to manage your AI agents")
            )
            .child(
                div()
                    .id("select-workspace-btn")
                    .px(px(24.))
                    .py(px(12.))
                    .rounded(px(6.))
                    .bg(rgb(0x0066cc))
                    .text_color(rgb(0xffffff))
                    .text_size(px(15.))
                    .font_weight(FontWeight::MEDIUM)
                    .cursor_pointer()
                    .hover(|style: StyleRefinement| style.bg(rgb(0x0077dd)))
                    .on_click(cx.listener(|this, _event, _window, cx| {
                        this.handle_add_workspace(cx);
                    }))
                    .child("Select Workspace")
            )
            .when(has_error, |el: Div| {
                if let Some(msg) = error_msg {
                    el.child(
                        div()
                            .px(px(16.))
                            .py(px(8.))
                            .rounded(px(4.))
                            .bg(rgb(0x3a1111))
                            .text_color(rgb(0xff4444))
                            .text_size(px(13.))
                            .max_w(px(400.))
                            .child(SharedString::from(msg))
                    )
                } else {
                    el
                }
            })
    }

    fn render_workspace_view(&self, cx: &mut Context<Self>, terminal_focus: &gpui::FocusHandle, cursor_blink_visible: bool) -> impl IntoElement {
        let sidebar_visible = self.sidebar_visible;
        let workspace_manager = self.workspace_manager.clone();
        let terminal_buffers = Arc::clone(&self.terminal_buffers);
        let split_tree = self.split_tree.clone();
        let focused_pane_index = self.focused_pane_index;
        let split_divider_drag = self.split_divider_drag.clone();
        let worktree_switch_loading = self.worktree_switch_loading;
        let _status_counts = self.status_counts.clone();
        let pane_statuses = self.pane_statuses.clone();
        let app_root_entity = cx.entity();

        // Get repo name and path for sidebar header
        let repo_name = self.workspace_manager.active_tab()
            .map(|t| t.name.clone())
            .unwrap_or_else(|| "workspace".to_string());
        let repo_path = self.workspace_manager.active_tab()
            .map(|t| t.path.clone())
            .unwrap_or_else(|| std::path::PathBuf::from("."));

        let notification_unread = self
            .notification_panel_model
            .as_ref()
            .map(|m| m.read(cx).unread_count)
            .unwrap_or_else(|| self.notification_manager.lock().map(|m| m.unread_count()).unwrap_or(0));
        let app_root_entity_for_toggle = app_root_entity.clone();
        let notification_panel_model_for_toggle = self.notification_panel_model.clone();
        let notification_panel_model_for_overlay = self.notification_panel_model.clone();
        let notification_panel_is_open = self.notification_panel_model.as_ref()
            .map(|m| m.read(cx).show_panel)
            .unwrap_or(false);
        let app_root_entity_for_add_ws = app_root_entity.clone();

        // Collect pane summaries for sidebar display
        let pane_summaries_data = self.pane_summary_model.as_ref()
            .map(|m| m.read(cx).summaries().clone())
            .unwrap_or_default();
        let running_frame = self.running_animation_frame;

        // Create sidebar with callbacks (cmux style: top controls in sidebar)
        let mut sidebar = Sidebar::new(&repo_name, repo_path.clone())
            .with_statuses(pane_statuses.clone())
            .with_pane_summaries(pane_summaries_data)
            .with_running_frame(running_frame)
            .with_context_menu(self.sidebar_context_menu)
            .on_toggle_sidebar(move |_window, cx| {
                let _ = cx.update_entity(&app_root_entity_for_toggle, |this: &mut AppRoot, cx| {
                    this.sidebar_visible = !this.sidebar_visible;
                    let visible = this.sidebar_visible;
                    if let Some(ref e) = this.topbar_entity {
                        let _ = cx.update_entity(e, |t: &mut TopBarEntity, cx| {
                            t.set_sidebar_visible(visible);
                            cx.notify();
                        });
                    }
                    cx.notify();
                });
            })
            .on_toggle_notifications(move |_window, cx| {
                if let Some(ref model) = notification_panel_model_for_toggle {
                    let _ = cx.update_entity(model, |m, cx| {
                        m.toggle_panel();
                        cx.notify();
                    });
                }
            })
            .on_add_workspace(move |_window, cx| {
                let _ = cx.update_entity(&app_root_entity_for_add_ws, |this: &mut AppRoot, cx| {
                    this.handle_add_workspace(cx);
                });
            })
            .with_notification_count(notification_unread);

        // Use cached worktrees (never call git in render)
        let worktrees = self.worktrees_for_render(&repo_path).to_vec();
        if !worktrees.is_empty() {
            sidebar.set_worktrees(worktrees);
            if let Some(idx) = self.active_worktree_index {
                if idx < sidebar.worktree_count() {
                    sidebar.select(idx);
                }
            } else {
                sidebar.select(0);
            }
        }
        let orphan_windows = self.orphan_tmux_windows_for_repo(&repo_path);
        sidebar.set_orphan_windows(orphan_windows);

        // Set up select callback
        let app_root_entity_for_sidebar = app_root_entity.clone();
        let terminal_focus_for_select = terminal_focus.clone();
        sidebar.on_select(move |idx: usize, window: &mut Window, cx: &mut App| {
            let _ = cx.update_entity(&app_root_entity_for_sidebar, |this: &mut AppRoot, cx| {
                this.pending_worktree_selection = Some(idx);
                this.process_pending_worktree_selection(cx);
                cx.notify();
            });
            // Clicking the sidebar may defocus the terminal. Restore focus immediately
            // so keyboard input works without waiting for the async switch to complete.
            let focus = terminal_focus_for_select.clone();
            window.on_next_frame(move |window, cx| {
                window.focus(&focus, cx);
            });
        });

        // Set up New Branch callback - opens the dialog
        let app_root_entity_for_new_branch = app_root_entity.clone();
        let dialog_focus = self.dialog_input_focus.clone();
        sidebar.on_new_branch(move |window, cx| {
            let _ = cx.update_entity(&app_root_entity_for_new_branch, |this: &mut AppRoot, cx| {
                this.open_new_branch_dialog(cx);
            });
            // Double on_next_frame so dialog DOM (and focusable input) is fully mounted before focus
            if let Some(ref focus) = dialog_focus {
                let focus = focus.clone();
                window.on_next_frame(move |window, _cx| {
                    let focus = focus.clone();
                    window.on_next_frame(move |window, cx| {
                        window.focus(&focus, cx);
                    });
                });
            }
        });

        // Set up Refresh callback - refreshes worktree list
        let app_root_entity_for_refresh = app_root_entity.clone();
        sidebar.on_refresh(move |_window, cx| {
            let _ = cx.update_entity(&app_root_entity_for_refresh, |this: &mut AppRoot, cx| {
                if let Some(repo_path) = this.workspace_manager.active_tab().map(|t| t.path.clone()) {
                    this.refresh_worktrees_for_repo(&repo_path);
                }
                cx.notify();
            });
        });

        // Set up Settings callback - opens the settings modal
        let app_root_entity_for_settings = app_root_entity.clone();
        let settings_focus_for_cb = self.settings_focus.clone().expect("settings_focus created in ensure_entities");
        sidebar.on_settings(move |window, cx| {
            let _ = cx.update_entity(&app_root_entity_for_settings, |this: &mut AppRoot, cx| {
                this.show_settings = true;
                this.settings_draft = Config::load().ok();
                this.settings_secrets_draft = Secrets::load().ok();
                cx.notify();
            });
            // Focus settings overlay on next frame (after DOM is mounted)
            let focus = settings_focus_for_cb.clone();
            window.on_next_frame(move |window, cx| {
                window.focus(&focus, cx);
            });
        });

        let app_root_entity_for_delete = app_root_entity.clone();
        let app_root_entity_for_view_diff = app_root_entity.clone();
        let app_root_entity_for_right_click = app_root_entity.clone();
        let app_root_entity_for_clear_menu = app_root_entity.clone();
        let app_root_entity_for_close_orphan = app_root_entity.clone();
        let repo_path_for_delete = repo_path.clone();
        let repo_path_for_close_orphan = repo_path.clone();
        let repo_path_for_view_diff = repo_path.clone();
        // Extra clones for the root-level context menu overlay
        let app_root_entity_for_menu_delete = app_root_entity.clone();
        let app_root_entity_for_menu_diff = app_root_entity.clone();
        let repo_path_for_menu_delete = repo_path.clone();
        let repo_path_for_menu_diff = repo_path.clone();
        sidebar.on_delete(move |idx, _window, cx| {
            let _ = cx.update_entity(&app_root_entity_for_delete, |this: &mut AppRoot, cx| {
                this.sidebar_context_menu = None;
                cx.notify();
            });
            let repo_path = repo_path_for_delete.clone();
            let entity = app_root_entity_for_delete.clone();
            cx.spawn(async move |cx| {
                let result = blocking::unblock(move || {
                    let worktrees = crate::worktree::discover_worktrees(&repo_path).ok()?;
                    let worktree = worktrees.get(idx).cloned()?;
                    let has_uncommitted = crate::worktree::has_uncommitted_changes(&worktree.path);
                    Some((worktrees, worktree, has_uncommitted, repo_path))
                }).await;
                if let Some((worktrees, worktree, has_uncommitted, repo_path)) = result {
                    let _ = cx.update_entity(&entity, |this: &mut AppRoot, cx: &mut _| {
                        this.cached_worktrees = worktrees;
                        this.cached_worktrees_repo = Some(repo_path);
                        this.delete_worktree_dialog.open(worktree, has_uncommitted);
                        cx.notify();
                    });
                }
            }).detach();
        });
        sidebar.on_close_orphan(move |window_name, _window, cx: &mut App| {
            let repo_path = repo_path_for_close_orphan.clone();
            let entity = app_root_entity_for_close_orphan.clone();
            let window_name = window_name.to_string();
            cx.spawn(async move |cx| {
                let _ = blocking::unblock(move || kill_tmux_window(&repo_path, &window_name)).await;
                let _ = cx.update_entity(&entity, |this: &mut AppRoot, cx: &mut _| {
                    this.cached_tmux_windows = None;
                    cx.notify();
                });
            }).detach();
        });
        sidebar.on_view_diff(move |idx, _window, cx| {
            let _ = cx.update_entity(&app_root_entity_for_view_diff, |this: &mut AppRoot, cx| {
                this.sidebar_context_menu = None;
                cx.notify();
            });
            let entity = app_root_entity_for_view_diff.clone();
            let repo_path = repo_path_for_view_diff.clone();
            cx.spawn(async move |cx| {
                let result = blocking::unblock(move || {
                    crate::worktree::discover_worktrees(&repo_path).ok().map(|wt| (wt, repo_path))
                }).await;
                let _ = cx.update_entity(&entity, |this: &mut AppRoot, cx: &mut _| {
                    if let Some((wt, repo_path)) = result {
                        this.cached_worktrees = wt;
                        this.cached_worktrees_repo = Some(repo_path);
                    }
                    this.open_diff_view_for_worktree_with_cache(idx, cx);
                });
            }).detach();
        });
        sidebar.on_right_click(move |idx, x, y, _window, cx| {
            let _ = cx.update_entity(&app_root_entity_for_right_click, |this: &mut AppRoot, cx| {
                this.sidebar_context_menu = Some((idx, x, y));
                cx.notify();
            });
        });

        let delete_dialog = {
            let app_root_entity_for_confirm = app_root_entity.clone();
            let app_root_entity_for_cancel = app_root_entity.clone();
            let mut dialog = DeleteWorktreeDialogUi::new()
                .on_confirm(move |wt, _window, cx| {
                    let _ = cx.update_entity(&app_root_entity_for_confirm, |this: &mut AppRoot, cx| {
                        this.confirm_delete_worktree(wt, cx);
                    });
                })
                .on_cancel(move |_window, cx| {
                    let _ = cx.update_entity(&app_root_entity_for_cancel, |this: &mut AppRoot, cx| {
                        this.close_delete_dialog(cx);
                    });
                });
            if self.delete_worktree_dialog.is_open() {
                if let Some(wt) = self.delete_worktree_dialog.worktree() {
                    dialog.open(wt.clone(), self.delete_worktree_dialog.has_uncommitted());
                }
            }
            if let Some(err) = self.delete_worktree_dialog.error_message() {
                dialog.set_error(err);
            }
            dialog
        };

        let close_tab_dialog = {
            let app_root_entity_for_confirm = app_root_entity.clone();
            let app_root_entity_for_cancel = app_root_entity.clone();
            let app_root_entity_for_toggle = app_root_entity.clone();
            let mut dialog = CloseTabDialogUi::new()
                .on_confirm(move |tab_index, kill_tmux, _window, cx| {
                    let _ = cx.update_entity(&app_root_entity_for_confirm, |this: &mut AppRoot, cx| {
                        this.confirm_close_tab(tab_index, kill_tmux, cx);
                    });
                })
                .on_cancel(move |_window, cx| {
                    let _ = cx.update_entity(&app_root_entity_for_cancel, |this: &mut AppRoot, cx| {
                        this.close_close_tab_dialog(cx);
                    });
                })
                .on_toggle_kill_tmux(move |_window, cx| {
                    let _ = cx.update_entity(&app_root_entity_for_toggle, |this: &mut AppRoot, cx| {
                        this.toggle_close_tab_kill_tmux(cx);
                    });
                });
            if self.close_tab_dialog.is_open() {
                if let (Some(idx), Some(path), Some(name)) = (
                    self.close_tab_dialog.tab_index(),
                    self.close_tab_dialog.workspace_path().cloned(),
                    self.close_tab_dialog.workspace_name().map(|s| s.to_string()),
                ) {
                    dialog.open(idx, path, name);
                    if !self.close_tab_dialog.kill_tmux() {
                        dialog.toggle_kill_tmux();
                    }
                }
            }
            dialog
        };

        let sidebar_context_menu = self.sidebar_context_menu;
        let terminal_context_menu = self.terminal_context_menu;
        let cached_worktrees = self.cached_worktrees.clone();

        // Terminal context menu clones
        let app_root_for_term_menu_overlay = app_root_entity.clone();
        let app_root_for_term_copy = app_root_entity.clone();
        let app_root_for_term_paste = app_root_entity.clone();
        let app_root_for_term_select_all = app_root_entity.clone();
        let app_root_for_term_clear = app_root_entity.clone();

        // Get selected text for Copy menu item
        let has_selection = terminal_context_menu.is_some() && {
            if let Some(ref target) = self.active_pane_target {
                if let Ok(buffers) = self.terminal_buffers.lock() {
                    if let Some(TerminalBuffer::Terminal { terminal, .. }) = buffers.get(target) {
                        terminal.selection_text().map(|t| !t.is_empty()).unwrap_or(false)
                    } else { false }
                } else { false }
            } else { false }
        };

        div()
            .id("workspace-view")
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0x21252b))
            .relative()
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_row()
                    .overflow_hidden()
                    .when(sidebar_visible, |el: Div| {
                        el.child(
                            div()
                                .w(px(self.sidebar_width as f32))
                                .flex_shrink_0()
                                .h_full()
                                .overflow_hidden()
                                .child(sidebar)
                        )
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .min_h_0()
                            .flex()
                            .flex_col()
                            .overflow_hidden()
                            .when(self.topbar_entity.is_some(), |el: Div| {
                                el.child(self.topbar_entity.as_ref().unwrap().clone())
                            })
                            .when(self.topbar_entity.is_none(), |el: Div| {
                                let app_root_entity_for_ws_select = app_root_entity.clone();
                                let app_root_entity_for_ws_close = app_root_entity.clone();
                                el.child(
                                    WorkspaceTabBar::new(workspace_manager.clone())
                                        .on_select_tab(move |idx, _window, app| {
                                            let _ = app.update_entity(&app_root_entity_for_ws_select, |this: &mut AppRoot, cx| {
                                                this.handle_workspace_tab_switch(idx, cx);
                                            });
                                        })
                                        .on_close_tab(move |idx, _window, app| {
                                            let _ = app.update_entity(&app_root_entity_for_ws_close, |this: &mut AppRoot, cx| {
                                                let closed_path = this.workspace_manager.get_tab(idx).map(|t| t.path.clone());
                                                this.workspace_manager.close_tab(idx);
                                                let _ = closed_path;
                                                if this.workspace_manager.is_empty() {
                                                    this.stop_current_session();
                                                } else {
                                                    this.stop_current_session();
                                                    this.start_session_for_active_tab(cx);
                                                }
                                                this.save_config();
                                                cx.notify();
                                            });
                                        })
                                )
                            })
                            .child({
                                let app_root_entity_for_ratio = app_root_entity.clone();
                                let app_root_entity_for_drag = app_root_entity.clone();
                                let app_root_entity_for_drag_end = app_root_entity.clone();
                                let app_root_entity_for_pane_click = app_root_entity.clone();
                                let terminal_focus_for_pane = terminal_focus.clone();
                                div()
                                    .flex_1()
                                    .min_h_0()
                                    .overflow_hidden()
                                    .cursor(gpui::CursorStyle::IBeam)
                                    .relative()
                                    .child(
                                        if worktree_switch_loading.is_some() {
                                            div()
                                                .size_full()
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .bg(rgb(0x1e1e1e))
                                                .text_color(rgb(0x888888))
                                                .text_size(px(14.))
                                                .child("Connecting to worktree...")
                                                .into_any_element()
                                        } else if let Some(ref term_entity) = self.terminal_area_entity {
                                            div().size_full().child(term_entity.clone()).into_any_element()
                                        } else {
                                            SplitPaneContainer::new(
                                                split_tree,
                                                terminal_buffers.clone(),
                                                focused_pane_index,
                                                &repo_name,
                                            )
                                            .with_cursor_blink_visible(cursor_blink_visible)
                                            .with_drag_state(split_divider_drag)
                                            .with_search(
                                                if self.search_active {
                                                    Some(self.search_query.clone())
                                                } else {
                                                    None
                                                },
                                                self.search_current_match,
                                            )
                                            .on_ratio_change(move |path, ratio, _window, cx| {
                                                let _ = cx.update_entity(&app_root_entity_for_ratio, |this: &mut AppRoot, cx| {
                                                    this.split_tree.update_ratio(&path, ratio);
                                                    cx.notify();
                                                });
                                            })
                                            .on_divider_drag_start(move |path, pos, ratio, is_vertical, _window, cx| {
                                                let _ = cx.update_entity(&app_root_entity_for_drag, |this: &mut AppRoot, cx| {
                                                    this.split_divider_drag = Some((path, pos, ratio, is_vertical));
                                                    cx.notify();
                                                });
                                            })
                                            .on_divider_drag_end(move |_window, cx| {
                                                let _ = cx.update_entity(&app_root_entity_for_drag_end, |this: &mut AppRoot, cx| {
                                                    this.split_divider_drag = None;
                                                    cx.notify();
                                                });
                                            })
                                            .on_pane_click(move |pane_idx, window, cx| {
                                                let _ = cx.update_entity(&app_root_entity_for_pane_click, |this: &mut AppRoot, cx| {
                                                    this.focused_pane_index = pane_idx;
                                                    if let Some(target) = this.split_tree.focus_index_to_pane_target(pane_idx) {
                                                        if let Some(rt) = &this.runtime {
                                                            let _ = rt.focus_pane(&target);
                                                        }
                                                        this.active_pane_target = Some(target.clone());
                                                        if let Ok(mut guard) = this.active_pane_target_shared.lock() {
                                                            *guard = target.clone();
                                                        }
                                                        this.terminal_needs_focus = false;
                                                        if let Ok(buffers) = this.terminal_buffers.lock() {
                                                            if let Some(TerminalBuffer::Terminal { focus_handle, .. }) = buffers.get(&target) {
                                                                window.focus(focus_handle, cx);
                                                            } else {
                                                                drop(buffers);
                                                                window.focus(&terminal_focus_for_pane, cx);
                                                            }
                                                        } else {
                                                            window.focus(&terminal_focus_for_pane, cx);
                                                        }
                                                    } else {
                                                        this.terminal_needs_focus = true;
                                                    }
                                                    cx.notify();
                                                });
                                            })
                                            .into_any_element()
                                        }
                                    )
                                    .when(self.search_active, |el| {
                                        el.child(
                                            div()
                                                .absolute()
                                                .top(px(2.0))
                                                .right(px(12.0))
                                                .bg(rgb(0x2e343e))
                                                .border_1()
                                                .border_color(rgb(0x5c6370))
                                                .rounded(px(4.0))
                                                .px(px(8.0))
                                                .py(px(4.0))
                                                .child(format!("🔍 {}_", self.search_query))
                                        )
                                    })
                            })
                    )
            )
            // Update banner (above status bar)
            .children(if self.update_available.is_some() && !self.update_downloading {
                let version = self.update_available.as_ref().map(|i| i.latest_version.display()).unwrap_or_default();
                Some(
                    div()
                        .id("update-banner")
                        .w_full()
                        .h(px(28.))
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_center()
                        .gap(px(12.))
                        .bg(rgb(0x1a3a2a))
                        .border_t_1()
                        .border_color(rgb(0x2d5f3f))
                        .text_size(px(12.))
                        .text_color(rgb(0x4ec9b0))
                        .child(format!("pmux {} is available", version))
                        .child(
                            div()
                                .id("update-now-btn")
                                .px(px(12.))
                                .py(px(2.))
                                .rounded(px(3.))
                                .bg(rgb(0x0e7a0d))
                                .text_color(rgb(0xffffff))
                                .text_size(px(11.))
                                .cursor_pointer()
                                .hover(|s| s.bg(rgb(0x12991e)))
                                .on_click(cx.listener(|this, _event: &ClickEvent, _window, cx| {
                                    this.trigger_update(cx);
                                }))
                                .child("Update Now"),
                        )
                        .child(
                            div()
                                .id("update-later-btn")
                                .px(px(8.))
                                .py(px(2.))
                                .cursor_pointer()
                                .text_color(rgb(0x888888))
                                .text_size(px(11.))
                                .hover(|s| s.text_color(rgb(0xcccccc)))
                                .on_click(cx.listener(|this, _event: &ClickEvent, _window, cx| {
                                    this.update_available = None;
                                    cx.notify();
                                }))
                                .child("Later"),
                        )
                        .child(
                            div()
                                .id("update-skip-btn")
                                .px(px(8.))
                                .py(px(2.))
                                .cursor_pointer()
                                .text_color(rgb(0x666666))
                                .text_size(px(11.))
                                .hover(|s| s.text_color(rgb(0xaaaaaa)))
                                .on_click(cx.listener(|this, _event: &ClickEvent, _window, cx| {
                                    this.skip_update_version();
                                    cx.notify();
                                }))
                                .child("Skip"),
                        )
                )
            } else if self.update_downloading {
                Some(
                    div()
                        .id("update-progress-banner")
                        .w_full()
                        .h(px(28.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(rgb(0x1a2a3a))
                        .border_t_1()
                        .border_color(rgb(0x2d4f6f))
                        .text_size(px(12.))
                        .text_color(rgb(0x6cb6ff))
                        .child("Downloading update...")
                )
            } else {
                None
            })
            .child({
                let repo_path = self.workspace_manager.active_tab().map(|t| t.path.clone());
                let worktree_branch = repo_path.and_then(|p| {
                    let wts = self.worktrees_for_render(&p);
                    let idx = self.active_worktree_index?;
                    wts.get(idx).map(|w| w.short_branch_name().to_string())
                });
                {
                    let status_counts = self
                        .status_counts_model
                        .as_ref()
                        .map(|m| m.read(cx).counts.clone())
                        .unwrap_or_else(|| self.status_counts.clone());
                    let backend = resolve_backend(Config::load().ok().as_ref());
                    StatusBar::from_context(
                        worktree_branch.as_deref(),
                        self.split_tree.pane_count(),
                        self.focused_pane_index,
                        &status_counts,
                        Some(backend.as_str()),
                    )
                }
            })
            .when(notification_panel_is_open, |el: Stateful<Div>| {
                let model_left = notification_panel_model_for_overlay.clone();
                let model_right = notification_panel_model_for_overlay.clone();
                let close_panel = move |cx: &mut App, model: &Option<Entity<NotificationPanelModel>>| {
                    if let Some(ref m) = model {
                        let _ = cx.update_entity(m, |model, cx| {
                            model.set_show_panel(false);
                            cx.notify();
                        });
                    }
                };
                el.child(
                    div()
                        .id("notification-panel-overlay")
                        .absolute()
                        .inset(px(0.))
                        .size_full()
                        .on_mouse_down(MouseButton::Left, move |_event, _window, cx| {
                            close_panel(cx, &model_left);
                        })
                        .on_mouse_down(MouseButton::Right, move |_event, _window, cx| {
                            close_panel(cx, &model_right);
                        })
                )
            })
            .when(self.notification_panel_entity.is_some(), |el: Stateful<Div>| {
                el.child(self.notification_panel_entity.as_ref().unwrap().clone())
            })
            // Context menu rendered above main content, below dialogs
            .when(sidebar_context_menu.is_some(), |el| {
                let app_root_entity_for_overlay = app_root_entity_for_clear_menu.clone();
                el.child(
                    div()
                        .id("context-menu-overlay")
                        .absolute()
                        .inset(px(0.))
                        .size_full()
                        .cursor_pointer()
                        .on_mouse_down(MouseButton::Left, move |_event, _window, cx| {
                            let _ = cx.update_entity(&app_root_entity_for_overlay, |this: &mut AppRoot, cx| {
                                this.sidebar_context_menu = None;
                                cx.notify();
                            });
                        })
                )
            })
            .when(sidebar_context_menu.is_some(), |el| {
                let (idx, click_x, click_y) = sidebar_context_menu.unwrap();
                let on_view_diff: Option<Arc<dyn Fn(usize, &mut Window, &mut App) + Send + Sync>> = {
                    let entity = app_root_entity_for_menu_diff.clone();
                    let repo_path = repo_path_for_menu_diff.clone();
                    Some(Arc::new(move |idx, _window, cx| {
                        let _ = cx.update_entity(&entity, |this: &mut AppRoot, cx| {
                            this.sidebar_context_menu = None;
                            cx.notify();
                        });
                        let entity2 = entity.clone();
                        let repo_path2 = repo_path.clone();
                        cx.spawn(async move |cx| {
                            let result = blocking::unblock(move || {
                                crate::worktree::discover_worktrees(&repo_path2).ok().map(|wt| (wt, repo_path2))
                            }).await;
                            let _ = cx.update_entity(&entity2, |this: &mut AppRoot, cx: &mut _| {
                                if let Some((wt, rp)) = result {
                                    this.cached_worktrees = wt;
                                    this.cached_worktrees_repo = Some(rp);
                                }
                                this.open_diff_view_for_worktree_with_cache(idx, cx);
                            });
                        }).detach();
                    }))
                };
                let on_delete: Option<Arc<dyn Fn(usize, &mut Window, &mut App) + Send + Sync>> = {
                    let entity = app_root_entity_for_menu_delete.clone();
                    let repo_path = repo_path_for_menu_delete.clone();
                    Some(Arc::new(move |idx, _window, cx| {
                        let _ = cx.update_entity(&entity, |this: &mut AppRoot, cx| {
                            this.sidebar_context_menu = None;
                            cx.notify();
                        });
                        let entity2 = entity.clone();
                        let repo_path2 = repo_path.clone();
                        cx.spawn(async move |cx| {
                            let result = blocking::unblock(move || {
                                let worktrees = crate::worktree::discover_worktrees(&repo_path2).ok()?;
                                let worktree = worktrees.get(idx).cloned()?;
                                let has_uncommitted = crate::worktree::has_uncommitted_changes(&worktree.path);
                                Some((worktrees, worktree, has_uncommitted, repo_path2))
                            }).await;
                            if let Some((worktrees, worktree, has_uncommitted, rp)) = result {
                                let _ = cx.update_entity(&entity2, |this: &mut AppRoot, cx: &mut _| {
                                    this.cached_worktrees = worktrees;
                                    this.cached_worktrees_repo = Some(rp);
                                    this.delete_worktree_dialog.open(worktree, has_uncommitted);
                                    cx.notify();
                                });
                            }
                        }).detach();
                    }))
                };
                let menu = Sidebar::render_context_menu(idx, on_view_diff, on_delete, &cached_worktrees);
                el.child(
                    div()
                        .id("root-context-menu-float")
                        .absolute()
                        .top(px(click_y))
                        .left(px(click_x))
                        .child(menu)
                )
            })
            // Terminal context menu overlay (dismiss on left-click)
            .when(terminal_context_menu.is_some(), |el| {
                el.child(
                    div()
                        .id("terminal-context-menu-overlay")
                        .absolute()
                        .inset(px(0.))
                        .size_full()
                        .cursor_pointer()
                        .on_mouse_down(MouseButton::Left, {
                            let entity = app_root_for_term_menu_overlay.clone();
                            move |_event, _window, cx| {
                                let _ = cx.update_entity(&entity, |this: &mut AppRoot, cx| {
                                    this.terminal_context_menu = None;
                                    cx.notify();
                                });
                            }
                        })
                        .on_mouse_down(MouseButton::Right, {
                            let entity = app_root_for_term_menu_overlay.clone();
                            move |_event, _window, cx| {
                                let _ = cx.update_entity(&entity, |this: &mut AppRoot, cx| {
                                    this.terminal_context_menu = None;
                                    cx.notify();
                                });
                            }
                        })
                )
            })
            // Terminal context menu float
            .when(terminal_context_menu.is_some(), |el| {
                let (click_x, click_y) = terminal_context_menu.unwrap();
                let mut menu = div()
                    .id("terminal-context-menu")
                    .min_w(px(180.))
                    .py(px(4.))
                    .rounded(px(6.))
                    .bg(rgb(0x282828))
                    .border_1().border_color(rgb(0x404040))
                    .shadow_lg()
                    .occlude()
                    .on_click(|_event, _window, cx| { cx.stop_propagation(); })
                    .flex().flex_col();

                // Copy
                {
                    let entity = app_root_for_term_copy.clone();
                    if has_selection {
                        menu = menu.child(
                            div()
                                .id("term-ctx-copy")
                                .mx(px(4.)).px(px(8.)).py(px(6.))
                                .rounded(px(4.))
                                .flex().flex_row().items_center().gap(px(8.))
                                .text_size(px(13.))
                                .text_color(rgb(0xdddddd))
                                .hover(|s: StyleRefinement| s.bg(rgb(0x3a3a3a)).text_color(rgb(0xffffff)))
                                .cursor_pointer()
                                .on_click(move |_event, _window, cx| {
                                    let _ = cx.update_entity(&entity, |this: &mut AppRoot, cx| {
                                        // Copy selected text to clipboard
                                        if let Some(ref target) = this.active_pane_target {
                                            if let Ok(buffers) = this.terminal_buffers.lock() {
                                                if let Some(TerminalBuffer::Terminal { terminal, .. }) = buffers.get(target) {
                                                    if let Some(text) = terminal.selection_text() {
                                                        if !text.is_empty() {
                                                            cx.write_to_clipboard(ClipboardItem::new_string(text));
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        this.terminal_context_menu = None;
                                        cx.notify();
                                    });
                                })
                                .child(svg().path("icons/copy.svg").size(px(15.)).flex_shrink_0().text_color(rgb(0xaaaaaa)))
                                .child(div().flex_1().child("Copy"))
                                .child(div().text_size(px(11.)).text_color(rgb(0x888888)).child("⌘C"))
                        );
                    } else {
                        menu = menu.child(
                            div()
                                .id("term-ctx-copy")
                                .mx(px(4.)).px(px(8.)).py(px(6.))
                                .rounded(px(4.))
                                .flex().flex_row().items_center().gap(px(8.))
                                .text_size(px(13.))
                                .text_color(rgb(0x666666))
                                .child(svg().path("icons/copy.svg").size(px(15.)).flex_shrink_0().text_color(rgb(0x555555)))
                                .child(div().flex_1().child("Copy"))
                                .child(div().text_size(px(11.)).text_color(rgb(0x555555)).child("⌘C"))
                        );
                    }
                }

                // Paste
                {
                    let entity = app_root_for_term_paste.clone();
                    menu = menu.child(
                        div()
                            .id("term-ctx-paste")
                            .mx(px(4.)).px(px(8.)).py(px(6.))
                            .rounded(px(4.))
                            .flex().flex_row().items_center().gap(px(8.))
                            .text_size(px(13.))
                            .text_color(rgb(0xdddddd))
                            .hover(|s: StyleRefinement| s.bg(rgb(0x3a3a3a)).text_color(rgb(0xffffff)))
                            .cursor_pointer()
                            .on_click(move |_event, _window, cx| {
                                let _ = cx.update_entity(&entity, |this: &mut AppRoot, cx| {
                                    // Paste clipboard content to terminal (text, image, or file paths)
                                    if let Some(clipboard) = cx.read_from_clipboard() {
                                        let text = build_paste_text_from_clipboard(&clipboard);
                                        if !text.is_empty() {
                                            if let (Some(runtime), Some(target)) = (&this.runtime, this.active_pane_target.as_ref()) {
                                                let bracketed = if let Ok(buffers) = this.terminal_buffers.lock() {
                                                    if let Some(TerminalBuffer::Terminal { terminal, .. }) = buffers.get(target) {
                                                        if terminal.display_offset() > 0 {
                                                            terminal.scroll_to_bottom();
                                                        }
                                                        terminal.mode().contains(alacritty_terminal::term::TermMode::BRACKETED_PASTE)
                                                    } else { false }
                                                } else { false };
                                                let mut bytes = Vec::with_capacity(text.len() + 12);
                                                if bracketed {
                                                    bytes.extend_from_slice(b"\x1b[200~");
                                                }
                                                bytes.extend_from_slice(text.replace('\n', "\r").as_bytes());
                                                if bracketed {
                                                    bytes.extend_from_slice(b"\x1b[201~");
                                                }
                                                let _ = runtime.send_input(target, &bytes);
                                            }
                                        }
                                    }
                                    this.terminal_context_menu = None;
                                    cx.notify();
                                });
                            })
                            .child(svg().path("icons/paste.svg").size(px(15.)).flex_shrink_0().text_color(rgb(0xaaaaaa)))
                            .child(div().flex_1().child("Paste"))
                            .child(div().text_size(px(11.)).text_color(rgb(0x888888)).child("⌘V"))
                    );
                }

                // Separator
                menu = menu.child(
                    div().mx(px(4.)).my(px(2.)).h(px(1.)).bg(rgb(0x3a3a3a))
                );

                // Select All
                {
                    let entity = app_root_for_term_select_all.clone();
                    menu = menu.child(
                        div()
                            .id("term-ctx-select-all")
                            .mx(px(4.)).px(px(8.)).py(px(6.))
                            .rounded(px(4.))
                            .flex().flex_row().items_center().gap(px(8.))
                            .text_size(px(13.))
                            .text_color(rgb(0xdddddd))
                            .hover(|s: StyleRefinement| s.bg(rgb(0x3a3a3a)).text_color(rgb(0xffffff)))
                            .cursor_pointer()
                            .on_click(move |_event, _window, cx| {
                                let _ = cx.update_entity(&entity, |this: &mut AppRoot, cx| {
                                    // Select all terminal content
                                    if let Some(ref target) = this.active_pane_target {
                                        if let Ok(buffers) = this.terminal_buffers.lock() {
                                            if let Some(TerminalBuffer::Terminal { terminal, .. }) = buffers.get(target) {
                                                terminal.select_all();
                                            }
                                        }
                                    }
                                    this.terminal_context_menu = None;
                                    cx.notify();
                                });
                            })
                            .child(svg().path("icons/select-all.svg").size(px(15.)).flex_shrink_0().text_color(rgb(0xaaaaaa)))
                            .child(div().flex_1().child("Select All"))
                            .child(div().text_size(px(11.)).text_color(rgb(0x888888)).child("⌘A"))
                    );
                }

                // Clear
                {
                    let entity = app_root_for_term_clear.clone();
                    menu = menu.child(
                        div()
                            .id("term-ctx-clear")
                            .mx(px(4.)).px(px(8.)).py(px(6.))
                            .rounded(px(4.))
                            .flex().flex_row().items_center().gap(px(8.))
                            .text_size(px(13.))
                            .text_color(rgb(0xdddddd))
                            .hover(|s: StyleRefinement| s.bg(rgb(0x3a3a3a)).text_color(rgb(0xffffff)))
                            .cursor_pointer()
                            .on_click(move |_event, _window, cx| {
                                let _ = cx.update_entity(&entity, |this: &mut AppRoot, cx| {
                                    // Send Ctrl+L to clear terminal
                                    if let (Some(runtime), Some(target)) = (&this.runtime, this.active_pane_target.as_ref()) {
                                        let _ = runtime.send_input(target, b"\x0c");
                                    }
                                    this.terminal_context_menu = None;
                                    cx.notify();
                                });
                            })
                            .child(svg().path("icons/clear.svg").size(px(15.)).flex_shrink_0().text_color(rgb(0xaaaaaa)))
                            .child(div().flex_1().child("Clear"))
                            .child(div().text_size(px(11.)).text_color(rgb(0x888888)).child("⌘K"))
                    );
                }

                el.child(
                    div()
                        .id("terminal-context-menu-float")
                        .absolute()
                        .top(px(click_y))
                        .left(px(click_x))
                        .child(menu)
                )
            })
            // Dialogs rendered last so they appear on top (absolute overlay)
            .child(delete_dialog)
            .child(close_tab_dialog)
            .when(self.new_branch_dialog_entity.is_some(), |el: Stateful<Div>| {
                el.child(self.new_branch_dialog_entity.as_ref().unwrap().clone())
            })
            .when(self.diff_view_entity.is_some(), |el: Stateful<Div>| {
                el.child(self.diff_view_entity.as_ref().unwrap().clone())
            })
    }
}

impl Render for AppRoot {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Track window focus state
        let is_focused_now = window.is_window_active();
        self.window_focused_shared.store(is_focused_now, std::sync::atomic::Ordering::Relaxed);

        // Notification click-to-focus: when window transitions unfocused → focused,
        // check for a pending notification jump target and auto-focus that pane.
        if is_focused_now && !self.was_window_focused {
            // Extract pending target before mutably borrowing self
            let jump_target = self.pending_notification_jump.lock().ok().and_then(|mut pending| {
                if let Some((ref pane_id, ref ts)) = *pending {
                    if ts.elapsed() < std::time::Duration::from_secs(30) {
                        let target = pane_id.clone();
                        *pending = None;
                        return Some(target);
                    }
                    *pending = None;
                }
                None
            });
            if let Some(target_pane) = jump_target {
                // Try jump within current split tree
                if let Some(idx) = self.split_tree.flatten().into_iter().position(|(t, _)| t == target_pane) {
                    self.focused_pane_index = idx;
                    self.active_pane_target = Some(target_pane.clone());
                    if let Ok(mut guard) = self.active_pane_target_shared.lock() {
                        *guard = target_pane.clone();
                    }
                    if let Some(ref rt) = self.runtime {
                        let _ = rt.focus_pane(&target_pane);
                    }
                    self.terminal_needs_focus = true;
                } else {
                    // Pane not in current split tree — try switching to its worktree
                    if let Some(wt_path) = extract_worktree_path_from_pane_id(&target_pane) {
                        if let Some(wt_idx) = self.cached_worktrees.iter().position(|wt| wt.path == wt_path) {
                            let branch = self.cached_worktrees[wt_idx].short_branch_name().to_string();
                            self.active_worktree_index = Some(wt_idx);
                            if let Some(tab) = self.workspace_manager.active_tab() {
                                let repo_path = tab.path.clone();
                                self.schedule_switch_to_worktree_async(&repo_path, &wt_path, &branch, wt_idx, cx);
                            }
                        }
                    }
                }
            }
        }
        self.was_window_focused = is_focused_now;

        // Open Settings when requested from menu (main.rs)
        if OPEN_SETTINGS_REQUESTED.swap(false, Ordering::SeqCst) {
            self.show_settings = true;
            self.settings_draft = Config::load().ok();
            self.settings_secrets_draft = Secrets::load().ok();
            // Focus settings overlay on next frame (after DOM is mounted)
            let focus = self.settings_focus.get_or_insert_with(|| cx.focus_handle()).clone();
            window.on_next_frame(move |window, cx| {
                window.focus(&focus, cx);
            });
        }

        // Terminal resize is driven entirely by TerminalElement's with_resize_callback,
        // which uses actual font-measured cell dimensions. Do NOT call runtime.resize()
        // from window bounds with hardcoded char sizes — that causes dimension mismatch
        // (e.g. programs draw 120-col lines but only 114 fit, causing line wrapping).

        // Cursor: Zed style - always visible, no blink
        let terminal_focus = self.terminal_focus.get_or_insert_with(|| cx.focus_handle()).clone();

        // Auto-focus terminal when workspace loads so keyboard input works without clicking.
        // Use double on_next_frame so terminal DOM is fully mounted after worktree switch.
        // Skip when settings modal is open to avoid stealing focus back from settings.
        if self.has_workspaces() && self.terminal_needs_focus && !self.show_settings {
            self.terminal_needs_focus = false;
            let target = self.active_pane_target.clone();
            let buffers = self.terminal_buffers.clone();
            let terminal_focus_for_frame = terminal_focus.clone();
            window.on_next_frame(move |window, _cx| {
                let target = target.clone();
                let buffers = buffers.clone();
                let terminal_focus_for_inner = terminal_focus_for_frame.clone();
                window.on_next_frame(move |window, cx| {
                    let buf = target.as_ref().and_then(|t| {
                        buffers.lock().ok().and_then(|g| g.get(t).cloned())
                    });
                    if let Some(TerminalBuffer::Terminal { focus_handle, .. }) = buf {
                        window.focus(&focus_handle, cx);
                        return;
                    }
                    window.focus(&terminal_focus_for_inner, cx);
                });
            });
        }

        let cursor_blink_visible = true; // Zed: cursor always visible, no blink
        let new_branch_dialog_open = self
            .new_branch_dialog_model
            .as_ref()
            .map_or(false, |e| e.read(cx).is_open);
        // Pre-build settings modal outside the main div chain to reduce type nesting depth
        // (deeply nested .when()/.child() chains cause proc-macro stack overflow in gpui_macros)
        let settings_open = self.show_settings;
        // Sync modal_overlay_open with any modal (settings or new branch dialog) so terminal
        // input callbacks are suppressed while a modal is visible.
        let any_modal_open = settings_open || new_branch_dialog_open;
        self.modal_overlay_open.store(any_modal_open, Ordering::Relaxed);
        let settings_modal_el = settings_open.then(|| self.render_settings_modal(cx));

        div()
            .id("app-root")
            .relative()
            .size_full()
            .bg(rgb(0x21252b))
            .text_color(rgb(0xabb2bf))
            .font_family(".SystemUIFont")
            .focusable()
            .track_focus(&terminal_focus)
            .when(!new_branch_dialog_open && !settings_open, |el| {
                el.on_action(cx.listener(Self::handle_paste))
                    .on_action(cx.listener(Self::handle_copy))
                    .on_key_down(cx.listener(|this, event, window, cx| {
                        this.handle_key_down(event, window, cx);
                    }))
            })
            .child(
                if let Some(ref deps) = self.dependency_check {
                    self.render_dependency_check_page(deps, cx).into_any_element()
                } else if self.has_workspaces() {
                    self.render_workspace_view(cx, &terminal_focus, cursor_blink_visible).into_any_element()
                } else {
                    self.render_startup_page(cx).into_any_element()
                },
            )
            .children(settings_modal_el)
    }
}

impl Default for AppRoot {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_worktree_path_local() {
        let path = extract_worktree_path_from_pane_id("local:/home/user/project");
        assert_eq!(path, Some(PathBuf::from("/home/user/project")));
    }

    #[test]
    fn test_extract_worktree_path_with_index() {
        let path = extract_worktree_path_from_pane_id("local:/home/user/project:1");
        assert_eq!(path, Some(PathBuf::from("/home/user/project")));
    }

    #[test]
    fn test_extract_worktree_path_with_large_index() {
        let path = extract_worktree_path_from_pane_id("local:/tmp/repo:42");
        assert_eq!(path, Some(PathBuf::from("/tmp/repo")));
    }

    #[test]
    fn test_extract_worktree_path_tmux_pane() {
        assert_eq!(extract_worktree_path_from_pane_id("%0"), None);
    }

    #[test]
    fn test_extract_worktree_path_no_prefix() {
        assert_eq!(extract_worktree_path_from_pane_id("some-other"), None);
    }
}
