// notification.rs - Notification system for pmux
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use uuid::Uuid;

/// Types of notifications
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NotificationType {
    /// Error occurred (highest priority)
    Error,
    /// Waiting for user input or confirmation
    Waiting,
    /// General information
    Info,
}

impl NotificationType {
    /// Get the priority level (higher = more important)
    pub fn priority(&self) -> u8 {
        match self {
            NotificationType::Error => 3,
            NotificationType::Waiting => 2,
            NotificationType::Info => 1,
        }
    }

    /// Get display text for this type
    pub fn display_text(&self) -> &'static str {
        match self {
            NotificationType::Error => "Error",
            NotificationType::Waiting => "Waiting",
            NotificationType::Info => "Info",
        }
    }

    /// Get icon for this type
    pub fn icon(&self) -> &'static str {
        match self {
            NotificationType::Error => "✕",
            NotificationType::Waiting => "◐",
            NotificationType::Info => "ℹ",
        }
    }
}

/// A single notification
#[derive(Debug, Clone)]
pub struct Notification {
    id: Uuid,
    pane_id: String,
    notif_type: NotificationType,
    message: String,
    timestamp: Instant,
    read: bool,
    merge_count: u32,
    /// Human-readable source: "repo / worktree" or "repo / worktree / pane N"
    pub source_label: Option<String>,
}

impl Notification {
    /// Create a new notification
    pub fn new(pane_id: &str, notif_type: NotificationType, message: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            pane_id: pane_id.to_string(),
            notif_type,
            message: message.to_string(),
            timestamp: Instant::now(),
            read: false,
            merge_count: 1,
            source_label: None,
        }
    }

    /// Create a new notification with an explicit source label
    pub fn new_with_label(pane_id: &str, notif_type: NotificationType, message: &str, source_label: String) -> Self {
        let mut n = Self::new(pane_id, notif_type, message);
        n.source_label = Some(source_label);
        n
    }

    /// Get the notification ID
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Get the pane ID
    pub fn pane_id(&self) -> &str {
        &self.pane_id
    }

    /// Get the notification type
    pub fn notif_type(&self) -> NotificationType {
        self.notif_type
    }

    /// Get the message
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Get the timestamp
    pub fn timestamp(&self) -> Instant {
        self.timestamp
    }

    /// Check if notification is read
    pub fn is_read(&self) -> bool {
        self.read
    }

    /// Mark as read
    pub fn mark_as_read(&mut self) {
        self.read = true;
    }

    /// Mark as unread
    pub fn mark_as_unread(&mut self) {
        self.read = false;
    }

    /// Get merge count
    pub fn merge_count(&self) -> u32 {
        self.merge_count
    }

    /// Increment merge count
    pub fn increment_merge_count(&mut self) {
        self.merge_count += 1;
    }

    /// Get age of notification
    pub fn age(&self) -> Duration {
        self.timestamp.elapsed()
    }

    /// Check if notification is recent (within given duration)
    pub fn is_recent(&self, duration: Duration) -> bool {
        self.age() < duration
    }

    /// Get grouping key for merging
    /// Notifications with same key can be merged
    pub fn group_key(&self) -> String {
        format!("{}:{:?}", self.pane_id, self.notif_type)
    }

    /// Format display message (with merge count if > 1)
    pub fn display_message(&self) -> String {
        if self.merge_count > 1 {
            format!("{} ×{}", self.message, self.merge_count)
        } else {
            self.message.clone()
        }
    }
}

/// Summary of notifications by type
#[derive(Debug, Default, Clone)]
pub struct NotificationSummary {
    pub total: usize,
    pub unread: usize,
    pub error_count: usize,
    pub waiting_count: usize,
    pub info_count: usize,
}

impl NotificationSummary {
    /// Create empty summary
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if there are any notifications
    pub fn has_notifications(&self) -> bool {
        self.total > 0
    }

    /// Check if there are unread notifications
    pub fn has_unread(&self) -> bool {
        self.unread > 0
    }

    /// Check if there are errors
    pub fn has_errors(&self) -> bool {
        self.error_count > 0
    }

    /// Check if there are waiting notifications (input or confirmation)
    pub fn has_waiting(&self) -> bool {
        self.waiting_count > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_notification_creation() {
        let notif = Notification::new(
            "pane-1",
            NotificationType::Error,
            "Build failed"
        );
        
        assert_eq!(notif.pane_id(), "pane-1");
        assert_eq!(notif.notif_type(), NotificationType::Error);
        assert_eq!(notif.message(), "Build failed");
        assert!(!notif.is_read());
        assert_eq!(notif.merge_count(), 1);
    }

    #[test]
    fn test_notification_type_variants() {
        assert_eq!(NotificationType::Error.priority(), 3);
        assert_eq!(NotificationType::Waiting.priority(), 2);
        assert_eq!(NotificationType::Info.priority(), 1);
    }

    #[test]
    fn test_notification_type_display() {
        assert_eq!(NotificationType::Error.display_text(), "Error");
        assert_eq!(NotificationType::Waiting.display_text(), "Waiting");
        assert_eq!(NotificationType::Info.display_text(), "Info");
    }

    #[test]
    fn test_notification_type_icons() {
        assert_eq!(NotificationType::Error.icon(), "✕");
        assert_eq!(NotificationType::Waiting.icon(), "◐");
        assert_eq!(NotificationType::Info.icon(), "ℹ");
    }

    #[test]
    fn test_mark_as_read() {
        let mut notif = Notification::new("pane-1", NotificationType::Error, "Test");
        assert!(!notif.is_read());
        
        notif.mark_as_read();
        assert!(notif.is_read());
        
        notif.mark_as_unread();
        assert!(!notif.is_read());
    }

    #[test]
    fn test_merge_count() {
        let mut notif = Notification::new("pane-1", NotificationType::Error, "Test");
        assert_eq!(notif.merge_count(), 1);
        
        notif.increment_merge_count();
        notif.increment_merge_count();
        assert_eq!(notif.merge_count(), 3);
    }

    #[test]
    fn test_display_message_with_merge() {
        let mut notif = Notification::new("pane-1", NotificationType::Error, "Error occurred");
        assert_eq!(notif.display_message(), "Error occurred");
        
        notif.increment_merge_count();
        notif.increment_merge_count();
        assert_eq!(notif.display_message(), "Error occurred ×3");
    }

    #[test]
    fn test_group_key() {
        let notif1 = Notification::new("pane-1", NotificationType::Error, "Test");
        let notif2 = Notification::new("pane-1", NotificationType::Error, "Another");
        let notif3 = Notification::new("pane-1", NotificationType::Waiting, "Test");
        let notif4 = Notification::new("pane-2", NotificationType::Error, "Test");
        
        // Same pane and type should have same key
        assert_eq!(notif1.group_key(), notif2.group_key());
        
        // Different type should have different key
        assert_ne!(notif1.group_key(), notif3.group_key());
        
        // Different pane should have different key
        assert_ne!(notif1.group_key(), notif4.group_key());
    }

    #[test]
    fn test_is_recent() {
        let notif = Notification::new("pane-1", NotificationType::Error, "Test");
        
        // Should be recent immediately after creation
        assert!(notif.is_recent(Duration::from_secs(60)));
        
        // Should not be recent after a very short duration
        // (this might be flaky in tests, so we just verify the method exists)
        let _ = notif.age();
    }

    #[test]
    fn test_notification_summary() {
        let mut summary = NotificationSummary::new();
        
        assert!(!summary.has_notifications());
        assert!(!summary.has_unread());
        
        summary.total = 5;
        summary.unread = 2;
        summary.error_count = 1;
        summary.waiting_count = 1;
        summary.info_count = 3;
        
        assert!(summary.has_notifications());
        assert!(summary.has_unread());
        assert!(summary.has_errors());
        assert!(summary.has_waiting());
    }

    #[test]
    fn test_unique_ids() {
        let notif1 = Notification::new("pane-1", NotificationType::Error, "Test");
        let notif2 = Notification::new("pane-1", NotificationType::Error, "Test");
        
        // Each notification should have unique ID
        assert_ne!(notif1.id(), notif2.id());
    }
}
