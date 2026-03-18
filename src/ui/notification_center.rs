//! NotificationCenter - manages notifications, panel state, and notification jump
//!
//! Extracted from AppRoot Phase 2.
//! Observes RuntimeManager for notification events.
//! Callbacks to AppRoot for pane jump navigation.

use crate::notification::NotificationType;
use crate::notification_manager::NotificationManager;
use crate::ui::models::NotificationPanelModel;
use crate::ui::notification_panel_entity::NotificationPanelEntity;
use gpui::{App, Context, Entity};
use std::sync::{Arc, Mutex};

pub struct NotificationCenter {
    pub manager: Arc<Mutex<NotificationManager>>,
    pub panel_model: Option<Entity<NotificationPanelModel>>,
    pub panel_entity: Option<Entity<NotificationPanelEntity>>,
    /// Pending notification jump: (pane_id, timestamp).
    /// Set when system notification sent; consumed on window focus transition.
    pub pending_jump: Arc<Mutex<Option<(String, std::time::Instant)>>>,
}

impl NotificationCenter {
    pub fn new() -> Self {
        Self {
            manager: Arc::new(Mutex::new(NotificationManager::new())),
            panel_model: None,
            panel_entity: None,
            pending_jump: Arc::new(Mutex::new(None)),
        }
    }

    /// Add a notification to the manager.
    pub fn add(&self, pane_id: &str, notif_type: NotificationType, message: &str) {
        if let Ok(mut mgr) = self.manager.lock() {
            mgr.add(pane_id, notif_type, message);
        }
    }

    /// Add a labeled notification. Returns true if notification was actually added (not duplicate).
    pub fn add_labeled(&self, pane_id: &str, notif_type: NotificationType, message: &str, label: String) -> bool {
        if let Ok(mut mgr) = self.manager.lock() {
            mgr.add_labeled(pane_id, notif_type, message, Some(label))
        } else {
            false
        }
    }

    /// Toggle notification panel visibility.
    pub fn toggle_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(ref model) = self.panel_model {
            let _ = model.update(cx, |m, cx| {
                m.toggle_panel();
                cx.notify();
            });
        }
        cx.notify();
    }

    /// Update unread count in panel model.
    pub fn sync_unread_count(&self, cx: &mut App) {
        let unread = self.manager.lock().map(|m| m.unread_count()).unwrap_or(0);
        if let Some(ref model) = self.panel_model {
            let _ = model.update(cx, |m, cx| {
                m.set_unread_count(unread);
                cx.notify();
            });
        }
    }

    /// Set pending jump target (called when system notification sent).
    pub fn set_pending_jump(&self, pane_id: String) {
        if let Ok(mut pending) = self.pending_jump.lock() {
            *pending = Some((pane_id, std::time::Instant::now()));
        }
    }

    /// Check and consume pending jump target. Returns pane_id if valid (within 30s).
    pub fn take_pending_jump(&self) -> Option<String> {
        if let Ok(mut pending) = self.pending_jump.lock() {
            if let Some((ref pane_id, ref ts)) = *pending {
                if ts.elapsed() < std::time::Duration::from_secs(30) {
                    let target = pane_id.clone();
                    *pending = None;
                    return Some(target);
                }
                *pending = None;
            }
        }
        None
    }

    /// Get unread count for display.
    pub fn unread_count(&self) -> usize {
        self.manager.lock().map(|m| m.unread_count()).unwrap_or(0)
    }

    /// Check if notification panel is open.
    pub fn is_panel_open(&self, cx: &App) -> bool {
        self.panel_model
            .as_ref()
            .map_or(false, |m| m.read(cx).show_panel)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pending_jump_set_and_take() {
        let nc = NotificationCenter::new();
        assert!(nc.take_pending_jump().is_none());

        nc.set_pending_jump("local:/path/feat".to_string());
        assert_eq!(nc.take_pending_jump(), Some("local:/path/feat".to_string()));
        // Second take returns None (consumed)
        assert!(nc.take_pending_jump().is_none());
    }

    #[test]
    fn test_add_notification() {
        let nc = NotificationCenter::new();
        nc.add("pane-1", NotificationType::Info, "test message");
        assert_eq!(nc.unread_count(), 1);
    }
}
