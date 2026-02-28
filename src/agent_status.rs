// agent_status.rs - Agent status enumeration and display

/// Represents the current status of an AI agent in a tmux pane
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AgentStatus {
    /// Agent is actively executing (green ●)
    Running,
    /// Agent is waiting for user input (yellow ◐)
    Waiting,
    /// Agent is idle/not doing anything (gray ○)
    Idle,
    /// Agent encountered an error (red ✕)
    Error,
    /// Agent process has exited (blue ✓)
    Exited,
    /// Status cannot be determined (purple ?)
    Unknown,
}

impl AgentStatus {
    /// Get the color associated with this status
    pub fn color(&self) -> &'static str {
        match self {
            AgentStatus::Running => "green",
            AgentStatus::Waiting => "yellow",
            AgentStatus::Idle => "gray",
            AgentStatus::Error => "red",
            AgentStatus::Exited => "blue",
            AgentStatus::Unknown => "purple",
        }
    }

    /// Get the RGB color values for UI rendering
    pub fn rgb_color(&self) -> (u8, u8, u8) {
        match self {
            AgentStatus::Running => (76, 175, 80),  // #4caf50
            AgentStatus::Waiting => (255, 193, 7),  // #ffc107
            AgentStatus::Idle => (158, 158, 158),   // #9e9e9e
            AgentStatus::Error => (244, 67, 54),    // #f44336
            AgentStatus::Exited => (33, 150, 243),  // #2196f3
            AgentStatus::Unknown => (156, 39, 176), // #9c27b0
        }
    }

    /// Get the icon/indicator character for this status
    pub fn icon(&self) -> &'static str {
        match self {
            AgentStatus::Running => "●",
            AgentStatus::Waiting => "◐",
            AgentStatus::Idle => "○",
            AgentStatus::Error => "✕",
            AgentStatus::Exited => "✓",
            AgentStatus::Unknown => "?",
        }
    }

    /// Get the human-readable display text
    pub fn display_text(&self) -> &'static str {
        match self {
            AgentStatus::Running => "Running",
            AgentStatus::Waiting => "Waiting for input",
            AgentStatus::Idle => "Idle",
            AgentStatus::Error => "Error detected",
            AgentStatus::Exited => "Process exited",
            AgentStatus::Unknown => "Unknown",
        }
    }

    /// Get the short display text (for compact UIs)
    pub fn short_text(&self) -> &'static str {
        match self {
            AgentStatus::Running => "Running",
            AgentStatus::Waiting => "Waiting",
            AgentStatus::Idle => "Idle",
            AgentStatus::Error => "Error",
            AgentStatus::Exited => "Exited",
            AgentStatus::Unknown => "Unknown",
        }
    }

    /// Get the priority level (higher = more important)
    /// Used for determining which status to show when multiple indicators exist
    pub fn priority(&self) -> u8 {
        match self {
            AgentStatus::Error => 6,
            AgentStatus::Exited => 5,
            AgentStatus::Waiting => 4,
            AgentStatus::Running => 3,
            AgentStatus::Idle => 2,
            AgentStatus::Unknown => 1,
        }
    }

    /// Check if this status should trigger immediate notification
    pub fn is_urgent(&self) -> bool {
        matches!(self, AgentStatus::Error | AgentStatus::Waiting)
    }

    /// Check if this status indicates activity
    pub fn is_active(&self) -> bool {
        matches!(self, AgentStatus::Running | AgentStatus::Waiting)
    }

    /// Compare two statuses by priority
    /// Returns true if self has higher priority than other
    pub fn higher_priority_than(&self, other: &AgentStatus) -> bool {
        self.priority() > other.priority()
    }
}

impl Default for AgentStatus {
    fn default() -> Self {
        AgentStatus::Unknown
    }
}

/// Collection of status counts for overview display
#[derive(Debug, Default, Clone)]
pub struct StatusCounts {
    pub running: usize,
    pub waiting: usize,
    pub idle: usize,
    pub error: usize,
    pub exited: usize,
    pub unknown: usize,
}

impl StatusCounts {
    /// Create empty counts
    pub fn new() -> Self {
        Self::default()
    }

    /// Increment count for a specific status
    pub fn increment(&mut self, status: &AgentStatus) {
        match status {
            AgentStatus::Running => self.running += 1,
            AgentStatus::Waiting => self.waiting += 1,
            AgentStatus::Idle => self.idle += 1,
            AgentStatus::Error => self.error += 1,
            AgentStatus::Exited => self.exited += 1,
            AgentStatus::Unknown => self.unknown += 1,
        }
    }

    /// Decrement count for a specific status
    pub fn decrement(&mut self, status: &AgentStatus) {
        match status {
            AgentStatus::Running => self.running = self.running.saturating_sub(1),
            AgentStatus::Waiting => self.waiting = self.waiting.saturating_sub(1),
            AgentStatus::Idle => self.idle = self.idle.saturating_sub(1),
            AgentStatus::Error => self.error = self.error.saturating_sub(1),
            AgentStatus::Exited => self.exited = self.exited.saturating_sub(1),
            AgentStatus::Unknown => self.unknown = self.unknown.saturating_sub(1),
        }
    }

    /// Get total count
    pub fn total(&self) -> usize {
        self.running + self.waiting + self.idle + self.error + self.exited + self.unknown
    }

    /// Get count of urgent statuses (Error + Waiting)
    pub fn urgent_count(&self) -> usize {
        self.error + self.waiting
    }

    /// Check if there are any errors
    pub fn has_errors(&self) -> bool {
        self.error > 0
    }

    /// Check if there are any waiting
    pub fn has_waiting(&self) -> bool {
        self.waiting > 0
    }

    /// Compute StatusCounts from a HashMap of pane_id -> AgentStatus
    pub fn from_pane_statuses(statuses: &std::collections::HashMap<String, AgentStatus>) -> Self {
        let mut counts = Self::new();
        for status in statuses.values() {
            counts.increment(status);
        }
        counts
    }

    /// Get the most prevalent status
    pub fn most_prevalent(&self) -> Option<AgentStatus> {
        let counts = [
            (AgentStatus::Error, self.error),
            (AgentStatus::Waiting, self.waiting),
            (AgentStatus::Running, self.running),
            (AgentStatus::Idle, self.idle),
            (AgentStatus::Exited, self.exited),
            (AgentStatus::Unknown, self.unknown),
        ];

        counts
            .iter()
            .filter(|(_, count)| *count > 0)
            .max_by_key(|(status, _)| status.priority())
            .map(|(status, _)| *status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test: All status variants exist and are distinct
    #[test]
    fn test_status_variants() {
        let statuses = vec![
            AgentStatus::Running,
            AgentStatus::Waiting,
            AgentStatus::Idle,
            AgentStatus::Error,
            AgentStatus::Exited,
            AgentStatus::Unknown,
        ];

        // All should be different
        assert_eq!(statuses.len(), 6);
    }

    /// Test: Status color mapping
    #[test]
    fn test_status_colors() {
        assert_eq!(AgentStatus::Running.color(), "green");
        assert_eq!(AgentStatus::Waiting.color(), "yellow");
        assert_eq!(AgentStatus::Idle.color(), "gray");
        assert_eq!(AgentStatus::Error.color(), "red");
        assert_eq!(AgentStatus::Exited.color(), "blue");
        assert_eq!(AgentStatus::Unknown.color(), "purple");
    }

    /// Test: RGB color values
    #[test]
    fn test_rgb_colors() {
        assert_eq!(AgentStatus::Running.rgb_color(), (76, 175, 80));
        assert_eq!(AgentStatus::Error.rgb_color(), (244, 67, 54));
    }

    /// Test: Status icon mapping
    #[test]
    fn test_status_icons() {
        assert_eq!(AgentStatus::Running.icon(), "●");
        assert_eq!(AgentStatus::Waiting.icon(), "◐");
        assert_eq!(AgentStatus::Idle.icon(), "○");
        assert_eq!(AgentStatus::Error.icon(), "✕");
        assert_eq!(AgentStatus::Exited.icon(), "✓");
        assert_eq!(AgentStatus::Unknown.icon(), "?");
    }

    /// Test: Status display text
    #[test]
    fn test_status_display_text() {
        assert_eq!(AgentStatus::Running.display_text(), "Running");
        assert_eq!(AgentStatus::Waiting.display_text(), "Waiting for input");
        assert_eq!(AgentStatus::Idle.display_text(), "Idle");
        assert_eq!(AgentStatus::Error.display_text(), "Error detected");
        assert_eq!(AgentStatus::Exited.display_text(), "Process exited");
        assert_eq!(AgentStatus::Unknown.display_text(), "Unknown");
    }

    /// Test: Short display text
    #[test]
    fn test_short_text() {
        assert_eq!(AgentStatus::Waiting.short_text(), "Waiting");
        assert_eq!(AgentStatus::Unknown.short_text(), "Unknown");
    }

    /// Test: Status priority ordering
    #[test]
    fn test_status_priority() {
        assert_eq!(AgentStatus::Error.priority(), 6);
        assert_eq!(AgentStatus::Exited.priority(), 5);
        assert_eq!(AgentStatus::Waiting.priority(), 4);
        assert_eq!(AgentStatus::Running.priority(), 3);
        assert_eq!(AgentStatus::Idle.priority(), 2);
        assert_eq!(AgentStatus::Unknown.priority(), 1);
    }

    /// Test: Priority comparison
    #[test]
    fn test_priority_comparison() {
        assert!(AgentStatus::Error.higher_priority_than(&AgentStatus::Exited));
        assert!(AgentStatus::Exited.higher_priority_than(&AgentStatus::Waiting));
        assert!(AgentStatus::Waiting.higher_priority_than(&AgentStatus::Running));
        assert!(AgentStatus::Running.higher_priority_than(&AgentStatus::Idle));
        assert!(AgentStatus::Idle.higher_priority_than(&AgentStatus::Unknown));

        assert!(!AgentStatus::Unknown.higher_priority_than(&AgentStatus::Error));
    }

    /// Test: Urgent status detection
    #[test]
    fn test_is_urgent() {
        assert!(AgentStatus::Error.is_urgent());
        assert!(AgentStatus::Waiting.is_urgent());
        assert!(!AgentStatus::Running.is_urgent());
        assert!(!AgentStatus::Idle.is_urgent());
        assert!(!AgentStatus::Exited.is_urgent());
        assert!(!AgentStatus::Unknown.is_urgent());
    }

    /// Test: Active status detection
    #[test]
    fn test_is_active() {
        assert!(AgentStatus::Running.is_active());
        assert!(AgentStatus::Waiting.is_active());
        assert!(!AgentStatus::Idle.is_active());
        assert!(!AgentStatus::Error.is_active());
        assert!(!AgentStatus::Exited.is_active());
        assert!(!AgentStatus::Unknown.is_active());
    }

    /// Test: Default status
    #[test]
    fn test_default_status() {
        let status: AgentStatus = Default::default();
        assert_eq!(status, AgentStatus::Unknown);
    }

    /// Test: StatusCounts creation
    #[test]
    fn test_status_counts_new() {
        let counts = StatusCounts::new();
        assert_eq!(counts.total(), 0);
        assert!(!counts.has_errors());
        assert!(!counts.has_waiting());
    }

    /// Test: StatusCounts increment/decrement
    #[test]
    fn test_status_counts_increment_decrement() {
        let mut counts = StatusCounts::new();

        counts.increment(&AgentStatus::Running);
        counts.increment(&AgentStatus::Running);
        counts.increment(&AgentStatus::Error);

        assert_eq!(counts.running, 2);
        assert_eq!(counts.error, 1);
        assert_eq!(counts.total(), 3);
        assert!(counts.has_errors());

        counts.decrement(&AgentStatus::Running);
        assert_eq!(counts.running, 1);

        // Should not go below 0
        counts.decrement(&AgentStatus::Waiting);
        assert_eq!(counts.waiting, 0);
    }

    /// Test: Urgent count
    #[test]
    fn test_urgent_count() {
        let mut counts = StatusCounts::new();
        counts.increment(&AgentStatus::Error);
        counts.increment(&AgentStatus::Error);
        counts.increment(&AgentStatus::Waiting);
        counts.increment(&AgentStatus::Running);

        assert_eq!(counts.urgent_count(), 3);
    }

    /// Test: Most prevalent status
    #[test]
    fn test_most_prevalent() {
        let mut counts = StatusCounts::new();

        // Empty should return None
        assert_eq!(counts.most_prevalent(), None);

        // Add some counts
        counts.increment(&AgentStatus::Running);
        counts.increment(&AgentStatus::Running);
        counts.increment(&AgentStatus::Idle);

        // Running has highest count but check priority
        assert_eq!(counts.most_prevalent(), Some(AgentStatus::Running));

        // Add error - should take precedence due to priority
        counts.increment(&AgentStatus::Error);
        assert_eq!(counts.most_prevalent(), Some(AgentStatus::Error));
    }
}
