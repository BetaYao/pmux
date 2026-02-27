//! agent.rs - Agent model and state machine
//!
//! Agent 作为一等公民，封装 worktree、panes、state。

use std::path::PathBuf;

use crate::agent_status::AgentStatus;
use crate::runtime::agent_runtime::AgentId;

/// Agent 模型：一个 worktree 对应一个 Agent，可管理多个 Pane。
#[derive(Clone, Debug)]
pub struct Agent {
    pub id: AgentId,
    pub worktree: PathBuf,
    pub state: AgentStatus,
    pub panes: Vec<String>,
}

impl Agent {
    pub fn new(id: AgentId, worktree: PathBuf) -> Self {
        Self {
            id,
            worktree,
            state: AgentStatus::Unknown,
            panes: Vec::new(),
        }
    }

    pub fn with_panes(mut self, panes: Vec<String>) -> Self {
        self.panes = panes;
        self
    }

    pub fn set_state(&mut self, state: AgentStatus) {
        self.state = state;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_new() {
        let agent = Agent::new("a1".to_string(), PathBuf::from("/repo/feat-x"));
        assert_eq!(agent.id, "a1");
        assert_eq!(agent.worktree, PathBuf::from("/repo/feat-x"));
        assert_eq!(agent.state, AgentStatus::Unknown);
        assert!(agent.panes.is_empty());
    }

    #[test]
    fn test_agent_with_panes() {
        let agent = Agent::new("a1".to_string(), PathBuf::from("/repo"))
            .with_panes(vec!["%0".to_string(), "%1".to_string()]);
        assert_eq!(agent.panes.len(), 2);
        assert_eq!(agent.panes[0], "%0");
    }

    #[test]
    fn test_agent_set_state() {
        let mut agent = Agent::new("a1".to_string(), PathBuf::from("/repo"));
        agent.set_state(AgentStatus::Running);
        assert_eq!(agent.state, AgentStatus::Running);
    }
}
