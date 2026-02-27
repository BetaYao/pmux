//! status_publisher.rs - Publishes AgentStateChange to EventBus
//!
//! Replaces StatusPoller. Polls panes via capture_fn, uses StatusDetector,
//! publishes to EventBus when status changes.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::agent_status::AgentStatus;
use crate::runtime::event_bus::{AgentStateChange, EventBus, Notification, NotificationType, RuntimeEvent};
use crate::status_detector::DebouncedStatusTracker;

/// Publishes agent status changes to EventBus. Replaces StatusPoller.
pub struct StatusPublisher {
    event_bus: Arc<EventBus>,
    tracker: Arc<std::sync::Mutex<HashMap<String, DebouncedStatusTracker>>>,
    running: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
    interval_ms: u64,
}

impl StatusPublisher {
    pub fn new(event_bus: Arc<EventBus>) -> Self {
        Self {
            event_bus,
            tracker: Arc::new(std::sync::Mutex::new(HashMap::new())),
            running: Arc::new(AtomicBool::new(false)),
            handle: None,
            interval_ms: 500,
        }
    }

    pub fn register_pane(&self, pane_id: &str) {
        if let Ok(mut t) = self.tracker.lock() {
            t.insert(pane_id.to_string(), DebouncedStatusTracker::new());
        }
    }

    pub fn unregister_pane(&self, pane_id: &str) {
        if let Ok(mut t) = self.tracker.lock() {
            t.remove(pane_id);
        }
    }

    pub fn start<F>(&mut self, capture_fn: F)
    where
        F: Fn(&str) -> Option<String> + Send + 'static,
    {
        self.stop();
        self.running.store(true, Ordering::Relaxed);
        let tracker = Arc::clone(&self.tracker);
        let event_bus = Arc::clone(&self.event_bus);
        let running = Arc::clone(&self.running);
        let interval = Duration::from_millis(self.interval_ms);

        self.handle = Some(thread::spawn(move || {
            while running.load(Ordering::Relaxed) {
                let pane_ids: Vec<String> = {
                    if let Ok(t) = tracker.lock() {
                        t.keys().cloned().collect()
                    } else {
                        vec![]
                    }
                };

                for pane_id in pane_ids {
                    if let Some(content) = capture_fn(&pane_id) {
                        let mut changed = false;
                        let new_status = {
                            if let Ok(mut t) = tracker.lock() {
                                let tr = t.entry(pane_id.clone()).or_insert_with(DebouncedStatusTracker::new);
                                changed = tr.update(&content);
                                tr.current_status()
                            } else {
                                AgentStatus::Unknown
                            }
                        };
                        if changed {
                            let agent_id = pane_id.split(':').next().unwrap_or(&pane_id).to_string();
                            event_bus.publish(RuntimeEvent::AgentStateChange(AgentStateChange {
                                agent_id: agent_id.clone(),
                                pane_id: Some(pane_id.clone()),
                                state: new_status,
                            }));
                            if new_status.is_urgent() {
                                let notif_type = match new_status {
                                    AgentStatus::Error => NotificationType::Error,
                                    AgentStatus::Waiting => NotificationType::WaitingInput,
                                    _ => continue,
                                };
                                event_bus.publish(RuntimeEvent::Notification(Notification {
                                    agent_id,
                                    pane_id: Some(pane_id),
                                    message: new_status.display_text().to_string(),
                                    notif_type,
                                }));
                            }
                        }
                    }
                }

                thread::sleep(interval);
            }
        }));
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_publisher_new() {
        let bus = Arc::new(EventBus::new(8));
        let pub_ = StatusPublisher::new(bus);
        pub_.register_pane("%0");
        pub_.unregister_pane("%0");
    }

    #[test]
    fn test_status_publisher_start_stop() {
        let bus = Arc::new(EventBus::new(8));
        let mut pub_ = StatusPublisher::new(Arc::clone(&bus));
        pub_.register_pane("%0");
        pub_.start(|_| Some("hello".to_string()));
        std::thread::sleep(Duration::from_millis(100));
        pub_.stop();
    }
}
