// ui/terminal_area_entity.rs - Terminal area Entity for scoped notify (Phase 4)
// When terminal content changes, only this entity re-renders, not full AppRoot.
use crate::split_tree::SplitNode;
use crate::ui::split_pane_container::SplitPaneContainer;
use crate::ui::terminal_view::TerminalBuffer;
use gpui::prelude::*;
use gpui::{App, Context, Window};
use std::collections::HashMap;
use std::sync::Arc;

/// Terminal area Entity - renders SplitPaneContainer; notify causes only this subtree to re-render.
pub struct TerminalAreaEntity {
    split_tree: SplitNode,
    terminal_buffers: Arc<std::sync::Mutex<HashMap<String, TerminalBuffer>>>,
    focused_pane_index: usize,
    repo_name: String,
    cursor_blink_visible: bool,
    split_divider_drag: Option<(Vec<bool>, f32, f32, bool)>,
    on_ratio_change: Option<Arc<dyn Fn(Vec<bool>, f32, &mut Window, &mut App)>>,
    on_divider_drag_start: Option<Arc<dyn Fn(Vec<bool>, f32, f32, bool, &mut Window, &mut App)>>,
    on_divider_drag_end: Option<Arc<dyn Fn(&mut Window, &mut App)>>,
    on_pane_click: Option<Arc<dyn Fn(usize, &mut Window, &mut App)>>,
    on_context_menu: Option<Arc<dyn Fn(f32, f32, &mut Window, &mut App)>>,
    search_query: Option<String>,
    search_current_match: usize,
}

impl TerminalAreaEntity {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        split_tree: SplitNode,
        terminal_buffers: Arc<std::sync::Mutex<HashMap<String, TerminalBuffer>>>,
        focused_pane_index: usize,
        repo_name: String,
        cursor_blink_visible: bool,
        split_divider_drag: Option<(Vec<bool>, f32, f32, bool)>,
        on_ratio_change: Option<Arc<dyn Fn(Vec<bool>, f32, &mut Window, &mut App)>>,
        on_divider_drag_start: Option<Arc<dyn Fn(Vec<bool>, f32, f32, bool, &mut Window, &mut App)>>,
        on_divider_drag_end: Option<Arc<dyn Fn(&mut Window, &mut App)>>,
        on_pane_click: Option<Arc<dyn Fn(usize, &mut Window, &mut App)>>,
        on_context_menu: Option<Arc<dyn Fn(f32, f32, &mut Window, &mut App)>>,
        search_query: Option<String>,
        search_current_match: usize,
    ) -> Self {
        Self {
            split_tree,
            terminal_buffers,
            focused_pane_index,
            repo_name,
            cursor_blink_visible,
            split_divider_drag,
            on_ratio_change,
            on_divider_drag_start,
            on_divider_drag_end,
            on_pane_click,
            on_context_menu,
            search_query,
            search_current_match,
        }
    }

    pub fn set_split_tree(&mut self, tree: SplitNode) {
        self.split_tree = tree;
    }

    pub fn set_focused_pane_index(&mut self, idx: usize) {
        self.focused_pane_index = idx;
    }

    pub fn set_split_divider_drag(&mut self, state: Option<(Vec<bool>, f32, f32, bool)>) {
        self.split_divider_drag = state;
    }

    pub fn set_search(&mut self, query: Option<String>, current: usize) {
        self.search_query = query;
        self.search_current_match = current;
    }
}

impl Render for TerminalAreaEntity {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let mut container = SplitPaneContainer::new(
            self.split_tree.clone(),
            Arc::clone(&self.terminal_buffers),
            self.focused_pane_index,
            &self.repo_name,
        )
        .with_search(self.search_query.clone(), self.search_current_match)
        .with_cursor_blink_visible(self.cursor_blink_visible)
        .with_drag_state(self.split_divider_drag.clone());

        if let Some(ref cb) = self.on_ratio_change {
            let cb = Arc::clone(cb);
            container = container.on_ratio_change(move |path, ratio, w, cx| cb(path, ratio, w, cx));
        }
        if let Some(ref cb) = self.on_divider_drag_start {
            let cb = Arc::clone(cb);
            container = container.on_divider_drag_start(move |path, pos, ratio, vert, w, cx| {
                cb(path, pos, ratio, vert, w, cx)
            });
        }
        if let Some(ref cb) = self.on_divider_drag_end {
            let cb = Arc::clone(cb);
            container = container.on_divider_drag_end(move |w, cx| cb(w, cx));
        }
        if let Some(ref cb) = self.on_pane_click {
            let cb = Arc::clone(cb);
            container = container.on_pane_click(move |idx, w, cx| cb(idx, w, cx));
        }
        if let Some(ref cb) = self.on_context_menu {
            let cb = Arc::clone(cb);
            container = container.on_context_menu(move |x, y, w, cx| cb(x, y, w, cx));
        }

        container.into_element()
    }
}
