// status_detector.rs - Agent status detection from OSC 133 shell markers
//
// Pure OSC 133 detection: no text pattern matching.
// Status is determined by shell lifecycle events (C=PreExec, D=PostExec).
use crate::agent_status::AgentStatus;
use crate::shell_integration::{ShellPhase, ShellPhaseInfo};

/// Process lifecycle status from the runtime layer.
/// This is the primary source for Agent status determination.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProcessStatus {
    /// Process is running normally
    Running,
    /// Process exited with code 0
    Exited,
    /// Process exited with non-zero code (crash/error)
    Error,
    /// Process status unknown (not started or monitoring unavailable)
    #[default]
    Unknown,
}

/// Detects agent status from OSC 133 shell phase and process lifecycle.
#[derive(Clone, Default)]
pub struct StatusDetector;

impl StatusDetector {
    pub fn new() -> Self {
        Self
    }

    /// Detect agent status from process lifecycle + OSC 133 shell phase.
    ///
    /// Priority: ProcessStatus > ShellPhase (OSC 133) > Unknown
    ///
    /// # Arguments
    /// * `process_status` - Primary status source from process lifecycle
    /// * `shell_info` - OSC 133 shell phase info (secondary source)
    /// * `content` - Terminal content (reserved for future use, not used currently)
    pub fn detect(
        &self,
        process_status: ProcessStatus,
        shell_info: Option<ShellPhaseInfo>,
        _content: &str,
    ) -> AgentStatus {
        // Priority 1: Process lifecycle (highest priority)
        match process_status {
            ProcessStatus::Exited => return AgentStatus::Exited,
            ProcessStatus::Error => return AgentStatus::Error,
            ProcessStatus::Running | ProcessStatus::Unknown => {
                // Fall through to OSC 133
            }
        }

        // Priority 2: OSC 133 shell phase
        if let Some(info) = shell_info {
            return match info.phase {
                ShellPhase::Running => AgentStatus::Running,
                ShellPhase::Input | ShellPhase::Prompt => AgentStatus::Idle,
                ShellPhase::Output => {
                    match info.last_post_exec_exit_code {
                        Some(0) => AgentStatus::Idle,
                        Some(_) => AgentStatus::Error,
                        None => AgentStatus::Idle, // no exit code, assume success
                    }
                }
                ShellPhase::Unknown => AgentStatus::Unknown,
            };
        }

        // No OSC 133 info available
        AgentStatus::Unknown
    }
}

/// Tracks status changes.
///
/// With pure OSC 133 detection, signals are authoritative and commit immediately.
/// Unknown status is preserved (no data = keep current state).
pub struct DebouncedStatusTracker {
    detector: StatusDetector,
    current_status: AgentStatus,
    pending_status: Option<AgentStatus>,
    pending_count: u8,
}

impl DebouncedStatusTracker {
    pub fn new() -> Self {
        Self {
            detector: StatusDetector::new(),
            current_status: AgentStatus::Unknown,
            pending_status: None,
            pending_count: 0,
        }
    }

    /// Update with full context (process status + shell info + content).
    /// Returns true if status changed.
    pub fn update(
        &mut self,
        process_status: ProcessStatus,
        shell_info: Option<ShellPhaseInfo>,
        content: &str,
    ) -> bool {
        let detected = self.detector.detect(process_status, shell_info, content);
        self.update_with_status(detected)
    }

    /// Update with text content only (uses ProcessStatus::Unknown, no shell info).
    /// Returns true if status changed.
    pub fn update_from_text(&mut self, content: &str) -> bool {
        let detected = self.detector.detect(ProcessStatus::Unknown, None, content);
        self.update_with_status(detected)
    }

    /// Update with a pre-detected status, returns true if status changed.
    /// Used by StatusPublisher when status is already detected via shell phase.
    ///
    /// With pure OSC 133 detection, all signals are authoritative — no debounce needed.
    /// Debounce was originally designed for unreliable text pattern matching.
    pub fn update_with_status(&mut self, detected: AgentStatus) -> bool {
        // Unknown = no data available; preserve current state, don't trigger change
        if detected == AgentStatus::Unknown {
            return false;
        }

        // OSC 133 signals are authoritative — commit immediately
        if self.current_status != detected {
            self.current_status = detected;
            self.pending_status = None;
            self.pending_count = 0;
            return true;
        }

        false
    }

    /// Get current status
    pub fn current_status(&self) -> AgentStatus {
        self.current_status
    }

    /// Get pending status if any
    pub fn pending_status(&self) -> Option<AgentStatus> {
        self.pending_status
    }

    /// Force set status (bypass debounce)
    pub fn force_status(&mut self, status: AgentStatus) {
        self.current_status = status;
        self.pending_status = None;
        self.pending_count = 0;
    }

    /// Reset tracker
    pub fn reset(&mut self) {
        self.current_status = AgentStatus::Unknown;
        self.pending_status = None;
        self.pending_count = 0;
    }
}

impl Default for DebouncedStatusTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell_integration::{ShellPhase, ShellPhaseInfo};

    // =====================================================================
    // ProcessStatus priority tests
    // =====================================================================

    #[test]
    fn test_process_exited() {
        let d = StatusDetector::new();
        assert_eq!(d.detect(ProcessStatus::Exited, None, ""), AgentStatus::Exited);
    }

    #[test]
    fn test_process_error() {
        let d = StatusDetector::new();
        assert_eq!(d.detect(ProcessStatus::Error, None, ""), AgentStatus::Error);
    }

    #[test]
    fn test_process_exited_overrides_osc133() {
        let d = StatusDetector::new();
        let info = ShellPhaseInfo { phase: ShellPhase::Running, last_post_exec_exit_code: None };
        assert_eq!(d.detect(ProcessStatus::Exited, Some(info), ""), AgentStatus::Exited);
    }

    #[test]
    fn test_process_error_overrides_osc133() {
        let d = StatusDetector::new();
        let info = ShellPhaseInfo { phase: ShellPhase::Running, last_post_exec_exit_code: None };
        assert_eq!(d.detect(ProcessStatus::Error, Some(info), ""), AgentStatus::Error);
    }

    // =====================================================================
    // OSC 133 shell phase tests
    // =====================================================================

    #[test]
    fn test_osc133_running() {
        let d = StatusDetector::new();
        let info = ShellPhaseInfo { phase: ShellPhase::Running, last_post_exec_exit_code: None };
        assert_eq!(d.detect(ProcessStatus::Running, Some(info), ""), AgentStatus::Running);
    }

    #[test]
    fn test_osc133_input_idle() {
        let d = StatusDetector::new();
        let info = ShellPhaseInfo { phase: ShellPhase::Input, last_post_exec_exit_code: None };
        assert_eq!(d.detect(ProcessStatus::Running, Some(info), ""), AgentStatus::Idle);
    }

    #[test]
    fn test_osc133_prompt_idle() {
        let d = StatusDetector::new();
        let info = ShellPhaseInfo { phase: ShellPhase::Prompt, last_post_exec_exit_code: None };
        assert_eq!(d.detect(ProcessStatus::Running, Some(info), ""), AgentStatus::Idle);
    }

    #[test]
    fn test_osc133_output_exit0_idle() {
        let d = StatusDetector::new();
        let info = ShellPhaseInfo { phase: ShellPhase::Output, last_post_exec_exit_code: Some(0) };
        assert_eq!(d.detect(ProcessStatus::Running, Some(info), ""), AgentStatus::Idle);
    }

    #[test]
    fn test_osc133_output_exit1_error() {
        let d = StatusDetector::new();
        let info = ShellPhaseInfo { phase: ShellPhase::Output, last_post_exec_exit_code: Some(1) };
        assert_eq!(d.detect(ProcessStatus::Running, Some(info), ""), AgentStatus::Error);
    }

    #[test]
    fn test_osc133_output_exit127_error() {
        let d = StatusDetector::new();
        let info = ShellPhaseInfo { phase: ShellPhase::Output, last_post_exec_exit_code: Some(127) };
        assert_eq!(d.detect(ProcessStatus::Running, Some(info), ""), AgentStatus::Error);
    }

    #[test]
    fn test_osc133_output_no_exit_code_idle() {
        let d = StatusDetector::new();
        let info = ShellPhaseInfo { phase: ShellPhase::Output, last_post_exec_exit_code: None };
        assert_eq!(d.detect(ProcessStatus::Running, Some(info), ""), AgentStatus::Idle);
    }

    #[test]
    fn test_osc133_unknown_phase() {
        let d = StatusDetector::new();
        let info = ShellPhaseInfo { phase: ShellPhase::Unknown, last_post_exec_exit_code: None };
        assert_eq!(d.detect(ProcessStatus::Running, Some(info), ""), AgentStatus::Unknown);
    }

    #[test]
    fn test_no_shell_info_unknown() {
        let d = StatusDetector::new();
        assert_eq!(d.detect(ProcessStatus::Unknown, None, ""), AgentStatus::Unknown);
    }

    // =====================================================================
    // DebouncedStatusTracker tests
    // =====================================================================

    #[test]
    fn test_debounce_creation() {
        let tracker = DebouncedStatusTracker::new();
        assert_eq!(tracker.current_status(), AgentStatus::Unknown);
    }

    #[test]
    fn test_immediate_commit() {
        let mut tracker = DebouncedStatusTracker::new();
        // OSC 133 signals commit immediately — no debounce
        assert!(tracker.update_with_status(AgentStatus::Running));
        assert_eq!(tracker.current_status(), AgentStatus::Running);
    }

    #[test]
    fn test_error_bypasses_debounce() {
        let mut tracker = DebouncedStatusTracker::new();
        assert!(tracker.update_with_status(AgentStatus::Error));
        assert_eq!(tracker.current_status(), AgentStatus::Error);
    }

    #[test]
    fn test_exited_bypasses_debounce() {
        let mut tracker = DebouncedStatusTracker::new();
        assert!(tracker.update_with_status(AgentStatus::Exited));
        assert_eq!(tracker.current_status(), AgentStatus::Exited);
    }

    #[test]
    fn test_unknown_preserves_current() {
        let mut tracker = DebouncedStatusTracker::new();
        tracker.update_with_status(AgentStatus::Running);
        assert_eq!(tracker.current_status(), AgentStatus::Running);

        // Unknown should NOT change status
        assert!(!tracker.update_with_status(AgentStatus::Unknown));
        assert_eq!(tracker.current_status(), AgentStatus::Running);
    }

    #[test]
    fn test_running_to_idle_immediate() {
        let mut tracker = DebouncedStatusTracker::new();
        tracker.update_with_status(AgentStatus::Running);
        assert_eq!(tracker.current_status(), AgentStatus::Running);

        // Running→Idle commits immediately (OSC 133 authoritative)
        assert!(tracker.update_with_status(AgentStatus::Idle));
        assert_eq!(tracker.current_status(), AgentStatus::Idle);
    }

    #[test]
    fn test_rapid_transitions() {
        let mut tracker = DebouncedStatusTracker::new();
        assert!(tracker.update_with_status(AgentStatus::Running));
        assert!(tracker.update_with_status(AgentStatus::Idle));
        assert!(tracker.update_with_status(AgentStatus::Running));
        assert_eq!(tracker.current_status(), AgentStatus::Running);
    }

    #[test]
    fn test_force_status() {
        let mut tracker = DebouncedStatusTracker::new();
        tracker.force_status(AgentStatus::Running);
        assert_eq!(tracker.current_status(), AgentStatus::Running);
    }

    #[test]
    fn test_tracker_reset() {
        let mut tracker = DebouncedStatusTracker::new();
        tracker.force_status(AgentStatus::Running);
        tracker.reset();
        assert_eq!(tracker.current_status(), AgentStatus::Unknown);
    }
}
