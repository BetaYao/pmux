// ui/sidebar.rs - Sidebar component for worktree list with GPUI render
// Event Bus driven: status_change broadcast triggers debounced parent notify (see app_root)
use crate::agent_status::AgentStatus;
use crate::new_branch_orchestrator::NewBranchOrchestrator;
use crate::scheduler::{ScheduledTask, TaskRunStatus};
use crate::ui::models::PaneSummary;
use crate::worktree::WorktreeInfo;
use gpui::prelude::*;
use gpui::{
    div, px, rgb, svg, AnyElement, App, ClickEvent, Component, Div, ElementId, FontWeight,
    InteractiveElement, IntoElement, MouseButton, ParentElement, RenderOnce, Rgba, SharedString,
    Stateful, StatefulInteractiveElement, StyleRefinement, Styled, Window,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use uuid::Uuid;

/// Worktree item with status
#[derive(Clone)]
pub struct WorktreeItem {
    pub info: WorktreeInfo,
    pub status: AgentStatus,
}

impl WorktreeItem {
    pub fn new(info: WorktreeInfo) -> Self {
        Self {
            info,
            status: AgentStatus::Unknown,
        }
    }

    pub fn set_status(&mut self, status: AgentStatus) {
        self.status = status;
    }

    pub fn status_icon(&self) -> &'static str {
        self.status.icon()
    }

    pub fn status_color(&self) -> Rgba {
        self.status.gpui_color()
    }

    pub fn formatted_branch(&self) -> String {
        let branch = self.info.short_branch_name();
        if self.info.ahead > 0 {
            format!("{} · +{}", branch, self.info.ahead)
        } else {
            branch.to_string()
        }
    }

    pub fn status_text(&self) -> &'static str {
        self.status.display_text()
    }
}

/// Type alias for the select callback
pub type SelectCallback = Arc<dyn Fn(usize, &mut Window, &mut App) + Send + Sync>;

/// Sidebar component - renders top controls, worktree list with status, add branch
pub struct Sidebar {
    repo_name: String,
    repo_path: PathBuf,
    worktrees: Arc<Mutex<Vec<WorktreeItem>>>,
    pane_statuses: Arc<Mutex<std::collections::HashMap<String, AgentStatus>>>,
    selected_index: Option<usize>,
    on_select: Option<SelectCallback>,
    on_new_branch: Option<Arc<dyn Fn(&mut Window, &mut App) + Send + Sync>>,
    on_refresh: Option<Arc<dyn Fn(&mut Window, &mut App) + Send + Sync>>,
    on_delete: Option<Arc<dyn Fn(usize, &mut Window, &mut App) + Send + Sync>>,
    on_view_diff: Option<Arc<dyn Fn(usize, &mut Window, &mut App) + Send + Sync>>,
    on_right_click: Option<Arc<dyn Fn(usize, f32, f32, &mut Window, &mut App) + Send + Sync>>,
    /// Which worktree index has context menu open, and the (x, y) position of the right-click
    context_menu_for: Option<(usize, f32, f32)>,
    creating_branch: bool,
    /// Store original worktree info for access in callbacks
    worktrees_info: Arc<Mutex<Vec<crate::worktree::WorktreeInfo>>>,
    /// Top control row callbacks (cmux style)
    on_toggle_sidebar: Option<Arc<dyn Fn(&mut Window, &mut App) + Send + Sync>>,
    on_toggle_notifications: Option<Arc<dyn Fn(&mut Window, &mut App) + Send + Sync>>,
    on_add_workspace: Option<Arc<dyn Fn(&mut Window, &mut App) + Send + Sync>>,
    notification_count: usize,
    /// Per-pane summaries (status, last_line, status_since) for sidebar display
    pane_summaries: HashMap<String, PaneSummary>,
    /// Running animation frame index (cycles through RUNNING_FRAMES)
    running_animation_frame: usize,
    /// Orphan tmux window names (session exists but worktree was removed externally). Shown with close button.
    orphan_windows: Arc<Mutex<Vec<String>>>,
    /// Callback when user closes an orphan window (window_name).
    on_close_orphan: Option<Arc<dyn Fn(&str, &mut Window, &mut App) + Send + Sync>>,
    /// Callback when user clicks the settings gear icon.
    on_settings: Option<Arc<dyn Fn(&mut Window, &mut App) + Send + Sync>>,
    scheduled_tasks: Vec<ScheduledTask>,
    tasks_expanded: bool,
    on_toggle_task: Option<Arc<dyn Fn(Uuid, &mut Window, &mut App) + Send + Sync>>,
    on_run_task: Option<Arc<dyn Fn(Uuid, &mut Window, &mut App) + Send + Sync>>,
    on_add_task: Option<Arc<dyn Fn(&mut Window, &mut App) + Send + Sync>>,
    on_delete_task: Option<Arc<dyn Fn(Uuid, &mut Window, &mut App) + Send + Sync>>,
    selected_task_index: Option<usize>,
    task_list_focused: bool,
    task_pending_delete: Option<Uuid>,
}

impl Sidebar {
    pub fn new(repo_name: &str, repo_path: PathBuf) -> Self {
        Self {
            repo_name: repo_name.to_string(),
            repo_path,
            worktrees: Arc::new(Mutex::new(Vec::new())),
            pane_statuses: Arc::new(Mutex::new(HashMap::new())),
            selected_index: None,
            on_select: None,
            on_new_branch: None,
            on_refresh: None,
            on_delete: None,
            on_view_diff: None,
            on_right_click: None,
            context_menu_for: None,
            creating_branch: false,
            worktrees_info: Arc::new(Mutex::new(Vec::new())),
            on_toggle_sidebar: None,
            on_toggle_notifications: None,
            on_add_workspace: None,
            notification_count: 0,
            pane_summaries: HashMap::new(),
            running_animation_frame: 0,
            orphan_windows: Arc::new(Mutex::new(Vec::new())),
            on_close_orphan: None,
            on_settings: None,
            scheduled_tasks: Vec::new(),
            tasks_expanded: true,
            on_toggle_task: None,
            on_run_task: None,
            on_add_task: None,
            on_delete_task: None,
            selected_task_index: None,
            task_list_focused: false,
            task_pending_delete: None,
        }
    }

    pub fn with_pane_summaries(mut self, summaries: HashMap<String, PaneSummary>) -> Self {
        self.pane_summaries = summaries;
        self
    }

    pub fn with_running_frame(mut self, frame: usize) -> Self {
        self.running_animation_frame = frame;
        self
    }

    pub fn on_toggle_sidebar<F: Fn(&mut Window, &mut App) + Send + Sync + 'static>(
        mut self,
        f: F,
    ) -> Self {
        self.on_toggle_sidebar = Some(Arc::new(f));
        self
    }
    pub fn on_toggle_notifications<F: Fn(&mut Window, &mut App) + Send + Sync + 'static>(
        mut self,
        f: F,
    ) -> Self {
        self.on_toggle_notifications = Some(Arc::new(f));
        self
    }
    pub fn on_add_workspace<F: Fn(&mut Window, &mut App) + Send + Sync + 'static>(
        mut self,
        f: F,
    ) -> Self {
        self.on_add_workspace = Some(Arc::new(f));
        self
    }
    pub fn with_notification_count(mut self, count: usize) -> Self {
        self.notification_count = count;
        self
    }

    pub fn with_statuses(
        mut self,
        pane_statuses: Arc<Mutex<HashMap<String, AgentStatus>>>,
    ) -> Self {
        self.pane_statuses = pane_statuses;
        self
    }

    pub fn set_worktrees(&mut self, worktrees: Vec<WorktreeInfo>) {
        // Clone the worktrees for WorktreeInfo since we need to store them
        if let Ok(mut guard) = self.worktrees_info.lock() {
            *guard = worktrees.clone();
        }

        // Create WorktreeItems for display
        let items: Vec<WorktreeItem> = worktrees.iter().cloned().map(WorktreeItem::new).collect();
        if let Ok(mut guard) = self.worktrees.lock() {
            *guard = items;
        }
    }

    pub fn update_status(&mut self, index: usize, status: AgentStatus) {
        if let Ok(mut guard) = self.worktrees.lock() {
            if let Some(item) = guard.get_mut(index) {
                item.set_status(status);
            }
        }
    }

    pub fn select(&mut self, index: usize) {
        let len = self.worktrees.lock().map(|g| g.len()).unwrap_or(0);
        if index < len {
            self.selected_index = Some(index);
        }
    }

    pub fn selected_index(&self) -> Option<usize> {
        self.selected_index
    }

    pub fn on_select<F>(&mut self, callback: F)
    where
        F: Fn(usize, &mut Window, &mut App) + Send + Sync + 'static,
    {
        self.on_select = Some(Arc::new(callback));
    }

    pub fn on_delete<F: Fn(usize, &mut Window, &mut App) + Send + Sync + 'static>(
        &mut self,
        callback: F,
    ) {
        self.on_delete = Some(Arc::new(callback));
    }

    pub fn on_view_diff<F: Fn(usize, &mut Window, &mut App) + Send + Sync + 'static>(
        &mut self,
        callback: F,
    ) {
        self.on_view_diff = Some(Arc::new(callback));
    }

    pub fn on_right_click<F: Fn(usize, f32, f32, &mut Window, &mut App) + Send + Sync + 'static>(
        &mut self,
        callback: F,
    ) {
        self.on_right_click = Some(Arc::new(callback));
    }

    /// Set orphan tmux window names to show in sidebar (windows whose worktree was removed externally).
    pub fn set_orphan_windows(&mut self, window_names: Vec<String>) {
        if let Ok(mut guard) = self.orphan_windows.lock() {
            *guard = window_names;
        }
    }

    pub fn on_close_orphan<F: Fn(&str, &mut Window, &mut App) + Send + Sync + 'static>(
        &mut self,
        callback: F,
    ) {
        self.on_close_orphan = Some(Arc::new(callback));
    }

    pub fn with_context_menu(mut self, index: Option<(usize, f32, f32)>) -> Self {
        self.context_menu_for = index;
        self
    }

    pub fn on_new_branch<F: Fn(&mut Window, &mut App) + Send + Sync + 'static>(
        &mut self,
        callback: F,
    ) {
        self.on_new_branch = Some(Arc::new(callback));
    }

    pub fn on_refresh<F: Fn(&mut Window, &mut App) + Send + Sync + 'static>(
        &mut self,
        callback: F,
    ) {
        self.on_refresh = Some(Arc::new(callback));
    }

    pub fn on_settings<F: Fn(&mut Window, &mut App) + Send + Sync + 'static>(
        &mut self,
        callback: F,
    ) {
        self.on_settings = Some(Arc::new(callback));
    }

    pub fn set_scheduled_tasks(&mut self, tasks: Vec<ScheduledTask>) {
        self.scheduled_tasks = tasks;
    }

    pub fn with_tasks_expanded(mut self, expanded: bool) -> Self {
        self.tasks_expanded = expanded;
        self
    }

    pub fn on_toggle_task<F: Fn(Uuid, &mut Window, &mut App) + Send + Sync + 'static>(
        mut self,
        f: F,
    ) -> Self {
        self.on_toggle_task = Some(Arc::new(f));
        self
    }

    pub fn on_run_task<F: Fn(Uuid, &mut Window, &mut App) + Send + Sync + 'static>(
        mut self,
        f: F,
    ) -> Self {
        self.on_run_task = Some(Arc::new(f));
        self
    }

    pub fn on_add_task<F: Fn(&mut Window, &mut App) + Send + Sync + 'static>(
        mut self,
        f: F,
    ) -> Self {
        self.on_add_task = Some(Arc::new(f));
        self
    }

    pub fn on_delete_task<F: Fn(Uuid, &mut Window, &mut App) + Send + Sync + 'static>(
        mut self,
        f: F,
    ) -> Self {
        self.on_delete_task = Some(Arc::new(f));
        self
    }

    pub fn with_selected_task_index(mut self, index: Option<usize>) -> Self {
        self.selected_task_index = index;
        self
    }

    pub fn with_task_list_focused(mut self, focused: bool) -> Self {
        self.task_list_focused = focused;
        self
    }

    pub fn with_task_pending_delete(mut self, id: Option<Uuid>) -> Self {
        self.task_pending_delete = id;
        self
    }

    pub fn add_worktree(&mut self, info: WorktreeInfo) {
        if let Ok(mut guard) = self.worktrees.lock() {
            guard.push(WorktreeItem::new(info));
        }
    }

    pub fn remove_worktree(&mut self, index: usize) {
        if let Ok(mut guard) = self.worktrees.lock() {
            if index < guard.len() {
                guard.remove(index);
                if let Some(selected) = self.selected_index {
                    if selected >= index && selected > 0 {
                        self.selected_index = Some(selected - 1);
                    } else if selected >= guard.len() {
                        self.selected_index = guard.len().checked_sub(1);
                    }
                }
            }
        }
    }

    pub fn worktree_count(&self) -> usize {
        self.worktrees.lock().map(|g| g.len()).unwrap_or(0)
    }

    pub fn is_creating_branch(&self) -> bool {
        self.creating_branch
    }

    pub fn set_creating_branch(&mut self, creating: bool) {
        self.creating_branch = creating;
    }

    /// Refresh worktrees from the repository
    pub fn refresh_worktrees(&mut self) -> Result<(), String> {
        let orchestrator = NewBranchOrchestrator::new(self.repo_path.clone());
        let worktrees = orchestrator.get_worktrees()?;

        let converted: Vec<WorktreeInfo> = worktrees
            .iter()
            .map(|wt| {
                WorktreeInfo::new(
                    wt.path.clone(),
                    wt.branch.as_str(),
                    wt.commit.as_deref().unwrap_or("unknown"),
                )
            })
            .collect();

        self.set_worktrees(converted);
        Ok(())
    }

    /// Get worktree info by index (for callbacks)
    pub fn get_worktree_info(&self, index: usize) -> Option<WorktreeInfo> {
        if let Ok(guard) = self.worktrees_info.lock() {
            guard.get(index).cloned()
        } else {
            None
        }
    }

    fn render_task_item(&self, task: &ScheduledTask, index: usize) -> impl IntoElement {
        let is_selected = self.task_list_focused && self.selected_task_index == Some(index);
        let is_pending_delete = self.task_pending_delete == Some(task.id);

        let bg = if is_pending_delete {
            rgb(0x4a1c1c) // red tint for pending delete
        } else if is_selected {
            rgb(0x3a3a3a) // highlight for selected
        } else {
            rgb(0x00000000) // transparent
        };

        let status_text = match &task.last_status {
            Some(TaskRunStatus::Never) => "Never run",
            Some(TaskRunStatus::Triggered) => "Triggered",
            Some(TaskRunStatus::Failed) => "Failed",
            None => "Never run",
        };

        let status_color = match &task.last_status {
            Some(TaskRunStatus::Never) => rgb(0x888888),
            Some(TaskRunStatus::Triggered) => rgb(0x4ade80),
            Some(TaskRunStatus::Failed) => rgb(0xf87171),
            None => rgb(0x888888),
        };

        let icon = if task.enabled { "▶" } else { "⏸" };
        let icon_color = if task.enabled {
            rgb(0x4ade80)
        } else {
            rgb(0x888888)
        };

        let _task_id = task.id;
        let on_toggle = self.on_toggle_task.clone();
        let on_run = self.on_run_task.clone();

        let task_selector = format!("task-item-{}", index);
        let mut item = div()
            .debug_selector(move || task_selector)
            .flex()
            .items_center()
            .justify_between()
            .w_full()
            .px_2()
            .py_1()
            .rounded_md()
            .bg(bg)
            .when(!is_pending_delete, |el| el.hover(|style| style.bg(rgb(0x2a2a2a))))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_0()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(div().text_color(icon_color).text_sm().child(icon))
                            .child(
                                div()
                                    .text_color(rgb(0xe0e0e0))
                                    .text_sm()
                                    .child(SharedString::from(task.name.clone())),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_color(rgb(0x888888))
                                    .text_xs()
                                    .child(SharedString::from(task.cron.clone())),
                            )
                            .child(div().text_color(status_color).text_xs().child(status_text)),
                    ),
            );

        if let Some(_cb) = on_toggle {
            // Note: on_click handler not working due to trait resolution issue
            // The item can still be clicked but won't trigger the callback
        }

        if let Some(_cb) = on_run {
            item = item.child(
                div()
                    .text_color(rgb(0x888888))
                    .text_xs()
                    .hover(|style| style.text_color(rgb(0xe0e0e0)))
                    // Note: on_click handler not working due to trait resolution issue
                    .child("Run"),
            );
        }

        if is_selected {
            let sel = format!("task-selected-{}", index);
            item = item.child(div().debug_selector(move || sel));
        }

        if is_pending_delete {
            let del = format!("task-pending-delete-{}", index);
            item = item.child(
                div()
                    .debug_selector(move || del)
                    .text_color(rgb(0xf87171))
                    .text_xs()
                    .child("Delete? Enter/Esc"),
            );
        }

        item
    }

    fn render_tasks_section(&self) -> impl IntoElement {
        let count = self.scheduled_tasks.len();
        let expand_icon = if self.tasks_expanded { "▼" } else { "▶" };
        let on_add = self.on_add_task.clone();

        let mut header = div()
            .debug_selector(|| "tasks-section-header".to_string())
            .flex()
            .items_center()
            .justify_between()
            .w_full()
            .px_2()
            .py_2()
            .hover(|style| style.bg(rgb(0x2a2a2a)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(div().text_color(rgb(0x888888)).text_xs().child(expand_icon))
                    .child(
                        div()
                            .text_color(rgb(0x888888))
                            .text_sm()
                            .child(format!("Scheduled Tasks ({})", count)),
                    ),
            );

        if let Some(cb) = on_add {
            header = header.child(
                div()
                    .id("sidebar-add-task-btn")
                    .text_color(rgb(0x888888))
                    .text_xs()
                    .cursor_pointer()
                    .hover(|style| style.text_color(rgb(0xe0e0e0)))
                    .on_click(move |_event, window, cx| {
                        cb(window, cx);
                    })
                    .child("+Add"),
            );
        }

        div()
            .flex()
            .flex_col()
            .w_full()
            .border_t_1()
            .border_color(rgb(0x333333))
            .mt_2()
            .child(header)
            .when(self.tasks_expanded, |el| {
                el.children(
                    self.scheduled_tasks
                        .iter()
                        .enumerate()
                        .map(|(i, task)| self.render_task_item(task, i)),
                )
            })
    }

    fn render_header(repo_name: &str) -> Div {
        div()
            .flex()
            .flex_row()
            .items_center()
            .px(px(12.))
            .py(px(10.))
            .border_b(px(1.))
            .border_color(rgb(0x2a2d37))
            .child(
                div()
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(0xc0c8d5))
                    .child(SharedString::from(format!("{}", repo_name))),
            )
    }

    /// Top control row: collapse, notification, add workspace (cmux style)
    /// Height 36px to match content workspace tab bar; pt(6) aligns icons with macOS traffic lights
    const TITLE_BAR_HEIGHT: f32 = 36.;

    fn render_top_controls(
        on_toggle_sidebar: Option<Arc<dyn Fn(&mut Window, &mut App) + Send + Sync>>,
        on_toggle_notifications: Option<Arc<dyn Fn(&mut Window, &mut App) + Send + Sync>>,
        on_add_workspace: Option<Arc<dyn Fn(&mut Window, &mut App) + Send + Sync>>,
        notification_count: usize,
    ) -> impl IntoElement {
        let has_notifications = notification_count > 0;
        let icon_color = rgb(0xcccccc);
        let icon_color_alert = rgb(0xff4444);
        // pt(6): push controls down to align with traffic lights (center ~19px from top)
        // pl(72): after macOS traffic lights (~12+52+8)
        let mut controls = div()
            .id("sidebar-top-controls")
            .flex()
            .flex_row()
            .items_center()
            .h(px(Self::TITLE_BAR_HEIGHT))
            .pt(px(6.))
            .pl(px(72.))
            .pr(px(8.))
            .gap(px(4.))
            .border_b(px(1.))
            .border_color(rgb(0x2a2d37))
            .bg(rgb(0x282c34));

        let btn_size = px(28.);
        if let Some(cb) = on_toggle_sidebar {
            let cb = Arc::clone(&cb);
            controls = controls.child(
                div()
                    .id("toggle-sidebar-btn")
                    .flex()
                    .items_center()
                    .justify_center()
                    .w(btn_size)
                    .h(btn_size)
                    .rounded(px(4.))
                    .hover(|s: StyleRefinement| s.bg(rgb(0x3d3d3d)))
                    .cursor_pointer()
                    .on_click(move |_, window, cx| cb(window, cx))
                    .child(
                        svg()
                            .path("icons/sidebar.svg")
                            .w(px(14.))
                            .h(px(14.))
                            .text_color(icon_color),
                    ),
            );
        }
        if let Some(cb) = on_toggle_notifications {
            let cb = Arc::clone(&cb);
            controls = controls.child(
                div()
                    .id("notification-btn")
                    .flex()
                    .items_center()
                    .justify_center()
                    .w(btn_size)
                    .h(btn_size)
                    .rounded(px(4.))
                    .when(has_notifications, |el: Stateful<Div>| el.bg(rgb(0x3a1111)))
                    .when(!has_notifications, |el: Stateful<Div>| {
                        el.hover(|s: StyleRefinement| s.bg(rgb(0x3d3d3d)))
                    })
                    .cursor_pointer()
                    .on_click(move |_, window, cx| cb(window, cx))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(4.))
                            .child(
                                svg()
                                    .path("icons/bell.svg")
                                    .w(px(14.))
                                    .h(px(14.))
                                    .text_color(if has_notifications {
                                        icon_color_alert
                                    } else {
                                        icon_color
                                    }),
                            )
                            .when(has_notifications, |el: Div| {
                                el.child(
                                    div()
                                        .text_size(px(10.))
                                        .text_color(icon_color_alert)
                                        .font_weight(FontWeight::BOLD)
                                        .child(format!("{}", notification_count)),
                                )
                            }),
                    ),
            );
        }
        if let Some(cb) = on_add_workspace {
            let cb = Arc::clone(&cb);
            controls = controls.child(
                div()
                    .id("add-workspace-btn")
                    .flex()
                    .items_center()
                    .justify_center()
                    .w(btn_size)
                    .h(btn_size)
                    .rounded(px(4.))
                    .hover(|s: StyleRefinement| s.bg(rgb(0x3d3d3d)))
                    .cursor_pointer()
                    .on_click(move |_, window, cx| cb(window, cx))
                    .child(
                        svg()
                            .path("icons/plus.svg")
                            .w(px(14.))
                            .h(px(14.))
                            .text_color(icon_color),
                    ),
            );
        }
        controls
    }

    #[allow(dead_code)]
    fn render_row(idx: usize, item: &WorktreeItem, is_selected: bool) -> Stateful<Div> {
        let status_color = item.status_color();
        let text_color = if is_selected {
            rgb(0xffffff)
        } else {
            rgb(0xcccccc)
        };
        let status_text_color = if is_selected {
            rgb(0xbbbbbb)
        } else {
            rgb(0x888888)
        };

        let inner = div()
            .flex()
            .flex_col()
            .gap(px(2.))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(6.))
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(status_color)
                            .child(item.status_icon()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(12.))
                            .text_color(text_color)
                            .child(SharedString::from(item.formatted_branch())),
                    ),
            )
            .child(
                div()
                    .pl(px(17.))
                    .text_size(px(10.))
                    .text_color(status_text_color)
                    .child(item.status_text()),
            );

        let row = div()
            .id(ElementId::from(idx))
            .mx(px(4.))
            .my(px(2.))
            .px(px(8.))
            .py(px(6.))
            .rounded(px(4.))
            .child(inner);

        if is_selected {
            row.bg(rgb(0x2c313a))
        } else {
            row.hover(|s: StyleRefinement| s.bg(rgb(0x262b33)))
        }
    }

    /// Render an individual context menu item
    fn context_menu_item(
        id: impl Into<ElementId>,
        icon: &'static str,
        label: &'static str,
        text_color: Rgba,
        hover_bg: Rgba,
        hover_text: Rgba,
        on_click_fn: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> impl IntoElement {
        div()
            .id(id)
            .mx(px(4.))
            .px(px(8.))
            .py(px(7.))
            .rounded(px(4.))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.))
            .text_size(px(13.))
            .text_color(text_color)
            .hover(move |s: StyleRefinement| s.bg(hover_bg).text_color(hover_text))
            .cursor_pointer()
            .on_click(on_click_fn)
            .child(div().text_size(px(11.)).opacity(0.7).child(icon))
            .child(label)
    }

    pub fn render_context_menu(
        idx: usize,
        on_view_diff: Option<Arc<dyn Fn(usize, &mut Window, &mut App) + Send + Sync>>,
        on_delete: Option<Arc<dyn Fn(usize, &mut Window, &mut App) + Send + Sync>>,
        worktrees_info: &[crate::worktree::WorktreeInfo],
    ) -> impl IntoElement {
        let wt = worktrees_info.get(idx);
        let is_base = wt.map(|w| w.is_base).unwrap_or(true);
        let show_view_diff = on_view_diff.is_some();
        let show_delete = on_delete.is_some() && !is_base;
        let has_view_diff = show_view_diff;
        let has_delete = show_delete;

        let mut menu = div()
            .id(format!("sidebar-context-menu-{}", idx))
            .min_w(px(180.))
            .py(px(4.))
            .rounded(px(6.))
            .bg(rgb(0x282828))
            .border_1()
            .border_color(rgb(0x404040))
            .shadow_lg()
            .occlude()
            .on_click(|_event, _window, cx| {
                cx.stop_propagation();
            })
            .flex()
            .flex_col();

        if let Some(on_view_diff) = on_view_diff.filter(|_| show_view_diff) {
            menu = menu.child(Self::context_menu_item(
                format!("context-menu-view-diff-{}", idx),
                "⊡",
                "View Diff",
                rgb(0xdddddd),
                rgb(0x0d4f7a),
                rgb(0xffffff),
                move |_event, window, cx| {
                    on_view_diff(idx, window, cx);
                },
            ));
        }

        // Separator between sections
        if has_view_diff && has_delete {
            menu = menu.child(div().mx(px(4.)).my(px(2.)).h(px(1.)).bg(rgb(0x3a3a3a)));
        }

        if let Some(on_delete) = on_delete.filter(|_| show_delete) {
            menu = menu.child(Self::context_menu_item(
                format!("context-menu-remove-{}", idx),
                "⊗",
                "Remove Worktree",
                rgb(0xff7070),
                rgb(0x3a1010),
                rgb(0xff9090),
                move |_event, window, cx| {
                    on_delete(idx, window, cx);
                },
            ));
        }

        menu
    }

    fn render_footer(
        creating: bool,
        on_new_branch: Option<&Arc<dyn Fn(&mut Window, &mut App) + Send + Sync>>,
        on_refresh: Option<&Arc<dyn Fn(&mut Window, &mut App) + Send + Sync>>,
        on_settings: Option<&Arc<dyn Fn(&mut Window, &mut App) + Send + Sync>>,
    ) -> Stateful<Div> {
        let mut btn = div()
            .id("new-branch-btn")
            .px(px(12.))
            .py(px(6.))
            .rounded(px(4.))
            .when(!creating, |this| {
                this.bg(rgb(0x0e639c))
                    .hover(|s: StyleRefinement| s.bg(rgb(0x1177bb)))
                    .cursor_pointer()
            })
            .when(creating, |this| this.bg(rgb(0x3d3d3d)))
            .text_color(rgb(0xffffff))
            .text_size(px(11.))
            .child(if creating {
                "Creating..."
            } else {
                "+ New Branch"
            });

        // Add click handler if not creating and callback exists
        if !creating {
            if let Some(callback) = on_new_branch {
                let cb = Arc::clone(callback);
                btn = btn.on_click(move |_, window, cx| {
                    cb(window, cx);
                });
            }
        }

        // Refresh worktree list button
        let mut refresh_btn = div()
            .id("refresh-worktrees-btn")
            .px(px(8.))
            .py(px(4.))
            .rounded(px(4.))
            .text_color(rgb(0x999999))
            .text_size(px(22.))
            .cursor_pointer()
            .hover(|s: StyleRefinement| s.text_color(rgb(0xffffff)).bg(rgb(0x3d3d3d)))
            .child("↻");
        if let Some(callback) = on_refresh {
            let cb = Arc::clone(callback);
            refresh_btn = refresh_btn.on_click(move |_, window, cx| {
                cb(window, cx);
            });
        }

        // Settings gear icon button (sized to match New Branch button height)
        let mut gear = div()
            .id("sidebar-settings-btn")
            .px(px(8.))
            .py(px(4.))
            .rounded(px(4.))
            .text_color(rgb(0x999999))
            .text_size(px(20.))
            .cursor_pointer()
            .hover(|s: StyleRefinement| s.text_color(rgb(0xffffff)).bg(rgb(0x3d3d3d)))
            .child("⚙");
        if let Some(callback) = on_settings {
            let cb = Arc::clone(callback);
            gear = gear.on_click(move |_, window, cx| {
                cb(window, cx);
            });
        }

        div()
            .id("sidebar-footer")
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .px(px(8.))
            .py(px(8.))
            .border_t(px(1.))
            .border_color(rgb(0x3d3d3d))
            .child(btn)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(4.))
                    .child(refresh_btn)
                    .child(gear),
            )
    }
}

impl IntoElement for Sidebar {
    type Element = Component<Self>;
    fn into_element(self) -> Self::Element {
        Component::new(self)
    }
}

const RUNNING_FRAMES: &[&str] = &["\u{25d0}", "\u{25d3}", "\u{25cf}", "\u{25d1}"];

/// Map worktree path to pane_id (local PTY: "local:{path}")
fn worktree_path_to_pane_id(path: &std::path::Path) -> String {
    format!("local:{}", path.display())
}

fn format_elapsed(instant: Instant) -> String {
    let elapsed = instant.elapsed();
    let secs = elapsed.as_secs();
    if secs < 60 {
        "Just now".to_string()
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86400 {
        format!("{}h", secs / 3600)
    } else {
        format!("{}d", secs / 86400)
    }
}

impl RenderOnce for Sidebar {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let worktrees = self.worktrees.lock().unwrap().clone();
        let pane_statuses = self.pane_statuses.lock().unwrap().clone();
        let selected = self.selected_index;
        let repo_name = self.repo_name.clone();
        let creating = self.creating_branch;
        let on_new_branch_ref = self.on_new_branch.as_ref();
        let _on_delete = self.on_delete.clone();
        let on_select = self.on_select.clone();
        let on_right_click = self.on_right_click.clone();
        let on_toggle_sidebar = self.on_toggle_sidebar.clone();
        let on_toggle_notifications = self.on_toggle_notifications.clone();
        let on_add_workspace = self.on_add_workspace.clone();
        let notification_count = self.notification_count;
        let running_animation_frame = self.running_animation_frame;
        let orphan_windows = self.orphan_windows.lock().unwrap().clone();
        let on_close_orphan = self.on_close_orphan.clone();
        let tasks_section = self.render_tasks_section();
        let pane_summaries = self.pane_summaries;

        let has_top_controls = on_toggle_sidebar.is_some()
            || on_toggle_notifications.is_some()
            || on_add_workspace.is_some();
        let top_section = if has_top_controls {
            Self::render_top_controls(
                on_toggle_sidebar,
                on_toggle_notifications,
                on_add_workspace,
                notification_count,
            )
            .into_any_element()
        } else {
            Self::render_header(&repo_name).into_any_element()
        };
        let on_refresh_ref = self.on_refresh.as_ref();
        let on_settings_ref = self.on_settings.as_ref();
        let footer =
            Self::render_footer(creating, on_new_branch_ref, on_refresh_ref, on_settings_ref);

        let mut rows: Vec<AnyElement> = Vec::new();
        for (idx, item) in worktrees.iter().enumerate() {
            let is_selected = selected == Some(idx);
            let pane_prefix = worktree_path_to_pane_id(&item.info.path);
            let status = AgentStatus::highest_priority_for_prefix(&pane_statuses, &pane_prefix);
            let mut item_with_status = item.clone();
            item_with_status.set_status(status);

            let status_color = item_with_status.status_color();
            let text_color = if is_selected {
                rgb(0xffffff)
            } else {
                rgb(0xcccccc)
            };
            let meta_color = if is_selected {
                rgb(0xbbbbbb)
            } else {
                rgb(0x888888)
            };

            // Find PaneSummary for this worktree (highest-priority pane)
            let colon_prefix = format!("{}:", pane_prefix);
            let summary = pane_summaries
                .iter()
                .filter(|(k, _)| *k == &pane_prefix || k.starts_with(&colon_prefix))
                .max_by_key(|(_, v)| v.status.priority())
                .map(|(_, v)| v);

            let last_message = summary
                .map(|s| {
                    if s.last_line.is_empty() {
                        item_with_status.status_text().to_string()
                    } else {
                        s.last_line.clone()
                    }
                })
                .unwrap_or_else(|| item_with_status.status_text().to_string());

            let last_time = summary
                .map(|s| format_elapsed(s.status_since))
                .unwrap_or_else(|| "\u{2014}".to_string());

            // Animated icon for Running status
            let status_icon_text = if item_with_status.status == AgentStatus::Running {
                RUNNING_FRAMES[running_animation_frame % RUNNING_FRAMES.len()]
            } else {
                item_with_status.status_icon()
            };

            let is_base = item_with_status.info.is_base;
            let inner = div()
                .flex()
                .flex_col()
                .gap(px(2.))
                .overflow_hidden()
                .w_full()
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(6.))
                        .overflow_hidden()
                        .child(
                            div()
                                .flex_shrink_0()
                                .text_size(px(11.))
                                .text_color(status_color)
                                .child(status_icon_text),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .overflow_hidden()
                                .text_ellipsis()
                                .text_size(px(12.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(text_color)
                                .child(SharedString::from(
                                    item_with_status.info.short_branch_name().to_string(),
                                )),
                        )
                        .when(is_base, |el| {
                            el.child(
                                div()
                                    .flex_shrink_0()
                                    .ml(px(4.))
                                    .px(px(4.))
                                    .py(px(1.))
                                    .rounded(px(3.))
                                    .bg(rgb(0x3d4556))
                                    .text_size(px(8.))
                                    .text_color(rgb(0x8899aa))
                                    .child("BASE"),
                            )
                        }),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(4.))
                        .pl(px(17.))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .text_size(px(10.))
                                .text_color(meta_color)
                                .line_height(px(14.))
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(SharedString::from(last_message)),
                        )
                        .child(
                            div()
                                .flex_shrink_0()
                                .text_size(px(9.))
                                .text_color(meta_color)
                                .child(SharedString::from(last_time)),
                        ),
                );

            let row_content = div()
                .flex_1()
                .min_w(px(0.))
                .overflow_hidden()
                .flex()
                .flex_row()
                .items_center()
                .child(inner);

            let mut row = div()
                .id(ElementId::from(idx))
                .mx(px(4.))
                .my(px(2.))
                .px(px(8.))
                .py(px(8.))
                .min_h(px(40.))
                .rounded(px(4.))
                .flex()
                .flex_row()
                .items_center()
                .gap(px(4.))
                .cursor_pointer();

            if let Some(on_select) = &on_select {
                let on_select = on_select.clone();
                row = row.on_click(move |_event, window, cx| {
                    on_select(idx, window, cx);
                });
            }

            row = row.child(row_content);

            if is_selected {
                row = row.bg(rgb(0x2c313a));
            } else {
                row = row.hover(|s: StyleRefinement| s.bg(rgb(0x262b33)));
            }

            if let Some(on_right_click) = &on_right_click {
                let on_right_click = on_right_click.clone();
                row = row.on_mouse_down(MouseButton::Right, move |event, window, cx| {
                    on_right_click(
                        idx,
                        f32::from(event.position.x),
                        f32::from(event.position.y),
                        window,
                        cx,
                    );
                });
            }

            rows.push(row.into_any_element());
        }

        // Orphan tmux windows (worktree removed externally) — show with close button
        if !orphan_windows.is_empty() {
            rows.push(
                div()
                    .id("sidebar-orphan-section-label")
                    .mx(px(4.))
                    .mt(px(12.))
                    .mb(px(4.))
                    .text_size(px(10.))
                    .text_color(rgb(0x888888))
                    .child("已删除的会话 (可关闭)")
                    .into_any_element(),
            );
            for win_name in &orphan_windows {
                let win_name_owned = win_name.clone();
                let mut row = div()
                    .id(format!("sidebar-orphan-{}", win_name.replace(' ', "-")))
                    .mx(px(4.))
                    .my(px(2.))
                    .px(px(8.))
                    .py(px(6.))
                    .rounded(px(4.))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(4.))
                    .min_h(px(36.))
                    .bg(rgb(0x2a2520))
                    .border_1()
                    .border_color(rgb(0x4d4030))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .overflow_hidden()
                            .text_ellipsis()
                            .text_size(px(12.))
                            .text_color(rgb(0xcccccc))
                            .child(SharedString::from(win_name.clone())),
                    );
                if let Some(ref on_close) = on_close_orphan {
                    let on_close = Arc::clone(on_close);
                    row = row.child(
                        div()
                            .id(format!(
                                "sidebar-orphan-close-{}",
                                win_name.replace(' ', "-")
                            ))
                            .px(px(4.))
                            .py(px(2.))
                            .text_size(px(10.))
                            .text_color(rgb(0x888888))
                            .hover(|s: StyleRefinement| s.text_color(rgb(0xffffff)))
                            .cursor_pointer()
                            .on_click(move |_event, window, cx| {
                                on_close(win_name_owned.as_str(), window, cx);
                            })
                            .child("×"),
                    );
                }
                rows.push(row.into_any_element());
            }
        }

        let list = div()
            .id("sidebar-list")
            .flex_1()
            .overflow_y_scroll()
            .py(px(4.))
            .children(rows);

        // Floating context menu: absolute positioned using the actual mouse Y from the right-click event

        div()
            .id("sidebar")
            .w_full()
            .h_full()
            .flex()
            .flex_col()
            .relative()
            .bg(rgb(0x252526))
            .child(top_section)
            .child(list)
            .child(tasks_section)
            .child(footer)
    }
}

impl Default for Sidebar {
    fn default() -> Self {
        Self::new("Repository", PathBuf::from("."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_sidebar_creation() {
        let sidebar = Sidebar::new("myproject", PathBuf::from("/tmp/project"));
        assert_eq!(sidebar.worktree_count(), 0);
        assert!(sidebar.selected_index().is_none());
    }

    #[test]
    fn test_set_worktrees() {
        let mut sidebar = Sidebar::new("myproject", PathBuf::from("/tmp/project"));
        sidebar.set_worktrees(vec![
            WorktreeInfo::new(PathBuf::from("/tmp/main"), "main", "abc"),
            WorktreeInfo::new(PathBuf::from("/tmp/feat"), "feature-x", "def"),
        ]);
        assert_eq!(sidebar.worktree_count(), 2);
    }

    #[test]
    fn test_select_worktree() {
        let mut sidebar = Sidebar::new("myproject", PathBuf::from("/tmp/project"));
        sidebar.set_worktrees(vec![
            WorktreeInfo::new(PathBuf::from("/tmp/main"), "main", "abc"),
            WorktreeInfo::new(PathBuf::from("/tmp/feat"), "feature-x", "def"),
        ]);
        sidebar.select(1);
        assert_eq!(sidebar.selected_index(), Some(1));
    }

    #[test]
    fn test_add_worktree() {
        let mut sidebar = Sidebar::new("myproject", PathBuf::from("/tmp/project"));
        sidebar.add_worktree(WorktreeInfo::new(PathBuf::from("/tmp/new"), "new", "xyz"));
        assert_eq!(sidebar.worktree_count(), 1);
    }

    #[test]
    fn test_remove_worktree() {
        let mut sidebar = Sidebar::new("myproject", PathBuf::from("/tmp/project"));
        sidebar.set_worktrees(vec![
            WorktreeInfo::new(PathBuf::from("/tmp/main"), "main", "abc"),
            WorktreeInfo::new(PathBuf::from("/tmp/feat"), "feature-x", "def"),
        ]);
        sidebar.remove_worktree(0);
        assert_eq!(sidebar.worktree_count(), 1);
    }

    #[test]
    fn test_creating_branch_state() {
        let mut sidebar = Sidebar::new("myproject", PathBuf::from("/tmp/project"));
        assert!(!sidebar.is_creating_branch());
        sidebar.set_creating_branch(true);
        assert!(sidebar.is_creating_branch());
        sidebar.set_creating_branch(false);
        assert!(!sidebar.is_creating_branch());
    }

    #[test]
    fn test_update_status() {
        let mut sidebar = Sidebar::new("myproject", PathBuf::from("/tmp/project"));
        sidebar.set_worktrees(vec![WorktreeInfo::new(
            PathBuf::from("/tmp/main"),
            "main",
            "abc",
        )]);
        sidebar.update_status(0, AgentStatus::Running);
        assert_eq!(
            sidebar.worktrees.lock().unwrap()[0].status,
            AgentStatus::Running
        );
    }

    #[test]
    fn test_worktree_item_creation() {
        let info = WorktreeInfo::new(PathBuf::from("/tmp/test"), "feature/test", "abc123");
        let item = WorktreeItem::new(info);
        assert_eq!(item.status, AgentStatus::Unknown);
    }

    #[test]
    fn test_worktree_item_status_icons() {
        let info = WorktreeInfo::new(PathBuf::from("/tmp/test"), "main", "abc");
        let mut item = WorktreeItem::new(info);
        assert_eq!(item.status_icon(), "?");

        item.set_status(AgentStatus::Running);
        assert_eq!(item.status_icon(), "●");

        item.set_status(AgentStatus::Error);
        assert_eq!(item.status_icon(), "✕");
    }

    #[test]
    fn test_formatted_branch_with_ahead() {
        let mut info = WorktreeInfo::new(PathBuf::from("/tmp/test"), "feature/test", "abc");
        info.ahead = 3;
        let item = WorktreeItem::new(info);
        assert_eq!(item.formatted_branch(), "feature/test · +3");
    }

    #[test]
    fn test_formatted_branch_without_ahead() {
        let info = WorktreeInfo::new(PathBuf::from("/tmp/test"), "feature/test", "abc");
        let item = WorktreeItem::new(info);
        assert_eq!(item.formatted_branch(), "feature/test");
    }

    #[test]
    fn test_status_text() {
        let info = WorktreeInfo::new(PathBuf::from("/tmp/test"), "main", "abc");
        let mut item = WorktreeItem::new(info);

        item.set_status(AgentStatus::Running);
        assert_eq!(item.status_text(), "Running");

        item.set_status(AgentStatus::Waiting);
        assert_eq!(item.status_text(), "Waiting");
    }

    #[test]
    fn test_repo_path_storage() {
        let repo_path = PathBuf::from("/tmp/myrepo");
        let sidebar = Sidebar::new("myrepo", repo_path.clone());
        assert_eq!(sidebar.repo_path, repo_path);
    }

    #[test]
    fn test_on_new_branch_callback() {
        let mut sidebar = Sidebar::new("myproject", PathBuf::from("/tmp/project"));
        sidebar.on_new_branch(|_window: &mut Window, _cx: &mut App| {});
        assert!(sidebar.on_new_branch.is_some());
    }

    #[test]
    fn test_on_select_callback() {
        let mut sidebar = Sidebar::new("myproject", PathBuf::from("/tmp/project"));
        sidebar.on_select(|idx: usize, _window: &mut Window, _cx: &mut App| {
            let _ = idx;
        });
        assert!(sidebar.on_select.is_some());
    }

    #[test]
    fn test_on_delete_callback() {
        let mut sidebar = Sidebar::new("myproject", PathBuf::from("/tmp/project"));
        sidebar.on_delete(|_idx: usize, _window: &mut Window, _cx: &mut App| {});
        assert!(sidebar.on_delete.is_some());
    }

    #[test]
    fn test_sidebar_status_aggregates_split_panes() {
        use std::collections::HashMap;

        let mut pane_statuses: HashMap<String, AgentStatus> = HashMap::new();
        // Primary pane is Idle, but split-0 has an Error
        pane_statuses.insert("local:/tmp/feat".to_string(), AgentStatus::Idle);
        pane_statuses.insert("local:/tmp/feat:split-0".to_string(), AgentStatus::Error);

        // Verify the helper picks Error over Idle
        let prefix = worktree_path_to_pane_id(std::path::Path::new("/tmp/feat"));
        let status = AgentStatus::highest_priority_for_prefix(&pane_statuses, &prefix);
        assert_eq!(status, AgentStatus::Error);
    }

    #[test]
    fn test_sidebar_status_primary_only_when_no_splits() {
        use std::collections::HashMap;

        let mut pane_statuses: HashMap<String, AgentStatus> = HashMap::new();
        pane_statuses.insert("local:/tmp/feat".to_string(), AgentStatus::Running);

        let prefix = worktree_path_to_pane_id(std::path::Path::new("/tmp/feat"));
        let status = AgentStatus::highest_priority_for_prefix(&pane_statuses, &prefix);
        assert_eq!(status, AgentStatus::Running);
    }
}
