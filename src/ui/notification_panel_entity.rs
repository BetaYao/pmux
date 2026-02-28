// ui/notification_panel_entity.rs - NotificationPanel Entity that observes NotificationPanelModel
// Phase 3: Entity with observe; re-renders only when model notifies
use crate::notification_manager::NotificationManager;
use crate::ui::models::NotificationPanelModel;
use crate::ui::notification_panel::{NotificationItem, NotificationPanel};
use gpui::prelude::*;
use gpui::*;
use std::sync::Arc;
use uuid::Uuid;

/// NotificationPanel Entity - observes NotificationPanelModel; re-renders when model notifies.
pub struct NotificationPanelEntity {
    #[allow(dead_code)]
    model: Entity<NotificationPanelModel>,
    show_panel: bool,
    unread_count: usize,
    notification_manager: Arc<std::sync::Mutex<NotificationManager>>,
    on_close: Arc<dyn Fn(&mut Window, &mut App)>,
    on_mark_read: Arc<dyn Fn(Uuid, &mut Window, &mut App)>,
    on_clear_all: Arc<dyn Fn(&mut Window, &mut App)>,
    on_jump_to_pane: Arc<dyn Fn(&str, &mut Window, &mut App)>,
}

impl NotificationPanelEntity {
    pub fn new(
        model: Entity<NotificationPanelModel>,
        notification_manager: Arc<std::sync::Mutex<NotificationManager>>,
        on_close: Arc<dyn Fn(&mut Window, &mut App)>,
        on_mark_read: Arc<dyn Fn(Uuid, &mut Window, &mut App)>,
        on_clear_all: Arc<dyn Fn(&mut Window, &mut App)>,
        on_jump_to_pane: Arc<dyn Fn(&str, &mut Window, &mut App)>,
        cx: &mut Context<Self>,
    ) -> Self {
        let show_panel = model.read(cx).show_panel;
        let unread_count = model.read(cx).unread_count;
        cx.observe(&model, |this, observed, cx| {
            this.show_panel = observed.read(cx).show_panel;
            this.unread_count = observed.read(cx).unread_count;
            cx.notify();
        })
        .detach();
        Self {
            model,
            show_panel,
            unread_count,
            notification_manager,
            on_close,
            on_mark_read,
            on_clear_all,
            on_jump_to_pane,
        }
    }
}

impl Render for NotificationPanelEntity {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        if !self.show_panel {
            return div().into_any_element();
        }

        let notification_items: Vec<NotificationItem> = self
            .notification_manager
            .lock()
            .map(|m| {
                m.recent(100)
                    .iter()
                    .enumerate()
                    .map(|(i, n)| NotificationItem::from_notification(n, i))
                    .collect()
            })
            .unwrap_or_default();

        let on_close = self.on_close.clone();
        let on_mark_read = self.on_mark_read.clone();
        let on_clear_all = self.on_clear_all.clone();
        let on_jump_to_pane = self.on_jump_to_pane.clone();

        let panel = NotificationPanel::new()
            .with_notifications(notification_items)
            .with_visible(true)
            .on_close(move |w, cx| on_close(w, cx))
            .on_mark_read(move |id, w, cx| on_mark_read(id, w, cx))
            .on_clear_all(move |w, cx| on_clear_all(w, cx))
            .on_jump_to_pane(move |pid, w, cx| on_jump_to_pane(pid, w, cx));

        div().child(panel).into_any_element()
    }
}
