// ui/tabbar.rs - TabBar component for pane switching within a workspace
use gpui::prelude::*;
use gpui::{App, Component, Div, Stateful, StyleRefinement, Window, div, px, rgb};
use std::sync::Arc;

/// Kind of tab: pane (terminal within current worktree) or review (diff view)
#[derive(Debug, Clone, PartialEq)]
pub enum TabKind {
    /// Pane index within current tmux window (worktree)
    Pane(usize),
    Review { branch: String, window_name: String },
}

/// Information about a single pane tab
#[derive(Debug, Clone)]
pub struct PaneTabInfo {
    pub index: usize,
    pub kind: TabKind,
    pub name: String,
    pub display_name: String,
    pub is_active: bool,
    pub is_modified: bool,
    pub status_icon: Option<String>,
}

impl PaneTabInfo {
    pub fn new(index: usize, name: &str, display_name: &str) -> Self {
        Self {
            index,
            kind: TabKind::Pane(index),
            name: name.to_string(),
            display_name: display_name.to_string(),
            is_active: false,
            is_modified: false,
            status_icon: None,
        }
    }

    /// Pane tab - terminal within current worktree's tmux window
    pub fn pane(index: usize, display: &str) -> Self {
        Self {
            index,
            kind: TabKind::Pane(index),
            name: display.to_string(),
            display_name: display.to_string(),
            is_active: false,
            is_modified: false,
            status_icon: None,
        }
    }

    pub fn review(index: usize, branch: &str, window_name: &str) -> Self {
        let display = format!("{branch}");
        Self {
            index,
            kind: TabKind::Review {
                branch: branch.to_string(),
                window_name: window_name.to_string(),
            },
            name: window_name.to_string(),
            display_name: display,
            is_active: false,
            is_modified: false,
            status_icon: None,
        }
    }

    pub fn with_active(mut self, active: bool) -> Self { self.is_active = active; self }
    pub fn with_modified(mut self, modified: bool) -> Self { self.is_modified = modified; self }
    pub fn with_status_icon(mut self, icon: &str) -> Self { self.status_icon = Some(icon.to_string()); self }

    pub fn shortcut(&self) -> Option<String> {
        if self.index < 8 { Some(format!("⌘{}", self.index + 1)) } else { None }
    }

    pub fn full_label(&self) -> String {
        match &self.kind {
            TabKind::Pane(_) => {
                let mut label = self.display_name.clone();
                if self.is_modified { label.push_str(" ●"); }
                label
            }
            TabKind::Review { branch, .. } => format!("review-{branch}"),
        }
    }

    pub fn is_review(&self) -> bool {
        matches!(&self.kind, TabKind::Review { .. })
    }
}

/// TabBar component - shows pane tabs within current workspace
pub struct TabBar {
    tabs: Vec<PaneTabInfo>,
    on_select_tab: Arc<dyn Fn(usize, &mut Window, &mut App)>,
    on_close_tab: Arc<dyn Fn(usize, &mut Window, &mut App)>,
    on_new_tab: Arc<dyn Fn(&mut Window, &mut App)>,
}

impl TabBar {
    pub fn new() -> Self {
        Self {
            tabs: Vec::new(),
            on_select_tab: Arc::new(|_, _, _| {}),
            on_close_tab: Arc::new(|_, _, _| {}),
            on_new_tab: Arc::new(|_, _| {}),
        }
    }

    pub fn with_tabs(mut self, tabs: Vec<PaneTabInfo>) -> Self { self.tabs = tabs; self }

    pub fn on_select_tab<F: Fn(usize, &mut Window, &mut App) + 'static>(mut self, f: F) -> Self {
        self.on_select_tab = Arc::new(f); self
    }
    pub fn on_close_tab<F: Fn(usize, &mut Window, &mut App) + 'static>(mut self, f: F) -> Self {
        self.on_close_tab = Arc::new(f); self
    }
    pub fn on_new_tab<F: Fn(&mut Window, &mut App) + 'static>(mut self, f: F) -> Self {
        self.on_new_tab = Arc::new(f); self
    }

    pub fn update_tabs(&mut self, tabs: Vec<PaneTabInfo>) { self.tabs = tabs; }
    pub fn tab_count(&self) -> usize { self.tabs.len() }
    pub fn has_tabs(&self) -> bool { !self.tabs.is_empty() }

    fn render_tab(&self, tab: &PaneTabInfo) -> impl IntoElement {
        let index = tab.index;
        let is_active = tab.is_active;
        let label = tab.full_label();
        let shortcut = tab.shortcut();
        let status_icon = tab.status_icon.clone();
        let on_select = self.on_select_tab.clone();
        let on_close = self.on_close_tab.clone();

        div()
            .id(format!("pane-tab-{}", index))
            .flex()
            .flex_row()
            .items_center()
            .flex_1()
            .min_w(px(0.))
            .px(px(12.))
            .py(px(6.))
            .rounded(px(6.))
            .when(is_active, |el: Stateful<Div>| {
                el.bg(rgb(0x3a3a3a))
            })
            .when(!is_active, |el: Stateful<Div>| {
                el.hover(|s: StyleRefinement| s.bg(rgb(0x2a2a2a)))
            })
            .cursor_pointer()
            .on_click(move |_, window, cx| { on_select(index, window, cx); })
            // Left: shortcut label
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
            // Center: tab name + status icon
            .child(
                div()
                    .flex().flex_row().items_center().gap(px(6.))
                    .flex_1().min_w(px(0.)).overflow_x_hidden()
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(if is_active { rgb(0xffffff) } else { rgb(0xaaaaaa) })
                            .overflow_x_hidden()
                            .whitespace_nowrap()
                            .child(label)
                    )
                    .when(status_icon.is_some(), |el: Div| {
                        el.child(div().text_size(px(9.)).text_color(rgb(0x888888)).flex_shrink_0().child(status_icon.unwrap()))
                    })
            )
            // Right: close button
            .child(
                div()
                    .id(format!("close-pane-tab-{}", index))
                    .ml(px(8.)).px(px(3.)).py(px(1.)).rounded(px(3.))
                    .text_size(px(12.)).text_color(rgb(0x555555))
                    .flex_shrink_0()
                    .hover(|s: StyleRefinement| s.text_color(rgb(0xffffff)).bg(rgb(0x555555)))
                    .cursor_pointer()
                    .on_click(move |_, window, cx| { on_close(index, window, cx); })
                    .child("×")
            )
    }
}

impl IntoElement for TabBar {
    type Element = Component<Self>;
    fn into_element(self) -> Self::Element { Component::new(self) }
}

impl RenderOnce for TabBar {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let on_new_tab = self.on_new_tab.clone();

        div()
            .id("tab-bar")
            .w_full().h(px(40.)).flex().flex_row().items_center()
            .px(px(6.)).gap(px(2.)).bg(rgb(0x1e1e1e))
            .border_b_1().border_color(rgb(0x2a2a2a))
            .child(
                div()
                    .flex_1().flex().flex_row().items_center().gap(px(2.)).overflow_x_hidden()
                    .children(
                        self.tabs.iter()
                            .map(|tab| self.render_tab(tab).into_any_element())
                            .collect::<Vec<_>>()
                    )
            )
            .child(
                div()
                    .id("new-tab-btn")
                    .px(px(6.)).py(px(4.)).rounded(px(6.))
                    .hover(|s: StyleRefinement| s.bg(rgb(0x3a3a3a)))
                    .cursor_pointer()
                    .on_click(move |_, window, cx| { on_new_tab(window, cx); })
                    .child(div().text_size(px(14.)).text_color(rgb(0x888888)).child("+"))
            )
    }
}

impl Default for TabBar {
    fn default() -> Self { Self::new() }
}

pub struct TabShortcuts;

impl TabShortcuts {
    pub fn parse_switch(key: &str, modifiers: bool) -> Option<usize> {
        if !modifiers { return None; }
        match key {
            "1" => Some(0), "2" => Some(1), "3" => Some(2), "4" => Some(3),
            "5" => Some(4), "6" => Some(5), "7" => Some(6), "8" => Some(7),
            _ => None,
        }
    }
    pub fn is_next_tab(key: &str, shift: bool, cmd: bool) -> bool { cmd && shift && key == "]" }
    pub fn is_prev_tab(key: &str, shift: bool, cmd: bool) -> bool { cmd && shift && key == "[" }
    pub fn is_close_tab(key: &str, cmd: bool) -> bool { cmd && key.to_lowercase() == "w" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pane_tab_info_creation() {
        let tab = PaneTabInfo::new(0, "main", "main");
        assert_eq!(tab.index, 0);
        assert!(!tab.is_active);
    }

    #[test]
    fn test_pane_tab_info_with_states() {
        let tab = PaneTabInfo::new(1, "feat-x", "feature-x")
            .with_active(true).with_modified(true).with_status_icon("●");
        assert!(tab.is_active);
        assert!(tab.is_modified);
        assert_eq!(tab.status_icon, Some("●".to_string()));
    }

    #[test]
    fn test_shortcut_generation() {
        assert_eq!(PaneTabInfo::new(0, "m", "m").shortcut(), Some("⌘1".to_string()));
        assert_eq!(PaneTabInfo::new(8, "m", "m").shortcut(), None);
    }

    #[test]
    fn test_full_label() {
        assert_eq!(PaneTabInfo::pane(0, "1").full_label(), "1");
        assert_eq!(PaneTabInfo::pane(0, "1").with_modified(true).full_label(), "1 ●");
    }

    #[test]
    fn test_review_tab_label() {
        let tab = PaneTabInfo::review(2, "feat-x", "review-feat-x");
        assert_eq!(tab.full_label(), "review-feat-x");
        assert!(tab.is_review());
    }

    #[test]
    fn test_tabbar_creation() {
        let tabbar = TabBar::new();
        assert_eq!(tabbar.tab_count(), 0);
        assert!(!tabbar.has_tabs());
    }

    #[test]
    fn test_tabbar_with_tabs() {
        let tabs = vec![
            PaneTabInfo::new(0, "main", "main").with_active(true),
            PaneTabInfo::new(1, "feat-x", "feature-x"),
        ];
        let tabbar = TabBar::new().with_tabs(tabs);
        assert_eq!(tabbar.tab_count(), 2);
    }

    #[test]
    fn test_update_tabs() {
        let mut tabbar = TabBar::new();
        tabbar.update_tabs(vec![PaneTabInfo::new(0, "m", "m"), PaneTabInfo::new(1, "f", "f")]);
        assert_eq!(tabbar.tab_count(), 2);
    }

    #[test]
    fn test_callback_registration() {
        let tabbar = TabBar::new()
            .on_select_tab(|_, _, _| {})
            .on_close_tab(|_, _, _| {})
            .on_new_tab(|_, _| {});
        assert_eq!(tabbar.tab_count(), 0);
    }

    #[test]
    fn test_shortcut_parsing() {
        assert_eq!(TabShortcuts::parse_switch("1", true), Some(0));
        assert_eq!(TabShortcuts::parse_switch("1", false), None);
        assert_eq!(TabShortcuts::parse_switch("9", true), None);
    }

    #[test]
    fn test_navigation_shortcuts() {
        assert!(TabShortcuts::is_next_tab("]", true, true));
        assert!(!TabShortcuts::is_next_tab("]", false, true));
        assert!(TabShortcuts::is_prev_tab("[", true, true));
    }

    #[test]
    fn test_close_shortcut() {
        assert!(TabShortcuts::is_close_tab("w", true));
        assert!(TabShortcuts::is_close_tab("W", true));
        assert!(!TabShortcuts::is_close_tab("w", false));
    }
}
