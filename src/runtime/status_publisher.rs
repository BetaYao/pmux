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
use crate::status_detector::{DebouncedStatusTracker, ProcessStatus, StatusDetector};
use crate::terminal::extract_last_line_filtered;

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

    /// Internal: publish a status change event and optional notification.
    /// Returns `true` if status changed.
    fn publish_status_change(
        &self,
        pane_id: &str,
        new_status: AgentStatus,
        content: &str,
        skip_patterns: &[String],
    ) -> bool {
        let mut tracker_guard = match self.tracker.lock() {
            Ok(g) => g,
            Err(_) => return false,
        };

        let tracker = match tracker_guard.get_mut(pane_id) {
            Some(t) => t,
            None => return false,
        };

        let prev_status = tracker.current_status();
        let changed = tracker.update_with_status(new_status);

        if changed {
            let current_status = tracker.current_status();
            let agent_id = pane_id.split(':').next().unwrap_or(pane_id).to_string();
            let last_line = extract_last_line_filtered(content, 80, skip_patterns);
            let last_line_opt = if last_line.is_empty() {
                None
            } else {
                Some(last_line.clone())
            };

            self.event_bus
                .publish(RuntimeEvent::AgentStateChange(AgentStateChange {
                    agent_id: agent_id.clone(),
                    pane_id: Some(pane_id.to_string()),
                    state: current_status,
                    prev_state: Some(prev_status),
                    last_line: last_line_opt,
                }));

            let should_notify = matches!(
                (prev_status, current_status),
                (AgentStatus::Running, AgentStatus::Idle)
                    | (_, AgentStatus::Waiting)
                    | (_, AgentStatus::Error)
                    | (_, AgentStatus::Exited)
            );

            if should_notify {
                let notif_type = match current_status {
                    AgentStatus::Error => NotificationType::Error,
                    AgentStatus::Waiting => NotificationType::WaitingInput,
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

    /// Check status for a specific pane and publish if changed.
    ///
    /// Call this when terminal content changes (e.g., after processing new PTY output).
    ///
    /// # Returns
    /// `true` if status changed and was published
    pub fn check_status(
        &self,
        pane_id: &str,
        process_status: ProcessStatus,
        shell_info: Option<ShellPhaseInfo>,
        content: &str,
        skip_patterns: &[String],
    ) -> bool {
        let new_status = self.detector.detect(process_status, shell_info, content);
        self.publish_status_change(pane_id, new_status, content, skip_patterns)
    }

    /// Force a specific status for a pane (bypassing OSC 133 / ProcessStatus detection).
    /// Used when agent text pattern matching determines status independently.
    ///
    /// # Returns
    /// `true` if status changed and was published
    pub fn force_status(
        &self,
        pane_id: &str,
        status: AgentStatus,
        content: &str,
        skip_patterns: &[String],
    ) -> bool {
        self.publish_status_change(pane_id, status, content, skip_patterns)
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
    use crate::status_detector::ProcessStatus;

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

        let running_info = ShellPhaseInfo {
            phase: ShellPhase::Running,
            last_post_exec_exit_code: None,
        };

        // OSC 133 Running commits immediately (no debounce)
        let changed = pub_.check_status("pane-1", ProcessStatus::Running, Some(running_info), "", &[]);
        assert!(changed);
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

        // Running phase commits immediately
        let changed = pub_.check_status("pane-1", ProcessStatus::Running, Some(info), "any content", &[]);
        assert!(changed);
        assert_eq!(pub_.current_status("pane-1"), AgentStatus::Running);
    }

    #[test]
    fn test_check_status_error_immediate() {
        let bus = Arc::new(EventBus::new(8));
        let pub_ = StatusPublisher::new(Arc::clone(&bus));
        pub_.register_pane("pane-1");

        // Set to Running first
        let running_info = ShellPhaseInfo {
            phase: ShellPhase::Running,
            last_post_exec_exit_code: None,
        };
        pub_.check_status("pane-1", ProcessStatus::Running, Some(running_info), "", &[]);
        assert_eq!(pub_.current_status("pane-1"), AgentStatus::Running);

        // Process error commits immediately
        let changed = pub_.check_status("pane-1", ProcessStatus::Error, None, "", &[]);
        assert!(changed);
        assert_eq!(pub_.current_status("pane-1"), AgentStatus::Error);
    }

    #[test]
    fn test_check_status_exited_immediate() {
        let bus = Arc::new(EventBus::new(8));
        let pub_ = StatusPublisher::new(Arc::clone(&bus));
        pub_.register_pane("pane-1");

        // Set to Running first
        let running_info = ShellPhaseInfo {
            phase: ShellPhase::Running,
            last_post_exec_exit_code: None,
        };
        pub_.check_status("pane-1", ProcessStatus::Running, Some(running_info), "", &[]);
        assert_eq!(pub_.current_status("pane-1"), AgentStatus::Running);

        // Exited commits immediately
        let changed = pub_.check_status("pane-1", ProcessStatus::Exited, None, "", &[]);
        assert!(changed);
        assert_eq!(pub_.current_status("pane-1"), AgentStatus::Exited);
    }

    #[test]
    fn test_no_polling_thread() {
        // This test verifies the new implementation has no polling thread
        let bus = Arc::new(EventBus::new(8));
        let pub_ = StatusPublisher::new(bus);
        pub_.register_pane("test");
        pub_.check_status("test", ProcessStatus::Running, None, "content", &[]);
    }

    #[test]
    fn test_running_to_idle_publishes_notification() {
        let bus = Arc::new(EventBus::new(32));
        let rx = bus.subscribe();
        let pub_ = StatusPublisher::new(Arc::clone(&bus));
        pub_.register_pane("pane-1");

        // Get to Running via OSC 133 Running phase
        let running_info = ShellPhaseInfo {
            phase: ShellPhase::Running,
            last_post_exec_exit_code: None,
        };
        pub_.check_status("pane-1", ProcessStatus::Running, Some(running_info), "", &[]);
        // drain events
        while rx.try_recv().is_ok() {}

        // Transition to Idle via shell phase PostExec with exit code 0
        let idle_info = ShellPhaseInfo {
            phase: ShellPhase::Output,
            last_post_exec_exit_code: Some(0),
        };
        pub_.check_status("pane-1", ProcessStatus::Running, Some(idle_info), "Done: 3 files changed", &[]);

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

    #[test]
    fn test_force_status_publishes_same_events_as_before() {
        let bus = Arc::new(EventBus::new(32));
        let rx = bus.subscribe();
        let pub_ = StatusPublisher::new(Arc::clone(&bus));
        pub_.register_pane("pane-1");

        // Force to Running
        let changed = pub_.force_status("pane-1", AgentStatus::Running, "some output", &[]);
        assert!(changed);
        assert_eq!(pub_.current_status("pane-1"), AgentStatus::Running);

        // Drain events
        let mut state_change_count = 0;
        while let Ok(ev) = rx.try_recv() {
            if let RuntimeEvent::AgentStateChange(sc) = ev {
                assert_eq!(sc.state, AgentStatus::Running);
                state_change_count += 1;
            }
        }
        assert_eq!(state_change_count, 1);
    }
}
