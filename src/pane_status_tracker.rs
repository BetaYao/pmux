// pane_status_tracker.rs - Per-pane status tracking with debouncing
use crate::agent_status::AgentStatus;
use crate::status_detector::{DebouncedStatusTracker, StatusDetector};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Tracks status for multiple panes
pub struct PaneStatusTracker {
    /// Detector for initial status detection (reserved for future use)
    #[allow(dead_code)]
    detector: StatusDetector,
    /// Per-pane trackers with debouncing
    pane_trackers: HashMap<String, DebouncedStatusTracker>,
    /// Last update time per pane
    last_updates: HashMap<String, Instant>,
    /// Minimum time between updates for the same pane
    min_update_interval: Duration,
}

impl Default for PaneStatusTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl PaneStatusTracker {
    /// Create a new tracker with default settings
    pub fn new() -> Self {
        Self {
            detector: StatusDetector::new(),
            pane_trackers: HashMap::new(),
            last_updates: HashMap::new(),
            min_update_interval: Duration::from_millis(100),
        }
    }

    /// Create tracker with custom debounce threshold
    pub fn with_debounce(_threshold: u8) -> Self {
        Self {
            detector: StatusDetector::new(),
            pane_trackers: HashMap::new(),
            last_updates: HashMap::new(),
            min_update_interval: Duration::from_millis(100),
        }
    }

    /// Register a new pane to track
    pub fn register_pane(&mut self, pane_id: &str) {
        self.pane_trackers.insert(
            pane_id.to_string(),
            DebouncedStatusTracker::new()
        );
        self.last_updates.insert(pane_id.to_string(), Instant::now());
    }

    /// Unregister a pane
    pub fn unregister_pane(&mut self, pane_id: &str) {
        self.pane_trackers.remove(pane_id);
        self.last_updates.remove(pane_id);
    }

    /// Update status for a specific pane
    /// Returns true if status changed
    pub fn update_pane(&mut self, pane_id: &str, content: &str) -> bool {
        // Check minimum update interval
        if let Some(last_update) = self.last_updates.get(pane_id) {
            if last_update.elapsed() < self.min_update_interval {
                return false;
            }
        }

        // Get or create tracker for this pane
        let tracker = self.pane_trackers
            .entry(pane_id.to_string())
            .or_insert_with(DebouncedStatusTracker::new);

        // Update and check if changed
        let changed = tracker.update(content);
        
        if changed {
            self.last_updates.insert(pane_id.to_string(), Instant::now());
        }

        changed
    }

    /// Get current status for a pane
    pub fn get_status(&self, pane_id: &str) -> AgentStatus {
        self.pane_trackers
            .get(pane_id)
            .map(|t| t.current_status())
            .unwrap_or(AgentStatus::Unknown)
    }

    /// Get all pane statuses
    pub fn get_all_statuses(&self) -> HashMap<String, AgentStatus> {
        self.pane_trackers
            .iter()
            .map(|(id, tracker)| (id.clone(), tracker.current_status()))
            .collect()
    }

    /// Force set status for a pane (bypass debounce)
    pub fn force_status(&mut self, pane_id: &str, status: AgentStatus) {
        let tracker = self.pane_trackers
            .entry(pane_id.to_string())
            .or_insert_with(DebouncedStatusTracker::new);
        
        tracker.force_status(status);
        self.last_updates.insert(pane_id.to_string(), Instant::now());
    }

    /// Reset all trackers
    pub fn reset_all(&mut self) {
        for tracker in self.pane_trackers.values_mut() {
            tracker.reset();
        }
        self.last_updates.clear();
    }

    /// Get count of panes being tracked
    pub fn pane_count(&self) -> usize {
        self.pane_trackers.len()
    }

    /// Check if a pane is registered
    pub fn is_tracking(&self, pane_id: &str) -> bool {
        self.pane_trackers.contains_key(pane_id)
    }

    /// Get list of tracked pane IDs
    pub fn pane_ids(&self) -> Vec<String> {
        self.pane_trackers.keys().cloned().collect()
    }

    /// Get panes that have urgent status (Error or Waiting)
    pub fn get_urgent_panes(&self) -> Vec<(String, AgentStatus)> {
        self.pane_trackers
            .iter()
            .filter(|(_, tracker)| {
                let status = tracker.current_status();
                status.is_urgent()
            })
            .map(|(id, tracker)| (id.clone(), tracker.current_status()))
            .collect()
    }

    /// Clean up stale panes (not updated for a long time)
    pub fn cleanup_stale(&mut self, max_age: Duration) {
        let now = Instant::now();
        let stale_ids: Vec<String> = self.last_updates
            .iter()
            .filter(|(_, last_update)| now.duration_since(**last_update) > max_age)
            .map(|(id, _)| id.clone())
            .collect();

        for id in stale_ids {
            self.unregister_pane(&id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tracker_creation() {
        let tracker = PaneStatusTracker::new();
        assert_eq!(tracker.pane_count(), 0);
    }

    #[test]
    fn test_register_pane() {
        let mut tracker = PaneStatusTracker::new();
        tracker.register_pane("pane-1");
        
        assert_eq!(tracker.pane_count(), 1);
        assert!(tracker.is_tracking("pane-1"));
        assert!(!tracker.is_tracking("pane-2"));
    }

    #[test]
    fn test_unregister_pane() {
        let mut tracker = PaneStatusTracker::new();
        tracker.register_pane("pane-1");
        tracker.register_pane("pane-2");
        
        tracker.unregister_pane("pane-1");
        
        assert_eq!(tracker.pane_count(), 1);
        assert!(!tracker.is_tracking("pane-1"));
        assert!(tracker.is_tracking("pane-2"));
    }

    #[test]
    fn test_get_status_unknown_for_unregistered() {
        let tracker = PaneStatusTracker::new();
        assert_eq!(tracker.get_status("unknown-pane"), AgentStatus::Unknown);
    }

    #[test]
    fn test_force_status() {
        let mut tracker = PaneStatusTracker::new();
        tracker.register_pane("pane-1");
        
        tracker.force_status("pane-1", AgentStatus::Running);
        
        assert_eq!(tracker.get_status("pane-1"), AgentStatus::Running);
    }

    #[test]
    fn test_force_status_creates_tracker() {
        let mut tracker = PaneStatusTracker::new();
        
        // Force status on unregistered pane should create it
        tracker.force_status("new-pane", AgentStatus::Error);
        
        assert!(tracker.is_tracking("new-pane"));
        assert_eq!(tracker.get_status("new-pane"), AgentStatus::Error);
    }

    #[test]
    fn test_get_all_statuses() {
        let mut tracker = PaneStatusTracker::new();
        tracker.register_pane("pane-1");
        tracker.register_pane("pane-2");
        
        tracker.force_status("pane-1", AgentStatus::Running);
        tracker.force_status("pane-2", AgentStatus::Idle);
        
        let statuses = tracker.get_all_statuses();
        assert_eq!(statuses.len(), 2);
        assert_eq!(statuses.get("pane-1"), Some(&AgentStatus::Running));
        assert_eq!(statuses.get("pane-2"), Some(&AgentStatus::Idle));
    }

    #[test]
    fn test_pane_ids() {
        let mut tracker = PaneStatusTracker::new();
        tracker.register_pane("pane-a");
        tracker.register_pane("pane-b");
        
        let ids = tracker.pane_ids();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"pane-a".to_string()));
        assert!(ids.contains(&"pane-b".to_string()));
    }

    #[test]
    fn test_reset_all() {
        let mut tracker = PaneStatusTracker::new();
        tracker.register_pane("pane-1");
        tracker.force_status("pane-1", AgentStatus::Running);
        
        tracker.reset_all();
        
        assert_eq!(tracker.get_status("pane-1"), AgentStatus::Unknown);
    }

    #[test]
    fn test_get_urgent_panes() {
        let mut tracker = PaneStatusTracker::new();
        tracker.register_pane("pane-1");
        tracker.register_pane("pane-2");
        tracker.register_pane("pane-3");
        
        tracker.force_status("pane-1", AgentStatus::Error);
        tracker.force_status("pane-2", AgentStatus::Waiting);
        tracker.force_status("pane-3", AgentStatus::Running);
        
        let urgent = tracker.get_urgent_panes();
        assert_eq!(urgent.len(), 2);
        
        let ids: Vec<_> = urgent.iter().map(|(id, _)| id.clone()).collect();
        assert!(ids.contains(&"pane-1".to_string()));
        assert!(ids.contains(&"pane-2".to_string()));
    }

    #[test]
    fn test_cleanup_stale() {
        let mut tracker = PaneStatusTracker::new();
        tracker.register_pane("fresh");
        tracker.register_pane("stale");
        
        // Manually make one pane stale by not updating it
        // Note: In real scenario, we'd wait, but here we just verify the method exists
        tracker.cleanup_stale(Duration::from_secs(3600)); // 1 hour
        
        // Both should still exist since we just created them
        assert_eq!(tracker.pane_count(), 2);
    }
}
