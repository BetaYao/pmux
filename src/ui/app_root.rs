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
use crate::runtime::backends::{create_runtime_from_env, recover_runtime, resolve_backend, session_name_for_workspace, window_name_for_worktree, window_target};
use crate::runtime::{RuntimeState, WorktreeState};
use crate::ui::{AppState, sidebar::Sidebar, workspace_tabbar::WorkspaceTabBar, terminal_controller::ResizeController, terminal_view::TerminalBuffer, terminal_area_entity::TerminalAreaEntity, notification_panel_entity::NotificationPanelEntity, new_branch_dialog_entity::NewBranchDialogEntity, delete_worktree_dialog_ui::DeleteWorktreeDialogUi, split_pane_container::SplitPaneContainer, diff_overlay::DiffOverlay, status_bar::StatusBar, models::{StatusCountsModel, NotificationPanelModel, NewBranchDialogModel}, topbar_entity::TopBarEntity};
use crate::split_tree::SplitNode;
use crate::workspace_manager::WorkspaceManager;
use crate::input::{key_to_xterm_escape, KeyModifiers};
use crate::window_state::PersistentAppState;
use crate::new_branch_orchestrator::{NewBranchOrchestrator, CreationResult, NotificationSender};
use crate::notification::Notification;
use gpui::prelude::FluentBuilder;
use gpui::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// When true, AppRoot will set show_settings=true and clear this flag at start of render.
/// Used by menu action (open_settings) to open Settings from main.rs without window access.
pub static OPEN_SETTINGS_REQUESTED: AtomicBool = AtomicBool::new(false);

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
    /// Whether EventBus subscription has been started (spawn once)
    event_bus_subscription_started: bool,
    /// NewBranchDialogModel + Entity - dialog state; Entity observes, re-renders only when model notifies
    new_branch_dialog_model: Option<Entity<NewBranchDialogModel>>,
    new_branch_dialog_entity: Option<Entity<NewBranchDialogEntity>>,
    /// Focus handle for new branch dialog input (focus on open)
    dialog_input_focus: Option<FocusHandle>,
    /// Delete worktree confirmation dialog
    delete_worktree_dialog: DeleteWorktreeDialogUi,
    /// Pending worktree selection to be processed on next render
    pending_worktree_selection: Option<usize>,
    /// When Some(idx): switching to worktree idx, show loading in terminal area
    worktree_switch_loading: Option<usize>,
    /// Current active worktree index (synced with Sidebar/TabBar)
    active_worktree_index: Option<usize>,
    /// Per-repo active worktree index for restoring state when switching workspace tabs
    per_repo_worktree_index: HashMap<PathBuf, usize>,
    /// Cached worktrees for active repo. Refreshed on workspace change, branch create/delete, explicit refresh.
    /// Avoids calling discover_worktrees in render path.
    cached_worktrees: Vec<crate::worktree::WorktreeInfo>,
    /// Repo path for which cached_worktrees is valid
    cached_worktrees_repo: Option<PathBuf>,
    /// Sidebar context menu: which worktree index has menu open
    sidebar_context_menu_index: Option<usize>,
    /// Review windows: branch -> window name (stub for local PTY)
    review_windows: HashMap<String, String>,
    /// When Some, diff overlay is shown: (branch, window_name, session, pane_target)
    diff_overlay_open: Option<(String, String, Option<String>, String)>,
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
    /// When true, search bar is visible and keyboard input appends to search_query
    search_active: bool,
    /// Current search query (when search_active)
    search_query: String,
    /// Index of current match when cycling (Enter/Cmd+G)
    search_current_match: usize,
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
        event_bus_subscription_started: false,
        new_branch_dialog_model: None,
        new_branch_dialog_entity: None,
        dialog_input_focus: None,
        delete_worktree_dialog: DeleteWorktreeDialogUi::new(),
            pending_worktree_selection: None,
            worktree_switch_loading: None,
            active_worktree_index: None,
            per_repo_worktree_index,
            cached_worktrees: Vec::new(),
            cached_worktrees_repo: None,
            sidebar_context_menu_index: None,
            review_windows: HashMap::new(),
            diff_overlay_open: None,
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
            status_counts_model: None,
            topbar_entity: None,
            notification_panel_model: None,
            notification_panel_entity: None,
            terminal_area_entity: None,
            search_active: false,
            search_query: String::new(),
            search_current_match: 0,
        }
    }

    /// Create StatusCountsModel and TopBarEntity when has_workspaces (Phase 0 spike).
    /// Called from init_workspace_restoration before attach_runtime so EventBus handler can use model.
    fn ensure_entities(&mut self, cx: &mut Context<Self>) {
        if self.dialog_input_focus.is_none() {
            self.dialog_input_focus = Some(cx.focus_handle());
        }
        if !self.has_workspaces() {
            return;
        }
        if self.status_counts_model.is_none() {
            let pane_statuses = Arc::clone(&self.pane_statuses);
            let model = cx.new(move |_cx| StatusCountsModel::new(pane_statuses));
            self.status_counts_model = Some(model);
        }
        if self.topbar_entity.is_none() {
            if let Some(ref model) = self.status_counts_model {
                let workspace_manager = self.workspace_manager.clone();
                let app_root_entity = cx.entity();
                let app_root_entity_select = app_root_entity.clone();
                let on_select = Arc::new(move |idx: usize, _w: &mut Window, cx: &mut App| {
                    let _ = cx.update_entity(&app_root_entity_select, |this: &mut AppRoot, cx| {
                        this.handle_workspace_tab_switch(idx, cx);
                        if let Some(ref e) = this.topbar_entity {
                            let _ = cx.update_entity(e, |t: &mut TopBarEntity, cx| {
                                t.set_workspace_manager(this.workspace_manager.clone());
                                cx.notify();
                            });
                        }
                    });
                });
                let app_root_entity_close = app_root_entity.clone();
                let on_close = Arc::new(move |idx: usize, _w: &mut Window, cx: &mut App| {
                    let _ = cx.update_entity(&app_root_entity_close, |this: &mut AppRoot, cx| {
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
                        if let Some(ref e) = this.topbar_entity {
                            let _ = cx.update_entity(e, |t: &mut TopBarEntity, cx| {
                                t.set_workspace_manager(this.workspace_manager.clone());
                                cx.notify();
                            });
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
                let entity = cx.new(move |cx| {
                    NotificationPanelEntity::new(
                        model,
                        notif_mgr,
                        on_close,
                        on_mark_read,
                        on_clear_all,
                        on_jump_to_pane,
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
                let on_create = {
                    let model = model.clone();
                    Arc::new(move |_window: &mut Window, cx: &mut App| {
                        let branch_name = model.read(cx).branch_name.clone();
                        if branch_name.trim().is_empty() {
                            return;
                        }
                        let _ = cx.update_entity(&model, |m: &mut NewBranchDialogModel, cx| {
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
                            this.terminal_needs_focus = true;
                            cx.notify();
                        });
                    }) as Arc<dyn Fn(&mut Window, &mut App) + Send + Sync>
                };
                let on_branch_name_change = {
                    let model = model.clone();
                    Arc::new(move |new_value: String, _window: &mut Window, cx: &mut App| {
                        let _ = cx.update_entity(&model, |m: &mut NewBranchDialogModel, cx| {
                            m.set_branch_name(&new_value);
                            m.validate();
                            cx.notify();
                        });
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
                self.new_branch_dialog_entity = Some(entity);
            }
        }
    }

    /// Initialize workspace restoration (call after AppRoot is created)
    /// Ensures all tmux sessions exist, attaches to active tab, restores per-repo worktree selection
    pub fn init_workspace_restoration(&mut self, cx: &mut Context<Self>) {
        self.ensure_entities(cx);
        if self.terminal_focus.is_none() {
            self.terminal_focus = Some(cx.focus_handle());
        }

        let repo_path = self.workspace_manager.active_tab().map(|t| t.path.clone());

        if let Some(path) = repo_path {
            self.refresh_worktrees_for_repo(&path);
            let worktrees = &self.cached_worktrees;

            let restored_idx = self.per_repo_worktree_index.get(&path).copied();
            if let Some(awi) = restored_idx {
                if awi < worktrees.len() {
                    self.active_worktree_index = Some(awi);
                    if let Some(wt) = worktrees.get(awi) {
                        let wt_path = wt.path.clone();
                        let branch = wt.short_branch_name().to_string();
                        self.schedule_switch_to_worktree_async(&path, &wt_path, &branch, awi, cx);
                        return;
                    }
                }
            }

            self.active_worktree_index = None;
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

    fn setup_local_terminal(
        &mut self,
        runtime: Arc<dyn AgentRuntime>,
        pane_target: &str,
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
        let _ = runtime.resize(&pane_target_str, cols, rows);

        // Check if pane is now at the correct size after the resize attempts
        let post_resize_dims = runtime.get_pane_dimensions(&pane_target_str);
        let resize_succeeded = post_resize_dims == (cols, rows);

        if !resize_succeeded {
            runtime.set_skip_initial_capture();
        }

        // #region agent log
        crate::debug_log::dbg_session_log(
            "app_root.rs:setup_local_terminal",
            "pre-subscribe state",
            &serde_json::json!({
                "dims_match": dims_match,
                "skip_capture": !resize_succeeded,
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

            let terminal = Arc::new(Terminal::new(
                pane_target_str.clone(),
                TerminalSize {
                    cols: cols as u16,
                    rows: rows as u16,
                    cell_width: 8.0,
                    cell_height: 16.0,
                },
            ));

            // Forward PTY write-back (terminal sequences like OSC response that need to go back to PTY)
            let pty_write_rx = terminal.pty_write_rx.clone();
            let runtime_for_pty = runtime.clone();
            let pane_for_pty = pane_target_str.clone();
            std::thread::spawn(move || {
                while let Ok(data) = pty_write_rx.recv() {
                    let _ = runtime_for_pty.send_input(&pane_for_pty, &data);
                }
            });

            let runtime_for_resize = runtime.clone();
            let pane_for_resize = pane_target_str.clone();
            let shared_dims_for_resize = Arc::clone(&self.shared_terminal_dims);
            let resize_callback: Arc<dyn Fn(u16, u16) + Send + Sync> = Arc::new(move |cols, rows| {
                // #region agent log
                crate::debug_log::dbg_session_log(
                    "app_root.rs:resize_callback(setup_local)",
                    "PTY resize fired",
                    &serde_json::json!({"cols": cols, "rows": rows}),
                    "H15",
                );
                // #endregion
                let _ = runtime_for_resize.resize(&pane_for_resize, cols, rows);
                if let Ok(mut dims) = shared_dims_for_resize.lock() {
                    *dims = Some((cols, rows));
                }
                if let Ok(mut cfg) = Config::load() {
                    cfg.last_terminal_cols = Some(cols);
                    cfg.last_terminal_rows = Some(rows);
                    let _ = cfg.save();
                }
            });

            // Feed existing pane content (prompt, history) into the Terminal before streaming
            if let Some(initial) = runtime.capture_initial_content(&pane_target_str) {
                if !initial.is_empty() {
                    terminal.process_output(&initial);
                }
            }

            let focus_handle = self.terminal_focus.get_or_insert_with(|| cx.focus_handle()).clone();
            if let Ok(mut buffers) = self.terminal_buffers.lock() {
                buffers.clear();
                buffers.insert(
                    pane_target_str.clone(),
                    TerminalBuffer::Terminal {
                        terminal: terminal.clone(),
                        focus_handle: focus_handle.clone(),
                        resize_callback: Some(resize_callback),
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
            let terminal_for_output = terminal.clone();
            let term_area_entity = self.terminal_area_entity.clone();
            let mut ext = ContentExtractor::new();

            cx.spawn(async move |entity, cx| {
                loop {
                    let chunk = match rx.recv_async().await {
                        Ok(c) => c,
                        Err(_) => break,
                    };
                    terminal_for_output.process_output(&chunk);
                    ext.feed(&chunk);
                    let shell_info = ShellPhaseInfo {
                        phase: ext.shell_phase(),
                        last_post_exec_exit_code: None,
                    };
                    let content_str = ext.take_content().0;
                    if let Some(ref pub_) = status_publisher {
                        let _ = pub_.check_status(
                            &pane_target_clone,
                            crate::status_detector::ProcessStatus::Running,
                            Some(shell_info),
                            &content_str,
                        );
                    }
                    let _ = entity.update(cx, |_, cx| cx.notify());
                    if let Some(ref tae) = term_area_entity {
                        let _ = cx.update_entity(tae, |_, cx| cx.notify());
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
        _terminal_area_entity: Option<Entity<TerminalAreaEntity>>,
        cx: &mut Context<Self>,
    ) {
        let pane_target_str = pane_target.to_string();
        let (cols, rows) = runtime.get_pane_dimensions(&pane_target_str);

        if let Some(rx) = runtime.subscribe_output(&pane_target_str) {
            use crate::terminal::{Terminal, TerminalSize};

            let terminal = Arc::new(Terminal::new(
                pane_target_str.clone(),
                TerminalSize {
                    cols: cols as u16,
                    rows: rows as u16,
                    cell_width: 8.0,
                    cell_height: 16.0,
                },
            ));

            let pty_write_rx = terminal.pty_write_rx.clone();
            let runtime_for_pty = runtime.clone();
            let pane_for_pty = pane_target_str.clone();
            std::thread::spawn(move || {
                while let Ok(data) = pty_write_rx.recv() {
                    let _ = runtime_for_pty.send_input(&pane_for_pty, &data);
                }
            });

            let runtime_for_resize = runtime.clone();
            let pane_for_resize = pane_target_str.clone();
            let resize_callback: Arc<dyn Fn(u16, u16) + Send + Sync> =
                Arc::new(move |cols, rows| {
                    let _ = runtime_for_resize.resize(&pane_for_resize, cols, rows);
                });

            // Feed existing pane content into the Terminal before streaming
            if let Some(initial) = runtime.capture_initial_content(&pane_target_str) {
                if !initial.is_empty() {
                    terminal.process_output(&initial);
                }
            }

            let focus_handle = self.terminal_focus.get_or_insert_with(|| cx.focus_handle()).clone();
            if let Ok(mut buffers) = self.terminal_buffers.lock() {
                buffers.insert(
                    pane_target_str.clone(),
                    TerminalBuffer::Terminal {
                        terminal: terminal.clone(),
                        focus_handle: focus_handle.clone(),
                        resize_callback: Some(resize_callback),
                    },
                );
            }

            let status_publisher = self.status_publisher.clone();
            let pane_target_clone = pane_target_str.clone();
            let terminal_for_output = terminal.clone();
            let term_area_entity = self.terminal_area_entity.clone();
            let mut ext = ContentExtractor::new();

            cx.spawn(async move |entity, cx| {
                loop {
                    let chunk = match rx.recv_async().await {
                        Ok(c) => c,
                        Err(_) => break,
                    };
                    terminal_for_output.process_output(&chunk);
                    ext.feed(&chunk);
                    let shell_info = ShellPhaseInfo {
                        phase: ext.shell_phase(),
                        last_post_exec_exit_code: None,
                    };
                    let content_str = ext.take_content().0;
                    if let Some(ref pub_) = status_publisher {
                        let _ = pub_.check_status(
                            &pane_target_clone,
                            crate::status_detector::ProcessStatus::Running,
                            Some(shell_info),
                            &content_str,
                        );
                    }
                    let _ = entity.update(cx, |_, cx| cx.notify());
                    if let Some(ref tae) = term_area_entity {
                        let _ = cx.update_entity(tae, |_, cx| cx.notify());
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

        let (split_tree, pane_targets): (SplitNode, Vec<String>) = match saved_split_tree {
            Some(tree) if tree.pane_count() > 1 => {
                let targets: Vec<String> = tree.flatten().into_iter().map(|(t, _)| t).collect();
                (tree, targets)
            }
            _ => {
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

        let status_publisher = StatusPublisher::new(Arc::clone(&self.event_bus));
        for pt in &pane_targets {
            status_publisher.register_pane(pt);
        }
        self.status_publisher = Some(status_publisher);

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
        let on_drag_start = Arc::new(move |path: Vec<bool>, pos: f32, ratio: f32, vert: bool, _w: &mut Window, cx: &mut App| {
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
            let _ = cx.update_entity(&app_root_for_drag_end, |this: &mut AppRoot, cx| {
                this.split_divider_drag = None;
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
            self.setup_local_terminal(runtime, &pane_targets[0], term_entity_for_setup, cx);
        } else {
            if let Ok(mut buffers) = self.terminal_buffers.lock() {
                buffers.clear();
            }
            for pt in &pane_targets {
                self.setup_pane_terminal_output(runtime.clone(), pt, self.terminal_area_entity.clone(), cx);
            }
        }

        if let Some(tab) = self.workspace_manager.active_tab() {
            let wp = tab.path.clone();
            self.save_runtime_state(&wp, worktree_path, branch_name);
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
                        if let Some(ref e) = this.topbar_entity {
                            let _ = cx.update_entity(e, |t: &mut TopBarEntity, cx| {
                                t.set_workspace_manager(this.workspace_manager.clone());
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
            self.refresh_worktrees_for_repo(&repo_path);
            let worktrees = &self.cached_worktrees;

            // Restore active_worktree_index for this repo
            let restored_idx = self.per_repo_worktree_index.get(&repo_path).copied();

            let (wt_path, branch, worktree_idx) = if worktrees.is_empty() {
                self.schedule_start_main_session(&repo_path, cx);
                return;
            } else if let Some(awi) = restored_idx {
                if awi < worktrees.len() {
                    let wt = &worktrees[awi];
                    self.active_worktree_index = Some(awi);
                    (wt.path.clone(), wt.short_branch_name().to_string(), awi)
                } else {
                    let wt = &worktrees[0];
                    self.active_worktree_index = Some(0);
                    (wt.path.clone(), wt.short_branch_name().to_string(), 0)
                }
            } else {
                let wt = &worktrees[0];
                self.active_worktree_index = Some(0);
                (wt.path.clone(), wt.short_branch_name().to_string(), 0)
            };

            self.schedule_switch_to_worktree_async(&repo_path, &wt_path, &branch, worktree_idx, cx);
        }
        cx.notify();
    }

    /// Start tmux session for the currently active workspace tab (no state save).
    /// Used when closing a tab to switch to the new active tab.
    /// Uses async runtime creation to avoid UI lag.
    fn start_session_for_active_tab(&mut self, cx: &mut Context<Self>) {
        if let Some(tab) = self.workspace_manager.active_tab() {
            let repo_path = tab.path.clone();
            self.refresh_worktrees_for_repo(&repo_path);
            let worktrees = &self.cached_worktrees;
            let restored_idx = self.per_repo_worktree_index.get(&repo_path).copied();

            if worktrees.is_empty() {
                self.active_worktree_index = None;
                self.schedule_start_main_session(&repo_path, cx);
            } else {
                let (wt_path, branch, idx) = if let Some(awi) = restored_idx {
                    if awi < worktrees.len() {
                        let wt = &worktrees[awi];
                        self.active_worktree_index = Some(awi);
                        (wt.path.clone(), wt.short_branch_name().to_string(), awi)
                    } else {
                        let wt = &worktrees[0];
                        self.active_worktree_index = Some(0);
                        (wt.path.clone(), wt.short_branch_name().to_string(), 0)
                    }
                } else {
                    let wt = &worktrees[0];
                    self.active_worktree_index = Some(0);
                    (wt.path.clone(), wt.short_branch_name().to_string(), 0)
                };
                self.schedule_switch_to_worktree_async(&repo_path, &wt_path, &branch, idx, cx);
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

    /// Try recover from runtime_state. For local PTY, always returns false (no session recovery).
    #[allow(dead_code)]
    fn try_recover_then_switch(
        &mut self,
        workspace_path: &Path,
        worktree_path: &Path,
        branch_name: &str,
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
        let worktree = match workspace
            .worktrees
            .iter()
            .find(|w| w.path.as_path() == worktree_path)
        {
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
            .unwrap_or_else(|| format!("local:{}", worktree_path.display()));

        let saved_split_tree = worktree
            .split_tree_json
            .as_deref()
            .and_then(|s| serde_json::from_str::<SplitNode>(s).ok());

        self.attach_runtime(runtime, pane_target, worktree_path, branch_name, cx, saved_split_tree);
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
        cx.spawn(async move |entity, cx| {
            let rx = std::sync::Arc::new(std::sync::Mutex::new(event_bus.subscribe()));
            loop {
                let rx_clone = rx.clone();
                let ev = blocking::unblock(move || rx_clone.lock().unwrap().recv()).await;
                match ev {
                    Ok(RuntimeEvent::AgentStateChange(e)) => {
                        if let Some(ref pane_id) = e.pane_id {
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
                                    let _ = entity.update(cx, |this, cx| {
                                        this.update_status_counts();
                                        cx.notify();
                                    });
                                }
                            }
                        }
                    }
                    Ok(RuntimeEvent::Notification(n)) => {
                        let pane_id = n.pane_id.as_deref().unwrap_or(&n.agent_id);
                        let notif_type = match n.notif_type {
                            crate::runtime::NotificationType::Error => NotificationType::Error,
                            crate::runtime::NotificationType::WaitingInput => NotificationType::Waiting,
                            crate::runtime::NotificationType::WaitingConfirm => {
                                NotificationType::WaitingConfirm
                            }
                            crate::runtime::NotificationType::Info => NotificationType::Info,
                        };
                        let message = n.message.clone();
                        let mut unread_after = 0usize;
                        if let Ok(mut mgr) = notification_manager.lock() {
                            if mgr.add(pane_id, notif_type, &message) {
                                system_notifier::notify("pmux", &message, notif_type);
                            }
                            unread_after = mgr.unread_count();
                        }
                        if let Some(ref np_model) = notification_panel_model {
                            let _ = cx.update_entity(np_model, |m, cx| {
                                m.set_unread_count(unread_after);
                                cx.notify();
                            });
                        }
                        let _ = entity.update(cx, |_, cx| cx.notify());
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
        let (repo_path, path, branch) = {
            let tab = match self.workspace_manager.active_tab() {
                Some(t) => t,
                None => return,
            };
            let repo_path = tab.path.clone();
            self.refresh_worktrees_for_repo(&repo_path);
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

        self.active_worktree_index = Some(idx);
        self.worktree_switch_loading = Some(idx);

        let workspace_path = self
            .workspace_manager
            .active_tab()
            .map(|t| t.path.clone())
            .unwrap_or_else(|| repo_path.clone());

        // Reuse existing runtime if switching worktrees within the same tmux session
        if self.current_runtime_matches_session(&workspace_path) {
            let runtime = self.runtime.as_ref().unwrap().clone();
            let window_name = window_name_for_worktree(&path, &branch);
            // #region agent log
            crate::debug_log::dbg_session_log(
                "app_root.rs:process_pending_worktree_selection",
                "detach_ui_from_runtime START (reuse session path)",
                &serde_json::json!({"window_name": &window_name, "branch": &branch, "idx": idx}),
                "H2",
            );
            // #endregion
            self.detach_ui_from_runtime();
            cx.notify();

            let path_clone = path.clone();
            let branch_clone = branch.clone();
            cx.spawn(async move |entity, cx| {
                // #region agent log
                crate::debug_log::dbg_session_log(
                    "app_root.rs:process_pending_worktree_selection:async",
                    "switch_window START",
                    &serde_json::json!({"window_name": &window_name}),
                    "H2",
                );
                // #endregion
                let wn = window_name.clone();
                let pc = path_clone.clone();
                let switch_result = blocking::unblock(move || {
                    runtime.switch_window(&wn, Some(&pc))
                }).await;

                let _ = entity.update(cx, |this: &mut AppRoot, cx| {
                    this.worktree_switch_loading = None;
                    // #region agent log
                    crate::debug_log::dbg_session_log(
                        "app_root.rs:process_pending_worktree_selection:async",
                        "switch_window DONE, calling attach_runtime",
                        &serde_json::json!({"ok": switch_result.is_ok()}),
                        "H2",
                    );
                    // #endregion
                    match switch_result {
                        Ok(()) => {
                            let rt = this.runtime.as_ref().unwrap().clone();
                            let pane_target = rt
                                .primary_pane_id()
                                .unwrap_or_else(|| format!("local:{}", path.display()));
                            // #region agent log
                            crate::debug_log::dbg_session_log(
                                "app_root.rs:process_pending_worktree_selection:async",
                                "attach_runtime with pane_target",
                                &serde_json::json!({"pane_target": &pane_target}),
                                "H3",
                            );
                            // #endregion
                            this.attach_runtime(rt, pane_target, &path, &branch_clone, cx, None);
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
        self.worktree_switch_loading = Some(worktree_idx);
        cx.notify();

        // #region agent log
        {
            let has_rt = self.runtime.is_some();
            let rt_type = self.runtime.as_ref().map(|r| r.backend_type()).unwrap_or("none");
            let session_match = self.current_runtime_matches_session(workspace_path);
            crate::debug_log::dbg_session_log(
                "app_root.rs:schedule_switch_to_worktree_async",
                "entry",
                &serde_json::json!({
                    "has_runtime": has_rt,
                    "runtime_type": rt_type,
                    "session_match": session_match,
                    "worktree_idx": worktree_idx,
                    "workspace_path": workspace_path.to_string_lossy(),
                    "worktree_path": worktree_path.to_string_lossy(),
                }),
                "H_backend",
            );
        }
        // #endregion

        // Reuse existing runtime if same tmux session
        if self.current_runtime_matches_session(workspace_path) {
            let runtime = self.runtime.as_ref().unwrap().clone();
            let window_name = window_name_for_worktree(worktree_path, branch_name);
            // #region agent log
            crate::debug_log::dbg_session_log(
                "app_root.rs:switch_reuse",
                "session matches – reuse path",
                &serde_json::json!({
                    "window_name": &window_name,
                    "worktree_path": worktree_path.to_string_lossy(),
                }),
                "H_switch1",
            );
            // #endregion
            self.detach_ui_from_runtime();

            let worktree_path = worktree_path.to_path_buf();
            let branch_name = branch_name.to_string();
            cx.spawn(async move |entity, cx| {
                let wn = window_name.clone();
                let pc = worktree_path.clone();
                // #region agent log
                let t0 = std::time::Instant::now();
                // #endregion
                let switch_result = blocking::unblock(move || {
                    runtime.switch_window(&wn, Some(&pc))
                }).await;

                let _ = entity.update(cx, |this: &mut AppRoot, cx| {
                    this.worktree_switch_loading = None;
                    // #region agent log
                    let elapsed = t0.elapsed().as_millis();
                    crate::debug_log::dbg_session_log(
                        "app_root.rs:switch_reuse",
                        "switch_window completed",
                        &serde_json::json!({
                            "elapsed_ms": elapsed,
                            "ok": switch_result.is_ok(),
                            "err": switch_result.as_ref().err().map(|e| e.to_string()),
                        }),
                        "H_switch2",
                    );
                    // #endregion
                    match switch_result {
                        Ok(()) => {
                            let rt = this.runtime.as_ref().unwrap().clone();
                            let pane_target = rt
                                .primary_pane_id()
                                .unwrap_or_else(|| format!("local:{}", worktree_path.display()));
                            // #region agent log
                            crate::debug_log::dbg_session_log(
                                "app_root.rs:switch_reuse",
                                "attaching after switch",
                                &serde_json::json!({
                                    "pane_target": &pane_target,
                                }),
                                "H_switch3",
                            );
                            // #endregion
                            this.attach_runtime(rt, pane_target, &worktree_path, &branch_name, cx, None);
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
                self.cached_worktrees = wt;
                self.cached_worktrees_repo = Some(repo_path.to_path_buf());
            }
            Err(_) => {
                self.cached_worktrees.clear();
                self.cached_worktrees_repo = None;
            }
        }
    }

    /// Get worktrees for current repo (from cache). Call from render.
    fn worktrees_for_render(&self, repo_path: &Path) -> &[crate::worktree::WorktreeInfo] {
        if self.cached_worktrees_repo.as_deref() == Some(repo_path) {
            &self.cached_worktrees
        } else {
            &[]
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

    /// Detach UI components from the runtime without dropping it.
    /// Used when switching worktrees within the same session — the -CC
    /// connection stays alive, only the terminal UI is torn down.
    fn detach_ui_from_runtime(&mut self) {
        self.resize_controller.reset_for_new_session();
        self.status_publisher.take();
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
                "b" => self.sidebar_visible = !self.sidebar_visible,
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
                    if event.keystroke.modifiers.shift {
                        self.open_diff_view(cx);
                    }
                }
                "w" => {
                    if let Some((branch, window_name, session, pane_target)) = self.diff_overlay_open.clone() {
                        self.close_diff_overlay(&branch, &window_name, session.as_deref(), &pane_target, cx);
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
                let bytes_opt = if let Ok(buffers) = self.terminal_buffers.lock() {
                    if let Some(TerminalBuffer::Terminal { terminal, .. }) = buffers.get(target) {
                        crate::terminal::key_to_bytes(&event, terminal.mode())
                    } else {
                        None
                    }
                } else {
                    None
                };

                let bytes_opt = bytes_opt.or_else(|| key_to_xterm_escape(&key_name, modifiers));

                if let Some(bytes) = bytes_opt {
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
            if let Some(rt) = &self.runtime {
                self.setup_pane_terminal_output(rt.clone(), &new_target, self.terminal_area_entity.clone(), cx);
            }
            if let Ok(mut guard) = self.pane_targets_shared.lock() {
                *guard = self.split_tree.flatten().into_iter().map(|(t, _)| t).collect();
            }
            if let Some(ref mut pub_) = self.status_publisher {
                pub_.register_pane(&new_target);
            }
            self.save_current_worktree_runtime_state();
            cx.notify();
        }
    }

    /// Save runtime state for the current active worktree. No-op if no tab or worktree.
    fn save_current_worktree_runtime_state(&mut self) {
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
        let worktrees = &self.cached_worktrees;

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

        let window_name = format!("review-{}", branch.replace('/', "-"));

        if let Some(rt) = &self.runtime {
            match rt.open_review(&worktree_path) {
                Ok(_) => {
                    self.review_windows.insert(branch.clone(), window_name.clone());
                    self.open_diff_overlay(&branch, &window_name, cx);
                }
                Err(e) => {
                    self.state.error_message = Some(format!("Failed to open diff view: {}", e));
                }
            }
        }
        cx.notify();
    }

    /// Open diff overlay (add buffer, set pane target for polling, show overlay)
    fn open_diff_overlay(&mut self, branch: &str, window_name: &str, cx: &mut Context<Self>) {
        let session = self
            .runtime
            .as_ref()
            .and_then(|rt| rt.session_info())
            .map(|(s, _)| s);
        let pane_target = session
            .as_ref()
            .map(|s| format!("{}:{}.0", s, window_name))
            .unwrap_or_else(|| format!("local:{}.0", window_name));

        // Add buffer for overlay pane (streaming will populate)
        if let Ok(mut buffers) = self.terminal_buffers.lock() {
            buffers.entry(pane_target.clone()).or_insert_with(|| {
                TerminalBuffer::Empty
            });
        }

        // Add to pane_targets_shared for multi-pane tracking
        if let Ok(mut guard) = self.pane_targets_shared.lock() {
            if !guard.contains(&pane_target) {
                guard.push(pane_target.clone());
            }
        }

        self.active_pane_target = Some(pane_target.clone());
        self.diff_overlay_open = Some((
            branch.to_string(),
            window_name.to_string(),
            session,
            pane_target.clone(),
        ));
        if let Ok(mut guard) = self.active_pane_target_shared.lock() {
            *guard = pane_target;
        }

        cx.notify();
    }

    /// Close diff overlay (kill tmux window, remove from buffers, switch back to worktree)
    fn close_diff_overlay(
        &mut self,
        branch: &str,
        window_name: &str,
        session: Option<&str>,
        pane_target: &str,
        cx: &mut Context<Self>,
    ) {
        if let (Some(rt), Some(s)) = (&self.runtime, session) {
            let target = format!("{}:{}", s, window_name);
            let _ = rt.kill_window(&target);
        }
        self.review_windows.remove(branch);
        self.diff_overlay_open = None;

        // Remove from terminal_buffers and pane_targets_shared
        if let Ok(mut buffers) = self.terminal_buffers.lock() {
            buffers.remove(pane_target);
        }
        if let Ok(mut guard) = self.pane_targets_shared.lock() {
            guard.retain(|t| t != &pane_target);
        }

        let worktree_path = self.workspace_manager.active_tab()
            .map(|t| t.path.clone())
            .unwrap_or_else(|| PathBuf::from("."));
        if let Some(idx) = self.active_worktree_index {
            self.refresh_worktrees_for_repo(&worktree_path);
            if let Some(wt) = self.cached_worktrees.get(idx) {
                let path = wt.path.clone();
                let br = wt.short_branch_name().to_string();
                self.switch_to_worktree(&path, &br, cx);
            }
        }
        cx.notify();
    }

    /// Opens the new branch dialog
    fn open_new_branch_dialog(&mut self, cx: &mut Context<Self>) {
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

        let win_name = window_name_for_worktree(&worktree.path, &branch);
        let target = window_target(&repo_path, &win_name);
        if let Some(rt) = &self.runtime {
            if let Err(e) = rt.kill_window(&target) {
                eprintln!("tmux kill-window failed (best-effort): {}", e);
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
        let app_root_entity_for_add_ws = app_root_entity.clone();

        // Create sidebar with callbacks (cmux style: top controls in sidebar)
        let mut sidebar = Sidebar::new(&repo_name, repo_path.clone())
            .with_statuses(pane_statuses.clone())
            .with_notification_manager(self.notification_manager.clone())
            .with_context_menu(self.sidebar_context_menu_index)
            .on_toggle_sidebar(move |_window, cx| {
                let _ = cx.update_entity(&app_root_entity_for_toggle, |this: &mut AppRoot, cx| {
                    this.sidebar_visible = !this.sidebar_visible;
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

        // Set up select callback
        let app_root_entity_for_sidebar = app_root_entity.clone();
        sidebar.on_select(move |idx: usize, _window: &mut Window, cx: &mut App| {
            let _ = cx.update_entity(&app_root_entity_for_sidebar, |this: &mut AppRoot, cx| {
                this.pending_worktree_selection = Some(idx);
                this.process_pending_worktree_selection(cx);
                cx.notify();
            });
        });

        // Set up New Branch callback - opens the dialog
        let app_root_entity_for_new_branch = app_root_entity.clone();
        let dialog_focus = self.dialog_input_focus.clone();
        sidebar.on_new_branch(move |window, cx| {
            let _ = cx.update_entity(&app_root_entity_for_new_branch, |this: &mut AppRoot, cx| {
                this.open_new_branch_dialog(cx);
            });
            if let Some(ref focus) = dialog_focus {
                let focus = focus.clone();
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
                this.refresh_worktrees_for_repo(&repo_path_for_delete);
                if let Some(wt) = this.cached_worktrees.get(idx) {
                    this.show_delete_dialog(wt.clone(), cx);
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
            .when(self.notification_panel_entity.is_some(), |el: Stateful<Div>| {
                el.child(self.notification_panel_entity.as_ref().unwrap().clone())
            })
            // Dialogs rendered last so they appear on top (absolute overlay)
            .child(delete_dialog)
            .when(self.new_branch_dialog_entity.is_some(), |el: Stateful<Div>| {
                el.child(self.new_branch_dialog_entity.as_ref().unwrap().clone())
            })
            .when(self.diff_overlay_open.is_some(), |el| {
                if let Some((branch, window_name, session, pane_target)) = &self.diff_overlay_open {
                    let buffer = terminal_buffers
                        .lock()
                        .ok()
                        .and_then(|g| g.get(pane_target).cloned())
                        .unwrap_or(TerminalBuffer::Empty);
                    let branch = branch.clone();
                    let window_name = window_name.clone();
                    let session = session.clone();
                    let pane_target = pane_target.clone();
                    let app_root_entity_for_diff_close = app_root_entity.clone();
                    el.child(
                        DiffOverlay::new(&branch, &pane_target, buffer)
                            .on_close(move |_window, cx| {
                                let _ = cx.update_entity(&app_root_entity_for_diff_close, |this: &mut AppRoot, cx| {
                                    this.close_diff_overlay(&branch, &window_name, session.as_deref(), &pane_target, cx);
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
        // Open Settings when requested from menu (main.rs)
        if OPEN_SETTINGS_REQUESTED.swap(false, Ordering::SeqCst) {
            self.show_settings = true;
            self.settings_draft = Config::load().ok();
            self.settings_secrets_draft = Secrets::load().ok();
        }

        // Resize runtime panes via ResizeController (debounced). gpui-terminal uses with_resize_callback.
        // Bug3 fix: Only schedule resize when buffer_count > 0. After stop_current_session, buffers
        // are cleared and setup_local_terminal inserts new buffers async. If we schedule with empty
        // buffers, on_next_frame resizes 0 engines. Skip until buffers exist.
        if self.has_workspaces() && !self.resize_controller.is_pending() {
            let buffer_count = self.terminal_buffers.lock().map(|b| b.len()).unwrap_or(0);
            if buffer_count > 0 {
                let bounds = window.window_bounds().get_bounds();
                let w = f32::from(bounds.size.width);
                let h = f32::from(bounds.size.height);
                let sidebar_w = if self.sidebar_visible { self.sidebar_width as f32 } else { 0. };
                let (cols, rows) = ResizeController::compute_dims_from_bounds(
                    w, h, self.sidebar_visible, sidebar_w,
                );
                let resize_result = self.resize_controller.maybe_resize(cols, rows);
                if let Some((cols, rows)) = resize_result {
                    self.resize_controller.set_pending(true);
                    let pane_targets: Vec<String> =
                        self.split_tree.flatten().into_iter().map(|(t, _)| t).collect();
                    let runtime = self.runtime.clone();
                    let entity = cx.entity();
                    window.on_next_frame(move |_window, cx| {
                        if let Some(ref rt) = runtime {
                            for pane_target in &pane_targets {
                                let _ = rt.resize(pane_target, cols, rows);
                            }
                        }
                        // Terminal: resize handled via with_resize_callback in TerminalElement
                        let _ = entity.update(cx, |this, cx| {
                            this.resize_controller.set_pending(false);
                            cx.notify();
                        });
                    });
                }
            }
        }

        // Cursor: Zed style - always visible, no blink
        let terminal_focus = self.terminal_focus.get_or_insert_with(|| cx.focus_handle()).clone();

        // Auto-focus terminal when workspace loads so keyboard input works without clicking.
        // Use double on_next_frame so terminal DOM is fully mounted after worktree switch.
        if self.has_workspaces() && self.terminal_needs_focus {
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
            .when(self.show_settings, |el| {
                let app_root_entity = cx.entity();
                let app_root_entity_for_close = app_root_entity.clone();
                // Use draft or load on demand
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
                let app_root_entity_save = app_root_entity.clone();
                let discord_status = if discord_configured { "已配置" } else { "未配置" };
                let kook_status = if kook_configured { "已配置" } else { "未配置" };
                let feishu_status = if feishu_configured { "已配置" } else { "未配置" };
                let channel_cards = div()
                    .flex()
                    .flex_col()
                    .gap(px(12.))
                    .child(Self::settings_channel_card_el(
                        "Discord",
                        "discord",
                        discord_status,
                        discord_enabled,
                        app_root_entity_discord,
                        |draft| {
                            draft.remote_channels.discord.enabled = !draft.remote_channels.discord.enabled;
                        },
                    ))
                    .child(Self::settings_channel_card_el(
                        "KOOK",
                        "kook",
                        kook_status,
                        kook_enabled,
                        app_root_entity_kook,
                        |draft| {
                            draft.remote_channels.kook.enabled = !draft.remote_channels.kook.enabled;
                        },
                    ))
                    .child(Self::settings_channel_card_el(
                        "飞书",
                        "feishu",
                        feishu_status,
                        feishu_enabled,
                        app_root_entity_feishu,
                        |draft| {
                            draft.remote_channels.feishu.enabled = !draft.remote_channels.feishu.enabled;
                        },
                    ));
                let save_button = div()
                    .id("settings-save-btn")
                    .px(px(16.))
                    .py(px(8.))
                    .rounded(px(6.))
                    .bg(rgb(0x0066cc))
                    .text_color(rgb(0xffffff))
                    .text_size(px(14.))
                    .font_weight(FontWeight::MEDIUM)
                    .cursor_pointer()
                    .hover(|style: StyleRefinement| style.bg(rgb(0x0077dd)))
                    .on_click(move |_event, _window, cx| {
                        let _ = cx.update_entity(&app_root_entity_save, |this: &mut AppRoot, cx| {
                            if let Some(ref draft) = this.settings_draft {
                                let mut current = Config::load().unwrap_or_default();
                                current.remote_channels = draft.remote_channels.clone();
                                let _ = current.save();
                            }
                            if let Some(ref secrets) = this.settings_secrets_draft {
                                let _ = secrets.save();
                            }
                            this.show_settings = false;
                            this.settings_draft = None;
                            this.settings_secrets_draft = None;
                            this.settings_configuring_channel = None;
                            cx.notify();
                        });
                    })
                    .child("Save");
                let settings_content = div()
                    .flex()
                    .flex_col()
                    .gap(px(20.))
                    .child(
                        div()
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
                                            cx.notify();
                                        });
                                    })
                                    .child("×")
                            )
                    )
                    .child(channel_cards)
                    .when(self.settings_configuring_channel.is_some(), |el| {
                        let channel = self.settings_configuring_channel.as_ref().unwrap().clone();
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
                        el.child(
                            div()
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
                                )
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .justify_end()
                            .child(save_button)
                    );
                let settings_card_with_content = div()
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
                    .on_click(|_event, _window, cx| {
                        cx.stop_propagation();
                    })
                    .child(settings_content);
                let settings_modal_with_content = div()
                    .id("settings-modal-overlay")
                    .absolute()
                    .inset(px(0.))
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(rgba(0x00000099u32))
                    .cursor_pointer()
                    .on_click(move |_event, _window, cx| {
                        let _ = cx.update_entity(&app_root_entity, |this: &mut AppRoot, cx| {
                            this.show_settings = false;
                            this.settings_draft = None;
                            this.settings_secrets_draft = None;
                            this.settings_configuring_channel = None;
                            cx.notify();
                        });
                    })
                    .child(settings_card_with_content);
                el.child(settings_modal_with_content)
            })
    }
}

impl Default for AppRoot {
    fn default() -> Self {
        Self::new()
    }
}
