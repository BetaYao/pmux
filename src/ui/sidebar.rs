// ui/sidebar.rs - Sidebar component for worktree list with GPUI render
// Event Bus driven: status_change broadcast triggers debounced parent notify (see app_root)
use gpui::prelude::*;
use gpui::{px, svg, *};
use crate::worktree::{WorktreeInfo, get_diff_stats};
use crate::agent_status::AgentStatus;
use crate::new_branch_orchestrator::NewBranchOrchestrator;
use crate::notification_manager::NotificationManager;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

/// Worktree item with status
#[derive(Clone)]
pub struct WorktreeItem {
    pub info: WorktreeInfo,
    pub status: AgentStatus,
}

impl WorktreeItem {
    pub fn new(info: WorktreeInfo) -> Self {
        Self { info, status: AgentStatus::Unknown }
    }

    pub fn set_status(&mut self, status: AgentStatus) { self.status = status; }

    pub fn status_icon(&self) -> &'static str {
        match self.status {
            AgentStatus::Running => "●",
            AgentStatus::Waiting => "◐",
            AgentStatus::Idle => "○",
            AgentStatus::Error => "✕",
            AgentStatus::Unknown => "?",
        }
    }

    pub fn status_color(&self) -> Rgba {
        match self.status {
            AgentStatus::Running => rgb(0x4caf50),
            AgentStatus::Waiting => rgb(0xffc107),
            AgentStatus::Idle => rgb(0x9e9e9e),
            AgentStatus::Error => rgb(0xf44336),
            AgentStatus::Unknown => rgb(0x9c27b0),
        }
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
        match self.status {
            AgentStatus::Running => "Running",
            AgentStatus::Waiting => "Waiting for input",
            AgentStatus::Idle => "Idle",
            AgentStatus::Error => "Error detected",
            AgentStatus::Unknown => "Unknown",
        }
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
    on_delete: Option<Arc<dyn Fn(usize, &mut Window, &mut App) + Send + Sync>>,
    on_view_diff: Option<Arc<dyn Fn(usize, &mut Window, &mut App) + Send + Sync>>,
    on_right_click: Option<Arc<dyn Fn(usize, &mut Window, &mut App) + Send + Sync>>,
    /// Which worktree index has context menu open (from parent state)
    context_menu_for: Option<usize>,
    creating_branch: bool,
    /// Store original worktree info for access in callbacks
    worktrees_info: Arc<Mutex<Vec<crate::worktree::WorktreeInfo>>>,
    /// Top control row callbacks (cmux style)
    on_toggle_sidebar: Option<Arc<dyn Fn(&mut Window, &mut App) + Send + Sync>>,
    on_toggle_notifications: Option<Arc<dyn Fn(&mut Window, &mut App) + Send + Sync>>,
    on_add_workspace: Option<Arc<dyn Fn(&mut Window, &mut App) + Send + Sync>>,
    notification_count: usize,
    /// For last message and timestamp per worktree
    notification_manager: Option<Arc<Mutex<NotificationManager>>>,
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
            notification_manager: None,
        }
    }

    pub fn with_notification_manager(mut self, mgr: Arc<Mutex<NotificationManager>>) -> Self {
        self.notification_manager = Some(mgr);
        self
    }

    pub fn on_toggle_sidebar<F: Fn(&mut Window, &mut App) + Send + Sync + 'static>(mut self, f: F) -> Self {
        self.on_toggle_sidebar = Some(Arc::new(f));
        self
    }
    pub fn on_toggle_notifications<F: Fn(&mut Window, &mut App) + Send + Sync + 'static>(mut self, f: F) -> Self {
        self.on_toggle_notifications = Some(Arc::new(f));
        self
    }
    pub fn on_add_workspace<F: Fn(&mut Window, &mut App) + Send + Sync + 'static>(mut self, f: F) -> Self {
        self.on_add_workspace = Some(Arc::new(f));
        self
    }
    pub fn with_notification_count(mut self, count: usize) -> Self {
        self.notification_count = count;
        self
    }

    pub fn with_statuses(mut self, pane_statuses: Arc<Mutex<HashMap<String, AgentStatus>>>) -> Self {
        self.pane_statuses = pane_statuses;
        self
    }

    pub fn set_worktrees(&mut self, worktrees: Vec<WorktreeInfo>) {
        // Clone the worktrees for WorktreeInfo since we need to store them
        if let Ok(mut guard) = self.worktrees_info.lock() { *guard = worktrees.clone(); }
        
        // Create WorktreeItems for display
        let items: Vec<WorktreeItem> = worktrees.iter().cloned().map(WorktreeItem::new).collect();
        if let Ok(mut guard) = self.worktrees.lock() { *guard = items; }
    }

    pub fn update_status(&mut self, index: usize, status: AgentStatus) {
        if let Ok(mut guard) = self.worktrees.lock() {
            if let Some(item) = guard.get_mut(index) { item.set_status(status); }
        }
    }

    pub fn select(&mut self, index: usize) {
        let len = self.worktrees.lock().map(|g| g.len()).unwrap_or(0);
        if index < len { self.selected_index = Some(index); }
    }

    pub fn selected_index(&self) -> Option<usize> { self.selected_index }

    pub fn on_select<F>(&mut self, callback: F)
    where
        F: Fn(usize, &mut Window, &mut App) + Send + Sync + 'static,
    {
        self.on_select = Some(Arc::new(callback));
    }

    pub fn on_delete<F: Fn(usize, &mut Window, &mut App) + Send + Sync + 'static>(&mut self, callback: F) {
        self.on_delete = Some(Arc::new(callback));
    }

    pub fn on_view_diff<F: Fn(usize, &mut Window, &mut App) + Send + Sync + 'static>(&mut self, callback: F) {
        self.on_view_diff = Some(Arc::new(callback));
    }

    pub fn on_right_click<F: Fn(usize, &mut Window, &mut App) + Send + Sync + 'static>(&mut self, callback: F) {
        self.on_right_click = Some(Arc::new(callback));
    }

    pub fn with_context_menu(mut self, index: Option<usize>) -> Self {
        self.context_menu_for = index;
        self
    }

    pub fn on_new_branch<F: Fn(&mut Window, &mut App) + Send + Sync + 'static>(&mut self, callback: F) {
        self.on_new_branch = Some(Arc::new(callback));
    }

    pub fn add_worktree(&mut self, info: WorktreeInfo) {
        if let Ok(mut guard) = self.worktrees.lock() { guard.push(WorktreeItem::new(info)); }
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
        
        let converted: Vec<WorktreeInfo> = worktrees.iter().map(|wt| {
            WorktreeInfo::new(
                wt.path.clone(),
                wt.branch.as_str(),
                wt.commit.as_deref().unwrap_or("unknown")
            )
        }).collect();
        
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

    fn render_diff_stats(add: u32, del: u32, files: u32, meta_color: Rgba) -> impl IntoElement {
        let mut row = div().flex().flex_row().items_center().gap(px(4.)).text_size(px(10.));
        if add == 0 && del == 0 && files == 0 {
            row = row.child(div().text_color(meta_color).child("—"));
        } else {
            if add > 0 {
                row = row.child(div().text_color(rgb(0x4caf50)).child(format!("+{}", add)));
            }
            if del > 0 {
                row = row.child(div().text_color(rgb(0xf44336)).child(format!("-{}", del)));
            }
            if files > 0 {
                row = row.child(div().text_color(meta_color).child(format!(" · {} File{}", files, if files == 1 { "" } else { "s" })));
            }
        }
        row
    }

    fn render_header(repo_name: &str) -> Div {
        div()
            .flex().flex_row().items_center()
            .px(px(12.)).py(px(10.))
            .border_b(px(1.)).border_color(rgb(0x3d3d3d))
            .child(
                div()
                    .text_size(px(13.)).font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(0xffffff))
                    .child(SharedString::from(format!("{}", repo_name)))
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
            .border_color(rgb(0x3d3d3d))
            .bg(rgb(0x252526));

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
                    .when(!has_notifications, |el: Stateful<Div>| el.hover(|s: StyleRefinement| s.bg(rgb(0x3d3d3d))))
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
        let text_color = if is_selected { rgb(0xffffff) } else { rgb(0xcccccc) };
        let status_text_color = if is_selected { rgb(0xbbbbbb) } else { rgb(0x888888) };

        let inner = div()
            .flex().flex_col().gap(px(2.))
            .child(
                div().flex().flex_row().items_center().gap(px(6.))
                    .child(div().text_size(px(11.)).text_color(status_color).child(item.status_icon()))
                    .child(div().flex_1().text_size(px(12.)).text_color(text_color).child(SharedString::from(item.formatted_branch())))
            )
            .child(div().pl(px(17.)).text_size(px(10.)).text_color(status_text_color).child(item.status_text()));

        let row = div()
            .id(ElementId::from(idx))
            .mx(px(4.)).my(px(2.)).px(px(8.)).py(px(6.))
            .rounded(px(4.))
            .child(inner);

        if is_selected {
            row.bg(rgb(0x094771))
        } else {
            row.hover(|s: StyleRefinement| s.bg(rgb(0x2a2d2e)))
        }
    }

    fn render_context_menu(
        idx: usize,
        on_view_diff: Option<Arc<dyn Fn(usize, &mut Window, &mut App) + Send + Sync>>,
        on_delete: Option<Arc<dyn Fn(usize, &mut Window, &mut App) + Send + Sync>>,
        worktrees_info: &[crate::worktree::WorktreeInfo],
    ) -> impl IntoElement {
        let mut menu = div()
            .id(format!("sidebar-context-menu-{}", idx))
            .mx(px(4.)).my(px(2.)).px(px(4.)).py(px(4.))
            .rounded(px(4.))
            .bg(rgb(0x2d2d2d))
            .border_1().border_color(rgb(0x3d3d3d))
            .flex().flex_col().gap(px(2.));

        // Only show View Diff for non-main worktrees (main...HEAD is empty for main branch)
        let show_view_diff = on_view_diff.is_some()
            && !worktrees_info.get(idx).map(|w| w.is_main).unwrap_or(true);
        if let Some(on_view_diff) = on_view_diff.filter(|_| show_view_diff) {
            let item = div()
                .id(format!("context-menu-view-diff-{}", idx))
                .px(px(8.)).py(px(6.))
                .text_size(px(12.)).text_color(rgb(0xcccccc))
                .hover(|s: StyleRefinement| s.bg(rgb(0x3d3d3d)))
                .cursor_pointer()
                .on_click(move |_event, window, cx| {
                    on_view_diff(idx, window, cx);
                })
                .child("View Diff");
            menu = menu.child(item);
        }
        if let Some(ref on_delete) = on_delete {
            let on_delete = Arc::clone(on_delete);
            let item = div()
                .id(format!("context-menu-remove-{}", idx))
                .px(px(8.)).py(px(6.))
                .text_size(px(12.)).text_color(rgb(0xcccccc))
                .hover(|s: StyleRefinement| s.bg(rgb(0x3d3d3d)))
                .cursor_pointer()
                .on_click(move |_event, window, cx| {
                    on_delete(idx, window, cx);
                })
                .child("Remove Worktree");
            menu = menu.child(item);
        }
        menu
    }

    fn render_footer(creating: bool, on_new_branch: Option<&Arc<dyn Fn(&mut Window, &mut App) + Send + Sync>>) -> Stateful<Div> {
        let mut btn = div()
            .id("new-branch-btn")
            .px(px(12.)).py(px(6.)).rounded(px(4.))
            .when(!creating, |this| {
                this.bg(rgb(0x0e639c))
                    .hover(|s: StyleRefinement| s.bg(rgb(0x1177bb)))
                    .cursor_pointer()
            })
            .when(creating, |this| {
                this.bg(rgb(0x3d3d3d))
            })
            .text_color(rgb(0xffffff)).text_size(px(11.))
            .child(if creating { "Creating..." } else { "+ New Branch" });

        // Add click handler if not creating and callback exists
        if !creating {
            if let Some(callback) = on_new_branch {
                let cb = Arc::clone(callback);
                btn = btn.on_click(move |_, window, cx| {
                    cb(window, cx);
                });
            }
        }

        div()
            .id("sidebar-footer")
            .flex().flex_row().items_center().justify_center()
            .px(px(8.)).py(px(8.))
            .border_t(px(1.)).border_color(rgb(0x3d3d3d))
            .child(btn)
    }
}

impl IntoElement for Sidebar {
    type Element = Component<Self>;
    fn into_element(self) -> Self::Element { Component::new(self) }
}

/// Map worktree path to pane_id (local PTY: "local:{path}")
fn worktree_path_to_pane_id(path: &std::path::Path) -> String {
    format!("local:{}", path.display())
}

fn format_elapsed(instant: Instant) -> String {
    let elapsed = instant.elapsed();
    let secs = elapsed.as_secs();
    if secs < 60 {
        "Now".to_string()
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86400 {
        format!("{}h", secs / 3600)
    } else {
        format!("{}d", secs / 86400)
    }
}

fn format_diff_stats(add: u32, del: u32, files: u32) -> String {
    if add == 0 && del == 0 && files == 0 {
        "—".to_string()
    } else {
        let mut parts = Vec::new();
        if add > 0 {
            parts.push(format!("+{}", add));
        }
        if del > 0 {
            parts.push(format!("-{}", del));
        }
        let change_part = if parts.is_empty() {
            "—".to_string()
        } else {
            parts.join(" ")
        };
        if files > 0 {
            format!("{} · {} File{}", change_part, files, if files == 1 { "" } else { "s" })
        } else {
            change_part
        }
    }
}

impl RenderOnce for Sidebar {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let worktrees = self.worktrees.lock().unwrap().clone();
        let worktrees_info = self.worktrees_info.lock().unwrap().clone();
        let pane_statuses = self.pane_statuses.lock().unwrap().clone();
        let selected = self.selected_index;
        let repo_name = self.repo_name.clone();
        let creating = self.creating_branch;
        let on_new_branch_ref = self.on_new_branch.as_ref();
        let on_delete = self.on_delete.clone();
        let on_select = self.on_select.clone();
        let on_view_diff = self.on_view_diff.clone();
        let on_right_click = self.on_right_click.clone();
        let context_menu_for = self.context_menu_for;
        let on_toggle_sidebar = self.on_toggle_sidebar.clone();
        let on_toggle_notifications = self.on_toggle_notifications.clone();
        let on_add_workspace = self.on_add_workspace.clone();
        let notification_count = self.notification_count;
        let notification_manager = self.notification_manager.clone();

        let has_top_controls = on_toggle_sidebar.is_some() || on_toggle_notifications.is_some() || on_add_workspace.is_some();
        let top_section = if has_top_controls {
            Self::render_top_controls(on_toggle_sidebar, on_toggle_notifications, on_add_workspace, notification_count).into_any_element()
        } else {
            Self::render_header(&repo_name).into_any_element()
        };
        let footer = Self::render_footer(creating, on_new_branch_ref);

        let mut rows: Vec<AnyElement> = Vec::new();
        for (idx, item) in worktrees.iter().enumerate() {
            let is_selected = selected == Some(idx);
            let pane_id = worktree_path_to_pane_id(&item.info.path);
            let status = pane_statuses.get(&pane_id).copied().unwrap_or(AgentStatus::Unknown);
            let mut item_with_status = item.clone();
            item_with_status.set_status(status);

            let status_color = item_with_status.status_color();
            let text_color = if is_selected { rgb(0xffffff) } else { rgb(0xcccccc) };
            let meta_color = if is_selected { rgb(0xbbbbbb) } else { rgb(0x888888) };
            let path_color = if is_selected { rgb(0xaaaaaa) } else { rgb(0x666666) };

            let (last_message, last_time) = notification_manager.as_ref().and_then(|mgr| {
                mgr.lock().ok().and_then(|m| {
                    m.by_pane(&pane_id).first().map(|n| {
                        (n.display_message(), format_elapsed(n.timestamp()))
                    })
                })
            }).unwrap_or_else(|| (item_with_status.status_text().to_string(), "—".to_string()));

            let (add, del, files) = get_diff_stats(&item.info.path).unwrap_or((0, 0, 0));
            let diff_str = format_diff_stats(add, del, files);

            let inner = div()
                .flex().flex_col().gap(px(2.))
                .child(
                    div().flex().flex_row().items_center().gap(px(6.))
                        .child(div().text_size(px(11.)).text_color(status_color).child(item_with_status.status_icon()))
                        .child(div().flex_1().text_size(px(12.)).font_weight(FontWeight::SEMIBOLD).text_color(text_color).child(SharedString::from(item_with_status.formatted_branch())))
                )
                .child(div().pl(px(17.)).text_size(px(10.)).text_color(meta_color).line_height(px(14.)).child(SharedString::from(last_message)))
                .child(
                    div().pl(px(17.)).flex().flex_row().items_center().justify_between().gap(px(4.))
                        .child(Self::render_diff_stats(add, del, files, meta_color))
                        .child(div().text_size(px(10.)).text_color(meta_color).flex_shrink_0().child(last_time))
                )
                .child(div().pl(px(17.)).text_size(px(10.)).text_color(path_color).font_family(".AppleSystemUIFontMonospaced").child(item.info.display_path()));

            let row_content = div()
                .flex_1()
                .flex()
                .flex_row()
                .items_center()
                .child(inner);

            let mut row = div()
                .id(ElementId::from(idx))
                .mx(px(4.)).my(px(2.)).px(px(8.)).py(px(8.))
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

            if let Some(ref on_delete) = on_delete {
                let on_delete = Arc::clone(on_delete);
                let delete_btn = div()
                    .id(format!("sidebar-delete-{}", idx))
                    .px(px(4.)).py(px(2.))
                    .text_size(px(10.)).text_color(rgb(0x666666))
                    .hover(|s: StyleRefinement| s.text_color(rgb(0xffffff)))
                    .cursor_pointer()
                    .on_click(move |_event, window, cx| {
                        on_delete(idx, window, cx);
                    })
                    .child("×");
                row = row.child(delete_btn);
            }

            if is_selected {
                row = row.bg(rgb(0x094771));
            } else {
                row = row.hover(|s: StyleRefinement| s.bg(rgb(0x2a2d2e)));
            }

            if let Some(on_right_click) = &on_right_click {
                let on_right_click = on_right_click.clone();
                row = row.on_mouse_down(MouseButton::Right, move |_event, window, cx| {
                    on_right_click(idx, window, cx);
                });
            }

            rows.push(row.into_any_element());

            if context_menu_for == Some(idx) {
                let menu_row = Self::render_context_menu(idx, on_view_diff.clone(), on_delete.clone(), &worktrees_info);
                rows.push(menu_row.into_any_element());
            }
        }

        let list = div()
            .id("sidebar-list")
            .flex_1()
            .overflow_y_scroll()
            .py(px(4.))
            .children(rows);

        div()
            .id("sidebar")
            .w_full().h_full().flex().flex_col()
            .bg(rgb(0x252526))
            .child(top_section)
            .child(list)
            .child(footer)
    }
}

impl Default for Sidebar {
    fn default() -> Self { Self::new("Repository", PathBuf::from(".")) }
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
        sidebar.set_worktrees(vec![
            WorktreeInfo::new(PathBuf::from("/tmp/main"), "main", "abc"),
        ]);
        sidebar.update_status(0, AgentStatus::Running);
        assert_eq!(sidebar.worktrees.lock().unwrap()[0].status, AgentStatus::Running);
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
        assert_eq!(item.status_text(), "Waiting for input");
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
}