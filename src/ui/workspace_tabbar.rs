// ui/workspace_tabbar.rs - Workspace tab bar for Content area (Chrome/ghostty style)
use gpui::prelude::*;
use gpui::*;
use std::sync::Arc;
use crate::workspace_manager::{WorkspaceManager, WorkspaceTab};

/// WorkspaceTabBar - full-width workspace tabs in Content area (cmux style)
pub struct WorkspaceTabBar {
    workspace_manager: WorkspaceManager,
    on_select_tab: Arc<dyn Fn(usize, &mut Window, &mut App)>,
    on_close_tab: Arc<dyn Fn(usize, &mut Window, &mut App)>,
}

impl WorkspaceTabBar {
    pub fn new(workspace_manager: WorkspaceManager) -> Self {
        Self {
            workspace_manager,
            on_select_tab: Arc::new(|_, _, _| {}),
            on_close_tab: Arc::new(|_, _, _| {}),
        }
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

    fn render_tab(&self, tab: &WorkspaceTab, index: usize, is_active: bool) -> impl IntoElement {
        let name = tab.name().to_string();
        let is_modified = tab.is_modified();
        let on_select = self.on_select_tab.clone();
        let on_close = self.on_close_tab.clone();
        let shortcut = if index < 8 { Some(format!("⌘{}", index + 1)) } else { None };

        div()
            .id(format!("workspace-tab-{}", index))
            .flex()
            .flex_row()
            .items_center()
            .px(px(12.))
            .py(px(6.))
            .flex_1()
            .min_w(px(0.))
            .rounded(px(6.))
            .when(is_active, |el: Stateful<Div>| {
                el.bg(rgb(0x3a3a3a))
            })
            .when(!is_active, |el: Stateful<Div>| {
                el.hover(|style: StyleRefinement| style.bg(rgb(0x2a2a2a)))
            })
            .cursor_pointer()
            .on_click(move |_, window, cx| { on_select(index, window, cx); })
            // Left: shortcut
            .when(shortcut.is_some(), |el: Stateful<Div>| {
                let sc = shortcut.clone().unwrap();
                el.child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(0x666666))
                        .mr(px(8.))
                        .flex_shrink_0()
                        .child(sc)
                )
            })
            // Center: tab name
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .overflow_hidden()
                    .text_ellipsis()
                    .text_size(px(12.))
                    .text_color(if is_active { rgb(0xffffff) } else { rgb(0xaaaaaa) })
                    .child(SharedString::from(if is_modified { format!("{} ●", name) } else { name }))
            )
            // Right: close button
            .child(
                div()
                    .id(format!("close-workspace-tab-{}", index))
                    .ml(px(8.))
                    .px(px(3.))
                    .py(px(1.))
                    .rounded(px(3.))
                    .text_size(px(12.))
                    .text_color(rgb(0x555555))
                    .flex_shrink_0()
                    .hover(|style: StyleRefinement| style.text_color(rgb(0xffffff)).bg(rgb(0x555555)))
                    .cursor_pointer()
                    .on_click(move |_, window, cx| { on_close(index, window, cx); })
                    .child("×")
            )
    }
}

impl IntoElement for WorkspaceTabBar {
    type Element = Component<Self>;
    fn into_element(self) -> Self::Element {
        Component::new(self)
    }
}

impl RenderOnce for WorkspaceTabBar {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        div()
            .id("workspace-tab-bar")
            .w_full()
            .h(px(40.))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(2.))
            .px(px(6.))
            .bg(rgb(0x1e1e1e))
            .border_b_1()
            .border_color(rgb(0x2a2a2a))
            .children(
                (0..self.workspace_manager.tab_count())
                    .filter_map(|i| {
                        self.workspace_manager.get_tab(i).map(|tab| {
                            let is_active = self.workspace_manager.active_tab_index() == Some(i);
                            self.render_tab(tab, i, is_active).into_any_element()
                        })
                    })
                    .collect::<Vec<_>>()
            )
    }
}
