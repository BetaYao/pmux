//! hooks/handler.rs - Resolve HookEvent cwd/session_id to pane_id, emit AgentStateChange

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::agent_status::AgentStatus;
use crate::hooks::server::HookPayload;
use crate::runtime::{
    EventAgentStateChange as AgentStateChange, HookEvent, Notification, NotificationType,
    RuntimeEvent, SharedEventBus,
};

/// Maps session_id → agent_id and cwd → agent_id.
/// Written by AppRoot when panes are registered/switched.
/// Read by HookEventHandler to resolve incoming hook events.
#[derive(Default)]
pub struct PaneIndex {
    /// session_id (from SessionStart hook) → agent_id
    pub by_session: HashMap<String, String>,
    /// normalized worktree path → agent_id (fallback)
    pub by_cwd: HashMap<String, String>,
}

impl PaneIndex {
    /// Register a pane's worktree path
    pub fn register_pane(&mut self, agent_id: &str, worktree_path: &str) {
        self.by_cwd.insert(normalize_path(worktree_path), agent_id.to_string());
    }

    /// Record session_id → agent_id mapping (from SessionStart hook)
    pub fn register_session(&mut self, session_id: &str, agent_id: &str) {
        self.by_session.insert(session_id.to_string(), agent_id.to_string());
    }

    /// Resolve hook event to agent_id: try session_id first, then longest cwd prefix match
    pub fn resolve(&self, session_id: &str, cwd: &str) -> Option<&str> {
        if !session_id.is_empty() {
            if let Some(id) = self.by_session.get(session_id) {
                return Some(id.as_str());
            }
        }
        let cwd_norm = normalize_path(cwd);
        self.by_cwd
            .iter()
            .filter(|(path, _)| cwd_norm.starts_with(path.as_str()))
            .max_by_key(|(path, _)| path.len())
            .map(|(_, id)| id.as_str())
    }
}

fn normalize_path(p: &str) -> String {
    p.trim_end_matches('/').to_string()
}

/// Processes incoming HookEvents and emits AgentStateChange + Notification to EventBus
pub struct HookEventHandler {
    pub index: Arc<RwLock<PaneIndex>>,
    pub event_bus: SharedEventBus,
}

impl HookEventHandler {
    pub fn new(index: Arc<RwLock<PaneIndex>>, event_bus: SharedEventBus) -> Self {
        Self { index, event_bus }
    }

    pub fn handle(&self, event: &HookEvent) {
        // On SessionStart: register session_id → agent_id mapping
        if event.hook_event_name == "SessionStart" && !event.session_id.is_empty() {
            let cwd_norm = normalize_path(&event.cwd);
            let agent_id = {
                let index = self.index.read().unwrap();
                index.by_cwd
                    .iter()
                    .filter(|(p, _)| cwd_norm.starts_with(p.as_str()))
                    .max_by_key(|(p, _)| p.len())
                    .map(|(_, id)| id.clone())
            };
            if let Some(agent_id) = agent_id {
                self.index.write().unwrap()
                    .register_session(&event.session_id, &agent_id);
            }
            return;
        }

        let agent_id = {
            let index = self.index.read().unwrap();
            index.resolve(&event.session_id, &event.cwd).map(|s| s.to_string())
        };
        let Some(agent_id) = agent_id else { return };

        let p = HookPayload {
            session_id: event.session_id.clone(),
            cwd: event.cwd.clone(),
            hook_event_name: event.hook_event_name.clone(),
            tool_name: event.tool_name.clone(),
            pmux_source: Some(event.source_tool.clone()),
        };
        let Some(status_str) = p.to_status() else { return };
        let status = AgentStatus::from_status_str(status_str);

        self.event_bus.publish(RuntimeEvent::AgentStateChange(AgentStateChange {
            agent_id: agent_id.clone(),
            pane_id: None,
            state: status.clone(),
            prev_state: None,
            last_line: Some(format!("[hook] {}", event.hook_event_name)),
        }));

        // Emit Notification for Waiting and Error transitions
        let notif_type = match status {
            AgentStatus::Waiting => Some(NotificationType::WaitingInput),
            AgentStatus::Error   => Some(NotificationType::Error),
            _ => None,
        };
        if let Some(ntype) = notif_type {
            self.event_bus.publish(RuntimeEvent::Notification(Notification {
                agent_id: agent_id.clone(),
                pane_id: None,
                message: format!("{}: {}", event.source_tool, event.hook_event_name),
                notif_type: ntype,
            }));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use crate::runtime::EventBus;

    fn make_handler() -> (HookEventHandler, Arc<RwLock<PaneIndex>>, flume::Receiver<RuntimeEvent>) {
        let bus = Arc::new(EventBus::new(16));
        let rx = bus.subscribe();
        let index = Arc::new(RwLock::new(PaneIndex::default()));
        let handler = HookEventHandler::new(Arc::clone(&index), Arc::clone(&bus));
        (handler, index, rx)
    }

    #[test]
    fn test_resolve_by_cwd_prefix() {
        let mut idx = PaneIndex::default();
        idx.register_pane("agent-1", "/workspace/repo-a");
        idx.register_pane("agent-2", "/workspace/repo-b");
        assert_eq!(idx.resolve("", "/workspace/repo-a/src"), Some("agent-1"));
        assert_eq!(idx.resolve("", "/workspace/repo-b/src/main.rs"), Some("agent-2"));
        assert_eq!(idx.resolve("", "/other/path"), None);
    }

    #[test]
    fn test_resolve_session_id_takes_priority() {
        let mut idx = PaneIndex::default();
        idx.register_pane("agent-1", "/workspace/repo");
        idx.register_session("sess-xyz", "agent-2");
        assert_eq!(idx.resolve("sess-xyz", "/workspace/repo"), Some("agent-2"));
    }

    #[test]
    fn test_hook_stop_emits_idle_state_change() {
        let (handler, index, rx) = make_handler();
        index.write().unwrap().register_pane("agent-1", "/workspace/repo");

        handler.handle(&HookEvent {
            session_id: String::new(),
            cwd: "/workspace/repo".to_string(),
            hook_event_name: "Stop".to_string(),
            tool_name: None,
            source_tool: "claude_code".to_string(),
        });

        let ev = rx.try_recv().unwrap();
        match ev {
            RuntimeEvent::AgentStateChange(a) => {
                assert_eq!(a.agent_id, "agent-1");
                assert_eq!(a.state, AgentStatus::Idle);
            }
            _ => panic!("expected AgentStateChange"),
        }
    }

    #[test]
    fn test_hook_waiting_emits_notification() {
        let (handler, index, rx) = make_handler();
        index.write().unwrap().register_pane("agent-1", "/workspace/repo");

        handler.handle(&HookEvent {
            session_id: String::new(),
            cwd: "/workspace/repo/subdir".to_string(),
            hook_event_name: "aider_waiting".to_string(),
            tool_name: None,
            source_tool: "aider".to_string(),
        });

        let ev1 = rx.try_recv().unwrap();
        assert!(matches!(ev1, RuntimeEvent::AgentStateChange(_)));

        let ev2 = rx.try_recv().unwrap();
        match ev2 {
            RuntimeEvent::Notification(n) => {
                assert!(matches!(n.notif_type, NotificationType::WaitingInput));
            }
            _ => panic!("expected Notification"),
        }
    }

    #[test]
    fn test_unknown_cwd_ignored() {
        let (handler, index, rx) = make_handler();
        index.write().unwrap().register_pane("agent-1", "/workspace/repo");

        handler.handle(&HookEvent {
            session_id: String::new(),
            cwd: "/other/unrelated/path".to_string(),
            hook_event_name: "Stop".to_string(),
            tool_name: None,
            source_tool: "claude_code".to_string(),
        });

        assert!(rx.try_recv().is_err(), "should emit nothing for unknown cwd");
    }
}
