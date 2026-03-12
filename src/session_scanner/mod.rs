//! session_scanner — Supplementary agent status detection from Claude Code JSONL session files.
//!
//! Watches Claude Code's project directories for JSONL file changes,
//! parses incremental content, and publishes structured status events
//! that supplement OSC 133 + text pattern detection.

pub mod binding;
pub mod file_watcher;
pub mod message_parser;
pub mod path_mapper;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;

use crate::agent_status::AgentStatus;
use crate::runtime::{EventAgentStateChange as AgentStateChange, EventBus, Notification, NotificationType, RuntimeEvent};
use binding::SessionBinding;
use file_watcher::JsonlFileWatcher;
use message_parser::{parse_jsonl_line, SessionEvent};

/// Coordinates JSONL file watching and event extraction for all panes.
///
/// Each pane can be associated with a worktree path. The scanner watches
/// the corresponding Claude Code project directory for JSONL changes and
/// publishes status events to the EventBus.
pub struct SessionScanner {
    binding: Arc<Mutex<SessionBinding>>,
    /// Active watchers keyed by pane_id. Each entry holds the watch thread's
    /// stop signal so we can shut it down when the pane is closed.
    active_watches: HashMap<String, WatchHandle>,
    event_bus: Arc<EventBus>,
}

/// Handle to a running file watch thread.
struct WatchHandle {
    stop_signal: Arc<std::sync::atomic::AtomicBool>,
    _thread: Option<thread::JoinHandle<()>>,
}

impl Drop for WatchHandle {
    fn drop(&mut self) {
        self.stop_signal
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

impl SessionScanner {
    pub fn new(event_bus: Arc<EventBus>) -> Self {
        Self {
            binding: Arc::new(Mutex::new(SessionBinding::new())),
            active_watches: HashMap::new(),
            event_bus,
        }
    }

    /// Start scanning for a pane's worktree.
    ///
    /// Called when a pane starts running an agent. Watches the corresponding
    /// Claude Code project directory for JSONL file changes.
    pub fn start_watching(&mut self, pane_id: &str, worktree_path: &Path) {
        // Stop any existing watch for this pane
        self.stop_watching(pane_id);

        let project_dir = path_mapper::worktree_to_claude_project_dir(worktree_path);
        if !project_dir.is_dir() {
            return; // Claude Code hasn't been used in this worktree
        }

        let stop_signal = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop_clone = stop_signal.clone();
        let pane_id_owned = pane_id.to_string();
        let binding = self.binding.clone();
        let event_bus = self.event_bus.clone();
        let project_dir_clone = project_dir.clone();

        let thread = thread::Builder::new()
            .name(format!("jsonl-watcher-{}", pane_id))
            .spawn(move || {
                watch_project_dir(
                    &pane_id_owned,
                    &project_dir_clone,
                    binding,
                    event_bus,
                    stop_clone,
                );
            });

        let handle = WatchHandle {
            stop_signal,
            _thread: thread.ok(),
        };
        self.active_watches.insert(pane_id.to_string(), handle);
    }

    /// Stop watching when pane is closed.
    pub fn stop_watching(&mut self, pane_id: &str) {
        // Dropping the WatchHandle sets the stop signal
        self.active_watches.remove(pane_id);
        if let Ok(mut b) = self.binding.lock() {
            b.unbind(pane_id);
        }
    }

    /// Get current binding for a pane.
    pub fn get_session_id(&self, pane_id: &str) -> Option<String> {
        self.binding
            .lock()
            .ok()
            .and_then(|b| b.get(pane_id).map(|s| s.session_id.clone()))
    }
}

/// Map a SessionEvent to an AgentStatus for status publishing.
fn event_to_status(event: &SessionEvent) -> AgentStatus {
    match event {
        SessionEvent::UserInput { .. } => AgentStatus::Running,
        SessionEvent::Thinking { .. } => AgentStatus::Running,
        SessionEvent::ToolUse { .. } => AgentStatus::Running,
        SessionEvent::ToolResult { is_error: true, .. } => AgentStatus::Error,
        SessionEvent::ToolResult { .. } => AgentStatus::Running,
        SessionEvent::TextOutput { .. } => AgentStatus::Running,
        SessionEvent::TurnEnd { .. } => AgentStatus::Idle,
    }
}

/// Background thread: watch a Claude project dir for JSONL changes.
///
/// Uses polling (500ms) instead of fsnotify to avoid the `notify` crate dependency.
/// JSONL files are append-only, so we just check file sizes periodically.
fn watch_project_dir(
    pane_id: &str,
    project_dir: &Path,
    binding: Arc<Mutex<SessionBinding>>,
    event_bus: Arc<EventBus>,
    stop_signal: Arc<std::sync::atomic::AtomicBool>,
) {
    let mut watcher = JsonlFileWatcher::new(project_dir.to_path_buf());

    // Skip existing content in all JSONL files (only care about new writes)
    if let Ok(entries) = std::fs::read_dir(project_dir) {
        for entry in entries.flatten() {
            if entry
                .path()
                .extension()
                .map(|e| e == "jsonl")
                .unwrap_or(false)
            {
                watcher.skip_existing(&entry.path());
            }
        }
    }

    let poll_interval = std::time::Duration::from_millis(500);

    while !stop_signal.load(std::sync::atomic::Ordering::Relaxed) {
        // Scan for JSONL files
        let jsonl_files: Vec<PathBuf> = std::fs::read_dir(project_dir)
            .into_iter()
            .flat_map(|entries| entries.flatten())
            .filter(|e| {
                e.path()
                    .extension()
                    .map(|ext| ext == "jsonl")
                    .unwrap_or(false)
            })
            .map(|e| e.path())
            .collect();

        for file_path in &jsonl_files {
            let new_lines = watcher.read_new_lines(file_path);
            if new_lines.is_empty() {
                continue;
            }

            let _file_session_id =
                JsonlFileWatcher::session_id_from_path(file_path).unwrap_or_default();

            for line in &new_lines {
                if let Some((session_id, event)) = parse_jsonl_line(line) {
                    // Update binding if session changed
                    if let Ok(mut b) = binding.lock() {
                        if b.session_changed(pane_id, &session_id) {
                            b.bind(
                                pane_id,
                                session_id.clone(),
                                file_path.clone(),
                                project_dir.to_path_buf(),
                            );
                        }
                    }

                    // Map event to status and publish
                    let status = event_to_status(&event);
                    let agent_id = pane_id
                        .split(':')
                        .next()
                        .unwrap_or(pane_id)
                        .to_string();

                    // Build a descriptive last_line from the event
                    let last_line = match &event {
                        SessionEvent::ToolUse { tool_name, .. } => {
                            Some(format!("Tool: {}", tool_name))
                        }
                        SessionEvent::Thinking { .. } => Some("Thinking...".to_string()),
                        SessionEvent::TurnEnd { .. } => Some("Turn complete".to_string()),
                        _ => None,
                    };

                    event_bus.publish(RuntimeEvent::AgentStateChange(AgentStateChange {
                        agent_id: agent_id.clone(),
                        pane_id: Some(pane_id.to_string()),
                        state: status,
                        prev_state: None,
                        last_line: last_line.clone(),
                    }));

                    // Publish notification for turn end
                    if matches!(event, SessionEvent::TurnEnd { .. }) {
                        event_bus.publish(RuntimeEvent::Notification(Notification {
                            agent_id,
                            pane_id: Some(pane_id.to_string()),
                            message: last_line.unwrap_or_else(|| "Idle".to_string()),
                            notif_type: NotificationType::Info,
                        }));
                    }
                }
            }
        }

        thread::sleep(poll_interval);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_to_status_mapping() {
        assert_eq!(
            event_to_status(&SessionEvent::UserInput {
                session_id: "s".into(),
                timestamp: "t".into()
            }),
            AgentStatus::Running
        );
        assert_eq!(
            event_to_status(&SessionEvent::Thinking {
                session_id: "s".into()
            }),
            AgentStatus::Running
        );
        assert_eq!(
            event_to_status(&SessionEvent::ToolUse {
                session_id: "s".into(),
                tool_name: "Bash".into(),
                tool_id: "t1".into()
            }),
            AgentStatus::Running
        );
        assert_eq!(
            event_to_status(&SessionEvent::ToolResult {
                session_id: "s".into(),
                tool_id: "t1".into(),
                is_error: false
            }),
            AgentStatus::Running
        );
        assert_eq!(
            event_to_status(&SessionEvent::ToolResult {
                session_id: "s".into(),
                tool_id: "t1".into(),
                is_error: true
            }),
            AgentStatus::Error
        );
        assert_eq!(
            event_to_status(&SessionEvent::TextOutput {
                session_id: "s".into()
            }),
            AgentStatus::Running
        );
        assert_eq!(
            event_to_status(&SessionEvent::TurnEnd {
                session_id: "s".into(),
                timestamp: "t".into()
            }),
            AgentStatus::Idle
        );
    }

    #[test]
    fn test_session_scanner_new() {
        let bus = Arc::new(EventBus::new(8));
        let scanner = SessionScanner::new(bus);
        assert!(scanner.get_session_id("pane-1").is_none());
    }

    #[test]
    fn test_start_watching_nonexistent_dir() {
        let bus = Arc::new(EventBus::new(8));
        let mut scanner = SessionScanner::new(bus);
        // Should silently skip — no project dir exists for this path
        scanner.start_watching("pane-1", Path::new("/nonexistent/worktree/path"));
        assert!(scanner.active_watches.is_empty());
    }

    #[test]
    fn test_stop_watching() {
        let bus = Arc::new(EventBus::new(8));
        let mut scanner = SessionScanner::new(bus);
        scanner.stop_watching("pane-1"); // Should not panic
    }

    #[test]
    fn test_full_flow_with_temp_dir() {
        use std::io::Write;

        let dir = tempfile::TempDir::new().unwrap();
        let project_dir = dir.path().to_path_buf();

        let bus = Arc::new(EventBus::new(32));
        let rx = bus.subscribe();

        // Create a JSONL file with initial content (should be skipped)
        let jsonl_path = project_dir.join("test-session.jsonl");
        {
            let mut f = std::fs::File::create(&jsonl_path).unwrap();
            writeln!(f, r#"{{"type":"user","sessionId":"test-session","message":{{"role":"user","content":[{{"type":"text","text":"hello"}}]}}}}"#).unwrap();
        }

        // Manually test the watcher logic (without spawning thread)
        let binding = Arc::new(Mutex::new(SessionBinding::new()));
        let mut watcher = JsonlFileWatcher::new(project_dir.clone());
        watcher.skip_existing(&jsonl_path);

        // Append new content
        {
            let mut f = std::fs::OpenOptions::new()
                .append(true)
                .open(&jsonl_path)
                .unwrap();
            writeln!(f, r#"{{"type":"assistant","sessionId":"test-session","message":{{"role":"assistant","content":[{{"type":"thinking","thinking":"analyzing"}}]}}}}"#).unwrap();
        }

        // Read new lines
        let lines = watcher.read_new_lines(&jsonl_path);
        assert_eq!(lines.len(), 1);

        // Parse and verify
        let (sid, event) = parse_jsonl_line(&lines[0]).unwrap();
        assert_eq!(sid, "test-session");
        assert!(matches!(event, SessionEvent::Thinking { .. }));
        assert_eq!(event_to_status(&event), AgentStatus::Running);
    }
}
