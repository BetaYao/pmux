// ui/app_root.rs - Root component for pmux GUI
use crate::agent_status::{StatusCounts, AgentStatus};
use crate::config::Config;
use crate::deps::{self, DependencyCheckResult};
use crate::file_selector::show_folder_picker_async;
use crate::git_utils::{is_git_repository, get_git_error_message, GitError};
use crate::notification::NotificationType;
use crate::notification_manager::NotificationManager;
use crate::system_notifier;
use crate::tmux::session::Session;
use crate::tmux::pane as tmux_pane;
use crate::tmux::window as tmux_window;
use crate::tmux::control_mode_attach;
use crate::terminal::TermBridge;
use crate::ui::{AppState, sidebar::Sidebar, workspace_tabbar::WorkspaceTabBar, terminal_view::{TerminalBuffer, TerminalContent}, notification_panel::{NotificationPanel, NotificationItem}, new_branch_dialog_ui::NewBranchDialogUi, delete_worktree_dialog_ui::DeleteWorktreeDialogUi, split_pane_container::SplitPaneContainer, diff_overlay::DiffOverlay, status_bar::StatusBar};
use crate::split_tree::SplitNode;
use crate::workspace_manager::WorkspaceManager;
use crate::input_handler::InputHandler;
use crate::window_state::PersistentAppState;
use crate::new_branch_orchestrator::{NewBranchOrchestrator, CreationResult, NotificationSender};
use crate::notification::Notification;
use gpui::prelude::FluentBuilder;
use gpui::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

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
    state: AppState,
    workspace_manager: WorkspaceManager,
    status_counts: StatusCounts,
    notification_manager: Arc<Mutex<NotificationManager>>,
    show_notification_panel: bool,
    sidebar_visible: bool,
    /// Per-pane terminal buffers (Legacy capture-pane or Term control mode)
    terminal_buffers: Arc<Mutex<HashMap<String, TerminalBuffer>>>,
    /// Split layout tree (single Pane or Vertical/Horizontal with children)
    split_tree: SplitNode,
    /// Index of focused pane in flatten() order
    focused_pane_index: usize,
    /// When dragging a divider: (path, start_pos, start_ratio, is_vertical)
    split_divider_drag: Option<(Vec<bool>, f32, f32, bool)>,
    /// Active tmux pane target (e.g. "sdlc-myproject:@0.%0")
    active_pane_target: Option<String>,
    /// Shared target for polling loop to read (updated when switching panes)
    active_pane_target_shared: Arc<Mutex<String>>,
    /// List of pane targets to poll (for multi-pane split layout)
    pane_targets_shared: Arc<Mutex<Vec<String>>>,
    /// Input handler for forwarding keyboard events to tmux
    input_handler: Option<InputHandler>,
    /// Real-time agent status per pane ID (tmux pane target format)
    pane_statuses: Arc<Mutex<HashMap<String, AgentStatus>>>,
    /// Status poller for background agent status detection
    status_poller: Option<Arc<Mutex<crate::status_poller::StatusPoller>>>,
    /// New branch dialog UI
    new_branch_dialog: NewBranchDialogUi,
    /// Delete worktree confirmation dialog
    delete_worktree_dialog: DeleteWorktreeDialogUi,
    /// Pending worktree selection to be processed on next render
    pending_worktree_selection: Option<usize>,
    /// Current active worktree index (synced with Sidebar/TabBar)
    active_worktree_index: Option<usize>,
    /// Per-repo active worktree index for restoring state when switching workspace tabs
    per_repo_worktree_index: HashMap<PathBuf, usize>,
    /// Sidebar context menu: which worktree index has menu open
    sidebar_context_menu_index: Option<usize>,
    /// Review windows: branch -> tmux window name (e.g. "review-feat-x")
    review_windows: HashMap<String, String>,
    /// When Some, diff overlay is shown: (branch, window_name, pane_target)
    diff_overlay_open: Option<(String, String, String)>,
    /// Sidebar width in pixels (persisted to state.json)
    sidebar_width: u32,
    /// When Some, dependency check failed - show self-check page
    dependency_check: Option<DependencyCheckResult>,
    /// Cursor blink tick for terminal (toggles every ~530ms)
    cursor_blink_tick: u32,
    /// Whether cursor blink timer has been started
    cursor_blink_timer_started: bool,
    /// When true, focus terminal area on next frame (keyboard input without clicking first)
    terminal_needs_focus: bool,
    /// When set to false, signals the control mode consumer loop to exit
    control_mode_running: Option<Arc<AtomicBool>>,
    /// Stable focus handle for terminal area (must persist across renders for key events)
    terminal_focus: Option<FocusHandle>,
}

impl AppRoot {
    /// Get sidebar width for persistence (clamped 200-400)
    pub fn sidebar_width(&self) -> u32 {
        self.sidebar_width.clamp(200, 400)
    }

    /// Save workspace state to Config (multi-repo paths, active index, per-repo worktree index)
    fn save_config(&self) {
        let mut config = Config::load().unwrap_or_default();
        let paths = self.workspace_manager.workspace_paths();
        config.save_workspaces(
            &paths,
            self.workspace_manager.active_tab_index().unwrap_or(0),
            &self.per_repo_worktree_index,
        );
        let _ = config.save();
    }

    pub fn new() -> Self {
        let config = Config::load().unwrap_or_default();
        let mut workspace_manager = WorkspaceManager::new();
        let mut per_repo_worktree_index = config.get_per_repo_worktree_index();

        // Load multi-repo workspace paths
        let workspace_paths = config.get_workspace_paths();
        for path in workspace_paths {
            if is_git_repository(&path) {
                workspace_manager.add_workspace(path);
            } else {
                eprintln!("AppRoot: Saved workspace is not a valid git repository: {:?}", path);
                per_repo_worktree_index.remove(&path);
            }
        }

        // Set active tab index (clamp to valid range)
        let active_idx = config.active_workspace_index.min(workspace_manager.tab_count().saturating_sub(1));
        if workspace_manager.tab_count() > 0 && active_idx < workspace_manager.tab_count() {
            workspace_manager.switch_to_tab(active_idx);
        }

        // If we had invalid paths, save cleaned config
        let paths = workspace_manager.workspace_paths();
        if paths.len() != config.workspace_paths.len() {
            let mut config = Config::load().unwrap_or_default();
            config.save_workspaces(
                &paths,
                workspace_manager.active_tab_index().unwrap_or(0),
                &per_repo_worktree_index,
            );
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
            show_notification_panel: false,
            sidebar_visible: true,
            terminal_buffers: Arc::new(Mutex::new(HashMap::new())),
            split_tree: SplitNode::pane(""),
            focused_pane_index: 0,
            split_divider_drag: None,
            active_pane_target: None,
            active_pane_target_shared: Arc::new(Mutex::new(String::new())),
            pane_targets_shared: Arc::new(Mutex::new(Vec::new())),
            input_handler: None,
            pane_statuses: Arc::new(Mutex::new(HashMap::new())),
            status_poller: None,
            new_branch_dialog: NewBranchDialogUi::new(),
            delete_worktree_dialog: DeleteWorktreeDialogUi::new(),
            pending_worktree_selection: None,
            active_worktree_index: None,
            per_repo_worktree_index,
            sidebar_context_menu_index: None,
            review_windows: HashMap::new(),
            diff_overlay_open: None,
            sidebar_width,
            dependency_check,
            cursor_blink_tick: 0,
            cursor_blink_timer_started: false,
            terminal_needs_focus: false,
            control_mode_running: None,
            terminal_focus: None,
        }
    }

    /// Initialize workspace restoration (call after AppRoot is created)
    /// Ensures all tmux sessions exist, attaches to active tab, restores per-repo worktree selection
    pub fn init_workspace_restoration(&mut self, cx: &mut Context<Self>) {
        // Stable focus handle must persist across renders; creating it here ensures key events reach handle_key_down
        if self.terminal_focus.is_none() {
            self.terminal_focus = Some(cx.focus_handle());
        }
        // Sessions are created on demand when switching worktrees or starting tmux (workspace=session)

        // Attach to active tab (full polling, input)
        let repo_name = self.workspace_manager.active_tab().map(|t| t.name.clone());
        let repo_path = self.workspace_manager.active_tab().map(|t| t.path.clone());
        if let (Some(name), Some(path)) = (repo_name, repo_path) {
            // Restore per-repo worktree selection if saved
            let restored_idx = self.per_repo_worktree_index.get(&path).copied();
            if let Some(awi) = restored_idx {
                if let Ok(worktrees) = crate::worktree::discover_worktrees(&path) {
                    if awi < worktrees.len() {
                        self.active_worktree_index = Some(awi);
                        if let Some(wt) = worktrees.get(awi) {
                            let wt_path = wt.path.clone();
                            let branch = wt.short_branch_name().to_string();
                            self.switch_to_worktree(&wt_path, &branch, cx);
                            return;
                        }
                    }
                }
            }

            // No saved worktree or invalid: use first worktree if any, else repo session
            self.active_worktree_index = None;
            if let Ok(worktrees) = crate::worktree::discover_worktrees(&path) {
                if !worktrees.is_empty() {
                    self.active_worktree_index = Some(0);
                    let wt = &worktrees[0];
                    let wt_path = wt.path.clone();
                    let branch = wt.short_branch_name().to_string();
                    self.switch_to_worktree(&wt_path, &branch, cx);
                    return;
                }
            }
            self.start_tmux_session(&name, &path, cx);
        }

    }

    /// Start tmux session and pane polling for the given repo name
    /// Sets up terminal content polling, status polling, and input handling
    fn start_tmux_session(&mut self, repo_name: &str, repo_path: &Path, cx: &mut Context<Self>) {
        let session = Session::new(repo_name);
        if let Err(e) = session.ensure_in(Some(repo_path)) {
            self.state.error_message = Some(format!("tmux error: {}", e));
            return;
        }

        // Get actual pane target from tmux (more reliable than hardcoding .0)
        let pane_target = tmux_pane::list_panes_for_window(session.name(), session.window_name())
            .ok()
            .and_then(|panes| panes.first().map(|p| p.target()))
            .unwrap_or_else(|| format!("{}:{}.0", session.name(), session.window_name()));
        self.active_pane_target = Some(pane_target.clone());
        self.split_tree = SplitNode::pane(&pane_target);
        self.focused_pane_index = 0;
        if let Ok(mut guard) = self.active_pane_target_shared.lock() {
            *guard = pane_target.clone();
        }
        if let Ok(mut guard) = self.pane_targets_shared.lock() {
            *guard = vec![pane_target.clone()];
        }

        // Initialize input handler for this session
        self.input_handler = Some(InputHandler::new(session.name().to_string()));
        self.terminal_needs_focus = true;

        // Initialize and register StatusPoller for agent status detection
        let status_poller = Arc::new(Mutex::new(crate::status_poller::StatusPoller::new()));
        {
            let mut poller = status_poller.lock().unwrap();
            poller.register_pane(&pane_target);
        }
        self.status_poller = Some(status_poller.clone());

        // Try control mode first; fallback to capture-pane polling on failure
        // Set PMUX_USE_CAPTURE_PANE=1 to force capture-pane (for debugging display issues)
        let session_name = session.name().to_string();
        let force_capture_pane = std::env::var("PMUX_USE_CAPTURE_PANE")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        let handle = if force_capture_pane {
            eprintln!("pmux: PMUX_USE_CAPTURE_PANE=1, forcing capture-pane fallback");
            None
        } else {
            control_mode_attach(&session_name)
                .inspect_err(|e| eprintln!("pmux: control mode attach failed ({}), using capture-pane fallback", e))
                .ok()
        };
        match handle {
            Some(handle) => {
                let (cols, rows) = tmux_pane::get_pane_dimensions(&pane_target);
                if let Ok(mut buffers) = self.terminal_buffers.lock() {
                    buffers.clear();
                    buffers.insert(
                        pane_target.clone(),
                        TerminalBuffer::Term(Arc::new(Mutex::new(TermBridge::new(cols, rows)))),
                    );
                }
                let terminal_buffers = self.terminal_buffers.clone();
                let running = Arc::new(AtomicBool::new(true));
                self.control_mode_running = Some(running.clone());
                let _entity = cx.entity();
                cx.spawn(async move |entity, cx| {
                    while running.load(Ordering::Relaxed) {
                        let mut needs_notify = false;
                        while let Some((target, bytes)) = handle.try_recv() {
                            if let Ok(mut buffers) = terminal_buffers.lock() {
                                let (cols, rows) = tmux_pane::get_pane_dimensions(&target);
                                let term = buffers
                                    .entry(target.to_string())
                                    .or_insert_with(|| {
                                        TerminalBuffer::Term(Arc::new(Mutex::new(
                                            TermBridge::new(cols, rows),
                                        )))
                                    });
                                if let TerminalBuffer::Term(t) = term {
                                    if let Ok(guard) = t.lock() {
                                        guard.advance(&bytes);
                                        needs_notify = true;
                                    }
                                }
                            }
                        }
                        if needs_notify {
                            let _ = entity.update(cx, |_, cx| cx.notify());
                        }
                        cx.background_executor().timer(Duration::from_millis(16)).await;
                    }
                    let _ = handle.shutdown();
                })
                .detach();
            }
            None => {
                let terminal_buffers = self.terminal_buffers.clone();
                let pane_targets = self.pane_targets_shared.clone();
                if let Ok(mut buffers) = terminal_buffers.lock() {
                    buffers.clear();
                    buffers.insert(
                        pane_target.clone(),
                        TerminalBuffer::Legacy(Arc::new(Mutex::new(TerminalContent::new()))),
                    );
                }
                let terminal_buffers = self.terminal_buffers.clone();
                let _entity = cx.entity();
                cx.spawn(async move |entity, cx| {
                    loop {
                        let targets = pane_targets.lock().map(|g| g.clone()).unwrap_or_default();
                        let mut updated = false;
                        for target in &targets {
                            if let Ok(text) = tmux_pane::capture_pane(target) {
                                if let Ok(mut buffers) = terminal_buffers.lock() {
                                    if let Some(TerminalBuffer::Legacy(content)) = buffers.get_mut(target) {
                                        if let Ok(mut guard) = content.lock() {
                                            guard.update(&text);
                                            updated = true;
                                        }
                                    }
                                }
                            }
                        }
                        if updated {
                            let _ = entity.update(cx, |_, cx| cx.notify());
                        }
                        cx.background_executor().timer(Duration::from_millis(200)).await;
                    }
                })
                .detach();
            }
        }

        // Start background status polling loop (500ms interval)
        // Polls StatusPoller for status changes and updates UI
        let pane_statuses = self.pane_statuses.clone();
        let status_poller_for_polling = status_poller.clone();
        cx.spawn(async move |entity, cx| {
            loop {
                // Check for status changes from StatusPoller
                if let Ok(poller) = status_poller_for_polling.lock() {
                    let current_status = poller.get_status(&pane_target);
                    let mut updated = false;

                    // Update shared status HashMap if status changed
                    if let Ok(mut statuses) = pane_statuses.lock() {
                        let previous = statuses.get(&pane_target);
                        if previous != Some(&current_status) {
                            statuses.insert(pane_target.clone(), current_status);
                            updated = true;
                        }
                    }

                    if updated {
                        let pane_target_for_notif = pane_target.clone();
                        let _ = entity.update(cx, |this, cx| {
                            this.update_status_counts();
                            if let Ok(statuses) = this.pane_statuses.lock() {
                                if let Some(&new_status) = statuses.get(&pane_target_for_notif) {
                                    if new_status.is_urgent() {
                                        let notif_type = match new_status {
                                            AgentStatus::Error => Some(NotificationType::Error),
                                            AgentStatus::Waiting => Some(NotificationType::Waiting),
                                            _ => None,
                                        };
                                        if let Some(nt) = notif_type {
                                            let message = new_status.display_text().to_string();
                                            if let Ok(mut mgr) = this.notification_manager.lock() {
                                                if mgr.add(&pane_target_for_notif, nt, &message) {
                                                    system_notifier::notify("pmux", &message, nt);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            cx.notify();
                        });
                    }
                }

                cx.background_executor().timer(Duration::from_millis(500)).await;
            }
        }).detach();

        // Start the StatusPoller background thread
        // This thread runs in background polling tmux panes for status detection
        if let Some(poller) = &self.status_poller {
            if let Ok(mut p) = poller.lock() {
                p.start();
            }
        }
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
                        // Save current repo state before switching to new workspace
                        if let Some(tab) = this.workspace_manager.active_tab() {
                            if let Some(awi) = this.active_worktree_index {
                                this.per_repo_worktree_index.insert(tab.path.clone(), awi);
                            }
                        }
                        let idx = this.workspace_manager.add_workspace(path.clone());
                        this.workspace_manager.switch_to_tab(idx);
                        this.state.error_message = None;

                        // Save config (multi-repo state)
                        this.save_config();

                        // Start tmux session + polling (use first worktree if any)
                        this.active_worktree_index = None;
                        if let Ok(worktrees) = crate::worktree::discover_worktrees(&path) {
                            if !worktrees.is_empty() {
                                this.active_worktree_index = Some(0);
                                let wt = &worktrees[0];
                                let wt_path = wt.path.clone();
                                let branch = wt.short_branch_name().to_string();
                                this.switch_to_worktree(&wt_path, &branch, cx);
                            } else {
                                let repo_name = path.file_name()
                                    .and_then(|n| n.to_str())
                                    .unwrap_or("workspace");
                                this.start_tmux_session(repo_name, &path, cx);
                            }
                        } else {
                            let repo_name = path.file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("workspace");
                            this.start_tmux_session(repo_name, &path, cx);
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

        // Save current repo's active_worktree_index before switching
        if let Some(tab) = self.workspace_manager.active_tab() {
            if let Some(awi) = self.active_worktree_index {
                self.per_repo_worktree_index.insert(tab.path.clone(), awi);
            }
        }

        self.workspace_manager.switch_to_tab(idx);
        self.save_config();
        self.stop_current_session();

        if let Some(tab) = self.workspace_manager.active_tab() {
            let repo_path = tab.path.clone();
            let repo_name = tab.name.clone();

            // Restore active_worktree_index for this repo
            let restored_idx = self.per_repo_worktree_index.get(&repo_path).copied();

            if let Some(awi) = restored_idx {
                if let Ok(worktrees) = crate::worktree::discover_worktrees(&repo_path) {
                    if awi < worktrees.len() {
                        self.active_worktree_index = Some(awi);
                        if let Some(wt) = worktrees.get(awi) {
                            let path = wt.path.clone();
                            let branch = wt.short_branch_name().to_string();
                            self.switch_to_worktree(&path, &branch, cx);
                            cx.notify();
                            return;
                        }
                    }
                }
            }

            // No saved worktree or invalid index: use first worktree if any
            self.active_worktree_index = None;
            if let Ok(worktrees) = crate::worktree::discover_worktrees(&repo_path) {
                if !worktrees.is_empty() {
                    self.active_worktree_index = Some(0);
                    let wt = &worktrees[0];
                    let wt_path = wt.path.clone();
                    let branch = wt.short_branch_name().to_string();
                    self.switch_to_worktree(&wt_path, &branch, cx);
                    cx.notify();
                    return;
                }
            }
            self.start_tmux_session(&repo_name, &repo_path, cx);
        }
        cx.notify();
    }

    /// Start tmux session for the currently active workspace tab (no state save).
    /// Used when closing a tab to switch to the new active tab.
    fn start_session_for_active_tab(&mut self, cx: &mut Context<Self>) {
        if let Some(tab) = self.workspace_manager.active_tab() {
            let repo_path = tab.path.clone();
            let repo_name = tab.name.clone();
            let restored_idx = self.per_repo_worktree_index.get(&repo_path).copied();

            if let Some(awi) = restored_idx {
                if let Ok(worktrees) = crate::worktree::discover_worktrees(&repo_path) {
                    if awi < worktrees.len() {
                        self.active_worktree_index = Some(awi);
                        if let Some(wt) = worktrees.get(awi) {
                            let path = wt.path.clone();
                            let branch = wt.short_branch_name().to_string();
                            self.switch_to_worktree(&path, &branch, cx);
                            cx.notify();
                            return;
                        }
                    }
                }
            }

            self.active_worktree_index = None;
            if let Ok(worktrees) = crate::worktree::discover_worktrees(&repo_path) {
                if !worktrees.is_empty() {
                    self.active_worktree_index = Some(0);
                    let wt = &worktrees[0];
                    let wt_path = wt.path.clone();
                    let branch = wt.short_branch_name().to_string();
                    self.switch_to_worktree(&wt_path, &branch, cx);
                } else {
                    self.start_tmux_session(&repo_name, &repo_path, cx);
                }
            } else {
                self.start_tmux_session(&repo_name, &repo_path, cx);
            }
        }
        cx.notify();
    }

    pub fn has_workspaces(&self) -> bool {
        !self.workspace_manager.is_empty()
    }

    /// Switch to a specific worktree
    /// Mapping: workspace=session, worktree=window, terminal=pane
    fn switch_to_worktree(&mut self, worktree_path: &Path, branch_name: &str, cx: &mut Context<Self>) {
        let repo_name = self.workspace_manager.active_tab()
            .map(|t| t.name.clone())
            .unwrap_or_else(|| "workspace".to_string());
        let session_name = format!("sdlc-{}", repo_name.replace('/', "-").replace(' ', "-").replace('\\', "-"));
        let window_name = branch_name.replace('/', "-");

        // When switching to a different session (different workspace), stop current first
        let same_session = self.input_handler.as_ref()
            .map(|h| h.session_name() == session_name)
            .unwrap_or(false);
        if !same_session {
            self.stop_current_session();
        }

        // Ensure workspace session exists; create with first window if not
        if !Session::exists(&session_name) {
            let session = Session::new(&repo_name);
            if let Err(e) = session.ensure_in(Some(worktree_path)) {
                self.state.error_message = Some(format!("tmux error for worktree {}: {}", worktree_path.display(), e));
                return;
            }
            let _ = tmux_window::rename_window(&format!("{}:control-tower", session_name), &window_name);
        } else {
            let windows = tmux_window::list_windows(&session_name).unwrap_or_default();
            let has_window = windows.iter().any(|w| w.name == window_name);
            if has_window {
                let _ = tmux_window::select_window(&session_name, &window_name);
            } else {
                if let Err(e) = tmux_window::create_window_with_cwd(&session_name, &window_name, worktree_path) {
                    self.state.error_message = Some(format!("tmux error creating window: {}", e));
                    return;
                }
            }
        }

        // Get actual pane target from tmux
        let pane_target = tmux_pane::list_panes_for_window(&session_name, &window_name)
            .ok()
            .and_then(|panes| panes.first().map(|p| p.target()))
            .unwrap_or_else(|| format!("{}:{}.0", session_name, window_name));
        let _ = tmux_pane::select_pane(&pane_target);
        let old_pane_target = self.active_pane_target.clone();
        self.active_pane_target = Some(pane_target.clone());
        self.split_tree = SplitNode::pane(&pane_target);
        self.focused_pane_index = 0;
        if let Ok(mut guard) = self.active_pane_target_shared.lock() {
            *guard = pane_target.clone();
        }
        if let Ok(mut guard) = self.pane_targets_shared.lock() {
            *guard = vec![pane_target.clone()];
        }
        self.terminal_needs_focus = true;

        if same_session {
            // Same session: just update pane tracking, no need to restart control mode
            if let Some(old) = old_pane_target.as_ref() {
                if let Some(poller) = &self.status_poller {
                    if let Ok(mut p) = poller.lock() {
                        p.unregister_pane(old);
                    }
                }
            }
            self.input_handler.get_or_insert_with(|| InputHandler::new(session_name.clone()));
            let status_poller = self.status_poller.get_or_insert_with(|| Arc::new(Mutex::new(crate::status_poller::StatusPoller::new())));
            if let Ok(mut poller) = status_poller.lock() {
                poller.register_pane(&pane_target);
                poller.start(); // restart to pick up new pane list
            }
            if let Ok(mut buffers) = self.terminal_buffers.lock() {
                let (cols, rows) = tmux_pane::get_pane_dimensions(&pane_target);
                buffers.entry(pane_target.clone()).or_insert_with(|| {
                    TerminalBuffer::Term(Arc::new(Mutex::new(TermBridge::new(cols, rows))))
                });
            }
            println!("Switched to worktree: {} (window: {})", worktree_path.display(), window_name);
            return;
        }

        // New session: full setup
        self.input_handler = Some(InputHandler::new(session_name.clone()));
        let status_poller = Arc::new(Mutex::new(crate::status_poller::StatusPoller::new()));
        {
            let mut poller = status_poller.lock().unwrap();
            poller.register_pane(&pane_target);
        }
        self.status_poller = Some(status_poller.clone());

        // Try control mode first; fallback to capture-pane polling on failure
        let force_capture_pane = std::env::var("PMUX_USE_CAPTURE_PANE")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        let handle = if force_capture_pane {
            eprintln!("pmux: PMUX_USE_CAPTURE_PANE=1, forcing capture-pane fallback");
            None
        } else {
            control_mode_attach(&session_name)
                .inspect_err(|e| eprintln!("pmux: control mode attach failed ({}), using capture-pane fallback", e))
                .ok()
        };
        match handle {
            Some(handle) => {
                let (cols, rows) = tmux_pane::get_pane_dimensions(&pane_target);
                if let Ok(mut buffers) = self.terminal_buffers.lock() {
                    buffers.clear();
                    buffers.insert(
                        pane_target.clone(),
                        TerminalBuffer::Term(Arc::new(Mutex::new(TermBridge::new(cols, rows)))),
                    );
                }
                let terminal_buffers = self.terminal_buffers.clone();
                let running = Arc::new(AtomicBool::new(true));
                self.control_mode_running = Some(running.clone());
                let _entity = cx.entity();
                cx.spawn(async move |entity, cx| {
                    while running.load(Ordering::Relaxed) {
                        let mut needs_notify = false;
                        while let Some((target, bytes)) = handle.try_recv() {
                            if let Ok(mut buffers) = terminal_buffers.lock() {
                                let (cols, rows) = tmux_pane::get_pane_dimensions(&target);
                                let term = buffers
                                    .entry(target.to_string())
                                    .or_insert_with(|| {
                                        TerminalBuffer::Term(Arc::new(Mutex::new(
                                            TermBridge::new(cols, rows),
                                        )))
                                    });
                                if let TerminalBuffer::Term(t) = term {
                                    if let Ok(guard) = t.lock() {
                                        guard.advance(&bytes);
                                        needs_notify = true;
                                    }
                                }
                            }
                        }
                        if needs_notify {
                            let _ = entity.update(cx, |_, cx| cx.notify());
                        }
                        cx.background_executor().timer(Duration::from_millis(16)).await;
                    }
                    let _ = handle.shutdown();
                })
                .detach();
            }
            None => {
                let terminal_buffers = self.terminal_buffers.clone();
                let pane_targets = self.pane_targets_shared.clone();
                if let Ok(mut buffers) = terminal_buffers.lock() {
                    buffers.clear();
                    buffers.insert(
                        pane_target.clone(),
                        TerminalBuffer::Legacy(Arc::new(Mutex::new(TerminalContent::new()))),
                    );
                }
                let terminal_buffers = self.terminal_buffers.clone();
                let _entity = cx.entity();
                cx.spawn(async move |entity, cx| {
                    loop {
                        let targets = pane_targets.lock().map(|g| g.clone()).unwrap_or_default();
                        let mut updated = false;
                        for target in &targets {
                            if let Ok(text) = tmux_pane::capture_pane(target) {
                                if let Ok(mut buffers) = terminal_buffers.lock() {
                                    if let Some(TerminalBuffer::Legacy(content)) = buffers.get_mut(target) {
                                        if let Ok(mut guard) = content.lock() {
                                            guard.update(&text);
                                            updated = true;
                                        }
                                    }
                                }
                            }
                        }
                        if updated {
                            let _ = entity.update(cx, |_, cx| cx.notify());
                        }
                        cx.background_executor().timer(Duration::from_millis(200)).await;
                    }
                })
                .detach();
            }
        }

        // Start status polling for this worktree
        let pane_statuses = self.pane_statuses.clone();
        let status_poller_for_polling = status_poller.clone();
        cx.spawn(async move |entity, cx| {
            loop {
                if let Ok(poller) = status_poller_for_polling.lock() {
                    let current_status = poller.get_status(&pane_target);
                    let mut updated = false;

                    if let Ok(mut statuses) = pane_statuses.lock() {
                        let previous = statuses.get(&pane_target);
                        if previous != Some(&current_status) {
                            statuses.insert(pane_target.clone(), current_status);
                            updated = true;
                        }
                    }

                    if updated {
                        let pane_target_for_notif = pane_target.clone();
                        let _ = entity.update(cx, |this, cx| {
                            this.update_status_counts();
                            if let Ok(statuses) = this.pane_statuses.lock() {
                                if let Some(&new_status) = statuses.get(&pane_target_for_notif) {
                                    if new_status.is_urgent() {
                                        let notif_type = match new_status {
                                            AgentStatus::Error => Some(NotificationType::Error),
                                            AgentStatus::Waiting => Some(NotificationType::Waiting),
                                            _ => None,
                                        };
                                        if let Some(nt) = notif_type {
                                            let message = new_status.display_text().to_string();
                                            if let Ok(mut mgr) = this.notification_manager.lock() {
                                                if mgr.add(&pane_target_for_notif, nt, &message) {
                                                    system_notifier::notify("pmux", &message, nt);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            cx.notify();
                        });
                    }
                }
                cx.background_executor().timer(Duration::from_millis(500)).await;
            }
        }).detach();

        // Start StatusPoller background thread
        if let Some(poller) = &self.status_poller {
            if let Ok(mut p) = poller.lock() {
                p.start();
            }
        }

        println!("Switched to worktree: {} (session: {})", worktree_path.display(), session_name);
    }

    /// Process pending worktree selection (called from render context)
    fn process_pending_worktree_selection(&mut self, cx: &mut Context<Self>) {
        if let Some(idx) = self.pending_worktree_selection.take() {
            // Get the current repo path and discover worktrees
            if let Some(tab) = self.workspace_manager.active_tab() {
                let repo_path = tab.path.clone();
                if let Ok(worktrees) = crate::worktree::discover_worktrees(&repo_path) {
                    if let Some(worktree) = worktrees.get(idx) {
                        let path = worktree.path.clone();
                        let branch = worktree.short_branch_name().to_string();
                        println!("Processing worktree selection: {} (branch: {})", path.display(), branch);
                        self.active_worktree_index = Some(idx);
                        self.switch_to_worktree(&path, &branch, cx);
                    }
                }
            }
        }
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

    /// Stop current tmux session and status polling
    /// Called when switching workspaces or cleaning up
    fn stop_current_session(&mut self) {
        // Signal control mode consumer loop to exit
        if let Some(running) = self.control_mode_running.take() {
            running.store(false, Ordering::Relaxed);
        }

        // Stop StatusPoller background thread
        if let Some(poller) = &self.status_poller {
            if let Ok(mut p) = poller.lock() {
                p.stop();
            }
        }
        self.status_poller = None;

        // Clear status tracking state
        if let Ok(mut statuses) = self.pane_statuses.lock() {
            statuses.clear();
        }
        self.status_counts = StatusCounts::new();

        // Clear input handler
        self.input_handler = None;
        self.active_pane_target = None;
    }

    /// Handle keyboard events
    fn handle_key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
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
                            let _ = tmux_pane::select_pane(&t);
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
                            let _ = tmux_pane::select_pane(&t);
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

        // Check for Cmd+key shortcuts (app shortcuts)
        if event.keystroke.modifiers.platform {
            match event.keystroke.key.as_str() {
                "b" => self.sidebar_visible = !self.sidebar_visible,
                "i" => self.show_notification_panel = !self.show_notification_panel,
                "d" => {
                    if event.keystroke.modifiers.shift {
                        self.handle_split_pane(false, cx); // horizontal
                    } else {
                        self.handle_split_pane(true, cx); // vertical
                    }
                    return;
                }
                "r" => {
                    if event.keystroke.modifiers.shift {
                        self.open_diff_view(cx);
                    }
                }
                "w" => {
                    if let Some((branch, window_name, _)) = self.diff_overlay_open.clone() {
                        self.close_diff_overlay(&branch, &window_name, cx);
                    }
                }
                "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" => {
                    if let Ok(idx) = event.keystroke.key.parse::<usize>() {
                        let idx = idx - 1; // 0-based
                        if idx < self.workspace_manager.tab_count() {
                            self.handle_workspace_tab_switch(idx, cx);
                        }
                    }
                }
                _ => {}
            }
            return; // Don't forward Cmd+key to tmux
        }

        // Forward all other keys to tmux via InputHandler
        // Use explicit pane target (session:window.pane) - session:window can fail in control mode
        let send_target = self.active_pane_target.as_deref();
        let key_name = event.keystroke.key.clone();
        match (&self.input_handler, send_target) {
            (Some(input_handler), Some(target)) => {
                if let Some((tmux_key, use_literal)) =
                    crate::input_handler::key_to_tmux(&key_name, false)
                {
                    if let Err(e) =
                        input_handler.send_key_to_target_with_literal(target, &tmux_key, use_literal)
                    {
                        eprintln!("pmux: send_key_to_target failed: {}", e);
                    } else {
                        eprintln!("pmux: key forwarded '{}' -> tmux target {}", key_name, target);
                    }
                }
            }
            _ => {
                eprintln!(
                    "pmux: key '{}' not forwarded (input_handler={} target={})",
                    key_name,
                    self.input_handler.is_some(),
                    send_target.unwrap_or("none")
                );
            }
        }
    }

    /// Handle split pane (⌘D vertical, ⌘⇧D horizontal)
    fn handle_split_pane(&mut self, vertical: bool, cx: &mut Context<Self>) {
        let Some(target) = self.split_tree.focus_index_to_pane_target(self.focused_pane_index) else {
            return;
        };
        let new_target = if vertical {
            tmux_pane::split_pane_vertical(&target)
        } else {
            tmux_pane::split_pane_horizontal(&target)
        };
        let Ok(new_target) = new_target else {
            return;
        };
        if let Some(new_tree) = self.split_tree.split_at_focused(
            self.focused_pane_index,
            vertical,
            new_target.clone(),
        ) {
            self.split_tree = new_tree;
            if let Ok(mut buffers) = self.terminal_buffers.lock() {
                let use_term = buffers.values().any(|b| matches!(b, TerminalBuffer::Term(_)));
                buffers.insert(
                    new_target.clone(),
                    if use_term {
                        let (cols, rows) = tmux_pane::get_pane_dimensions(&new_target);
                        TerminalBuffer::Term(Arc::new(Mutex::new(TermBridge::new(cols, rows))))
                    } else {
                        TerminalBuffer::Legacy(Arc::new(Mutex::new(TerminalContent::new())))
                    },
                );
            }
            if let Ok(mut guard) = self.pane_targets_shared.lock() {
                *guard = self.split_tree.flatten().into_iter().map(|(t, _)| t).collect();
            }
            if let Some(ref poller) = self.status_poller {
                if let Ok(mut p) = poller.lock() {
                    p.register_pane(&new_target);
                }
            }
            cx.notify();
        }
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

        let worktrees = match crate::worktree::discover_worktrees(&repo_path) {
            Ok(w) => w,
            Err(_) => return,
        };

        let idx = worktree_idx.unwrap_or(0);
        let worktree = match worktrees.get(idx) {
            Some(w) => w,
            None => return,
        };

        // Diff view only makes sense for non-main branches (main...HEAD is empty for main)
        if worktree.is_main {
            self.state.error_message = Some("Diff view is not available for the main branch.".to_string());
            cx.notify();
            return;
        }

        let branch = worktree.short_branch_name().to_string();
        let worktree_path = worktree.path.clone();

        let existing_window = self.review_windows.get(&branch).cloned();
        if let Some(window_name) = existing_window {
            self.open_diff_overlay(&branch, &window_name, cx);
            return;
        }

        if self.active_worktree_index != Some(idx) {
            self.switch_to_worktree(&worktree_path, &branch, cx);
        }

        let session_name = match &self.active_pane_target {
            Some(t) => t.split(':').next().unwrap_or("").to_string(),
            None => return,
        };

        let window_name = format!("review-{}", branch.replace('/', "-"));
        // Preload diffview module first (needed for lazy-loading plugin managers like lazy.nvim)
        let command = "nvim -c 'lua require(\"diffview\")' -c 'DiffviewOpen main...HEAD'";

        match tmux_window::create_window_with_command(&session_name, &window_name, &worktree_path, command) {
            Ok(_) => {
                self.review_windows.insert(branch.clone(), window_name.clone());
                self.open_diff_overlay(&branch, &window_name, cx);
            }
            Err(e) => {
                self.state.error_message = Some(format!("Failed to open diff view: {}", e));
            }
        }
        cx.notify();
    }

    /// Open diff overlay (add buffer, set pane target for polling, show overlay)
    fn open_diff_overlay(&mut self, branch: &str, window_name: &str, cx: &mut Context<Self>) {
        let session_name = self.active_pane_target
            .as_ref()
            .and_then(|t| t.split(':').next().map(String::from))
            .unwrap_or_else(|| "sdlc-workspace".to_string());

        let pane_target = format!("{}:{}.0", session_name, window_name);

        // Add buffer for overlay pane so capture-pane can populate it
        if let Ok(mut buffers) = self.terminal_buffers.lock() {
            buffers.entry(pane_target.clone()).or_insert_with(|| {
                TerminalBuffer::Legacy(Arc::new(Mutex::new(TerminalContent::new())))
            });
        }

        // Add to pane_targets_shared so the capture-pane loop polls this pane
        if let Ok(mut guard) = self.pane_targets_shared.lock() {
            if !guard.contains(&pane_target) {
                guard.push(pane_target.clone());
            }
        }

        self.active_pane_target = Some(pane_target.clone());
        self.diff_overlay_open = Some((branch.to_string(), window_name.to_string(), pane_target.clone()));
        if let Ok(mut guard) = self.active_pane_target_shared.lock() {
            *guard = pane_target;
        }

        cx.notify();
    }

    /// Close diff overlay (kill tmux window, remove from buffers, switch back to worktree)
    fn close_diff_overlay(&mut self, branch: &str, window_name: &str, cx: &mut Context<Self>) {
        let session_name = self.active_pane_target
            .as_ref()
            .and_then(|t| t.split(':').next().map(String::from))
            .unwrap_or_else(|| "sdlc-workspace".to_string());
        let target = format!("{}:{}", session_name, window_name);
        let pane_target = format!("{}:{}.0", session_name, window_name);

        let _ = tmux_window::kill_window(&target);
        self.review_windows.remove(branch);
        self.diff_overlay_open = None;

        // Remove from terminal_buffers and pane_targets_shared
        if let Ok(mut buffers) = self.terminal_buffers.lock() {
            buffers.remove(&pane_target);
        }
        if let Ok(mut guard) = self.pane_targets_shared.lock() {
            guard.retain(|t| t != &pane_target);
        }

        let worktree_path = self.workspace_manager.active_tab()
            .map(|t| t.path.clone())
            .unwrap_or_else(|| PathBuf::from("."));
        if let Some(idx) = self.active_worktree_index {
            if let Ok(worktrees) = crate::worktree::discover_worktrees(&worktree_path) {
                if let Some(wt) = worktrees.get(idx) {
                    let path = wt.path.clone();
                    let br = wt.short_branch_name().to_string();
                    self.switch_to_worktree(&path, &br, cx);
                }
            }
        }
        cx.notify();
    }

    /// Opens the new branch dialog
    fn open_new_branch_dialog(&mut self, cx: &mut Context<Self>) {
        self.new_branch_dialog.open();
        cx.notify();
    }

    /// Closes the new branch dialog
    fn close_new_branch_dialog(&mut self, cx: &mut Context<Self>) {
        self.new_branch_dialog.close();
        self.terminal_needs_focus = true;
        cx.notify();
    }

    /// Creates a new branch and worktree
    fn create_branch(&mut self, cx: &mut Context<Self>) {
        let branch_name = self.new_branch_dialog.branch_name().to_string();
        
        if branch_name.trim().is_empty() {
            return;
        }

        let repo_path = self.workspace_manager.active_tab()
            .map(|t| t.path.clone())
            .unwrap_or_else(|| std::path::PathBuf::from("."));

        self.new_branch_dialog.start_creating();
        cx.notify();

        // Create worktree in background
        let repo_path_clone = repo_path.clone();
        let branch_name_clone = branch_name.clone();

        let notification_manager = self.notification_manager.clone();
        cx.spawn(async move |entity, cx| {
            let sender = Arc::new(Mutex::new(AppNotificationSender {
                manager: notification_manager,
            }));
            let orchestrator = NewBranchOrchestrator::new(repo_path_clone.clone())
                .with_notification_sender(sender);
            let result = orchestrator.create_branch_async(&branch_name_clone).await;

            let _ = entity.update(cx, |this: &mut AppRoot, cx| {
                match result {
                    CreationResult::Success { worktree_path, branch_name: _ } => {
                        this.new_branch_dialog.complete_creating(true);
                        // Refresh sidebar
                        this.refresh_sidebar(cx);
                        println!("Successfully created worktree at: {:?}", worktree_path);
                    }
                    CreationResult::ValidationFailed { error } => {
                        this.new_branch_dialog.set_error(&error);
                        this.new_branch_dialog.complete_creating(false);
                    }
                    CreationResult::BranchExists { branch_name } => {
                        this.new_branch_dialog.set_error(&format!("Branch '{}' already exists", branch_name));
                        this.new_branch_dialog.complete_creating(false);
                    }
                    CreationResult::GitFailed { error } => {
                        this.new_branch_dialog.set_error(&format!("Git error: {}", error));
                        this.new_branch_dialog.complete_creating(false);
                    }
                    CreationResult::TmuxFailed { worktree_path: _, branch_name: _, error } => {
                        this.new_branch_dialog.set_error(&format!("Tmux error: {}", error));
                        this.new_branch_dialog.complete_creating(false);
                    }
                }
                cx.notify();
            });
        }).detach();
    }

    /// Refreshes the sidebar to show updated worktrees
    fn refresh_sidebar(&mut self, cx: &mut Context<Self>) {
        // The sidebar will refresh on next render
        cx.notify();
    }

    /// Shows the delete worktree confirmation dialog
    fn show_delete_dialog(&mut self, worktree: crate::worktree::WorktreeInfo, cx: &mut Context<Self>) {
        let has_uncommitted = crate::worktree::has_uncommitted_changes(&worktree.path);
        self.delete_worktree_dialog.open(worktree, has_uncommitted);
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

        let repo_name = self.workspace_manager.active_tab()
            .map(|t| t.name.clone())
            .unwrap_or_else(|| "workspace".to_string());
        let session_name = format!("sdlc-{}", repo_name.replace('/', "-").replace(' ', "-").replace('\\', "-"));
        let window_name = branch.replace('/', "-");
        let target = format!("{}:{}", session_name, window_name);

        if let Err(e) = tmux_window::kill_window(&target) {
            eprintln!("tmux kill-window failed (best-effort): {}", e);
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
                if let Ok(worktrees) = crate::worktree::discover_worktrees(&repo_path) {
                    if worktrees.is_empty() {
                        self.active_worktree_index = None;
                        self.stop_current_session();
                    } else {
                        self.active_worktree_index = Some(0);
                        if let Some(wt) = worktrees.first() {
                            let path = wt.path.clone();
                            let branch = wt.short_branch_name().to_string();
                            self.switch_to_worktree(&path, &branch, cx);
                        }
                    }
                }
            }
            Err(e) => {
                self.delete_worktree_dialog.set_error(&e.to_string());
            }
        }
        cx.notify();
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
        let show_notifications = self.show_notification_panel;
        let workspace_manager = self.workspace_manager.clone();
        let terminal_buffers = self.terminal_buffers.lock()
            .map(|g| g.clone())
            .unwrap_or_default();
        let split_tree = self.split_tree.clone();
        let focused_pane_index = self.focused_pane_index;
        let split_divider_drag = self.split_divider_drag.clone();
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

        let notification_unread = self.notification_manager.lock().map(|m| m.unread_count()).unwrap_or(0);
        let app_root_entity_for_toggle = app_root_entity.clone();
        let app_root_entity_for_notif = app_root_entity.clone();
        let app_root_entity_for_add_ws = app_root_entity.clone();

        // Create sidebar with callbacks (cmux style: top controls in sidebar)
        let mut sidebar = Sidebar::new(&repo_name, repo_path.clone())
            .with_statuses(pane_statuses.clone())
            .with_context_menu(self.sidebar_context_menu_index)
            .on_toggle_sidebar(move |_window, cx| {
                let _ = cx.update_entity(&app_root_entity_for_toggle, |this: &mut AppRoot, cx| {
                    this.sidebar_visible = !this.sidebar_visible;
                    cx.notify();
                });
            })
            .on_toggle_notifications(move |_window, cx| {
                let _ = cx.update_entity(&app_root_entity_for_notif, |this: &mut AppRoot, cx| {
                    this.show_notification_panel = !this.show_notification_panel;
                    cx.notify();
                });
            })
            .on_add_workspace(move |_window, cx| {
                let _ = cx.update_entity(&app_root_entity_for_add_ws, |this: &mut AppRoot, cx| {
                    this.handle_add_workspace(cx);
                });
            })
            .with_notification_count(notification_unread);

        // Load worktrees from git and sync Sidebar selection with active worktree
        let worktrees = crate::worktree::discover_worktrees(&repo_path).unwrap_or_default();
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

        // Set up select callback
        let app_root_entity_for_sidebar = app_root_entity.clone();
        sidebar.on_select(move |idx: usize, _window: &mut Window, cx: &mut App| {
            let _ = cx.update_entity(&app_root_entity_for_sidebar, |this: &mut AppRoot, cx| {
                this.pending_worktree_selection = Some(idx);
                this.process_pending_worktree_selection(cx);
                cx.notify();
            });
        });

        // Focus handle for the new branch dialog input - created here so we can focus it when dialog opens
        let input_focus = cx.focus_handle();
        let input_focus_for_sidebar = input_focus.clone();

        // Set up New Branch callback - opens the dialog
        // Get Entity from window at click time (not from cx.entity() at render time) -
        // the latter can be invalid when click originates from inside Sidebar Component
        sidebar.on_new_branch(move |window, cx| {
            if let Some(Some(root)) = window.root::<AppRoot>() {
                let _ = cx.update_entity(&root, |this: &mut AppRoot, cx| {
                    this.open_new_branch_dialog(cx);
                });
                // Focus input on next frame (after dialog is rendered)
                let focus = input_focus_for_sidebar.clone();
                window.on_next_frame(move |window, cx| {
                    window.focus(&focus, cx);
                });
            }
        });

        let app_root_entity_for_delete = app_root_entity.clone();
        let app_root_entity_for_view_diff = app_root_entity.clone();
        let app_root_entity_for_right_click = app_root_entity.clone();
        let app_root_entity_for_clear_menu = app_root_entity.clone();
        let repo_path_for_delete = repo_path.clone();
        sidebar.on_delete(move |idx, _window, cx| {
            let _ = cx.update_entity(&app_root_entity_for_delete, |this: &mut AppRoot, cx| {
                this.sidebar_context_menu_index = None;
                if let Ok(worktrees) = crate::worktree::discover_worktrees(&repo_path_for_delete) {
                    if let Some(wt) = worktrees.get(idx) {
                        this.show_delete_dialog(wt.clone(), cx);
                    }
                }
            });
        });
        sidebar.on_view_diff(move |idx, _window, cx| {
            let _ = cx.update_entity(&app_root_entity_for_view_diff, |this: &mut AppRoot, cx| {
                this.sidebar_context_menu_index = None;
                this.open_diff_view_for_worktree(Some(idx), cx);
            });
        });
        sidebar.on_right_click(move |idx, _window, cx| {
            let _ = cx.update_entity(&app_root_entity_for_right_click, |this: &mut AppRoot, cx| {
                this.sidebar_context_menu_index = Some(idx);
                cx.notify();
            });
        });

        // Create dialog with callbacks - use window.root() for Create (same as New Branch) so it works when click originates from dialog
        let app_root_entity_for_close = app_root_entity.clone();
        let app_root_entity_for_input = app_root_entity.clone();
        let new_branch_dialog = NewBranchDialogUi::new()
            .with_focus_handle(input_focus.clone())
            .on_create(move |window, cx| {
                if let Some(Some(root)) = window.root::<AppRoot>() {
                    let _ = cx.update_entity(&root, |this: &mut AppRoot, cx| {
                        this.create_branch(cx);
                    });
                }
            })
            .on_close(move |_window, cx| {
                let _ = cx.update_entity(&app_root_entity_for_close, |this: &mut AppRoot, cx| {
                    this.close_new_branch_dialog(cx);
                });
            })
            .on_branch_name_change(move |new_value, _window, cx| {
                let _ = cx.update_entity(&app_root_entity_for_input, |this: &mut AppRoot, cx| {
                    this.new_branch_dialog.set_branch_name(&new_value);
                    this.new_branch_dialog.validate();
                    cx.notify();
                });
            });

        // Apply current dialog state
        let mut new_branch_dialog = new_branch_dialog;
        if self.new_branch_dialog.is_open() {
            new_branch_dialog.open();
        }
        new_branch_dialog.set_branch_name(self.new_branch_dialog.branch_name());
        if self.new_branch_dialog.has_error() {
            new_branch_dialog.set_error(self.new_branch_dialog.error_message());
        }
        if self.new_branch_dialog.is_creating() {
            new_branch_dialog.start_creating();
        }

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

        div()
            .id("workspace-view")
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0x1e1e1e))
            .relative()
            .when(self.sidebar_context_menu_index.is_some(), |el| {
                let app_root_entity_for_overlay = app_root_entity_for_clear_menu.clone();
                el.child(
                    div()
                        .id("context-menu-overlay")
                        .absolute()
                        .inset(px(0.))
                        .size_full()
                        .cursor_pointer()
                        .on_click(move |_event, _window, cx| {
                            let _ = cx.update_entity(&app_root_entity_for_overlay, |this: &mut AppRoot, cx| {
                                this.sidebar_context_menu_index = None;
                                cx.notify();
                            });
                        })
                )
            })
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
                                .h_full()
                                .child(sidebar)
                        )
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .flex()
                            .flex_col()
                            .overflow_hidden()
                            .child({
                                let app_root_entity_for_ws_select = app_root_entity.clone();
                                let app_root_entity_for_ws_close = app_root_entity.clone();
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
                                            if let Some(path) = closed_path {
                                                this.per_repo_worktree_index.remove(&path);
                                            }
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
                            })
                            .child({
                                let app_root_entity_for_ratio = app_root_entity.clone();
                                let app_root_entity_for_drag = app_root_entity.clone();
                                let app_root_entity_for_drag_end = app_root_entity.clone();
                                let app_root_entity_for_pane_click = app_root_entity.clone();
                                let terminal_focus_for_click = terminal_focus.clone();
                                div()
                                    .flex_1()
                                    .min_h_0()
                                    .overflow_hidden()
                                    .cursor(gpui::CursorStyle::IBeam)
                                    .on_mouse_down(gpui::MouseButton::Left, move |_event, window, cx| {
                                        window.focus(&terminal_focus_for_click, cx);
                                    })
                                    .child(
                                        SplitPaneContainer::new(
                                            split_tree,
                                            terminal_buffers.clone(),
                                            focused_pane_index,
                                            &repo_name,
                                        )
                                        .with_cursor_blink_visible(cursor_blink_visible)
                                        .with_drag_state(split_divider_drag)
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
                                        .on_pane_click(move |pane_idx, _window, cx| {
                                            let _ = cx.update_entity(&app_root_entity_for_pane_click, |this: &mut AppRoot, cx| {
                                                this.focused_pane_index = pane_idx;
                                                if let Some(target) = this.split_tree.focus_index_to_pane_target(pane_idx) {
                                                    let _ = tmux_pane::select_pane(&target);
                                                    this.active_pane_target = Some(target.clone());
                                                    if let Ok(mut guard) = this.active_pane_target_shared.lock() {
                                                        *guard = target;
                                                    }
                                                }
                                                cx.notify();
                                            });
                                        })
                                    )
                            })
                    )
            )
            .child({
                let worktree_branch = self.workspace_manager.active_tab()
                    .and_then(|t| crate::worktree::discover_worktrees(&t.path).ok())
                    .and_then(|wts| {
                        let idx = self.active_worktree_index?;
                        wts.get(idx).map(|w| w.short_branch_name().to_string())
                    });
                StatusBar::from_context(
                    worktree_branch.as_deref(),
                    self.split_tree.pane_count(),
                    self.focused_pane_index,
                    &self.status_counts,
                )
            })
            .when(show_notifications, |el: Stateful<Div>| {
                let notification_items: Vec<NotificationItem> = self
                    .notification_manager
                    .lock()
                    .map(|m| {
                        m.recent(100)
                            .iter()
                            .enumerate()
                            .map(|(i, n)| NotificationItem::from_notification(n, i))
                            .collect()
                    })
                    .unwrap_or_default();
                let app_root_entity_for_close = app_root_entity.clone();
                let app_root_entity_for_clear = app_root_entity.clone();
                let app_root_entity_for_read = app_root_entity.clone();
                el.child(
                    NotificationPanel::new()
                        .with_notifications(notification_items)
                        .with_visible(true)
                        .on_close(move |_window, cx| {
                            let _ = cx.update_entity(&app_root_entity_for_close, |this: &mut AppRoot, cx| {
                                this.show_notification_panel = false;
                                cx.notify();
                            });
                        })
                        .on_clear_all(move |_window, cx| {
                            let _ = cx.update_entity(&app_root_entity_for_clear, |this: &mut AppRoot, cx| {
                                if let Ok(mut mgr) = this.notification_manager.lock() {
                                    mgr.clear_all();
                                }
                                cx.notify();
                            });
                        })
                        .on_mark_read(move |id, _window, cx| {
                            let _ = cx.update_entity(&app_root_entity_for_read, |this: &mut AppRoot, cx| {
                                if let Ok(mut mgr) = this.notification_manager.lock() {
                                    mgr.mark_read(id);
                                }
                                cx.notify();
                            });
                        })
                )
            })
            // Dialogs rendered last so they appear on top (absolute overlay)
            .child(delete_dialog)
            .child(new_branch_dialog)
            .when(self.diff_overlay_open.is_some(), |el| {
                if let Some((branch, window_name, pane_target)) = &self.diff_overlay_open {
                    let buffer = terminal_buffers.get(pane_target).cloned().unwrap_or_else(|| {
                        TerminalBuffer::Legacy(Arc::new(Mutex::new(TerminalContent::new())))
                    });
                    let branch = branch.clone();
                    let window_name = window_name.clone();
                    let app_root_entity_for_diff_close = app_root_entity.clone();
                    el.child(
                        DiffOverlay::new(&branch, pane_target, buffer)
                            .on_close(move |_window, cx| {
                                let _ = cx.update_entity(&app_root_entity_for_diff_close, |this: &mut AppRoot, cx| {
                                    this.close_diff_overlay(&branch, &window_name, cx);
                                });
                            })
                    )
                } else {
                    el
                }
            })
    }
}

impl Render for AppRoot {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Start cursor blink timer once when workspace is shown
        if self.has_workspaces() && !self.cursor_blink_timer_started {
            self.cursor_blink_timer_started = true;
            cx.spawn(async move |entity, cx| {
                loop {
                    cx.background_executor().timer(Duration::from_millis(530)).await;
                    let _ = entity.update(cx, |this: &mut AppRoot, cx| {
                        this.cursor_blink_tick = this.cursor_blink_tick.wrapping_add(1);
                        cx.notify();
                    });
                }
            }).detach();
        }

        let terminal_focus = self.terminal_focus.get_or_insert_with(|| cx.focus_handle()).clone();

        // Auto-focus terminal when workspace loads so keyboard input works without clicking
        if self.has_workspaces() && self.terminal_needs_focus {
            self.terminal_needs_focus = false;
            let terminal_focus_for_frame = terminal_focus.clone();
            window.on_next_frame(move |window, cx| {
                window.focus(&terminal_focus_for_frame, cx);
            });
        }

        let cursor_blink_visible = self.cursor_blink_tick % 2 == 0;
        div()
            .id("app-root")
            .size_full()
            .bg(rgb(0x1e1e1e))
            .text_color(rgb(0xcccccc))
            .font_family(".SystemUIFont")
            .focusable()
            .track_focus(&terminal_focus)
            .on_key_down(cx.listener(|this, event, window, cx| {
                this.handle_key_down(event, window, cx);
            }))
            .child(
                if let Some(ref deps) = self.dependency_check {
                    self.render_dependency_check_page(deps, cx).into_any_element()
                } else if self.has_workspaces() {
                    self.render_workspace_view(cx, &terminal_focus, cursor_blink_visible).into_any_element()
                } else {
                    self.render_startup_page(cx).into_any_element()
                },
            )
    }
}

impl Default for AppRoot {
    fn default() -> Self {
        Self::new()
    }
}
