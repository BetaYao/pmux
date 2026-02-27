//! runtime - Agent Runtime layer for pmux
//!
//! AgentRuntime trait and local PTY backend. UI 只依赖此层 API。

mod agent;
mod agent_runtime;
mod event_bus;
mod state;
mod status_publisher;
pub mod backends;

pub use agent::Agent;
pub use agent_runtime::{AgentId, AgentStateChange, PaneId, RuntimeError, TerminalEvent};
pub use agent_runtime::AgentRuntime;
pub use event_bus::{AgentStateChange as EventAgentStateChange, EventBus, Notification, NotificationType, RuntimeEvent, SharedEventBus, TerminalOutput};
pub use state::{RuntimeState, RuntimeStateError, WorktreeState, WorkspaceState};
pub use status_publisher::StatusPublisher;

/// Check if a backend session exists. For local PTY, always false (no session recovery).
pub fn session_exists(backend: &str, _session_id: &str) -> bool {
    match backend {
        "local" => false,
        _ => false,
    }
}
