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
use crate::runtime::{AgentRuntime, EventBus, RuntimeEvent, StatusPublisher};
use crate::runtime::backends::{create_runtime_from_env, legacy_window_name_for_worktree, list_tmux_windows, recover_runtime, resolve_backend, window_name_for_worktree, window_target};
use crate::runtime::{RuntimeState, WorktreeState};
use crate::ui::{AppState, workspace_tabbar::WorkspaceTabBar, terminal_controller::ResizeController, terminal_view::TerminalBuffer, terminal_area_entity::TerminalAreaEntity, notification_panel_entity::NotificationPanelEntity, new_branch_dialog_entity::NewBranchDialogEntity, close_tab_dialog_ui::CloseTabDialogUi, delete_worktree_dialog_ui::DeleteWorktreeDialogUi, diff_view::DiffViewOverlay, status_bar::StatusBar, models::{StatusCountsModel, NotificationPanelModel, NewBranchDialogModel, PaneSummaryModel}, topbar_entity::TopBarEntity, task_dialog::TaskDialog};
use crate::scheduler::SchedulerManager;
use crate::split_tree::SplitNode;
use crate::workspace_manager::WorkspaceManager;
use crate::window_state::PersistentAppState;
use crate::new_branch_orchestrator::{NewBranchOrchestrator, CreationResult, NotificationSender};
use crate::notification::Notification;
use gpui::prelude::FluentBuilder;
use gpui::prelude::*;
use gpui::{actions, AnyElement, App, ClipboardEntry, ClipboardItem, Div, Entity, FocusHandle, Image, ImageFormat, KeyDownEvent, MouseButton, Stateful, Window, div, px, rgb};
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
pub(crate) fn build_paste_text_from_clipboard(clipboard: &ClipboardItem) -> String {
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


/// Main application root component
pub struct AppRoot {
    pub(crate) state: AppState,
    pub(crate) workspace_manager: WorkspaceManager,
    pub(crate) status_counts: StatusCounts,
    pub(crate) notification_manager: Arc<Mutex<NotificationManager>>,
    /// DialogManager Entity — manages settings modal, new branch dialog, delete/close dialogs
    pub(crate) dialog_mgr: Option<Entity<crate::ui::dialog_manager::DialogManager>>,
    /// NotificationCenter Entity — manages notifications, panel state, notification jump
    pub(crate) notification_center: Option<Entity<crate::ui::notification_center::NotificationCenter>>,
    /// RuntimeManager Entity — manages runtime lifecycle, status, animation
    pub(crate) runtime_mgr: Option<Entity<crate::ui::runtime_manager::RuntimeManager>>,
    /// TerminalManager Entity — manages terminal buffers, resize, focus, search
    pub(crate) terminal_mgr: Option<Entity<crate::ui::terminal_manager::TerminalManager>>,
    /// SplitPaneManager Entity — manages split layout, pane focus, divider drag
    pub(crate) split_pane_mgr: Option<Entity<crate::ui::split_pane_manager::SplitPaneManager>>,
    /// SchedulerManager Entity — manages scheduled tasks and cron jobs
    pub(crate) scheduler_manager: Option<Entity<SchedulerManager>>,
    pub(crate) sidebar_visible: bool,
    pub(crate) tasks_expanded: bool,
    /// Index of currently selected task in the task list (for keyboard navigation)
    pub(crate) selected_task_index: Option<usize>,
    /// Whether the task list area has keyboard focus (arrow keys navigate tasks)
    pub(crate) task_list_focused: bool,
    /// Task ID pending deletion (waiting for Enter/Escape confirmation)
    pub(crate) task_pending_delete: Option<uuid::Uuid>,
    /// Per-pane terminal buffers (Term = pipe-pane/control mode streaming; Legacy = error placeholder only)
    pub(crate) terminal_buffers: Arc<Mutex<HashMap<String, TerminalBuffer>>>,
    /// Split layout tree (single Pane or Vertical/Horizontal with children)
    pub(crate) split_tree: SplitNode,
    /// Index of focused pane in flatten() order
    pub(crate) focused_pane_index: usize,
    /// When dragging a divider: (path, start_pos, start_ratio, is_vertical)
    pub(crate) split_divider_drag: Option<(Vec<bool>, f32, f32, bool)>,
    /// Active pane target (e.g. "local:/path/to/worktree")
    pub(crate) active_pane_target: Option<String>,
    /// Shared target for input routing (updated when switching panes)
    pub(crate) active_pane_target_shared: Arc<Mutex<String>>,
    /// List of pane targets (for multi-pane split layout)
    pub(crate) pane_targets_shared: Arc<Mutex<Vec<String>>>,
    /// Runtime for terminal/backend operations (local PTY)
    pub(crate) runtime: Option<Arc<dyn AgentRuntime>>,
    /// Real-time agent status per pane ID
    pub(crate) pane_statuses: Arc<Mutex<HashMap<String, AgentStatus>>>,
    /// Event Bus for status/notification events
    pub(crate) event_bus: Arc<EventBus>,
    /// Status publisher (publishes to EventBus, replaces StatusPoller)
    pub(crate) status_publisher: Option<StatusPublisher>,
    /// JSONL session scanner for supplementary agent status detection
    pub(crate) session_scanner: Option<crate::session_scanner::SessionScanner>,
    /// Status key base for current worktree (e.g. "local:/path/to/worktree")
    pub(crate) status_key_base: Option<String>,
    /// Whether EventBus subscription has been started (spawn once)
    pub(crate) event_bus_subscription_started: bool,
    /// NewBranchDialogModel + Entity - dialog state; Entity observes, re-renders only when model notifies
    pub(crate) new_branch_dialog_model: Option<Entity<NewBranchDialogModel>>,
    pub(crate) new_branch_dialog_entity: Option<Entity<NewBranchDialogEntity>>,
    /// Focus handle for new branch dialog input (focus on open)
    pub(crate) dialog_input_focus: Option<FocusHandle>,
    /// Delete worktree confirmation dialog
    pub(crate) delete_worktree_dialog: DeleteWorktreeDialogUi,
    /// Close tab confirmation dialog (with tmux session cleanup option)
    pub(crate) close_tab_dialog: CloseTabDialogUi,
    /// Pending worktree selection to be processed on next render
    pub(crate) pending_worktree_selection: Option<usize>,
    /// When Some(idx): switching to worktree idx, show loading in terminal area
    pub(crate) worktree_switch_loading: Option<usize>,
    /// Current active worktree index (synced with Sidebar/TabBar)
    pub(crate) active_worktree_index: Option<usize>,
    /// Cached worktrees for active repo. Refreshed on workspace change, branch create/delete, explicit refresh.
    /// Avoids calling discover_worktrees in render path.
    pub(crate) cached_worktrees: Vec<crate::worktree::WorktreeInfo>,
    /// Repo path for which cached_worktrees is valid
    pub(crate) cached_worktrees_repo: Option<PathBuf>,
    /// Cached tmux window names for the current repo; filled once when opening repo to avoid repeated list-windows calls.
    pub(crate) cached_tmux_windows: Option<(PathBuf, Vec<String>)>,
    /// Maps worktree path → repo path (workspace tab path). Built incrementally
    /// when worktrees are discovered for each repo. Used for per-tab agent counts.
    pub(crate) worktree_to_repo_map: HashMap<PathBuf, PathBuf>,
    /// Sidebar context menu: which worktree index has menu open, and mouse (x, y) position
    pub(crate) sidebar_context_menu: Option<(usize, f32, f32)>,
    /// Terminal context menu: mouse (x, y) position when right-clicked
    pub(crate) terminal_context_menu: Option<(f32, f32)>,
    /// Built-in diff view entity (replaces nvim+diffview overlay)
    pub(crate) diff_view_entity: Option<Entity<DiffViewOverlay>>,
    /// Sidebar width in pixels (persisted to state.json)
    pub(crate) sidebar_width: u32,
    /// When Some, dependency check failed - show self-check page
    pub(crate) dependency_check: Option<DependencyCheckResult>,
    /// When true, focus terminal area on next frame (keyboard input without clicking first)
    pub(crate) terminal_needs_focus: bool,
    /// Stable focus handle for terminal area (must persist across renders for key events)
    pub(crate) terminal_focus: Option<FocusHandle>,
    /// ResizeController: debounced window bounds → (cols, rows) for runtime resize.
    /// Resize is driven here; gpui-terminal uses with_resize_callback.
    pub(crate) resize_controller: ResizeController,
    /// Last (cols, rows) we resized to. Used to initialize new engines at full size (avoids flash).
    pub(crate) preferred_terminal_dims: Option<(u16, u16)>,
    /// Shared dims updated by resize callback (callable from paint phase without cx).
    pub(crate) shared_terminal_dims: Arc<std::sync::Mutex<Option<(u16, u16)>>>,
    /// When true, show the Settings modal overlay
    pub(crate) show_settings: bool,
    /// Draft config when Settings is open; None when closed. Updated on open and by toggles.
    pub(crate) settings_draft: Option<Config>,
    /// Draft secrets when Settings is open; None when closed.
    pub(crate) settings_secrets_draft: Option<Secrets>,
    /// Which channel config panel is open: "discord", "kook", "feishu"
    pub(crate) settings_configuring_channel: Option<String>,
    /// Which agent is being edited in the Agent Detect settings (index into agent_detect.agents)
    pub(crate) settings_editing_agent: Option<usize>,
    /// Active settings tab: "channels" or "agent_detect"
    #[allow(dead_code)]
    pub(crate) settings_tab: String,
    /// Focus handle for the settings modal (steals focus from terminal when open)
    pub(crate) settings_focus: Option<FocusHandle>,
    /// Which settings text field is focused: "agent-name-{idx}", "rule-patterns-{agent_idx}-{rule_idx}"
    pub(crate) settings_focused_field: Option<String>,
    /// StatusCountsModel - TopBar/StatusBar observe this for entity-scoped re-render (Phase 0 spike)
    pub(crate) status_counts_model: Option<Entity<StatusCountsModel>>,
    /// TopBar Entity - observes StatusCountsModel, re-renders only when status changes
    pub(crate) topbar_entity: Option<Entity<TopBarEntity>>,
    /// NotificationPanelModel - show_panel, unread_count; Panel + bell observe this
    pub(crate) notification_panel_model: Option<Entity<NotificationPanelModel>>,
    /// NotificationPanel Entity - observes model, re-renders only when panel state changes
    pub(crate) notification_panel_entity: Option<Entity<NotificationPanelEntity>>,
    /// Terminal area Entity - when content changes, notify this instead of AppRoot (Phase 4)
    pub(crate) terminal_area_entity: Option<Entity<TerminalAreaEntity>>,
    /// When true, new branch (or other modal) dialog is open; terminal output loop skips
    /// notifying terminal area so the main thread stays responsive for dialog input (e.g. in large repos).
    pub(crate) modal_overlay_open: Arc<AtomicBool>,
    /// When true, a split divider is being dragged; resize callbacks skip runtime.resize()
    /// to prevent tmux feedback loop (resize-pane redistributes space, fighting the UI ratio).
    pub(crate) split_dragging: Arc<AtomicBool>,
    /// IME: set on Enter (no Cmd/Alt); cleared when replace_text_in_range runs or after 50ms timeout. Ensures "commit + Enter" sends text then \\r (no extra newline).
    pub(crate) ime_pending_enter: Arc<AtomicBool>,
    /// When true, search bar is visible and keyboard input appends to search_query
    pub(crate) search_active: bool,
    /// Current search query (when search_active)
    pub(crate) search_query: String,
    /// Index of current match when cycling (Enter/Cmd+G)
    pub(crate) search_current_match: usize,
    /// PaneSummaryModel - per-pane last_line + status_since for Sidebar
    pub(crate) pane_summary_model: Option<Entity<PaneSummaryModel>>,
    /// Running animation frame index (cycles through RUNNING_FRAMES)
    pub(crate) running_animation_frame: usize,
    /// Running animation timer task (250ms tick)
    pub(crate) running_animation_task: Option<gpui::Task<()>>,
    /// Whether the pmux window is focused (shared with event loop for notification suppression)
    pub(crate) window_focused_shared: Arc<AtomicBool>,
    /// Timestamp of last user keyboard input (shared with event loop for notification suppression)
    pub(crate) last_input_time: Arc<Mutex<std::time::Instant>>,
    /// Pending notification jump target: (pane_id, timestamp). Set when a system notification is
    /// sent so that clicking the notification (which activates the window) auto-focuses the pane.
    pub(crate) pending_notification_jump: Arc<Mutex<Option<(String, std::time::Instant)>>>,
    /// Previous window focus state, used to detect unfocused→focused transitions for notification click-to-focus.
    pub(crate) was_window_focused: bool,
    /// Available update info (set by background check)
    pub(crate) update_available: Option<crate::updater::UpdateInfo>,
    /// Whether an update download is in progress
    pub(crate) update_downloading: bool,
    /// Webhook pane index (maps agent IDs to pane targets for hook routing)
    pub pane_index: Option<std::sync::Arc<std::sync::RwLock<crate::hooks::handler::PaneIndex>>>,
    /// Webhook hook event handler (processes HookEvents from WebhookServer)
    pub hook_handler: Option<std::sync::Arc<crate::hooks::handler::HookEventHandler>>,
    /// Task dialog entity (Some when open)
    pub(crate) task_dialog: Option<Entity<TaskDialog>>,
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
            dialog_mgr: None,
            notification_center: None,
            runtime_mgr: None,
            terminal_mgr: None,
            split_pane_mgr: None,
            scheduler_manager: None,
            sidebar_visible: true,
            tasks_expanded: true,
            selected_task_index: None,
            task_list_focused: false,
            task_pending_delete: None,
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
            pane_index: None,
            hook_handler: None,
            task_dialog: None,
        }
    }

    // -- Test accessors (used by integration tests) --
    #[cfg(test)]
    pub fn has_task_dialog(&self) -> bool { self.task_dialog.is_some() }
    #[cfg(test)]
    pub fn is_tasks_expanded(&self) -> bool { self.tasks_expanded }
    #[cfg(test)]
    pub fn is_task_list_focused(&self) -> bool { self.task_list_focused }

    /// Create StatusCountsModel and TopBarEntity when has_workspaces (Phase 0 spike).
    /// Called from init_workspace_restoration before attach_runtime so EventBus handler can use model.
    fn ensure_entities(&mut self, cx: &mut Context<Self>) {
        // Create DialogManager entity (Phase 1 extraction)
        if self.dialog_mgr.is_none() {
            let modal_flag = self.modal_overlay_open.clone();
            let dm = cx.new(|cx| {
                let mut mgr = crate::ui::dialog_manager::DialogManager::new(modal_flag);
                mgr.ensure_focus_handles(cx);
                mgr
            });
            self.dialog_mgr = Some(dm);
        }
        // Create RuntimeManager entity (Phase 3 extraction)
        if self.runtime_mgr.is_none() {
            let event_bus = self.event_bus.clone();
            let modal_flag = self.modal_overlay_open.clone();
            let window_focused = self.window_focused_shared.clone();
            let last_input = self.last_input_time.clone();
            let rm = cx.new(|_cx| {
                crate::ui::runtime_manager::RuntimeManager::new(
                    event_bus, modal_flag, window_focused, last_input,
                )
            });
            self.runtime_mgr = Some(rm);
        }
        // Create SplitPaneManager entity (Phase 5 extraction)
        if self.split_pane_mgr.is_none() {
            let spm = cx.new(|_cx| crate::ui::split_pane_manager::SplitPaneManager::new());
            self.split_pane_mgr = Some(spm);
        }
        // Create TerminalManager entity (Phase 4 extraction)
        if self.terminal_mgr.is_none() {
            let modal_flag = self.modal_overlay_open.clone();
            let split_drag = self.split_dragging.clone();
            let tm = cx.new(|cx| {
                let mut mgr = crate::ui::terminal_manager::TerminalManager::new(modal_flag, split_drag);
                mgr.ensure_focus(cx);
                mgr
            });
            self.terminal_mgr = Some(tm);
        }
        // Create NotificationCenter entity (Phase 2 extraction)
        if self.notification_center.is_none() {
            let nc = cx.new(|_cx| crate::ui::notification_center::NotificationCenter::new());
            self.notification_center = Some(nc);
        }
        // Create SchedulerManager entity
        if self.scheduler_manager.is_none() {
            let sm = cx.new(|cx| SchedulerManager::new(cx));
            self.scheduler_manager = Some(sm);
        }
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
        self.ensure_notification_panel_entity(cx);
        self.ensure_new_branch_dialog_entity(cx);
    }

    /// Create NotificationPanelEntity with all callbacks.
    fn ensure_notification_panel_entity(&mut self, cx: &mut Context<Self>) {
        if self.notification_panel_entity.is_some() { return; }
        let Some(ref model) = self.notification_panel_model else { return; };
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
                let unread_after = if let Ok(mut m) = mgr.lock() {
                    m.clear(uuid);
                    m.unread_count()
                } else { 0 };
                let _ = cx.update_entity(&np_model, |m: &mut crate::ui::models::NotificationPanelModel, cx| {
                    m.set_unread_count(unread_after);
                    cx.notify();
                });
                let pane_id = pane_id.to_string();
                let _ = cx.update_entity(&entity, |this: &mut AppRoot, cx| {
                    if let Some(idx) = this.split_tree.flatten().into_iter().position(|(t, _)| t == pane_id) {
                        // Pane is in the current split tree — focus it directly
                        this.focused_pane_index = idx;
                        this.active_pane_target = Some(pane_id.clone());
                        if let Ok(mut guard) = this.active_pane_target_shared.lock() {
                            *guard = pane_id.clone();
                        }
                        if let Some(ref rt) = this.runtime {
                            let _ = rt.focus_pane(&pane_id);
                        }
                        this.terminal_needs_focus = true;
                    } else if let Some(wt_path) = extract_worktree_path_from_pane_id(&pane_id) {
                        // Pane is in a different worktree — switch to it
                        if let Some(wt_idx) = this.cached_worktrees.iter().position(|wt| wt.path == wt_path) {
                            let branch = this.cached_worktrees[wt_idx].short_branch_name().to_string();
                            this.active_worktree_index = Some(wt_idx);
                            if let Some(tab) = this.workspace_manager.active_tab() {
                                let repo_path = tab.path.clone();
                                this.schedule_switch_to_worktree_async(&repo_path, &wt_path, &branch, wt_idx, cx);
                            }
                        }
                    }
                    cx.notify();
                });
            })
        };
        let entity = cx.new(move |cx| {
            NotificationPanelEntity::new(
                model, notif_mgr, on_close, on_mark_read, on_clear_all,
                on_jump_to_pane, on_dismiss_and_jump, cx,
            )
        });
        self.notification_panel_entity = Some(entity);
    }

    /// Create NewBranchDialogEntity with callbacks.
    fn ensure_new_branch_dialog_entity(&mut self, cx: &mut Context<Self>) {
        if self.new_branch_dialog_model.is_none() {
            let model = cx.new(|_cx| NewBranchDialogModel::new());
            self.new_branch_dialog_model = Some(model);
        }
        if self.new_branch_dialog_entity.is_some() { return; }
        let (Some(ref model), Some(ref focus)) =
            (&self.new_branch_dialog_model, &self.dialog_input_focus) else { return; };
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
            NewBranchDialogEntity::new(model, focus, on_create, on_close, on_branch_name_change, cx)
        });
        *entity_holder.lock() = Some(entity.clone());
        self.new_branch_dialog_entity = Some(entity);
    }

    /// Initialize workspace restoration (call after AppRoot is created).
    /// Attaches to session; current worktree is derived from tmux window name (no persist).
    pub fn init_workspace_restoration(&mut self, cx: &mut Context<Self>) {
        self.ensure_entities(cx);
        if self.terminal_focus.is_none() {
            self.terminal_focus = Some(cx.focus_handle());
        }

        // Start webhook server for AI tool hooks (Claude Code, Gemini CLI, Codex, Aider)
        {
            use std::sync::{Arc, RwLock};
            let port = crate::hooks::WEBHOOK_PORT.load(std::sync::atomic::Ordering::SeqCst) as u16;
            if port > 0 {
                // Set up PaneIndex and HookEventHandler
                let pane_index = Arc::new(RwLock::new(crate::hooks::handler::PaneIndex::default()));
                let hook_handler = Arc::new(crate::hooks::handler::HookEventHandler::new(
                    Arc::clone(&pane_index),
                    Arc::clone(&self.event_bus),
                ));
                // Store on self for use in the event loop
                self.pane_index = Some(pane_index);
                self.hook_handler = Some(hook_handler);

                // Start the HTTP server in background
                let srv = crate::hooks::server::WebhookServer::new(port, Arc::clone(&self.event_bus));
                if let Err(e) = srv.start() {
                    eprintln!("pmux: webhook server failed to start on port {}: {}", port, e);
                } else {
                    // Auto-install hooks for Claude Code, Gemini CLI, Codex, Aider if not configured
                    let check = crate::hooks::setup_check::SetupCheckResult::run(port);
                    if !check.is_all_good() {
                        let results = check.install_all(port);
                        for (tool, ok) in &results {
                            if *ok {
                                eprintln!("pmux: installed hooks for {}", tool);
                            } else {
                                eprintln!("pmux: failed to install hooks for {}", tool);
                            }
                        }
                    }
                }
            }
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
    pub(crate) fn trigger_update(&mut self, cx: &mut Context<Self>) {
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
    pub(crate) fn skip_update_version(&mut self) {
        if let Some(ref info) = self.update_available {
            let version_tag = info.latest_version.display();
            if let Ok(mut config) = Config::load() {
                config.auto_update.skipped_version = Some(version_tag);
                let _ = config.save();
            }
            self.update_available = None;
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
                // Ghostty views handle their own resize via GPUI layout;
                // runtime resize is synced through TerminalManager.
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
                        if let Some(TerminalBuffer::GhosttyTerminal { focus_handle, .. }) = buffers.get(&target) {
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

        if let Some(ref tm) = self.terminal_mgr {
            // Sync shared state to TerminalManager before setup.
            // CRITICAL: tm.focus must use AppRoot's terminal_focus, NOT its own.
            // The on_key_down div tracks terminal_focus; if TerminalBuffer uses a different
            // FocusHandle, focusing the terminal for InputHandler breaks on_key_down delivery.
            let app_focus = self.terminal_focus.clone();
            tm.update(cx, |tm, _cx| {
                tm.buffers = self.terminal_buffers.clone();
                if let Some(f) = app_focus {
                    tm.focus = Some(f);
                }
                tm.status_publisher = self.status_publisher.clone();
                tm.area_entity = self.terminal_area_entity.clone();
            });
            if pane_targets.len() == 1 {
                let rt = runtime.clone();
                let pt = pane_targets[0].clone();
                tm.update(cx, |tm, cx| {
                    let (cols, rows) = tm.resolve_terminal_dims();
                    if let Err(e) = tm.setup_ghostty_terminal_pane(&rt, &pt, cols, rows, cx) {
                        log::error!("setup_ghostty_terminal_pane failed: {}", e);
                    }
                });
            } else {
                if let Ok(mut buffers) = self.terminal_buffers.lock() {
                    buffers.clear();
                }
                for (_i, pt) in pane_targets.iter().enumerate() {
                    let rt = runtime.clone();
                    let pt = pt.clone();
                    tm.update(cx, |tm, cx| {
                        let (cols, rows) = tm.resolve_terminal_dims();
                        if let Err(e) = tm.setup_ghostty_terminal_pane(&rt, &pt, cols, rows, cx) {
                            log::error!("setup_ghostty_terminal_pane failed for pane {}: {}", pt, e);
                        }
                    });
                }
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
    pub(crate) fn handle_add_workspace(&mut self, cx: &mut Context<Self>) {
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
    pub(crate) fn handle_workspace_tab_switch(&mut self, idx: usize, cx: &mut Context<Self>) {
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
    pub(crate) fn start_session_for_active_tab(&mut self, cx: &mut Context<Self>) {
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
        if !backend.starts_with("tmux") {
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
        if !backend.starts_with("tmux") {
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
        let hook_handler = self.hook_handler.clone();
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
                    Ok(RuntimeEvent::HookEvent(hook_ev)) => {
                        if let Some(ref handler) = hook_handler {
                            handler.handle(&hook_ev);
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
    fn switch_to_worktree(&mut self, worktree_path: &Path, branch_name: &str, cx: &mut Context<Self>) {
        let workspace_path = self
            .workspace_manager
            .active_tab()
            .map(|t| t.path.clone())
            .unwrap_or_else(|| worktree_path.to_path_buf());

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

    /// Process pending worktree selection (called from render context).
    pub(crate) fn process_pending_worktree_selection(&mut self, cx: &mut Context<Self>) {
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
    fn schedule_switch_to_worktree_async(
        &mut self,
        workspace_path: &Path,
        worktree_path: &Path,
        branch_name: &str,
        worktree_idx: usize,
        cx: &mut Context<Self>,
    ) {
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
    pub(crate) fn refresh_worktrees_for_repo(&mut self, repo_path: &Path) {
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
    pub(crate) fn worktrees_for_render(&self, repo_path: &Path) -> &[crate::worktree::WorktreeInfo] {
        if self.cached_worktrees_repo.as_deref() == Some(repo_path) {
            &self.cached_worktrees
        } else {
            &[]
        }
    }

    /// Tmux window names that have no corresponding worktree (worktree removed externally). Empty when not tmux backend.
    /// Uses cached_tmux_windows when repo matches to avoid repeated list-windows calls.
    pub(crate) fn orphan_tmux_windows_for_repo(&self, repo_path: &Path) -> Vec<String> {
        let backend = self.effective_backend();
        if !backend.starts_with("tmux") {
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
    pub(crate) fn compute_per_tab_active_counts(&self) -> Vec<usize> {
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
    pub(crate) fn stop_current_session(&mut self) {
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
                if let Some(TerminalBuffer::GhosttyTerminal { .. }) = buffers.get(target) {
                    false
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
            let selected_text: Option<String> = if let Ok(buffers) = self.terminal_buffers.lock() {
                if let Some(TerminalBuffer::GhosttyTerminal { .. }) = buffers.get(target) {
                    None
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
    /// Handle search-mode keys (Escape, Enter, Backspace, printable chars).
    /// Returns true if the key was consumed.
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
                // Sync to DialogManager
                if let Some(ref dm) = self.dialog_mgr {
                    dm.update(cx, |dm, cx| dm.close_settings(cx));
                }
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

        // Modal: when task dialog is open, block all keys (TaskDialog handles its own keys)
        if self.task_dialog.is_some() {
            return;
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
            if self.handle_search_key(event, cx) {
                return;
            }
        }

        // Task list focused: arrow keys navigate tasks, Enter/Escape handle confirmation
        if self.task_list_focused {
            match event.keystroke.key.as_str() {
                "up" => {
                    if !event.keystroke.modifiers.platform {
                        if let Some(idx) = self.selected_task_index {
                            if idx > 0 {
                                self.selected_task_index = Some(idx - 1);
                                self.task_pending_delete = None; // cancel pending delete on nav
                                cx.notify();
                            }
                        }
                        return;
                    }
                }
                "down" => {
                    if !event.keystroke.modifiers.platform {
                        let task_count = self.scheduler_manager.as_ref()
                            .map(|m| m.read(cx).tasks().len())
                            .unwrap_or(0);
                        if let Some(idx) = self.selected_task_index {
                            if idx + 1 < task_count {
                                self.selected_task_index = Some(idx + 1);
                                self.task_pending_delete = None;
                                cx.notify();
                            }
                        }
                        return;
                    }
                }
                "enter" => {
                    // Confirm pending delete
                    if let Some(id) = self.task_pending_delete.take() {
                        if let Some(ref manager) = self.scheduler_manager {
                            manager.update(cx, |m, cx| {
                                let _ = m.remove_task(id, cx);
                            });
                        }
                        // Adjust selected index
                        let task_count = self.scheduler_manager.as_ref()
                            .map(|m| m.read(cx).tasks().len())
                            .unwrap_or(0);
                        if task_count == 0 {
                            self.selected_task_index = None;
                        } else if let Some(idx) = self.selected_task_index {
                            if idx >= task_count {
                                self.selected_task_index = Some(task_count - 1);
                            }
                        }
                        cx.notify();
                    }
                    return;
                }
                "escape" => {
                    if self.task_pending_delete.is_some() {
                        self.task_pending_delete = None;
                    } else {
                        self.task_list_focused = false;
                        self.selected_task_index = None;
                    }
                    cx.notify();
                    return;
                }
                _ => {}
            }
        }

        // Check for Cmd+key shortcuts (app shortcuts)
        if event.keystroke.modifiers.platform {
            self.handle_shortcut(event, cx);
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
                            if let Some(TerminalBuffer::GhosttyTerminal { .. }) = buffers.get(target) {
                                // Ghostty entity handles scroll internally
                                false
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
        self.forward_key_to_terminal(event, cx);
    }

    /// Handle split pane (⌘D vertical, ⌘⇧D horizontal)
    pub(crate) fn handle_split_pane(&mut self, vertical: bool, cx: &mut Context<Self>) {
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
            if let (Some(rt), Some(ref tm)) = (&self.runtime, &self.terminal_mgr) {
                let rt = rt.clone();
                let te = self.terminal_area_entity.clone();
                let sp = self.status_publisher.clone();
                let bufs = self.terminal_buffers.clone();
                let app_focus = self.terminal_focus.clone();
                tm.update(cx, |tm, cx| {
                    tm.buffers = bufs;
                    if let Some(f) = app_focus { tm.focus = Some(f); }
                    tm.status_publisher = sp;
                    tm.area_entity = te.clone();
                    let (cols, rows) = tm.resolve_terminal_dims();
                    if let Err(e) = tm.setup_ghostty_terminal_pane(&rt, &new_target, cols, rows, cx) {
                        log::error!("setup_ghostty_terminal_pane failed for split pane {}: {}", new_target, e);
                    }
                });
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
    pub(crate) fn handle_close_pane(&mut self, cx: &mut Context<Self>) {
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
    pub(crate) fn open_diff_view(&mut self, cx: &mut Context<Self>) {
        self.open_diff_view_for_worktree(self.active_worktree_index, cx);
    }

    /// Opens diff view for a specific worktree index
    pub(crate) fn open_diff_view_for_worktree(&mut self, worktree_idx: Option<usize>, cx: &mut Context<Self>) {
        let repo_path = self.workspace_manager.active_tab()
            .map(|t| t.path.clone())
            .unwrap_or_else(|| PathBuf::from("."));

        self.refresh_worktrees_for_repo(&repo_path);
        let idx = worktree_idx.unwrap_or(0);
        self.open_diff_view_for_worktree_with_cache(idx, cx);
    }

    /// Opens diff view using cached worktrees (no refresh). Call after cache is populated.
    pub(crate) fn open_diff_view_for_worktree_with_cache(&mut self, idx: usize, cx: &mut Context<Self>) {
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
    pub(crate) fn open_task_dialog(&mut self, cx: &mut Context<Self>) {
        let app_root_entity = cx.entity();
        let app_root_entity_cancel = app_root_entity.clone();
        let entity = cx.new(|cx| {
            let mut dialog = TaskDialog::new(cx);
            dialog.set_on_save(move |task, _window, cx| {
                let _ = cx.update_entity(&app_root_entity, |this: &mut AppRoot, cx| {
                    if let Some(ref manager) = this.scheduler_manager {
                        manager.update(cx, |m, cx| {
                            let _ = m.add_task(task, cx);
                        });
                    }
                    this.task_dialog = None;
                    this.terminal_needs_focus = true;
                    cx.notify();
                });
            });
            dialog.set_on_cancel(move |_window, cx| {
                let _ = cx.update_entity(&app_root_entity_cancel, |this: &mut AppRoot, cx| {
                    this.task_dialog = None;
                    this.terminal_needs_focus = true;
                    cx.notify();
                });
            });
            dialog
        });
        self.task_dialog = Some(entity);
        cx.notify();
    }

    pub(crate) fn open_new_branch_dialog(&mut self, cx: &mut Context<Self>) {
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
    pub(crate) fn close_close_tab_dialog(&mut self, cx: &mut Context<Self>) {
        self.close_tab_dialog.close();
        self.terminal_needs_focus = true;
        cx.notify();
    }

    /// Toggles the kill_tmux checkbox in the close-tab dialog
    pub(crate) fn toggle_close_tab_kill_tmux(&mut self, cx: &mut Context<Self>) {
        self.close_tab_dialog.toggle_kill_tmux();
        cx.notify();
    }

    /// Confirms tab close: removes tab, stops session, optionally kills tmux session
    pub(crate) fn confirm_close_tab(&mut self, tab_index: usize, kill_tmux: bool, cx: &mut Context<Self>) {
        let closed_path = self.workspace_manager.get_tab(tab_index).map(|t| t.path.clone());
        self.workspace_manager.close_tab(tab_index);

        if self.workspace_manager.is_empty() {
            self.stop_current_session();
        } else {
            self.stop_current_session();
            self.start_session_for_active_tab(cx);
        }

        // Evict pane statuses for closed workspace (#5 collection eviction)
        if let Some(ref path) = closed_path {
            let prefix = format!("local:{}", path.display());
            if let Ok(mut statuses) = self.pane_statuses.lock() {
                let colon_prefix = format!("{}:", prefix);
                statuses.retain(|k, _| k != &prefix && !k.starts_with(&colon_prefix));
            }
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
    pub(crate) fn close_delete_dialog(&mut self, cx: &mut Context<Self>) {
        self.delete_worktree_dialog.close();
        self.terminal_needs_focus = true;
        cx.notify();
    }

    /// Confirms worktree deletion (tmux kill-window + git worktree remove)
    pub(crate) fn confirm_delete_worktree(&mut self, worktree: crate::worktree::WorktreeInfo, cx: &mut Context<Self>) {
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

        // Evict pane statuses for deleted worktree (#5 collection eviction)
        {
            let prefix = format!("local:{}", worktree_path.display());
            if let Ok(mut statuses) = self.pane_statuses.lock() {
                let colon_prefix = format!("{}:", prefix);
                statuses.retain(|k, _| k != &prefix && !k.starts_with(&colon_prefix));
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

    // Settings render methods moved to DialogManager (src/ui/dialog_manager.rs)

    fn render_settings_modal_via_dialog_mgr(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let dialog_mgr = self.dialog_mgr.clone()?;
        if !dialog_mgr.read(cx).is_settings_open() {
            return None;
        }
        Some(dialog_mgr.update(cx, |dm, cx| {
            dm.render_settings_modal(cx).into_any_element()
        }))
    }


    fn render_workspace_view(&self, cx: &mut Context<Self>, terminal_focus: &gpui::FocusHandle, cursor_blink_visible: bool) -> impl IntoElement {
        let sidebar_visible = self.sidebar_visible;
        let focused_pane_index = self.focused_pane_index;
        let worktree_switch_loading = self.worktree_switch_loading;
        let app_root_entity = cx.entity();

        let repo_name = self.workspace_manager.active_tab()
            .map(|t| t.name.clone())
            .unwrap_or_else(|| "workspace".to_string());
        let repo_path = self.workspace_manager.active_tab()
            .map(|t| t.path.clone())
            .unwrap_or_else(|| std::path::PathBuf::from("."));

        let notification_panel_model_for_overlay = self.notification_panel_model.clone();
        let notification_panel_is_open = self.notification_panel_model.as_ref()
            .map(|m| m.read(cx).show_panel)
            .unwrap_or(false);

        let sidebar = self.build_sidebar(cx, &repo_name, &repo_path, terminal_focus);

        // Entity clones for context menus
        let app_root_entity_for_clear_menu = app_root_entity.clone();
        let _cached_worktrees = self.cached_worktrees.clone();

        let delete_dialog = self.build_delete_dialog(cx);
        let close_tab_dialog = self.build_close_tab_dialog(cx);

        let sidebar_context_menu = self.sidebar_context_menu;
        let terminal_context_menu = self.terminal_context_menu;
        let cached_worktrees = self.cached_worktrees.clone();

        // Terminal context menu clones
        let app_root_for_term_menu_overlay = app_root_entity.clone();

        // Get selected text for Copy menu item
        let has_selection = terminal_context_menu.is_some() && {
            if let Some(ref target) = self.active_pane_target {
                if let Ok(buffers) = self.terminal_buffers.lock() {
                    if let Some(TerminalBuffer::GhosttyTerminal { .. }) = buffers.get(target) {
                        false
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
                                    WorkspaceTabBar::new(self.workspace_manager.clone())
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
                            .child(self.build_terminal_content_area(
                                cx, terminal_focus, &repo_name,
                                self.split_tree.clone(), Arc::clone(&self.terminal_buffers),
                                focused_pane_index, self.split_divider_drag.clone(),
                                worktree_switch_loading, cursor_blink_visible,
                            ))
                    )
            )
            // Update banner (above status bar)
            .children(self.render_update_banner(cx))
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
                let menu = self.build_sidebar_context_menu(cx, idx, &repo_path, &cached_worktrees);
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
                let menu = self.build_terminal_context_menu(cx, has_selection);
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
            // Sync to DialogManager
            if let Some(ref dm) = self.dialog_mgr {
                let config = Config::load().unwrap_or_default();
                let secrets = Secrets::load().unwrap_or_default();
                dm.update(cx, |dm, cx| dm.open_settings(config, secrets, cx));
            }
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
                    if let Some(TerminalBuffer::GhosttyTerminal { focus_handle, .. }) = buf {
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
        let task_dialog_open = self.task_dialog.is_some();
        let any_modal_open = settings_open || new_branch_dialog_open || task_dialog_open;
        self.modal_overlay_open.store(any_modal_open, Ordering::Relaxed);
        let settings_modal_el = if settings_open { self.render_settings_modal_via_dialog_mgr(cx) } else { None };

        div()
            .id("app-root")
            .relative()
            .size_full()
            .bg(rgb(0x21252b))
            .text_color(rgb(0xabb2bf))
            .font_family(".SystemUIFont")
            .focusable()
            .track_focus(&terminal_focus)
            .when(!new_branch_dialog_open && !settings_open && !task_dialog_open, |el| {
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
            .when(task_dialog_open, |el| {
                el.child(self.task_dialog.as_ref().unwrap().clone())
            })
    }
}

impl Default for AppRoot {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "app_root_test.rs"]
mod app_root_test;

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
