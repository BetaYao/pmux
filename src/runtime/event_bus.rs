//! event_bus.rs - Runtime Event Bus for Agent state, Terminal output, Notifications

use std::sync::Arc;
use std::time::Instant;

use crate::agent_status::AgentStatus;
use crate::runtime::agent_runtime::{AgentId, PaneId};

#[derive(Clone, Debug)]
pub enum RuntimeEvent {
    AgentStateChange(AgentStateChange),
    TerminalOutput(TerminalOutput),
    Notification(Notification),
}

#[derive(Clone, Debug)]
pub struct AgentStateChange {
    pub agent_id: AgentId,
    pub pane_id: Option<PaneId>,
    pub state: AgentStatus,
    pub prev_state: Option<AgentStatus>,
    pub last_line: Option<String>,
}

#[derive(Clone, Debug)]
pub struct TerminalOutput {
    pub pane_id: PaneId,
    pub bytes: Vec<u8>,
    pub timestamp: Instant,
}

#[derive(Clone, Debug)]
pub struct Notification {
    pub agent_id: AgentId,
    /// Pane ID for NotificationManager grouping (e.g. tmux pane target)
    pub pane_id: Option<PaneId>,
    pub message: String,
    pub notif_type: NotificationType,
}

#[derive(Clone, Debug)]
pub enum NotificationType {
    WaitingInput,
    Error,
    Info,
}

/// Event Bus - publish/subscribe for runtime events.
/// Uses flume for Sync receiver (works with blocking::unblock).
pub struct EventBus {
    tx: flume::Sender<RuntimeEvent>,
    rx: std::sync::Mutex<flume::Receiver<RuntimeEvent>>,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (tx, rx) = flume::bounded(capacity);
        Self {
            tx,
            rx: std::sync::Mutex::new(rx),
        }
    }

    pub fn publish(&self, event: RuntimeEvent) {
        let _ = self.tx.send(event);
    }

    pub fn subscribe(&self) -> flume::Receiver<RuntimeEvent> {
        self.rx.lock().unwrap().clone()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new(256)
    }
}

/// Shared EventBus for app-wide use
pub type SharedEventBus = Arc<EventBus>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_bus_publish_subscribe() {
        let bus = EventBus::new(8);
        let rx = bus.subscribe();
        bus.publish(RuntimeEvent::AgentStateChange(AgentStateChange {
            agent_id: "a1".to_string(),
            pane_id: Some("%0".to_string()),
            state: AgentStatus::Running,
            prev_state: Some(AgentStatus::Idle),
            last_line: Some("thinking...".to_string()),
        }));
        let ev = rx.recv().unwrap();
        match ev {
            RuntimeEvent::AgentStateChange(a) => {
                assert_eq!(a.agent_id, "a1");
                assert_eq!(a.pane_id, Some("%0".to_string()));
            }
            _ => panic!("expected AgentStateChange"),
        }
    }
}
