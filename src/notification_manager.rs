// notification_manager.rs - Notification management and storage
use crate::notification::{Notification, NotificationSummary, NotificationType};
use std::collections::VecDeque;
use std::time::Duration;

/// Maximum number of notifications to keep in memory
const MAX_NOTIFICATIONS: usize = 100;

/// Default merge window duration
const DEFAULT_MERGE_WINDOW: Duration = Duration::from_secs(30);

/// Manages all notifications
pub struct NotificationManager {
    notifications: VecDeque<Notification>,
    merge_window: Duration,
}

impl Default for NotificationManager {
    fn default() -> Self {
        Self::new()
    }
}

impl NotificationManager {
    /// Create a new empty manager
    pub fn new() -> Self {
        Self {
            notifications: VecDeque::with_capacity(MAX_NOTIFICATIONS),
            merge_window: DEFAULT_MERGE_WINDOW,
        }
    }

    /// Create manager with custom merge window
    pub fn with_merge_window(mut self, window: Duration) -> Self {
        self.merge_window = window;
        self
    }

    /// Add a notification (may merge with recent similar notification)
    /// Returns true if a new notification was added, false if merged
    pub fn add(&mut self, pane_id: &str, notif_type: NotificationType, message: &str) -> bool {
        self.add_labeled(pane_id, notif_type, message, None)
    }

    /// Add a notification with a human-readable source label.
    /// Returns true if a new notification was added, false if merged.
    pub fn add_labeled(&mut self, pane_id: &str, notif_type: NotificationType, message: &str, source_label: Option<String>) -> bool {
        let group_key = format!("{}:{:?}", pane_id, notif_type);

        // Check for recent mergeable notification
        if let Some(existing) = self.find_mergeable(&group_key) {
            existing.increment_merge_count();
            existing.mark_as_unread(); // Reset read status on update
            return false;
        }

        // Create new notification
        let mut notification = Notification::new(pane_id, notif_type, message);
        notification.source_label = source_label;

        // Add to front (most recent first)
        self.notifications.push_front(notification);

        // Trim if exceeding max
        while self.notifications.len() > MAX_NOTIFICATIONS {
            self.notifications.pop_back();
        }

        true
    }

    /// Find a notification that can be merged with
    fn find_mergeable(&mut self, group_key: &str) -> Option<&mut Notification> {
        self.notifications.iter_mut().find(|n| {
            n.group_key() == group_key && n.is_recent(self.merge_window)
        })
    }

    /// Get all notifications
    pub fn all(&self) -> &VecDeque<Notification> {
        &self.notifications
    }

    /// Get unread notifications
    pub fn unread(&self) -> Vec<&Notification> {
        self.notifications.iter()
            .filter(|n| !n.is_read())
            .collect()
    }

    /// Get recent notifications (last N)
    pub fn recent(&self, count: usize) -> Vec<&Notification> {
        self.notifications.iter().take(count).collect()
    }

    /// Get notifications by pane
    pub fn by_pane(&self, pane_id: &str) -> Vec<&Notification> {
        self.notifications.iter()
            .filter(|n| n.pane_id() == pane_id)
            .collect()
    }

    /// Get notifications by type
    pub fn by_type(&self, notif_type: NotificationType) -> Vec<&Notification> {
        self.notifications.iter()
            .filter(|n| n.notif_type() == notif_type)
            .collect()
    }

    /// Mark a notification as read by ID
    pub fn mark_read(&mut self, id: uuid::Uuid) -> bool {
        if let Some(notif) = self.notifications.iter_mut().find(|n| n.id() == id) {
            notif.mark_as_read();
            true
        } else {
            false
        }
    }

    /// Mark all notifications as read
    pub fn mark_all_read(&mut self) {
        for notif in &mut self.notifications {
            notif.mark_as_read();
        }
    }

    /// Clear a specific notification
    pub fn clear(&mut self, id: uuid::Uuid) -> bool {
        let initial_len = self.notifications.len();
        self.notifications.retain(|n| n.id() != id);
        self.notifications.len() < initial_len
    }

    /// Clear all notifications
    pub fn clear_all(&mut self) {
        self.notifications.clear();
    }

    /// Clear read notifications
    pub fn clear_read(&mut self) {
        self.notifications.retain(|n| !n.is_read());
    }

    /// Get total count
    pub fn count(&self) -> usize {
        self.notifications.len()
    }

    /// Get unread count
    pub fn unread_count(&self) -> usize {
        self.notifications.iter().filter(|n| !n.is_read()).count()
    }

    /// Check if has any notifications
    pub fn is_empty(&self) -> bool {
        self.notifications.is_empty()
    }

    /// Check if has unread notifications
    pub fn has_unread(&self) -> bool {
        self.unread_count() > 0
    }

    /// Get summary statistics
    pub fn summary(&self) -> NotificationSummary {
        let mut summary = NotificationSummary::new();
        summary.total = self.count();
        summary.unread = self.unread_count();

        for notif in &self.notifications {
            match notif.notif_type() {
                NotificationType::Error => summary.error_count += 1,
                NotificationType::Waiting => summary.waiting_count += 1,
                NotificationType::WaitingConfirm => summary.waiting_confirm_count += 1,
                NotificationType::Info => summary.info_count += 1,
            }
        }

        summary
    }

    /// Get the most recent notification
    pub fn latest(&self) -> Option<&Notification> {
        self.notifications.front()
    }

    /// Get the oldest notification
    pub fn oldest(&self) -> Option<&Notification> {
        self.notifications.back()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manager_creation() {
        let manager = NotificationManager::new();
        assert!(manager.is_empty());
        assert_eq!(manager.count(), 0);
    }

    #[test]
    fn test_add_notification() {
        let mut manager = NotificationManager::new();
        
        let added = manager.add("pane-1", NotificationType::Error, "Test error");
        assert!(added);
        assert_eq!(manager.count(), 1);
    }

    #[test]
    fn test_notification_merge() {
        let mut manager = NotificationManager::new();
        
        // First notification
        let added1 = manager.add("pane-1", NotificationType::Error, "Error occurred");
        assert!(added1);
        
        // Similar notification within merge window should merge
        let added2 = manager.add("pane-1", NotificationType::Error, "Another error");
        assert!(!added2); // Merged, not added
        
        assert_eq!(manager.count(), 1);
        
        // Check merge count
        let notif = manager.latest().unwrap();
        assert_eq!(notif.merge_count(), 2);
    }

    #[test]
    fn test_no_merge_different_pane() {
        let mut manager = NotificationManager::new();
        
        manager.add("pane-1", NotificationType::Error, "Error");
        manager.add("pane-2", NotificationType::Error, "Error");
        
        // Should not merge - different panes
        assert_eq!(manager.count(), 2);
    }

    #[test]
    fn test_no_merge_different_type() {
        let mut manager = NotificationManager::new();
        
        manager.add("pane-1", NotificationType::Error, "Error");
        manager.add("pane-1", NotificationType::Waiting, "Waiting");
        
        // Should not merge - different types
        assert_eq!(manager.count(), 2);
    }

    #[test]
    fn test_unread_notifications() {
        let mut manager = NotificationManager::new();
        
        manager.add("pane-1", NotificationType::Error, "Error 1");
        manager.add("pane-1", NotificationType::Error, "Error 2");
        
        let unread = manager.unread();
        assert_eq!(unread.len(), 1); // Merged into one
        assert!(!unread[0].is_read());
    }

    #[test]
    fn test_mark_read() {
        let mut manager = NotificationManager::new();
        
        manager.add("pane-1", NotificationType::Error, "Error");
        let id = manager.latest().unwrap().id();
        
        assert_eq!(manager.unread_count(), 1);
        
        manager.mark_read(id);
        assert_eq!(manager.unread_count(), 0);
    }

    #[test]
    fn test_mark_all_read() {
        let mut manager = NotificationManager::new();
        
        manager.add("pane-1", NotificationType::Error, "Error 1");
        manager.add("pane-2", NotificationType::Waiting, "Waiting");
        
        manager.mark_all_read();
        
        assert_eq!(manager.unread_count(), 0);
        assert!(manager.latest().unwrap().is_read());
    }

    #[test]
    fn test_clear_notification() {
        let mut manager = NotificationManager::new();
        
        manager.add("pane-1", NotificationType::Error, "Error 1");
        manager.add("pane-2", NotificationType::Error, "Error 2");
        
        let id = manager.by_pane("pane-1")[0].id();
        
        assert!(manager.clear(id));
        assert_eq!(manager.count(), 1);
    }

    #[test]
    fn test_clear_all() {
        let mut manager = NotificationManager::new();
        
        manager.add("pane-1", NotificationType::Error, "Error");
        manager.add("pane-2", NotificationType::Waiting, "Waiting");
        
        manager.clear_all();
        assert!(manager.is_empty());
    }

    #[test]
    fn test_by_pane_filter() {
        let mut manager = NotificationManager::new();
        
        manager.add("pane-1", NotificationType::Error, "Error");
        manager.add("pane-1", NotificationType::Waiting, "Waiting");
        manager.add("pane-2", NotificationType::Error, "Error");
        
        let pane1_notifs = manager.by_pane("pane-1");
        assert_eq!(pane1_notifs.len(), 2);
    }

    #[test]
    fn test_by_type_filter() {
        let mut manager = NotificationManager::new();
        
        manager.add("pane-1", NotificationType::Error, "Error 1");
        manager.add("pane-2", NotificationType::Error, "Error 2");
        manager.add("pane-3", NotificationType::Waiting, "Waiting");
        
        let errors = manager.by_type(NotificationType::Error);
        assert_eq!(errors.len(), 2);
    }

    #[test]
    fn test_summary() {
        let mut manager = NotificationManager::new();
        
        manager.add("pane-1", NotificationType::Error, "Error");
        manager.add("pane-2", NotificationType::Waiting, "Waiting");
        manager.add("pane-3", NotificationType::Info, "Info");
        
        let summary = manager.summary();
        assert_eq!(summary.total, 3);
        assert_eq!(summary.error_count, 1);
        assert_eq!(summary.waiting_count, 1);
        assert_eq!(summary.info_count, 1);
    }

    #[test]
    fn test_max_limit() {
        let mut manager = NotificationManager::new();
        
        // Add more than max notifications
        for i in 0..MAX_NOTIFICATIONS + 10 {
            manager.add(&format!("pane-{}", i), NotificationType::Info, "Test");
        }
        
        // Should be limited to MAX_NOTIFICATIONS
        assert_eq!(manager.count(), MAX_NOTIFICATIONS);
    }

    #[test]
    fn test_recent_notifications() {
        let mut manager = NotificationManager::new();
        
        for i in 0..5 {
            manager.add(&format!("pane-{}", i), NotificationType::Info, "Test");
        }
        
        let recent = manager.recent(3);
        assert_eq!(recent.len(), 3);
    }
}
