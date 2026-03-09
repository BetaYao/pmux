// ui/topbar_entity.rs - TopBar Entity that observes StatusCountsModel
// Phase 2: Entity with workspace tabs, status count; re-renders only when model notifies
use crate::agent_status::StatusCounts;
use crate::ui::models::StatusCountsModel;
use crate::workspace_manager::{WorkspaceManager, WorkspaceTab};
use gpui::prelude::*;
use gpui::*;
use std::sync::Arc;

/// TopBar Entity - observes StatusCountsModel; re-renders only when model notifies.
/// Shows workspace tabs and status count.
pub struct TopBarEntity {
    #[allow(dead_code)] // Held to keep observe subscription alive
    status_counts_model: Entity<StatusCountsModel>,
    counts: StatusCounts,
    workspace_manager: WorkspaceManager,
    on_select_tab: Arc<dyn Fn(usize, &mut Window, &mut App)>,
    on_close_tab: Arc<dyn Fn(usize, &mut Window, &mut App)>,
    sidebar_visible: bool,
}

impl TopBarEntity {
    pub fn new(
        status_counts_model: Entity<StatusCountsModel>,
        workspace_manager: WorkspaceManager,
        on_select_tab: Arc<dyn Fn(usize, &mut Window, &mut App)>,
        on_close_tab: Arc<dyn Fn(usize, &mut Window, &mut App)>,
        cx: &mut Context<Self>,
    ) -> Self {
        let counts = status_counts_model.read(cx).counts.clone();
        cx.observe(&status_counts_model, |this, observed, cx| {
            this.counts = observed.read(cx).counts.clone();
            cx.notify();
        })
        .detach();
        Self {
            status_counts_model,
            counts,
            workspace_manager,
            on_select_tab,
            on_close_tab,
            sidebar_visible: true,
        }
    }

    pub fn set_workspace_manager(&mut self, wm: WorkspaceManager) {
        self.workspace_manager = wm;
    }

    pub fn set_sidebar_visible(&mut self, visible: bool) {
        self.sidebar_visible = visible;
    }

    fn render_workspace_tab(
        &self,
        tab: &WorkspaceTab,
        index: usize,
        is_active: bool,
    ) -> impl IntoElement {
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
            .min_w(px(48.))
            .max_w(px(200.))
            .when(is_active, |el: Stateful<Div>| {
                el.bg(rgb(0x2c313a))
            })
            .when(!is_active, |el: Stateful<Div>| {
                el.bg(rgb(0x21252b))
                    .hover(|style: StyleRefinement| style.bg(rgb(0x2c313a)))
            })
            .cursor_pointer()
            .on_click(move |_, window, cx| { on_select(index, window, cx); })
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .text_ellipsis()
                    .text_size(px(12.))
                    .text_color(if is_active { rgb(0xc0c8d5) } else { rgb(0x5c6370) })
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

impl Render for TopBarEntity {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        // When sidebar is hidden, add left padding to clear macOS traffic light buttons (~80px)
        let left_pad = if self.sidebar_visible { px(8.) } else { px(80.) };
        div()
            .id("topbar-entity")
            .w_full()
            .h(px(36.))
            .flex()
            .flex_row()
            .items_center()
            .pl(left_pad)
            .pr(px(8.))
            .gap(px(8.))
            .bg(rgb(0x282c34))
            .border_b_1()
            .border_color(rgb(0x2a2d37))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
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
    }
}
