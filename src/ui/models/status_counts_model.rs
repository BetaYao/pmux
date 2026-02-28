// ui/models/status_counts_model.rs - Shared model for agent status counts
use crate::agent_status::{AgentStatus, StatusCounts};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Shared model for agent status counts. TopBar/StatusBar observe this.
/// Holds pane_statuses and computes counts; does NOT implement Render.
pub struct StatusCountsModel {
    pane_statuses: Arc<Mutex<HashMap<String, AgentStatus>>>,
    pub counts: StatusCounts,
}

impl StatusCountsModel {
    pub fn new(pane_statuses: Arc<Mutex<HashMap<String, AgentStatus>>>) -> Self {
        let mut s = Self {
            pane_statuses,
            counts: StatusCounts::new(),
        };
        s.recompute_counts();
        s
    }

    pub fn set_counts(&mut self, counts: StatusCounts) {
        self.counts = counts;
    }

    /// Update status for a pane and recompute aggregate counts.
    pub fn update_pane_status(&mut self, pane_id: &str, status: AgentStatus) {
        let needs_recompute = {
            if let Ok(mut statuses) = self.pane_statuses.lock() {
                let prev = statuses.get(pane_id);
                if prev != Some(&status) {
                    statuses.insert(pane_id.to_string(), status);
                    true
                } else {
                    false
                }
            } else {
                false
            }
        };
        if needs_recompute {
            self.recompute_counts();
        }
    }

    /// Recompute counts from current pane_statuses.
    pub fn recompute_counts(&mut self) {
        let mut counts = StatusCounts::new();
        if let Ok(statuses) = self.pane_statuses.lock() {
            for status in statuses.values() {
                counts.increment(status);
            }
        }
        self.counts = counts;
    }

    /// Shared pane_statuses ref for Sidebar (per-pane status display).
    pub fn pane_statuses(&self) -> Arc<Mutex<HashMap<String, AgentStatus>>> {
        Arc::clone(&self.pane_statuses)
    }
}

