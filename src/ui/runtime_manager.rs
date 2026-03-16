//! RuntimeManager - manages runtime lifecycle, EventBus, StatusPublisher, and agent status.
//!
//! Extracted from AppRoot Phase 3.
//! Publishes status changes to StatusCountsModel (TopBar) and PaneSummaryModel (Sidebar).

use crate::agent_status::{AgentStatus, StatusCounts};
use crate::runtime::{AgentRuntime, EventBus, StatusPublisher};
use crate::ui::models::{StatusCountsModel, PaneSummaryModel};
use crate::ui::topbar_entity::TopBarEntity;
use gpui::*;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::sync::atomic::AtomicBool;

pub struct RuntimeManager {
    pub runtime: Option<Arc<dyn AgentRuntime>>,
    pub event_bus: Arc<EventBus>,
    pub status_publisher: Option<StatusPublisher>,
    pub session_scanner: Option<crate::session_scanner::SessionScanner>,
    pub pane_statuses: Arc<Mutex<HashMap<String, AgentStatus>>>,
    pub status_counts: StatusCounts,
    pub status_key_base: Option<String>,
    pub event_bus_subscription_started: bool,
    pub pane_index: Option<Arc<std::sync::RwLock<crate::hooks::handler::PaneIndex>>>,
    pub hook_handler: Option<Arc<crate::hooks::handler::HookEventHandler>>,
    pub modal_overlay_open: Arc<AtomicBool>,
    // Models
    pub status_counts_model: Option<Entity<StatusCountsModel>>,
    pub topbar_entity: Option<Entity<TopBarEntity>>,
    pub pane_summary_model: Option<Entity<PaneSummaryModel>>,
    // Animation
    pub running_animation_frame: usize,
    pub running_animation_task: Option<gpui::Task<()>>,
    // Shared state from AppRoot (for notification suppression)
    pub window_focused_shared: Arc<AtomicBool>,
    pub last_input_time: Arc<Mutex<std::time::Instant>>,
}

const RUNNING_ANIMATION_INTERVAL_MS: u64 = 250;
/// Running animation frames (currently used for display text, kept for future use)
#[allow(dead_code)]
const RUNNING_FRAMES: [&str; 4] = ["⠋", "⠙", "⠹", "⠸"];

impl RuntimeManager {
    pub fn new(
        event_bus: Arc<EventBus>,
        modal_overlay_open: Arc<AtomicBool>,
        window_focused_shared: Arc<AtomicBool>,
        last_input_time: Arc<Mutex<std::time::Instant>>,
    ) -> Self {
        Self {
            runtime: None,
            event_bus,
            status_publisher: None,
            session_scanner: None,
            pane_statuses: Arc::new(Mutex::new(HashMap::new())),
            status_counts: StatusCounts::new(),
            status_key_base: None,
            event_bus_subscription_started: false,
            pane_index: None,
            hook_handler: None,
            modal_overlay_open,
            status_counts_model: None,
            topbar_entity: None,
            pane_summary_model: None,
            running_animation_frame: 0,
            running_animation_task: None,
            window_focused_shared,
            last_input_time,
        }
    }

    /// Clear pane statuses for a given key base prefix (on workspace switch).
    pub fn clear_statuses_for_prefix(&mut self, prefix: &str) {
        if let Ok(mut statuses) = self.pane_statuses.lock() {
            let colon_prefix = format!("{}:", prefix);
            statuses.retain(|k, _| k != prefix && !k.starts_with(&colon_prefix));
        }
    }

    /// Update status counts from current pane_statuses.
    pub fn refresh_status_counts(&mut self) {
        if let Ok(statuses) = self.pane_statuses.lock() {
            self.status_counts = StatusCounts::from_pane_statuses_per_worktree(&statuses);
        }
    }

    /// Check if current runtime matches the given workspace's tmux session.
    pub fn current_runtime_matches_session(&self, workspace_path: &std::path::Path) -> bool {
        if let Some(ref rt) = self.runtime {
            if let Some((session, _)) = rt.session_info() {
                return session == crate::runtime::backends::session_name_for_workspace(workspace_path);
            }
        }
        false
    }

    /// Manage the running animation timer (250ms tick when any pane is Running).
    pub fn manage_running_animation(&mut self, cx: &mut Context<Self>) {
        let has_running = self.pane_summary_model
            .as_ref()
            .map(|m| m.read(cx).has_running())
            .unwrap_or(false);

        if has_running && self.running_animation_task.is_none() {
            let pane_summary_model = self.pane_summary_model.clone();
            self.running_animation_task = Some(cx.spawn(async move |entity, cx| {
                loop {
                    blocking::unblock(|| std::thread::sleep(std::time::Duration::from_millis(RUNNING_ANIMATION_INTERVAL_MS))).await;
                    let should_continue = entity.update(cx, |this, cx| {
                        this.running_animation_frame = this.running_animation_frame.wrapping_add(1);
                        let still_running = pane_summary_model
                            .as_ref()
                            .map(|m| m.read(cx).has_running())
                            .unwrap_or(false);
                        if still_running {
                            cx.notify();
                            true
                        } else {
                            this.running_animation_frame = 0;
                            this.running_animation_task = None;
                            cx.notify();
                            false
                        }
                    });
                    match should_continue {
                        Ok(true) => continue,
                        _ => break,
                    }
                }
            }));
        } else if !has_running && self.running_animation_task.is_some() {
            self.running_animation_task = None;
            self.running_animation_frame = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clear_statuses_for_prefix() {
        let bus = Arc::new(EventBus::new(8));
        let modal = Arc::new(AtomicBool::new(false));
        let focused = Arc::new(AtomicBool::new(true));
        let last_input = Arc::new(Mutex::new(std::time::Instant::now()));
        let mut rm = RuntimeManager::new(bus, modal, focused, last_input);

        if let Ok(mut statuses) = rm.pane_statuses.lock() {
            statuses.insert("local:/path/feat".to_string(), AgentStatus::Running);
            statuses.insert("local:/path/feat:split-0".to_string(), AgentStatus::Idle);
            statuses.insert("local:/path/main".to_string(), AgentStatus::Waiting);
        }

        rm.clear_statuses_for_prefix("local:/path/feat");

        if let Ok(statuses) = rm.pane_statuses.lock() {
            assert_eq!(statuses.len(), 1);
            assert!(statuses.contains_key("local:/path/main"));
        }
    }

    #[test]
    fn test_refresh_status_counts() {
        let bus = Arc::new(EventBus::new(8));
        let modal = Arc::new(AtomicBool::new(false));
        let focused = Arc::new(AtomicBool::new(true));
        let last_input = Arc::new(Mutex::new(std::time::Instant::now()));
        let mut rm = RuntimeManager::new(bus, modal, focused, last_input);

        if let Ok(mut statuses) = rm.pane_statuses.lock() {
            statuses.insert("local:/a".to_string(), AgentStatus::Running);
            statuses.insert("local:/b".to_string(), AgentStatus::Error);
        }

        rm.refresh_status_counts();
        assert_eq!(rm.status_counts.running, 1);
        assert_eq!(rm.status_counts.error, 1);
        assert_eq!(rm.status_counts.total(), 2);
    }

    #[test]
    fn test_current_runtime_matches_session_no_runtime() {
        let bus = Arc::new(EventBus::new(8));
        let modal = Arc::new(AtomicBool::new(false));
        let focused = Arc::new(AtomicBool::new(true));
        let last_input = Arc::new(Mutex::new(std::time::Instant::now()));
        let rm = RuntimeManager::new(bus, modal, focused, last_input);

        assert!(!rm.current_runtime_matches_session(std::path::Path::new("/test")));
    }
}
