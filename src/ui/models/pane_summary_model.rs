// ui/models/pane_summary_model.rs - Per-pane summary (status, last_line, status_since)
use crate::agent_status::AgentStatus;
use std::collections::HashMap;
use std::time::Instant;

/// Summary data for a single pane, used by Sidebar.
#[derive(Clone, Debug)]
pub struct PaneSummary {
    pub status: AgentStatus,
    pub last_line: String,
    pub status_since: Instant,
}

/// Shared model: tracks per-pane summaries for Sidebar display.
pub struct PaneSummaryModel {
    summaries: HashMap<String, PaneSummary>,
}

impl PaneSummaryModel {
    pub fn new() -> Self {
        Self {
            summaries: HashMap::new(),
        }
    }

    /// Update a pane's summary. Returns (status_changed, previous_status).
    /// Resets `status_since` only when status changes. Updates `last_line` if non-empty regardless.
    pub fn update(
        &mut self,
        pane_id: &str,
        status: AgentStatus,
        last_line: Option<String>,
    ) -> (bool, Option<AgentStatus>) {
        let now = Instant::now();
        if let Some(existing) = self.summaries.get_mut(pane_id) {
            let prev = existing.status;
            let changed = prev != status;
            if changed {
                existing.status = status;
                existing.status_since = now;
            }
            if let Some(ref ll) = last_line {
                if !ll.is_empty() {
                    existing.last_line = ll.clone();
                }
            }
            (changed, Some(prev))
        } else {
            self.summaries.insert(
                pane_id.to_string(),
                PaneSummary {
                    status,
                    last_line: last_line.unwrap_or_default(),
                    status_since: now,
                },
            );
            (true, None)
        }
    }

    /// All summaries (for Sidebar to read).
    pub fn summaries(&self) -> &HashMap<String, PaneSummary> {
        &self.summaries
    }

    /// Whether any pane is currently Running (for animation timer lifecycle).
    pub fn has_running(&self) -> bool {
        self.summaries.values().any(|s| s.status == AgentStatus::Running)
    }

    /// Highest-priority pane summary for a worktree prefix.
    /// Matches pane_id == prefix or pane_id starts with "{prefix}:".
    pub fn summary_for_prefix(&self, prefix: &str) -> Option<PaneSummary> {
        let colon_prefix = format!("{}:", prefix);
        self.summaries
            .iter()
            .filter(|(k, _)| *k == prefix || k.starts_with(&colon_prefix))
            .max_by_key(|(_, v)| v.status.priority())
            .map(|(_, v)| v.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_update_new_pane() {
        let mut model = PaneSummaryModel::new();
        let (changed, prev) = model.update("pane-1", AgentStatus::Running, Some("hello".into()));
        assert!(changed);
        assert_eq!(prev, None);
        assert_eq!(model.summaries()["pane-1"].status, AgentStatus::Running);
        assert_eq!(model.summaries()["pane-1"].last_line, "hello");
    }

    #[test]
    fn test_update_resets_since_on_status_change() {
        let mut model = PaneSummaryModel::new();
        model.update("p", AgentStatus::Running, Some("line1".into()));
        let since1 = model.summaries()["p"].status_since;
        std::thread::sleep(std::time::Duration::from_millis(5));
        let (changed, prev) = model.update("p", AgentStatus::Idle, Some("line2".into()));
        assert!(changed);
        assert_eq!(prev, Some(AgentStatus::Running));
        assert!(model.summaries()["p"].status_since > since1);
    }

    #[test]
    fn test_update_keeps_since_on_same_status() {
        let mut model = PaneSummaryModel::new();
        model.update("p", AgentStatus::Running, Some("line1".into()));
        let since1 = model.summaries()["p"].status_since;
        let (changed, _) = model.update("p", AgentStatus::Running, Some("line2".into()));
        assert!(!changed);
        assert_eq!(model.summaries()["p"].status_since, since1);
        assert_eq!(model.summaries()["p"].last_line, "line2");
    }

    #[test]
    fn test_has_running() {
        let mut model = PaneSummaryModel::new();
        assert!(!model.has_running());
        model.update("p1", AgentStatus::Running, None);
        assert!(model.has_running());
        model.update("p1", AgentStatus::Idle, None);
        assert!(!model.has_running());
    }

    #[test]
    fn test_summary_for_prefix() {
        let mut model = PaneSummaryModel::new();
        model.update("local:/feat", AgentStatus::Idle, Some("idle".into()));
        model.update("local:/feat:split-0", AgentStatus::Error, Some("err".into()));
        model.update("local:/other", AgentStatus::Running, None);

        let s = model.summary_for_prefix("local:/feat").unwrap();
        assert_eq!(s.status, AgentStatus::Error); // Error > Idle
        assert_eq!(s.last_line, "err");
    }

    #[test]
    fn test_empty_last_line_not_overwritten() {
        let mut model = PaneSummaryModel::new();
        model.update("p", AgentStatus::Running, Some("hello".into()));
        model.update("p", AgentStatus::Running, Some("".into()));
        assert_eq!(model.summaries()["p"].last_line, "hello");
    }
}
