// ui/notification_panel.rs - Notification panel component with GPUI render
use gpui::prelude::*;
use gpui::{App, Component, Div, FontWeight, Rgba, Stateful, StyleRefinement, Window, div, px, relative, rgb};
use std::sync::Arc;
use crate::notification::{Notification, NotificationType};
use std::time::Instant;
use uuid::Uuid;

/// Notification item for rendering
#[derive(Clone)]
pub struct NotificationItem {
    pub id: String,
    pub uuid: Uuid,
    pub pane_id: String,
    pub notif_type: NotificationType,
    pub message: String,
    pub source_label: Option<String>,
    pub timestamp: Instant,
    pub read: bool,
}

impl NotificationItem {
    pub fn from_notification(notif: &Notification, _index: usize) -> Self {
        Self {
            id: notif.id().to_string(),
            uuid: notif.id(),
            pane_id: notif.pane_id().to_string(),
            notif_type: notif.notif_type(),
            message: notif.display_message(),
            source_label: notif.source_label.clone(),
            timestamp: notif.timestamp(),
            read: notif.is_read(),
        }
    }

    pub fn icon(&self) -> &'static str {
        match self.notif_type {
            NotificationType::Error => "✕",
            NotificationType::Waiting => "◐",
            NotificationType::Info => "ℹ",
        }
    }

    pub fn color(&self) -> Rgba {
        match self.notif_type {
            NotificationType::Error => rgb(0xff4444),
            NotificationType::Waiting => rgb(0xffaa00),
            NotificationType::Info => rgb(0x4488ff),
        }
    }

    pub fn formatted_time(&self) -> String {
        let elapsed = self.timestamp.elapsed();
        if elapsed.as_secs() < 60 {
            "just now".to_string()
        } else if elapsed.as_secs() < 3600 {
            format!("{} min ago", elapsed.as_secs() / 60)
        } else if elapsed.as_secs() < 86400 {
            format!("{} hours ago", elapsed.as_secs() / 3600)
        } else {
            format!("{} days ago", elapsed.as_secs() / 86400)
        }
    }
}

/// NotificationPanel component - shows notifications in a popup panel
pub struct NotificationPanel {
    notifications: Vec<NotificationItem>,
    is_visible: bool,
    on_close: Arc<dyn Fn(&mut Window, &mut App)>,
    on_mark_read: Arc<dyn Fn(Uuid, &mut Window, &mut App)>,
    on_clear_all: Arc<dyn Fn(&mut Window, &mut App)>,
    on_jump_to_pane: Arc<dyn Fn(&str, &mut Window, &mut App)>,
    /// Called when user clicks a notification item: clears it and jumps to the source pane
    on_dismiss_and_jump: Arc<dyn Fn(Uuid, &str, &mut Window, &mut App)>,
}

impl NotificationPanel {
    pub fn new() -> Self {
        Self {
            notifications: Vec::new(),
            is_visible: false,
            on_close: Arc::new(|_, _| {}),
            on_mark_read: Arc::new(|_: Uuid, _, _| {}),
            on_clear_all: Arc::new(|_, _| {}),
            on_jump_to_pane: Arc::new(|_, _, _| {}),
            on_dismiss_and_jump: Arc::new(|_: Uuid, _, _, _| {}),
        }
    }

    pub fn with_notifications(mut self, notifications: Vec<NotificationItem>) -> Self {
        self.notifications = notifications;
        self
    }

    pub fn with_visible(mut self, visible: bool) -> Self {
        self.is_visible = visible;
        self
    }

    pub fn update_notifications(&mut self, notifications: Vec<NotificationItem>) {
        self.notifications = notifications;
    }

    pub fn show(&mut self) { self.is_visible = true; }
    pub fn hide(&mut self) { self.is_visible = false; }
    pub fn toggle(&mut self) { self.is_visible = !self.is_visible; }
    pub fn is_visible(&self) -> bool { self.is_visible }

    pub fn unread_count(&self) -> usize {
        self.notifications.iter().filter(|n| !n.read).count()
    }

    pub fn on_close<F: Fn(&mut Window, &mut App) + 'static>(mut self, f: F) -> Self {
        self.on_close = Arc::new(f); self
    }
    pub fn on_mark_read<F: Fn(Uuid, &mut Window, &mut App) + 'static>(mut self, f: F) -> Self {
        self.on_mark_read = Arc::new(f); self
    }
    pub fn on_clear_all<F: Fn(&mut Window, &mut App) + 'static>(mut self, f: F) -> Self {
        self.on_clear_all = Arc::new(f); self
    }
    pub fn on_jump_to_pane<F: Fn(&str, &mut Window, &mut App) + 'static>(mut self, f: F) -> Self {
        self.on_jump_to_pane = Arc::new(f); self
    }
    pub fn on_dismiss_and_jump<F: Fn(Uuid, &str, &mut Window, &mut App) + 'static>(mut self, f: F) -> Self {
        self.on_dismiss_and_jump = Arc::new(f); self
    }

    fn render_notification(&self, item: &NotificationItem, index: usize) -> impl IntoElement {
        let icon = item.icon();
        let color = item.color();
        let time = item.formatted_time();
        let message = item.message.clone();
        let source_label = item.source_label.clone();
        let pane_id = item.pane_id.clone();
        let is_read = item.read;
        let item_uuid = item.uuid;
        let on_mark_read = self.on_mark_read.clone();
        let on_dismiss = self.on_dismiss_and_jump.clone();
        let pane_id_for_click = pane_id.clone();

        div()
            .id(format!("notif-item-{}", index))
            .w_full()
            .px(px(12.))
            .py(px(10.))
            .border_b_1()
            .border_color(rgb(0x333333))
            .when(!is_read, |el: Stateful<Div>| el.bg(rgb(0x252525)))
            .when(is_read, |el: Stateful<Div>| el.bg(rgb(0x1e1e1e)))
            .hover(|s: StyleRefinement| s.bg(rgb(0x2e2e2e)))
            .cursor_pointer()
            .on_click(move |_, window, cx| {
                on_dismiss(item_uuid, &pane_id_for_click, window, cx);
            })
            .child(
                div()
                    .flex().flex_row().items_start().gap(px(8.))
                    // Icon column
                    .child(
                        div()
                            .flex_shrink_0()
                            .pt(px(1.))
                            .text_size(px(11.))
                            .text_color(color)
                            .child(icon)
                    )
                    // Content column
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.)) // allow shrinking below content width
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            // Message line (wraps naturally)
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .line_height(relative(1.4))
                                    .text_color(if is_read { rgb(0x888888) } else { rgb(0xeeeeee) })
                                    .child(message)
                            )
                            // Meta row: source label (left) + time (right)
                            .child(
                                div()
                                    .flex().flex_row().items_center().justify_between()
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w(px(0.))
                                            .text_size(px(10.))
                                            .text_color(rgb(0x555555))
                                            .when(source_label.is_some(), |el| {
                                                el.child(source_label.unwrap_or_default())
                                            })
                                    )
                                    .child(
                                        div()
                                            .flex_shrink_0()
                                            .text_size(px(10.))
                                            .text_color(rgb(0x555555))
                                            .child(time)
                                    )
                            )
                    )
                    // Mark-read button (✓) for unread items
                    .child(
                        div()
                            .id(format!("mark-read-{}", index))
                            .flex_shrink_0()
                            .when(!is_read, |el: Stateful<Div>| {
                                el.pt(px(1.))
                                    .px(px(4.))
                                    .text_size(px(11.))
                                    .text_color(rgb(0x555555))
                                    .hover(|s: StyleRefinement| s.text_color(rgb(0x88cc88)))
                                    .cursor_pointer()
                                    .on_click(move |_event, window, cx| {
                                        cx.stop_propagation();
                                        on_mark_read(item_uuid, window, cx);
                                    })
                                    .child("✓")
                            })
                    )
            )
    }

    fn render_empty(&self) -> impl IntoElement {
        div()
            .w_full().h(px(120.)).flex().flex_col().items_center().justify_center().gap(px(8.))
            .child(div().text_size(px(24.)).text_color(rgb(0x555555)).child("🔕"))
            .child(div().text_size(px(13.)).text_color(rgb(0x888888)).child("No notifications yet"))
            .child(div().text_size(px(11.)).text_color(rgb(0x666666)).child("Desktop notifications will appear here."))
    }
}

impl IntoElement for NotificationPanel {
    type Element = Component<Self>;
    fn into_element(self) -> Self::Element { Component::new(self) }
}

impl RenderOnce for NotificationPanel {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        if !self.is_visible {
            return div().into_any_element();
        }

        let has_notifications = !self.notifications.is_empty();
        let unread_count = self.unread_count();
        let on_close = self.on_close.clone();
        let on_clear_all = self.on_clear_all.clone();

        div()
            .id("notification-panel")
            .w(px(420.))
            .max_h(px(480.))
            .bg(rgb(0x252525))
            .rounded(px(6.))
            .shadow_lg()
            .border_1()
            .border_color(rgb(0x3d3d3d))
            .occlude()
            .on_click(|_event, _window, cx| {
                cx.stop_propagation();
            })
            .flex()
            .flex_col()
            .child(
                div()
                    .w_full().flex().flex_row().items_center().justify_between()
                    .px(px(12.)).py(px(8.)).border_b_1().border_color(rgb(0x3d3d3d))
                    .child(
                        div()
                            .flex().flex_row().items_center().gap(px(6.))
                            .child(
                                div()
                                    .text_size(px(13.)).font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(0xffffff)).child("Notifications")
                            )
                            .when(unread_count > 0, |el: Div| {
                                el.child(
                                    div()
                                        .px(px(5.)).py(px(1.)).rounded(px(10.)).bg(rgb(0xff4444))
                                        .text_size(px(10.)).font_weight(FontWeight::BOLD)
                                        .text_color(rgb(0xffffff)).child(format!("{}", unread_count))
                                )
                            })
                    )
                    .child(
                        div()
                            .flex().flex_row().items_center().gap(px(8.))
                            .when(has_notifications, |el: Div| {
                                el.child(
                                    div()
                                        .id("clear-all-btn")
                                        .text_size(px(11.)).text_color(rgb(0x888888))
                                        .hover(|s: StyleRefinement| s.text_color(rgb(0xffffff)))
                                        .cursor_pointer()
                                        .on_click(move |_, window, cx| { on_clear_all(window, cx); })
                                        .child("Clear")
                                )
                            })
                            .child(
                                div()
                                    .id("close-panel-btn")
                                    .text_size(px(14.)).text_color(rgb(0x888888))
                                    .hover(|s: StyleRefinement| s.text_color(rgb(0xffffff)))
                                    .cursor_pointer()
                                    .on_click(move |_, window, cx| { on_close(window, cx); })
                                    .child("×")
                            )
                    )
            )
            .child(
                if has_notifications {
                    div()
                        .id("notif-list")
                        .flex_1()
                        .overflow_y_scroll()
                        .children(
                            self.notifications
                                .iter()
                                .enumerate()
                                .map(|(i, item)| self.render_notification(item, i).into_any_element())
                                .collect::<Vec<_>>()
                        )
                        .into_any_element()
                } else {
                    self.render_empty().into_any_element()
                }
            )
            .into_any_element()
    }
}

impl Default for NotificationPanel {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notification::Notification;

    #[test]
    fn test_notification_item_creation() {
        let notif = Notification::new("pane-1", NotificationType::Error, "Test error");
        let item = NotificationItem::from_notification(&notif, 0);
        assert_eq!(item.id, notif.id().to_string());
        assert_eq!(item.pane_id, "pane-1");
        assert!(!item.read);
    }

    #[test]
    fn test_icon_mapping() {
        let error_item = NotificationItem {
            id: "1".to_string(), uuid: Uuid::nil(), pane_id: "p1".to_string(),
            notif_type: NotificationType::Error, message: "err".to_string(),
            source_label: None, timestamp: Instant::now(), read: false,
        };
        assert_eq!(error_item.icon(), "✕");

        let info_item = NotificationItem {
            id: "2".to_string(), uuid: Uuid::nil(), pane_id: "p2".to_string(),
            notif_type: NotificationType::Info, message: "ok".to_string(),
            source_label: None, timestamp: Instant::now(), read: false,
        };
        assert_eq!(info_item.icon(), "ℹ");
    }

    #[test]
    fn test_color_mapping() {
        let error_item = NotificationItem {
            id: "1".to_string(), uuid: Uuid::nil(), pane_id: "p1".to_string(),
            notif_type: NotificationType::Error, message: "err".to_string(),
            source_label: None, timestamp: Instant::now(), read: false,
        };
        let _ = error_item.color();
    }

    #[test]
    fn test_timestamp_formatting() {
        let item = NotificationItem {
            id: "1".to_string(), uuid: Uuid::nil(), pane_id: "p1".to_string(),
            notif_type: NotificationType::Info, message: "msg".to_string(),
            source_label: None, timestamp: Instant::now(), read: false,
        };
        assert_eq!(item.formatted_time(), "just now");
    }

    #[test]
    fn test_panel_creation() {
        let panel = NotificationPanel::new();
        assert!(!panel.is_visible());
        assert_eq!(panel.unread_count(), 0);
    }

    #[test]
    fn test_visibility_toggle() {
        let mut panel = NotificationPanel::new();
        panel.show(); assert!(panel.is_visible());
        panel.hide(); assert!(!panel.is_visible());
        panel.toggle(); assert!(panel.is_visible());
        panel.toggle(); assert!(!panel.is_visible());
    }

    #[test]
    fn test_unread_count() {
        let items = vec![
            NotificationItem { id: "1".to_string(), uuid: Uuid::nil(), pane_id: "p1".to_string(), notif_type: NotificationType::Error, message: "err".to_string(), source_label: None, timestamp: Instant::now(), read: false },
            NotificationItem { id: "2".to_string(), uuid: Uuid::nil(), pane_id: "p2".to_string(), notif_type: NotificationType::Info, message: "info".to_string(), source_label: None, timestamp: Instant::now(), read: true },
            NotificationItem { id: "3".to_string(), uuid: Uuid::nil(), pane_id: "p3".to_string(), notif_type: NotificationType::Waiting, message: "wait".to_string(), source_label: None, timestamp: Instant::now(), read: false },
        ];
        let panel = NotificationPanel::new().with_notifications(items);
        assert_eq!(panel.unread_count(), 2);
    }

    #[test]
    fn test_callback_registration() {
        let panel = NotificationPanel::new()
            .on_close(|_, _| {})
            .on_mark_read(|_: Uuid, _, _| {})
            .on_clear_all(|_, _| {})
            .on_jump_to_pane(|_, _, _| {});
        assert!(!panel.is_visible());
    }
}
