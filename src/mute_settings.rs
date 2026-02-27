// mute_settings.rs - Notification mute settings
use crate::notification::NotificationType;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::time::{Duration, Instant};

/// Settings for muting notifications
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MuteSettings {
    /// Globally mute all notifications
    pub global_mute: bool,
    /// Muted pane IDs
    pub muted_panes: HashSet<String>,
    /// Muted notification types
    pub muted_types: HashSet<NotificationType>,
    /// Temporary mute until this time (None = permanent)
    #[serde(skip)]
    pub temporary_mute_until: Option<Instant>,
}

impl Default for MuteSettings {
    fn default() -> Self {
        Self::new()
    }
}

impl MuteSettings {
    /// Create new settings with no mutes
    pub fn new() -> Self {
        Self {
            global_mute: false,
            muted_panes: HashSet::new(),
            muted_types: HashSet::new(),
            temporary_mute_until: None,
        }
    }

    /// Check if a notification should be muted
    pub fn is_muted(&self, pane_id: &str, notif_type: NotificationType) -> bool {
        // Check global mute
        if self.global_mute {
            return true;
        }

        // Check temporary mute
        if let Some(until) = self.temporary_mute_until {
            if Instant::now() < until {
                return true;
            }
        }

        // Check pane-specific mute
        if self.muted_panes.contains(pane_id) {
            return true;
        }

        // Check type-specific mute
        if self.muted_types.contains(&notif_type) {
            return true;
        }

        false
    }

    /// Enable global mute
    pub fn enable_global_mute(&mut self) {
        self.global_mute = true;
    }

    /// Disable global mute
    pub fn disable_global_mute(&mut self) {
        self.global_mute = false;
    }

    /// Toggle global mute
    pub fn toggle_global_mute(&mut self) -> bool {
        self.global_mute = !self.global_mute;
        self.global_mute
    }

    /// Mute a specific pane
    pub fn mute_pane(&mut self, pane_id: &str) {
        self.muted_panes.insert(pane_id.to_string());
    }

    /// Unmute a specific pane
    pub fn unmute_pane(&mut self, pane_id: &str) {
        self.muted_panes.remove(pane_id);
    }

    /// Toggle pane mute status
    pub fn toggle_pane_mute(&mut self, pane_id: &str) -> bool {
        if self.muted_panes.contains(pane_id) {
            self.unmute_pane(pane_id);
            false
        } else {
            self.mute_pane(pane_id);
            true
        }
    }

    /// Check if a pane is muted
    pub fn is_pane_muted(&self, pane_id: &str) -> bool {
        self.muted_panes.contains(pane_id)
    }

    /// Mute a notification type
    pub fn mute_type(&mut self, notif_type: NotificationType) {
        self.muted_types.insert(notif_type);
    }

    /// Unmute a notification type
    pub fn unmute_type(&mut self, notif_type: NotificationType) {
        self.muted_types.remove(&notif_type);
    }

    /// Toggle type mute status
    pub fn toggle_type_mute(&mut self, notif_type: NotificationType) -> bool {
        if self.muted_types.contains(&notif_type) {
            self.unmute_type(notif_type);
            false
        } else {
            self.mute_type(notif_type);
            true
        }
    }

    /// Check if a type is muted
    pub fn is_type_muted(&self, notif_type: NotificationType) -> bool {
        self.muted_types.contains(&notif_type)
    }

    /// Enable temporary mute for a duration
    pub fn enable_temporary_mute(&mut self, duration: Duration) {
        self.temporary_mute_until = Some(Instant::now() + duration);
    }

    /// Disable temporary mute
    pub fn disable_temporary_mute(&mut self) {
        self.temporary_mute_until = None;
    }

    /// Check if temporary mute is active
    pub fn is_temporarily_muted(&self) -> bool {
        match self.temporary_mute_until {
            Some(until) => Instant::now() < until,
            None => false,
        }
    }

    /// Get remaining temporary mute time
    pub fn temporary_mute_remaining(&self) -> Option<Duration> {
        self.temporary_mute_until.map(|until| {
            let now = Instant::now();
            if now < until {
                until - now
            } else {
                Duration::from_secs(0)
            }
        })
    }

    /// Clear all mutes
    pub fn clear_all(&mut self) {
        self.global_mute = false;
        self.muted_panes.clear();
        self.muted_types.clear();
        self.temporary_mute_until = None;
    }

    /// Get count of muted panes
    pub fn muted_pane_count(&self) -> usize {
        self.muted_panes.len()
    }

    /// Get list of muted pane IDs
    pub fn muted_panes(&self) -> &HashSet<String> {
        &self.muted_panes
    }

    /// Get list of muted types
    pub fn muted_types(&self) -> &HashSet<NotificationType> {
        &self.muted_types
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_settings() {
        let settings = MuteSettings::new();
        assert!(!settings.global_mute);
        assert!(settings.muted_panes.is_empty());
        assert!(settings.muted_types.is_empty());
    }

    #[test]
    fn test_global_mute() {
        let mut settings = MuteSettings::new();
        
        assert!(!settings.is_muted("pane-1", NotificationType::Error));
        
        settings.enable_global_mute();
        assert!(settings.global_mute);
        assert!(settings.is_muted("pane-1", NotificationType::Error));
        
        settings.disable_global_mute();
        assert!(!settings.is_muted("pane-1", NotificationType::Error));
    }

    #[test]
    fn test_pane_mute() {
        let mut settings = MuteSettings::new();
        
        settings.mute_pane("pane-1");
        assert!(settings.is_pane_muted("pane-1"));
        assert!(settings.is_muted("pane-1", NotificationType::Error));
        
        // Other panes should not be muted
        assert!(!settings.is_muted("pane-2", NotificationType::Error));
        
        settings.unmute_pane("pane-1");
        assert!(!settings.is_pane_muted("pane-1"));
    }

    #[test]
    fn test_type_mute() {
        let mut settings = MuteSettings::new();
        
        settings.mute_type(NotificationType::Info);
        assert!(settings.is_type_muted(NotificationType::Info));
        assert!(settings.is_muted("pane-1", NotificationType::Info));
        
        // Other types should not be muted
        assert!(!settings.is_muted("pane-1", NotificationType::Error));
    }

    #[test]
    fn test_temporary_mute() {
        let mut settings = MuteSettings::new();
        
        // Enable temporary mute for 1 hour
        settings.enable_temporary_mute(Duration::from_secs(3600));
        assert!(settings.is_temporarily_muted());
        assert!(settings.is_muted("pane-1", NotificationType::Error));
        
        // Check remaining time exists
        assert!(settings.temporary_mute_remaining().is_some());
        
        settings.disable_temporary_mute();
        assert!(!settings.is_temporarily_muted());
    }

    #[test]
    fn test_temporary_mute_expired() {
        let mut settings = MuteSettings::new();
        
        // Enable temporary mute for very short duration
        settings.enable_temporary_mute(Duration::from_millis(1));
        
        // Wait for it to expire
        std::thread::sleep(Duration::from_millis(10));
        
        assert!(!settings.is_temporarily_muted());
        assert!(!settings.is_muted("pane-1", NotificationType::Error));
    }

    #[test]
    fn test_toggle_global_mute() {
        let mut settings = MuteSettings::new();
        
        let result = settings.toggle_global_mute();
        assert!(result);
        assert!(settings.global_mute);
        
        let result = settings.toggle_global_mute();
        assert!(!result);
        assert!(!settings.global_mute);
    }

    #[test]
    fn test_toggle_pane_mute() {
        let mut settings = MuteSettings::new();
        
        let result = settings.toggle_pane_mute("pane-1");
        assert!(result); // Now muted
        
        let result = settings.toggle_pane_mute("pane-1");
        assert!(!result); // Now unmuted
    }

    #[test]
    fn test_clear_all() {
        let mut settings = MuteSettings::new();
        
        settings.enable_global_mute();
        settings.mute_pane("pane-1");
        settings.mute_type(NotificationType::Error);
        settings.enable_temporary_mute(Duration::from_secs(3600));
        
        settings.clear_all();
        
        assert!(!settings.global_mute);
        assert!(!settings.is_pane_muted("pane-1"));
        assert!(!settings.is_type_muted(NotificationType::Error));
        assert!(!settings.is_temporarily_muted());
    }

    #[test]
    fn test_muted_counts() {
        let mut settings = MuteSettings::new();
        
        settings.mute_pane("pane-1");
        settings.mute_pane("pane-2");
        settings.mute_type(NotificationType::Error);
        
        assert_eq!(settings.muted_pane_count(), 2);
        assert_eq!(settings.muted_panes().len(), 2);
        assert_eq!(settings.muted_types().len(), 1);
    }

    #[test]
    fn test_priority_order() {
        let mut settings = MuteSettings::new();
        
        // Global mute has highest priority
        settings.mute_pane("pane-1");
        assert!(settings.is_muted("pane-1", NotificationType::Error));
        
        settings.enable_global_mute();
        assert!(settings.is_muted("pane-2", NotificationType::Error)); // Different pane also muted
        
        settings.disable_global_mute();
        assert!(!settings.is_muted("pane-2", NotificationType::Error)); // Only pane-1 muted now
    }
}
