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

        div()
            .id(format!("workspace-tab-{}", index))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(4.))
            .px(px(12.))
            .py(px(8.))
            .flex_1()
            .min_w(px(80.))
            .max_w(px(200.))
            .when(is_active, |el: Stateful<Div>| {
                el.bg(rgb(0x2d2d2d))
                    .border_b_2()
                    .border_color(rgb(0x0066cc))
            })
            .when(!is_active, |el: Stateful<Div>| {
                el.bg(rgb(0x1e1e1e))
                    .hover(|style: StyleRefinement| style.bg(rgb(0x2d2d2d)))
            })
            .cursor_pointer()
            .on_click(move |_, window, cx| { on_select(index, window, cx); })
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .text_ellipsis()
                    .text_size(px(12.))
                    .text_color(if is_active { rgb(0xffffff) } else { rgb(0xaaaaaa) })
                    .child(SharedString::from(if is_modified { format!("{} ●", name) } else { name }))
            )
            .child(
                div()
                    .id(format!("close-workspace-tab-{}", index))
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
            .h(px(36.))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(2.))
            .bg(rgb(0x1e1e1e))
            .border_b_1()
            .border_color(rgb(0x3d3d3d))
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
