// ui/split_pane_container.rs - Split pane layout with draggable dividers
use crate::split_tree::SplitNode;
use crate::ui::terminal_view::{TerminalBuffer, TerminalContent, TerminalView};
use gpui::prelude::*;
use gpui::{relative, CursorStyle, *};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

const DIVIDER_WIDTH: f32 = 4.0;
const RATIO_SENSITIVITY: f32 = 0.002; // pixels to ratio: 500px drag = 1.0 ratio change

/// Split pane container - recursively renders SplitNode tree with draggable dividers
pub struct SplitPaneContainer {
    split_tree: SplitNode,
    terminal_buffers: HashMap<String, TerminalBuffer>,
    focused_pane_index: usize,
    repo_name: String,
    /// When true, cursor is in "visible" phase of blink
    cursor_blink_visible: bool,
    /// When set, we're dragging - (path, start_pos, start_ratio, is_vertical)
    drag_state: Option<(Vec<bool>, f32, f32, bool)>,
    /// Callback when user drags a divider: (path, new_ratio)
    on_ratio_change: Option<Arc<dyn Fn(Vec<bool>, f32, &mut Window, &mut App)>>,
    /// Callback when user starts dragging: (path, mouse_pos, current_ratio, is_vertical)
    on_divider_drag_start: Option<Arc<dyn Fn(Vec<bool>, f32, f32, bool, &mut Window, &mut App)>>,
    /// Callback when user ends dragging
    on_divider_drag_end: Option<Arc<dyn Fn(&mut Window, &mut App)>>,
    /// Callback when user clicks a pane: (pane_index)
    on_pane_click: Option<Arc<dyn Fn(usize, &mut Window, &mut App)>>,
}

impl SplitPaneContainer {
    pub fn new(
        split_tree: SplitNode,
        terminal_buffers: HashMap<String, TerminalBuffer>,
        focused_pane_index: usize,
        repo_name: &str,
    ) -> Self {
        Self {
            split_tree,
            terminal_buffers,
            focused_pane_index,
            repo_name: repo_name.to_string(),
            cursor_blink_visible: true,
            drag_state: None,
            on_ratio_change: None,
            on_divider_drag_start: None,
            on_divider_drag_end: None,
            on_pane_click: None,
        }
    }

    pub fn with_cursor_blink_visible(mut self, visible: bool) -> Self {
        self.cursor_blink_visible = visible;
        self
    }

    pub fn with_drag_state(mut self, state: Option<(Vec<bool>, f32, f32, bool)>) -> Self {
        self.drag_state = state;
        self
    }

    pub fn on_ratio_change<F: Fn(Vec<bool>, f32, &mut Window, &mut App) + 'static>(mut self, f: F) -> Self {
        self.on_ratio_change = Some(Arc::new(f));
        self
    }

    pub fn on_divider_drag_start<F: Fn(Vec<bool>, f32, f32, bool, &mut Window, &mut App) + 'static>(mut self, f: F) -> Self {
        self.on_divider_drag_start = Some(Arc::new(f));
        self
    }

    pub fn on_divider_drag_end<F: Fn(&mut Window, &mut App) + 'static>(mut self, f: F) -> Self {
        self.on_divider_drag_end = Some(Arc::new(f));
        self
    }

    pub fn on_pane_click<F: Fn(usize, &mut Window, &mut App) + 'static>(mut self, f: F) -> Self {
        self.on_pane_click = Some(Arc::new(f));
        self
    }

    fn pane_title(&self, pane_target: &str) -> String {
        // Extract short name from target (e.g. "sess:win.%0" -> "pane 0")
        if let Some(dot) = pane_target.rfind('.') {
            format!("pane {}", &pane_target[dot + 1..])
        } else {
            "terminal".to_string()
        }
    }
}

impl IntoElement for SplitPaneContainer {
    type Element = Component<Self>;
    fn into_element(self) -> Self::Element {
        Component::new(self)
    }
}

impl RenderOnce for SplitPaneContainer {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let split_tree = self.split_tree.clone();
        let terminal_buffers = self.terminal_buffers.clone();
        let focused_pane_index = self.focused_pane_index;
        let repo_name = self.repo_name.clone();
        let on_ratio_change = self.on_ratio_change.clone();
        let on_divider_drag_start = self.on_divider_drag_start.clone();
        let on_divider_drag_end = self.on_divider_drag_end.clone();
        let on_pane_click = self.on_pane_click.clone();
        let drag_state = self.drag_state.clone();
        let drag_state_for_up = self.drag_state.clone();

        let cursor_blink_visible = self.cursor_blink_visible;
        let content = self.render_node(
            &split_tree,
            &terminal_buffers,
            focused_pane_index,
            &repo_name,
            0,
            cursor_blink_visible,
            vec![],
            &on_ratio_change,
            &on_divider_drag_start,
            &on_pane_click,
        );

        let is_dragging = drag_state.is_some();

        div()
            .id("split-pane-container")
            .size_full()
            .min_h_0()
            .flex()
            .flex_1()
            .when(is_dragging, |el| {
                el.cursor(CursorStyle::ResizeColumn)
            })
            .on_mouse_move(move |event: &MouseMoveEvent, window, cx| {
                if let Some((path, start_pos, start_ratio, is_vertical)) = &drag_state {
                    let pos: f32 = if *is_vertical {
                        event.position.x.into()
                    } else {
                        event.position.y.into()
                    };
                    let delta = pos - start_pos;
                    let ratio_delta = delta * RATIO_SENSITIVITY;
                    let new_ratio = (start_ratio + ratio_delta).clamp(
                        SplitNode::MIN_RATIO,
                        SplitNode::MAX_RATIO,
                    );
                    if let Some(ref cb) = on_ratio_change {
                        cb(path.clone(), new_ratio, window, cx);
                    }
                }
            })
            .on_mouse_up(MouseButton::Left, move |_event, window, cx| {
                if drag_state_for_up.is_some() {
                    if let Some(ref cb) = on_divider_drag_end {
                        cb(window, cx);
                    }
                }
            })
            .child(content)
    }
}

impl SplitPaneContainer {
    fn render_node(
        &self,
        node: &SplitNode,
        terminal_buffers: &HashMap<String, TerminalBuffer>,
        focused_pane_index: usize,
        repo_name: &str,
        pane_index_offset: usize,
        cursor_blink_visible: bool,
        path: Vec<bool>,
        on_ratio_change: &Option<Arc<dyn Fn(Vec<bool>, f32, &mut Window, &mut App)>>,
        on_divider_drag_start: &Option<Arc<dyn Fn(Vec<bool>, f32, f32, bool, &mut Window, &mut App)>>,
        on_pane_click: &Option<Arc<dyn Fn(usize, &mut Window, &mut App)>>,
    ) -> impl IntoElement {
        match node {
            SplitNode::Pane { target } => {
                let buffer = terminal_buffers
                    .get(target)
                    .cloned()
                    .unwrap_or_else(|| {
                        TerminalBuffer::Legacy(Arc::new(Mutex::new(TerminalContent::new())))
                    });
                let title = self.pane_title(target);
                let is_focused = pane_index_offset == focused_pane_index;
                let pane_idx = pane_index_offset;
                let on_click = on_pane_click.clone();
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .min_h(px(0.))
                    .cursor(gpui::CursorStyle::IBeam)
                    .when(is_focused, |el| {
                        el.border(px(2.)).border_color(rgb(0x0066cc))
                    })
                    .on_mouse_down(MouseButton::Left, move |_event, window, cx| {
                        if let Some(ref cb) = on_click {
                            cb(pane_idx, window, cx);
                        }
                    })
                    .child(
                        TerminalView::with_buffer(target, &title, buffer)
                            .with_focused(is_focused)
                            .with_cursor_visible(cursor_blink_visible)
                            .into_element()
                    )
                    .into_any_element()
            }
            SplitNode::Vertical { ratio, left, right } => {
                let r = ratio.clamp(SplitNode::MIN_RATIO, SplitNode::MAX_RATIO);
                let left_count = left.pane_count();
                let path_clone = path.clone();
                let ratio_val = *ratio;
                let on_drag_start = on_divider_drag_start.clone();

                let mut path_left = path.clone();
                path_left.push(false);
                let mut path_right = path.clone();
                path_right.push(true);

                let left_el = self.render_node(
                    left,
                    terminal_buffers,
                    focused_pane_index,
                    repo_name,
                    pane_index_offset,
                    cursor_blink_visible,
                    path_left,
                    on_ratio_change,
                    on_divider_drag_start,
                    on_pane_click,
                );
                let right_el = self.render_node(
                    right,
                    terminal_buffers,
                    focused_pane_index,
                    repo_name,
                    pane_index_offset + left_count,
                    cursor_blink_visible,
                    path_right,
                    on_ratio_change,
                    on_divider_drag_start,
                    on_pane_click,
                );

                let divider = div()
                    .w(px(DIVIDER_WIDTH))
                    .flex_shrink_0()
                    .bg(rgb(0x3d3d3d))
                    .cursor(CursorStyle::ResizeColumn)
                    .hover(|s: StyleRefinement| s.bg(rgb(0x4d4d4d)))
                    .on_mouse_down(MouseButton::Left, move |event: &MouseDownEvent, window, cx| {
                        if let Some(ref cb) = on_drag_start {
                            cb(path_clone.clone(), event.position.x.into(), ratio_val, true, window, cx);
                        }
                    });

                div()
                    .flex()
                    .flex_row()
                    .size_full()
                    .min_h(px(0.))
                    .child(
                        div()
                            .flex()
                            .flex_1()
                            .min_w(px(0.))
                            .flex_basis(relative(r))
                            .flex_grow()
                            .child(left_el)
                    )
                    .child(divider)
                    .child(
                        div()
                            .flex()
                            .flex_1()
                            .min_w(px(0.))
                            .flex_basis(relative(1.0 - r))
                            .flex_grow()
                            .child(right_el)
                    )
                    .into_any_element()
            }
            SplitNode::Horizontal { ratio, top, bottom } => {
                let r = ratio.clamp(SplitNode::MIN_RATIO, SplitNode::MAX_RATIO);
                let top_count = top.pane_count();
                let path_clone = path.clone();
                let ratio_val = *ratio;
                let on_drag_start = on_divider_drag_start.clone();

                let mut path_top = path.clone();
                path_top.push(false);
                let mut path_bottom = path.clone();
                path_bottom.push(true);

                let top_el = self.render_node(
                    top,
                    terminal_buffers,
                    focused_pane_index,
                    repo_name,
                    pane_index_offset,
                    cursor_blink_visible,
                    path_top,
                    on_ratio_change,
                    on_divider_drag_start,
                    on_pane_click,
                );
                let bottom_el = self.render_node(
                    bottom,
                    terminal_buffers,
                    focused_pane_index,
                    repo_name,
                    pane_index_offset + top_count,
                    cursor_blink_visible,
                    path_bottom,
                    on_ratio_change,
                    on_divider_drag_start,
                    on_pane_click,
                );

                let divider = div()
                    .h(px(DIVIDER_WIDTH))
                    .flex_shrink_0()
                    .bg(rgb(0x3d3d3d))
                    .cursor(CursorStyle::ResizeRow)
                    .hover(|s: StyleRefinement| s.bg(rgb(0x4d4d4d)))
                    .on_mouse_down(MouseButton::Left, move |event: &MouseDownEvent, window, cx| {
                        if let Some(ref cb) = on_drag_start {
                            cb(path_clone.clone(), event.position.y.into(), ratio_val, false, window, cx);
                        }
                    });

                div()
                    .flex()
                    .flex_col()
                    .size_full()
                    .min_h(px(0.))
                    .child(
                        div()
                            .flex()
                            .flex_1()
                            .min_h(px(0.))
                            .flex_basis(relative(r))
                            .flex_grow()
                            .child(top_el)
                    )
                    .child(divider)
                    .child(
                        div()
                            .flex()
                            .flex_1()
                            .min_h(px(0.))
                            .flex_basis(relative(1.0 - r))
                            .flex_grow()
                            .child(bottom_el)
                    )
                    .into_any_element()
            }
        }
    }
}
