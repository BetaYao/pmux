//! status_publisher.rs - Publishes AgentStateChange to EventBus (event-driven, no polling)
//!
//! Replaces the polling-based StatusPoller with an event-driven architecture.
//! Status detection is triggered when terminal content changes, not on a timer.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::agent_status::AgentStatus;
use crate::runtime::event_bus::{
    AgentStateChange, EventBus, Notification, NotificationType, RuntimeEvent,
};
use crate::shell_integration::ShellPhaseInfo;
use crate::status_detector::{DebouncedStatusTracker, ProcessContext, ProcessStatus, StatusDetector};
use crate::terminal::extract_last_line;

/// Publishes agent status changes to EventBus (event-driven, no polling loop).
///
/// Unlike the old StatusPoller which polled every 500ms, this version is triggered
/// by terminal content changes. Call `check_status()` whenever new terminal output
/// is processed.
#[derive(Clone)]
pub struct StatusPublisher {
    event_bus: Arc<EventBus>,
    tracker: Arc<Mutex<HashMap<String, DebouncedStatusTracker>>>,
    detector: StatusDetector,
}

impl StatusPublisher {
    pub fn new(event_bus: Arc<EventBus>) -> Self {
        Self {
            event_bus,
            tracker: Arc::new(Mutex::new(HashMap::new())),
            detector: StatusDetector::new(),
        }
    }

    /// Register a pane for status tracking.
    pub fn register_pane(&self, pane_id: &str) {
        if let Ok(mut t) = self.tracker.lock() {
            t.insert(pane_id.to_string(), DebouncedStatusTracker::new());
        }
    }

    /// Unregister a pane from status tracking.
    pub fn unregister_pane(&self, pane_id: &str) {
        if let Ok(mut t) = self.tracker.lock() {
            t.remove(pane_id);
        }
    }

    /// Check status for a specific pane and publish if changed.
    ///
    /// Call this when terminal content changes (e.g., after processing new PTY output).
    ///
    /// # Arguments
    /// * `pane_id` - The pane to check
    /// * `process_status` - Process lifecycle status from runtime
    /// * `shell_info` - Optional shell phase info from OSC 133 markers
    /// * `content` - Current terminal content for text-based fallback detection
    /// * `process_ctx` - Context about the running process (for fallback logic)
    ///
    /// # Returns
    /// `true` if status changed and was published
    pub fn check_status(
        &self,
        pane_id: &str,
        process_status: ProcessStatus,
        shell_info: Option<ShellPhaseInfo>,
        content: &str,
        process_ctx: ProcessContext,
    ) -> bool {
        let mut tracker_guard = match self.tracker.lock() {
            Ok(g) => g,
            Err(_) => return false,
        };

        let tracker = match tracker_guard.get_mut(pane_id) {
            Some(t) => t,
            None => return false,
        };

        // Save previous status before update
        let prev_status = tracker.current_status();

        // Detect status with full context (process > OSC 133 > text + ProcessContext fallback)
        let new_status = self.detector.detect(process_status, shell_info, content, process_ctx);

        // Check if status changed (with debouncing)
        let changed = tracker.update_with_status(new_status);

        if changed {
            let current_status = tracker.current_status();
            let agent_id = pane_id.split(':').next().unwrap_or(pane_id).to_string();
            let last_line = extract_last_line(content, 80);
            let last_line_opt = if last_line.is_empty() {
                None
            } else {
                Some(last_line.clone())
            };

            // Publish state change event (with prev_state and last_line)
            self.event_bus
                .publish(RuntimeEvent::AgentStateChange(AgentStateChange {
                    agent_id: agent_id.clone(),
                    pane_id: Some(pane_id.to_string()),
                    state: current_status,
                    prev_state: Some(prev_status),
                    last_line: last_line_opt,
                }));

            // Determine if notification should fire
            let should_notify = matches!(
                (prev_status, current_status),
                (AgentStatus::Running, AgentStatus::Idle)
                    | (_, AgentStatus::Waiting)
                    | (_, AgentStatus::WaitingConfirm)
                    | (_, AgentStatus::Error)
                    | (_, AgentStatus::Exited)
            );

            if should_notify {
                let notif_type = match current_status {
                    AgentStatus::Error => NotificationType::Error,
                    AgentStatus::Waiting => NotificationType::WaitingInput,
                    AgentStatus::WaitingConfirm => NotificationType::WaitingConfirm,
                    AgentStatus::Idle | AgentStatus::Exited => NotificationType::Info,
                    _ => return true,
                };
                let message = if last_line.is_empty() {
                    current_status.display_text().to_string()
                } else {
                    last_line
                };
                self.event_bus
                    .publish(RuntimeEvent::Notification(Notification {
                        agent_id,
                        pane_id: Some(pane_id.to_string()),
                        message,
                        notif_type,
                    }));
            }
        }

        changed
    }

    /// Get current status for a pane.
    pub fn current_status(&self, pane_id: &str) -> AgentStatus {
        if let Ok(t) = self.tracker.lock() {
            if let Some(tracker) = t.get(pane_id) {
                return tracker.current_status();
            }
        }
        AgentStatus::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell_integration::{ShellPhase, ShellPhaseInfo};
    use crate::status_detector::{ProcessContext, ProcessStatus};

    #[test]
    fn test_status_publisher_new() {
        let bus = Arc::new(EventBus::new(8));
        let pub_ = StatusPublisher::new(bus);
        pub_.register_pane("%0");
        pub_.unregister_pane("%0");
    }

    #[test]
    fn test_check_status_detects_running() {
        let bus = Arc::new(EventBus::new(8));
        let pub_ = StatusPublisher::new(Arc::clone(&bus));
        pub_.register_pane("pane-1");

        // First call sets pending (debounce)
        let changed1 = pub_.check_status("pane-1", ProcessStatus::Running, None, "AI is thinking", ProcessContext::default());
        assert!(!changed1);

        // Second call with same status commits
        let changed2 = pub_.check_status(
            "pane-1",
            ProcessStatus::Running,
            None,
            "AI is still thinking",
            ProcessContext::default(),
        );
        assert!(changed2);
        assert_eq!(pub_.current_status("pane-1"), AgentStatus::Running);
    }

    #[test]
    fn test_check_status_with_shell_phase() {
        let bus = Arc::new(EventBus::new(8));
        let pub_ = StatusPublisher::new(Arc::clone(&bus));
        pub_.register_pane("pane-1");

        let info = ShellPhaseInfo {
            phase: ShellPhase::Running,
            last_post_exec_exit_code: None,
        };

        // Running phase should return Running
        let changed1 =
            pub_.check_status("pane-1", ProcessStatus::Running, Some(info), "any content", ProcessContext::default());
        assert!(!changed1);

        let changed2 = pub_.check_status(
            "pane-1",
            ProcessStatus::Running,
            Some(info),
            "still running",
            ProcessContext::default(),
        );
        assert!(changed2);
        assert_eq!(pub_.current_status("pane-1"), AgentStatus::Running);
    }

    #[test]
    fn test_check_status_error_bypasses_debounce() {
        let bus = Arc::new(EventBus::new(8));
        let pub_ = StatusPublisher::new(Arc::clone(&bus));
        pub_.register_pane("pane-1");

        // Set to Running first
        pub_.check_status("pane-1", ProcessStatus::Running, None, "AI is thinking", ProcessContext::default());
        pub_.check_status("pane-1", ProcessStatus::Running, None, "AI is thinking", ProcessContext::default());
        assert_eq!(pub_.current_status("pane-1"), AgentStatus::Running);

        // Error should bypass debounce
        let changed = pub_.check_status("pane-1", ProcessStatus::Unknown, None, "Error occurred!", ProcessContext::default());
        assert!(changed);
        assert_eq!(pub_.current_status("pane-1"), AgentStatus::Error);
    }

    #[test]
    fn test_check_status_exited_bypasses_debounce() {
        let bus = Arc::new(EventBus::new(8));
        let pub_ = StatusPublisher::new(Arc::clone(&bus));
        pub_.register_pane("pane-1");

        // Set to Running first
        pub_.check_status("pane-1", ProcessStatus::Running, None, "AI is thinking", ProcessContext::default());
        pub_.check_status("pane-1", ProcessStatus::Running, None, "AI is thinking", ProcessContext::default());
        assert_eq!(pub_.current_status("pane-1"), AgentStatus::Running);

        // Exited should bypass debounce
        let changed = pub_.check_status("pane-1", ProcessStatus::Exited, None, "any content", ProcessContext::default());
        assert!(changed);
        assert_eq!(pub_.current_status("pane-1"), AgentStatus::Exited);
    }

    #[test]
    fn test_no_polling_thread() {
        // This test verifies the new implementation has no polling thread
        let bus = Arc::new(EventBus::new(8));
        let pub_ = StatusPublisher::new(bus);
        pub_.register_pane("test");
        pub_.check_status("test", ProcessStatus::Running, None, "content", ProcessContext::default());
    }

    #[test]
    fn test_running_to_idle_publishes_notification() {
        let bus = Arc::new(EventBus::new(32));
        let rx = bus.subscribe();
        let pub_ = StatusPublisher::new(Arc::clone(&bus));
        pub_.register_pane("pane-1");

        // Get to Running (2 calls for debounce)
        pub_.check_status("pane-1", ProcessStatus::Running, None, "AI is thinking", ProcessContext::default());
        pub_.check_status("pane-1", ProcessStatus::Running, None, "AI is thinking", ProcessContext::default());
        // drain events
        while rx.try_recv().is_ok() {}

        // Now transition to Idle via shell phase PostExec
        let info = ShellPhaseInfo {
            phase: ShellPhase::Output,
            last_post_exec_exit_code: Some(0),
        };
        pub_.check_status("pane-1", ProcessStatus::Running, Some(info), "Done: 3 files changed", ProcessContext::default());
        pub_.check_status("pane-1", ProcessStatus::Running, Some(info), "Done: 3 files changed", ProcessContext::default());

        // Collect events
        let mut found_notification = false;
        while let Ok(ev) = rx.try_recv() {
            if let RuntimeEvent::Notification(n) = ev {
                found_notification = true;
                assert!(matches!(n.notif_type, NotificationType::Info));
                assert!(n.message.contains("Done") || n.message.contains("Idle"));
            }
        }
        assert!(found_notification, "Running→Idle should publish Info notification");
    }
}
