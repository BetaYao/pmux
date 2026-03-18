// ui/topbar.rs - TopBar component with workspace tabs and status overview
use gpui::prelude::*;
use gpui::{App, Component, Div, FontWeight, Stateful, StyleRefinement, Window, div, px, rgb};
use std::sync::Arc;
use crate::agent_status::StatusCounts;
use crate::workspace_manager::{WorkspaceManager, WorkspaceTab};

/// TopBar component - shows workspace tabs and global controls
pub struct TopBar {
    workspace_manager: WorkspaceManager,
    status_counts: StatusCounts,
    /// Override for notification bell count (from NotificationManager). If None, uses status_counts.error + status_counts.waiting
    notification_count_override: Option<usize>,
    on_add_workspace: Arc<dyn Fn(&mut Window, &mut App)>,
    on_select_tab: Arc<dyn Fn(usize, &mut Window, &mut App)>,
    on_close_tab: Arc<dyn Fn(usize, &mut Window, &mut App)>,
    on_toggle_sidebar: Arc<dyn Fn(&mut Window, &mut App)>,
    on_toggle_notifications: Arc<dyn Fn(&mut Window, &mut App)>,
}

impl TopBar {
    pub fn new(workspace_manager: WorkspaceManager) -> Self {
        Self {
            workspace_manager,
            status_counts: StatusCounts::new(),
            notification_count_override: None,
            on_add_workspace: Arc::new(|_, _| {}),
            on_select_tab: Arc::new(|_, _, _| {}),
            on_close_tab: Arc::new(|_, _, _| {}),
            on_toggle_sidebar: Arc::new(|_, _| {}),
            on_toggle_notifications: Arc::new(|_, _| {}),
        }
    }

    pub fn with_status_counts(mut self, counts: StatusCounts) -> Self {
        self.status_counts = counts;
        self
    }

    pub fn with_notification_count(mut self, count: usize) -> Self {
        self.notification_count_override = Some(count);
        self
    }

    pub fn on_add_workspace<F>(mut self, callback: F) -> Self
    where F: Fn(&mut Window, &mut App) + 'static {
        self.on_add_workspace = Arc::new(callback);
        self
    }

    pub fn on_select_tab<F>(mut self, callback: F) -> Self
    where F: Fn(usize, &mut Window, &mut App) + 'static {
        self.on_select_tab = Arc::new(callback);
        self
    }

    pub fn on_close_tab<F>(mut self, callback: F) -> Self
    where F: Fn(usize, &mut Window, &mut App) + 'static {
        self.on_close_tab = Arc::new(callback);
        self
    }

    pub fn on_toggle_sidebar<F>(mut self, callback: F) -> Self
    where F: Fn(&mut Window, &mut App) + 'static {
        self.on_toggle_sidebar = Arc::new(callback);
        self
    }

    pub fn on_toggle_notifications<F>(mut self, callback: F) -> Self
    where F: Fn(&mut Window, &mut App) + 'static {
        self.on_toggle_notifications = Arc::new(callback);
        self
    }

    pub fn update_status_counts(&mut self, counts: StatusCounts) {
        self.status_counts = counts;
    }

    fn notification_count(&self) -> usize {
        self.notification_count_override.unwrap_or_else(|| {
            self.status_counts.error + self.status_counts.waiting
        })
    }

    fn render_workspace_tab(&self, tab: &WorkspaceTab, index: usize, is_active: bool) -> impl IntoElement {
        let name = tab.name().to_string();
        let is_modified = tab.is_modified();
        let on_select = self.on_select_tab.clone();
        let on_close = self.on_close_tab.clone();

        div()
            .id(format!("workspace-tab-{}", index))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(4.))
            .px(px(12.))
            .py(px(6.))
            .when(is_active, |el: Stateful<Div>| {
                el.bg(rgb(0x3d3d3d))
                    .border_b_2()
                    .border_color(rgb(0x0066cc))
            })
            .when(!is_active, |el: Stateful<Div>| {
                el.bg(rgb(0x2d2d2d))
                    .hover(|style: StyleRefinement| style.bg(rgb(0x353535)))
            })
            .cursor_pointer()
            .on_click(move |_, window, cx| { on_select(index, window, cx); })
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(if is_active { rgb(0xffffff) } else { rgb(0xaaaaaa) })
                    .child(format!("📁 {}", name))
            )
            .when(is_modified, |el: Stateful<Div>| {
                el.child(
                    div()
                        .text_size(px(10.))
                        .text_color(rgb(0x0066cc))
                        .child("●")
                )
            })
            .child(
                div()
                    .id(format!("close-tab-{}", index))
                    .ml(px(4.))
                    .px(px(4.))
                    .text_size(px(11.))
                    .text_color(rgb(0x888888))
                    .hover(|style: StyleRefinement| style.text_color(rgb(0xffffff)))
                    .cursor_pointer()
                    .on_click(move |_, window, cx| { on_close(index, window, cx); })
                    .child("×")
            )
    }
}

impl IntoElement for TopBar {
    type Element = Component<Self>;
    fn into_element(self) -> Self::Element {
        Component::new(self)
    }
}

impl RenderOnce for TopBar {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let notification_count = self.notification_count();
        let has_notifications = notification_count > 0;
        let on_add_workspace = self.on_add_workspace.clone();
        let on_toggle_sidebar = self.on_toggle_sidebar.clone();
        let on_toggle_notifications = self.on_toggle_notifications.clone();

        div()
            .id("top-bar")
            .w_full()
            .h(px(36.))
            .flex()
            .flex_row()
            .items_center()
            .px(px(8.))
            .gap(px(8.))
            .bg(rgb(0x252525))
            .border_b_1()
            .border_color(rgb(0x3d3d3d))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        div()
                            .id("toggle-sidebar-btn")
                            .px(px(8.))
                            .py(px(4.))
                            .rounded(px(4.))
                            .hover(|style: StyleRefinement| style.bg(rgb(0x3d3d3d)))
                            .cursor_pointer()
                            .on_click(move |_, window, cx| { on_toggle_sidebar(window, cx); })
                            .child(div().text_size(px(14.)).text_color(rgb(0xcccccc)).child("≡"))
                    )
                    .child(
                        div()
                            .id("notification-btn")
                            .px(px(8.))
                            .py(px(4.))
                            .rounded(px(4.))
                            .when(has_notifications, |el: Stateful<Div>| el.bg(rgb(0x3a1111)))
                            .when(!has_notifications, |el: Stateful<Div>| {
                                el.hover(|style: StyleRefinement| style.bg(rgb(0x3d3d3d)))
                            })
                            .cursor_pointer()
                            .on_click(move |_, window, cx| { on_toggle_notifications(window, cx); })
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap(px(4.))
                                    .child(
                                        div()
                                            .text_size(px(12.))
                                            .text_color(if has_notifications { rgb(0xff4444) } else { rgb(0xcccccc) })
                                            .child("🔔")
                                    )
                                    .when(has_notifications, |el: Div| {
                                        el.child(
                                            div()
                                                .text_size(px(10.))
                                                .text_color(rgb(0xff4444))
                                                .font_weight(FontWeight::BOLD)
                                                .child(format!("{}", notification_count))
                                        )
                                    })
                            )
                    )
                    .child(
                        div()
                            .id("add-workspace-btn")
                            .px(px(8.))
                            .py(px(4.))
                            .rounded(px(4.))
                            .hover(|style: StyleRefinement| style.bg(rgb(0x3d3d3d)))
                            .cursor_pointer()
                            .on_click(move |_, window, cx| { on_add_workspace(window, cx); })
                            .child(div().text_size(px(12.)).text_color(rgb(0xcccccc)).child("📂"))
                    )
            )
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(2.))
                    .overflow_x_hidden()
                    .children(
                        (0..self.workspace_manager.tab_count())
                            .filter_map(|i| {
                                self.workspace_manager.get_tab(i).map(|tab| {
                                    let is_active = self.workspace_manager.active_tab_index() == Some(i);
                                    self.render_workspace_tab(tab, i, is_active).into_any_element()
                                })
                            })
                            .collect::<Vec<_>>()
                    )
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(4.))
                    .child(div().px(px(6.)).py(px(4.)).rounded(px(4.)).hover(|s: StyleRefinement| s.bg(rgb(0x3d3d3d))).cursor_pointer().child(div().text_size(px(11.)).text_color(rgb(0x888888)).child("⌨")))
                    .child(div().px(px(6.)).py(px(4.)).rounded(px(4.)).hover(|s: StyleRefinement| s.bg(rgb(0x3d3d3d))).cursor_pointer().child(div().text_size(px(11.)).text_color(rgb(0x888888)).child("⊞")))
                    .child(div().px(px(6.)).py(px(4.)).rounded(px(4.)).hover(|s: StyleRefinement| s.bg(rgb(0x3d3d3d))).cursor_pointer().child(div().text_size(px(11.)).text_color(rgb(0x888888)).child("⊟")))
            )
    }
}

impl Default for TopBar {
    fn default() -> Self {
        Self::new(WorkspaceManager::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_topbar_creation() {
        let manager = WorkspaceManager::new();
        let topbar = TopBar::new(manager);
        assert_eq!(topbar.notification_count(), 0);
    }

    #[test]
    fn test_topbar_with_status() {
        use crate::agent_status::AgentStatus;
        let manager = WorkspaceManager::new();
        let mut counts = StatusCounts::new();
        counts.increment(&AgentStatus::Error);
        counts.increment(&AgentStatus::Waiting);
        let topbar = TopBar::new(manager).with_status_counts(counts);
        assert_eq!(topbar.notification_count(), 2);
    }

    #[test]
    fn test_topbar_with_workspaces() {
        let mut manager = WorkspaceManager::new();
        manager.add_workspace(PathBuf::from("/tmp/project1"));
        manager.add_workspace(PathBuf::from("/tmp/project2"));
        let topbar = TopBar::new(manager);
        assert_eq!(topbar.workspace_manager.tab_count(), 2);
    }

    #[test]
    fn test_callback_registration() {
        let manager = WorkspaceManager::new();
        let called = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let called_clone = called.clone();
        let _topbar = TopBar::new(manager).on_add_workspace(move |_, _| {
            called_clone.store(true, std::sync::atomic::Ordering::SeqCst);
        });
        assert!(!called.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[test]
    fn test_update_status_counts() {
        use crate::agent_status::AgentStatus;
        let manager = WorkspaceManager::new();
        let mut topbar = TopBar::new(manager);
        assert_eq!(topbar.notification_count(), 0);
        let mut counts = StatusCounts::new();
        counts.increment(&AgentStatus::Error);
        topbar.update_status_counts(counts);
        assert_eq!(topbar.notification_count(), 1);
    }
}
