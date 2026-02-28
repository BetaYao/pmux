// status_detector.rs - Agent status detection from terminal output
// Priority: Process lifecycle > OSC 133 markers > Text patterns (fallback)
use crate::agent_status::AgentStatus;
use crate::shell_integration::{ShellPhase, ShellPhaseInfo};
use regex::Regex;

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

/// Detects agent status from terminal content
#[derive(Clone)]
pub struct StatusDetector {
    /// Keywords that indicate Running status
    running_patterns: Vec<Regex>,
    /// Keywords that indicate Waiting status
    waiting_patterns: Vec<Regex>,
    /// Keywords that indicate permission/approval request (WaitingConfirm)
    confirm_patterns: Vec<Regex>,
    /// Keywords that indicate Error status
    error_patterns: Vec<Regex>,
    /// Number of lines to check from the end
    check_line_count: usize,
}

impl Default for StatusDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl StatusDetector {
    /// Create a new StatusDetector with default patterns
    pub fn new() -> Self {
        Self {
            running_patterns: vec![
                Regex::new(r"(?i)thinking|analyzing|processing").unwrap(),
                Regex::new(r"(?i)reasoning|streaming").unwrap(),
                Regex::new(r"(?i)writing|generating|creating").unwrap(),
                Regex::new(r"(?i)running tool|executing|performing").unwrap(),
                Regex::new(r"(?i)loading|downloading|uploading").unwrap(),
                Regex::new(r"(?i)in progress|working on|busy").unwrap(),
                Regex::new(r"(?i)esc to interrupt|^\s*>").unwrap(),
            ],
            waiting_patterns: vec![
                Regex::new(r"^\?\s").unwrap(),
                Regex::new(r"^>\s").unwrap(),
                Regex::new(r"(?i)human:|user:|awaiting input").unwrap(),
                Regex::new(r"(?i)press enter|hit enter|continue\\?").unwrap(),
                Regex::new(r"(?i)waiting for|ready for").unwrap(),
                Regex::new(r"(?i)your turn|input required").unwrap(),
            ],
            confirm_patterns: vec![
                Regex::new(r"(?i)(requires approval|needs approval|permission to|don't ask again)").unwrap(),
                Regex::new(r"(?i)(Accept|Reject|Allow|Deny)\s+(all|this)").unwrap(),
                Regex::new(r"(?i)Always allow|Always deny").unwrap(),
                Regex::new(r"(?i)This command requires").unwrap(),
                Regex::new(r"(?i)approval required|approve\s").unwrap(),
                Regex::new(r"(?i)Run without asking").unwrap(),
            ],
            error_patterns: vec![
                Regex::new(r"(?i)error|exception|failure|failed").unwrap(),
                Regex::new(r"(?i)panic|abort|crash").unwrap(),
                Regex::new(r"(?i)traceback|stack trace").unwrap(),
                Regex::new(r"(?i)syntax error|compile error").unwrap(),
                Regex::new(r"(?i)command not found|exit code [1-9]").unwrap(),
            ],
            check_line_count: 50,
        }
    }

    /// Create detector with custom line count
    pub fn with_line_count(mut self, count: usize) -> Self {
        self.check_line_count = count;
        self
    }

    /// Add custom running pattern
    pub fn add_running_pattern(mut self, pattern: &str) -> Result<Self, regex::Error> {
        self.running_patterns.push(Regex::new(pattern)?);
        Ok(self)
    }

    /// Add custom waiting pattern
    pub fn add_waiting_pattern(mut self, pattern: &str) -> Result<Self, regex::Error> {
        self.waiting_patterns.push(Regex::new(pattern)?);
        Ok(self)
    }

    /// Add custom confirm pattern (permission/approval request)
    pub fn add_confirm_pattern(mut self, pattern: &str) -> Result<Self, regex::Error> {
        self.confirm_patterns.push(Regex::new(pattern)?);
        Ok(self)
    }

    /// Add custom error pattern
    pub fn add_error_pattern(mut self, pattern: &str) -> Result<Self, regex::Error> {
        self.error_patterns.push(Regex::new(pattern)?);
        Ok(self)
    }

    /// Detect status with full context.
    /// Priority: Process lifecycle > OSC 133 markers > Text patterns (fallback)
    ///
    /// # Arguments
    /// * `process_status` - Primary status source from process lifecycle
    /// * `shell_info` - OSC 133 shell phase info (secondary source)
    /// * `content` - Terminal content for text-based fallback detection
    pub fn detect(
        &self,
        process_status: ProcessStatus,
        shell_info: Option<ShellPhaseInfo>,
        content: &str,
    ) -> AgentStatus {
        // Priority 1: Process lifecycle (highest priority)
        match process_status {
            ProcessStatus::Exited => return AgentStatus::Exited,
            ProcessStatus::Error => return AgentStatus::Error,
            ProcessStatus::Running => {
                // Process is running, check OSC 133 for more detail
            }
            ProcessStatus::Unknown => {
                // Fall through to OSC 133 / text detection
            }
        }

        // Priority 2: OSC 133 markers
        if let Some(info) = shell_info {
            match info.phase {
                ShellPhase::Running => return AgentStatus::Running,
                ShellPhase::Input | ShellPhase::Prompt => return AgentStatus::Waiting,
                ShellPhase::Output => {
                    if let Some(code) = info.last_post_exec_exit_code {
                        if code != 0 {
                            return AgentStatus::Error;
                        }
                    }
                    // Exit code 0 or unknown - fall through to text detection
                }
                ShellPhase::Unknown => {
                    // Fall through to text detection
                }
            }
        }

        // Priority 3: Text-based detection (fallback only)
        self.detect_from_text(content)
    }

    /// Detect status from text patterns only (fallback method).
    /// Priority: Confirm > Error > Waiting > Running > Idle > Unknown
    pub fn detect_from_text(&self, content: &str) -> AgentStatus {
        let processed = self.preprocess(content);

        if self.matches_confirm(&processed) {
            return AgentStatus::WaitingConfirm;
        }

        if self.matches_error(&processed) {
            return AgentStatus::Error;
        }

        if self.matches_waiting(&processed) {
            return AgentStatus::Waiting;
        }

        if self.matches_running(&processed) {
            return AgentStatus::Running;
        }

        if !processed.trim().is_empty() {
            return AgentStatus::Idle;
        }

        AgentStatus::Unknown
    }

    /// Preprocess content: remove ANSI codes and get last N lines
    fn preprocess(&self, content: &str) -> String {
        // Remove ANSI escape sequences
        let ansi_regex = Regex::new(r"\x1b\[[0-9;]*m").unwrap();
        let without_ansi = ansi_regex.replace_all(content, "");

        // Get last N lines
        let lines: Vec<&str> = without_ansi.lines().collect();
        let start = lines.len().saturating_sub(self.check_line_count);
        lines[start..].join("\n")
    }

    /// Check if content matches any running pattern
    fn matches_running(&self, content: &str) -> bool {
        self.running_patterns.iter().any(|re| re.is_match(content))
    }

    /// Check if content matches any waiting pattern
    fn matches_waiting(&self, content: &str) -> bool {
        self.waiting_patterns.iter().any(|re| re.is_match(content))
    }

    /// Check if content matches any confirm pattern (permission/approval)
    fn matches_confirm(&self, content: &str) -> bool {
        self.confirm_patterns.iter().any(|re| re.is_match(content))
    }

    /// Check if content matches any error pattern
    fn matches_error(&self, content: &str) -> bool {
        self.error_patterns.iter().any(|re| re.is_match(content))
    }

    /// Get detection confidence score (0.0 - 1.0)
    /// Higher score means more confident in the detection
    pub fn confidence(&self, content: &str) -> f32 {
        let processed = self.preprocess(content);

        // Count matching patterns
        let error_matches = self
            .error_patterns
            .iter()
            .filter(|re| re.is_match(&processed))
            .count();
        let waiting_matches = self
            .waiting_patterns
            .iter()
            .filter(|re| re.is_match(&processed))
            .count();
        let confirm_matches = self
            .confirm_patterns
            .iter()
            .filter(|re| re.is_match(&processed))
            .count();
        let running_matches = self
            .running_patterns
            .iter()
            .filter(|re| re.is_match(&processed))
            .count();

        let total_checks = self.error_patterns.len()
            + self.waiting_patterns.len()
            + self.confirm_patterns.len()
            + self.running_patterns.len();

        let max_matches = error_matches
            .max(waiting_matches)
            .max(confirm_matches)
            .max(running_matches);

        if max_matches == 0 {
            return 0.5; // Medium confidence for Idle/Unknown
        }

        (max_matches as f32 / total_checks as f32).min(1.0)
    }
}

/// Tracks status changes with debouncing
pub struct DebouncedStatusTracker {
    detector: StatusDetector,
    current_status: AgentStatus,
    pending_status: Option<AgentStatus>,
    pending_count: u8,
    debounce_threshold: u8,
}

impl DebouncedStatusTracker {
    /// Create new tracker with default debounce (2 confirmations)
    pub fn new() -> Self {
        Self {
            detector: StatusDetector::new(),
            current_status: AgentStatus::Unknown,
            pending_status: None,
            pending_count: 0,
            debounce_threshold: 2,
        }
    }

    /// Create tracker with custom debounce threshold
    pub fn with_debounce(threshold: u8) -> Self {
        Self {
            detector: StatusDetector::new(),
            current_status: AgentStatus::Unknown,
            pending_status: None,
            pending_count: 0,
            debounce_threshold: threshold,
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

    /// Update with text content only (uses ProcessStatus::Unknown).
    /// Returns true if status changed.
    pub fn update_from_text(&mut self, content: &str) -> bool {
        let detected = self.detector.detect(ProcessStatus::Unknown, None, content);
        self.update_with_status(detected)
    }

    /// Update with a pre-detected status, returns true if status changed.
    /// Used by StatusPublisher when status is already detected via shell phase.
    pub fn update_with_status(&mut self, detected: AgentStatus) -> bool {
        // Error, Exited, and WaitingConfirm (urgent) always update immediately
        if detected == AgentStatus::Error
            || detected == AgentStatus::Exited
            || detected == AgentStatus::WaitingConfirm
        {
            if self.current_status != detected {
                self.current_status = detected;
                self.pending_status = None;
                self.pending_count = 0;
                return true;
            }
            return false;
        }

        // Check if this matches pending status
        if Some(detected) == self.pending_status {
            self.pending_count = self.pending_count.saturating_add(1);

            // If we've seen this enough times, commit the change
            if self.pending_count >= self.debounce_threshold {
                if self.current_status != detected {
                    self.current_status = detected;
                    self.pending_status = None;
                    self.pending_count = 0;
                    return true;
                }
            }
        } else {
            // New pending status
            self.pending_status = Some(detected);
            self.pending_count = 1;
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
    use crate::shell_integration::{
        MarkerKind, ParsedMarker, ShellMarker, ShellPhase, ShellPhaseInfo,
    };
    use crate::terminal::TerminalEngine;

    // --- Process status priority tests ---

    #[test]
    fn test_process_exited_overrides_osc133() {
        let detector = StatusDetector::new();
        let info = ShellPhaseInfo {
            phase: ShellPhase::Running,
            last_post_exec_exit_code: None,
        };
        // Process exited should override OSC 133 Running
        let status = detector.detect(ProcessStatus::Exited, Some(info), "any content");
        assert_eq!(status, AgentStatus::Exited);
    }

    #[test]
    fn test_process_error_overrides_osc133() {
        let detector = StatusDetector::new();
        let info = ShellPhaseInfo {
            phase: ShellPhase::Running,
            last_post_exec_exit_code: None,
        };
        // Process error should override OSC 133 Running
        let status = detector.detect(ProcessStatus::Error, Some(info), "any content");
        assert_eq!(status, AgentStatus::Error);
    }

    #[test]
    fn test_process_running_with_osc133() {
        let detector = StatusDetector::new();
        let info = ShellPhaseInfo {
            phase: ShellPhase::Running,
            last_post_exec_exit_code: None,
        };
        // Process running + OSC 133 Running = Running
        let status = detector.detect(ProcessStatus::Running, Some(info), "any content");
        assert_eq!(status, AgentStatus::Running);
    }

    #[test]
    fn test_process_running_with_osc133_waiting() {
        let detector = StatusDetector::new();
        let info = ShellPhaseInfo {
            phase: ShellPhase::Input,
            last_post_exec_exit_code: None,
        };
        // Process running + OSC 133 Input = Waiting
        let status = detector.detect(ProcessStatus::Running, Some(info), "any content");
        assert_eq!(status, AgentStatus::Waiting);
    }

    // --- OSC 133 marker tests ---

    #[test]
    fn test_osc133_running() {
        let detector = StatusDetector::new();
        let info = ShellPhaseInfo {
            phase: ShellPhase::Running,
            last_post_exec_exit_code: None,
        };
        let status = detector.detect(ProcessStatus::Running, Some(info), "any content");
        assert_eq!(status, AgentStatus::Running);
    }

    #[test]
    fn test_osc133_input_waiting() {
        let detector = StatusDetector::new();
        let info = ShellPhaseInfo {
            phase: ShellPhase::Input,
            last_post_exec_exit_code: None,
        };
        let status = detector.detect(ProcessStatus::Running, Some(info), "any content");
        assert_eq!(status, AgentStatus::Waiting);
    }

    #[test]
    fn test_osc133_prompt_waiting() {
        let detector = StatusDetector::new();
        let info = ShellPhaseInfo {
            phase: ShellPhase::Prompt,
            last_post_exec_exit_code: None,
        };
        let status = detector.detect(ProcessStatus::Running, Some(info), "any content");
        assert_eq!(status, AgentStatus::Waiting);
    }

    #[test]
    fn test_osc133_output_error() {
        let detector = StatusDetector::new();
        let info = ShellPhaseInfo {
            phase: ShellPhase::Output,
            last_post_exec_exit_code: Some(1),
        };
        let status = detector.detect(ProcessStatus::Running, Some(info), "any content");
        assert_eq!(status, AgentStatus::Error);
    }

    #[test]
    fn test_osc133_output_success_fallback() {
        let detector = StatusDetector::new();
        let info = ShellPhaseInfo {
            phase: ShellPhase::Output,
            last_post_exec_exit_code: Some(0),
        };
        // No text patterns match, should be Idle
        let status = detector.detect(ProcessStatus::Running, Some(info), "$ echo done");
        assert_eq!(status, AgentStatus::Idle);
    }

    // --- Text fallback tests ---

    #[test]
    fn test_text_fallback_running() {
        let detector = StatusDetector::new();
        // Process unknown, no OSC 133 -> text detection
        let status = detector.detect(ProcessStatus::Unknown, None, "AI is thinking");
        assert_eq!(status, AgentStatus::Running);
    }

    #[test]
    fn test_text_fallback_waiting() {
        let detector = StatusDetector::new();
        let status = detector.detect(ProcessStatus::Unknown, None, "? What next?");
        assert_eq!(status, AgentStatus::Waiting);
    }

    #[test]
    fn test_text_fallback_error() {
        let detector = StatusDetector::new();
        let status = detector.detect(ProcessStatus::Unknown, None, "Error: file not found");
        assert_eq!(status, AgentStatus::Error);
    }

    #[test]
    fn test_text_fallback_idle() {
        let detector = StatusDetector::new();
        let status = detector.detect(ProcessStatus::Unknown, None, "Just some regular text");
        assert_eq!(status, AgentStatus::Idle);
    }

    #[test]
    fn test_text_fallback_unknown() {
        let detector = StatusDetector::new();
        let status = detector.detect(ProcessStatus::Unknown, None, "");
        assert_eq!(status, AgentStatus::Unknown);
    }

    #[test]
    fn test_osc133_overrides_text() {
        let detector = StatusDetector::new();
        let info = ShellPhaseInfo {
            phase: ShellPhase::Running,
            last_post_exec_exit_code: None,
        };
        // OSC 133 Running should override text "error" pattern
        let status = detector.detect(ProcessStatus::Running, Some(info), "error in log");
        assert_eq!(status, AgentStatus::Running);
    }

    #[test]
    fn test_integration_with_terminal_engine() {
        let (tx, rx) = flume::unbounded();
        let engine = TerminalEngine::new(80, 24, rx);
        drop(tx);

        let detector = StatusDetector::new();

        // Initial state: Unknown
        let info = ShellPhaseInfo {
            phase: engine.shell_phase(),
            last_post_exec_exit_code: engine.last_post_exec_exit_code(),
        };
        assert_eq!(info.phase, ShellPhase::Unknown);
        let status = detector.detect(ProcessStatus::Running, Some(info), "hello");
        assert_eq!(status, AgentStatus::Idle);

        // After PreExec: Running
        {
            let mut state = engine.shell_state();
            let marker = ShellMarker::from_parsed(
                ParsedMarker {
                    kind: MarkerKind::PreExec,
                    exit_code: None,
                },
                0,
                0,
            );
            state.add_marker(marker);
        }
        let info = ShellPhaseInfo {
            phase: engine.shell_phase(),
            last_post_exec_exit_code: engine.last_post_exec_exit_code(),
        };
        assert_eq!(info.phase, ShellPhase::Running);
        let status = detector.detect(ProcessStatus::Running, Some(info), "any content");
        assert_eq!(status, AgentStatus::Running);

        // After PostExec with error: Error
        {
            let mut state = engine.shell_state();
            let marker = ShellMarker::from_parsed(
                ParsedMarker {
                    kind: MarkerKind::PostExec,
                    exit_code: Some(1),
                },
                1,
                0,
            );
            state.add_marker(marker);
        }
        let info = ShellPhaseInfo {
            phase: engine.shell_phase(),
            last_post_exec_exit_code: engine.last_post_exec_exit_code(),
        };
        assert_eq!(info.phase, ShellPhase::Output);
        assert_eq!(info.last_post_exec_exit_code, Some(1));
        let status = detector.detect(ProcessStatus::Running, Some(info), "output");
        assert_eq!(status, AgentStatus::Error);
    }

    #[test]
    fn test_detector_creation() {
        let detector = StatusDetector::new();
        let _ = detector.detect(ProcessStatus::Unknown, None, "test content");
    }

    #[test]
    fn test_detect_from_text_running() {
        let detector = StatusDetector::new();
        assert_eq!(
            detector.detect_from_text("AI is thinking about your request"),
            AgentStatus::Running
        );
        assert_eq!(
            detector.detect_from_text("Writing code..."),
            AgentStatus::Running
        );
        assert_eq!(
            detector.detect_from_text("Running tool: grep"),
            AgentStatus::Running
        );
        assert_eq!(
            detector.detect_from_text("Loading data from API"),
            AgentStatus::Running
        );
    }

    #[test]
    fn test_detect_from_text_confirm() {
        let detector = StatusDetector::new();
        assert_eq!(
            detector.detect_from_text("This command requires approval"),
            AgentStatus::WaitingConfirm
        );
        assert_eq!(
            detector.detect_from_text("Allow this command to run?"),
            AgentStatus::WaitingConfirm
        );
        assert_eq!(
            detector.detect_from_text("Always allow  Always deny"),
            AgentStatus::WaitingConfirm
        );
        assert_eq!(
            detector.detect_from_text("Permission to run bash command"),
            AgentStatus::WaitingConfirm
        );
    }

    #[test]
    fn test_detect_from_text_waiting() {
        let detector = StatusDetector::new();
        assert_eq!(
            detector.detect_from_text("? What would you like to do?"),
            AgentStatus::Waiting
        );
        assert_eq!(
            detector.detect_from_text("> Enter your choice:"),
            AgentStatus::Waiting
        );
        assert_eq!(
            detector.detect_from_text("Human: please review"),
            AgentStatus::Waiting
        );
        assert_eq!(
            detector.detect_from_text("Press enter to continue"),
            AgentStatus::Waiting
        );
    }

    #[test]
    fn test_detect_from_text_error() {
        let detector = StatusDetector::new();
        assert_eq!(
            detector.detect_from_text("Error: file not found"),
            AgentStatus::Error
        );
        assert_eq!(
            detector.detect_from_text("Traceback (most recent call):"),
            AgentStatus::Error
        );
        assert_eq!(
            detector.detect_from_text("Command failed with exit code 1"),
            AgentStatus::Error
        );
        assert_eq!(
            detector.detect_from_text("Panic: runtime error"),
            AgentStatus::Error
        );
    }

    #[test]
    fn test_detect_from_text_idle() {
        let detector = StatusDetector::new();
        assert_eq!(
            detector.detect_from_text("Just some regular text"),
            AgentStatus::Idle
        );
        assert_eq!(detector.detect_from_text("Hello world"), AgentStatus::Idle);
        assert_eq!(detector.detect_from_text("$ ls -la"), AgentStatus::Idle);
    }

    #[test]
    fn test_detect_from_text_unknown() {
        let detector = StatusDetector::new();
        assert_eq!(detector.detect_from_text(""), AgentStatus::Unknown);
        assert_eq!(detector.detect_from_text("   "), AgentStatus::Unknown);
        assert_eq!(detector.detect_from_text("\n\n\n"), AgentStatus::Unknown);
    }

    #[test]
    fn test_priority_ordering_text() {
        let detector = StatusDetector::new();

        // Confirm > Error (permission prompts take precedence)
        let content = "This command requires approval. Allow / Deny";
        assert_eq!(detector.detect_from_text(content), AgentStatus::WaitingConfirm);

        // Error when no confirm patterns
        let content = "An error occurred while processing";
        assert_eq!(detector.detect_from_text(content), AgentStatus::Error);

        // Waiting > Running
        let content = "awaiting input from user";
        assert_eq!(detector.detect_from_text(content), AgentStatus::Waiting);
    }

    #[test]
    fn test_ansi_removal() {
        let detector = StatusDetector::new();
        let with_ansi = "\x1b[32mAI is\x1b[0m \x1b[1mthinking\x1b[0m";
        assert_eq!(detector.detect_from_text(with_ansi), AgentStatus::Running);
    }

    #[test]
    fn test_line_limit() {
        let detector = StatusDetector::with_line_count(StatusDetector::new(), 2);
        let content = "Old content without keywords\nNew content\nAI is thinking";
        assert_eq!(detector.detect_from_text(content), AgentStatus::Running);
    }

    #[test]
    fn test_custom_patterns() {
        let detector = StatusDetector::new()
            .add_running_pattern(r"custom_running")
            .unwrap();
        assert_eq!(
            detector.detect_from_text("custom_running now"),
            AgentStatus::Running
        );
    }

    #[test]
    fn test_confidence() {
        let detector = StatusDetector::new();
        let conf = detector.confidence("AI is thinking and writing code");
        assert!(conf > 0.0 && conf <= 1.0);
        let conf = detector.confidence("random text without keywords");
        assert_eq!(conf, 0.5);
    }

    // --- DebouncedStatusTracker tests ---

    #[test]
    fn test_debounced_tracker_creation() {
        let tracker = DebouncedStatusTracker::new();
        assert_eq!(tracker.current_status(), AgentStatus::Unknown);
        assert_eq!(tracker.pending_status(), None);
    }

    #[test]
    fn test_debounce_requires_multiple_calls() {
        let mut tracker = DebouncedStatusTracker::with_debounce(2);

        // First call sets pending
        let changed = tracker.update(ProcessStatus::Unknown, None, "AI is thinking");
        assert!(!changed);
        assert_eq!(tracker.current_status(), AgentStatus::Unknown);
        assert_eq!(tracker.pending_status(), Some(AgentStatus::Running));

        // Second call with same status commits
        let changed = tracker.update(ProcessStatus::Unknown, None, "AI is still thinking");
        assert!(changed);
        assert_eq!(tracker.current_status(), AgentStatus::Running);
        assert_eq!(tracker.pending_status(), None);
    }

    #[test]
    fn test_error_bypasses_debounce() {
        let mut tracker = DebouncedStatusTracker::with_debounce(2);

        // Set to running first
        tracker.update(ProcessStatus::Unknown, None, "AI is thinking");
        let changed = tracker.update(ProcessStatus::Unknown, None, "AI is thinking");
        assert!(changed);
        assert_eq!(tracker.current_status(), AgentStatus::Running);

        // Error should immediately change (bypasses debounce)
        let changed = tracker.update(ProcessStatus::Unknown, None, "Error occurred!");
        assert!(changed);
        assert_eq!(tracker.current_status(), AgentStatus::Error);
    }

    #[test]
    fn test_exited_bypasses_debounce() {
        let mut tracker = DebouncedStatusTracker::with_debounce(2);

        // Set to running first
        tracker.update(ProcessStatus::Running, None, "AI is thinking");
        tracker.update(ProcessStatus::Running, None, "AI is thinking");
        assert_eq!(tracker.current_status(), AgentStatus::Running);

        // Exited should immediately change (bypasses debounce)
        let changed = tracker.update(ProcessStatus::Exited, None, "any content");
        assert!(changed);
        assert_eq!(tracker.current_status(), AgentStatus::Exited);
    }

    #[test]
    fn test_different_status_resets_debounce() {
        let mut tracker = DebouncedStatusTracker::with_debounce(2);

        // Start with running
        tracker.update(ProcessStatus::Unknown, None, "AI is thinking");

        // Different status resets counter
        tracker.update(ProcessStatus::Unknown, None, "? What next?");
        assert_eq!(tracker.pending_status(), Some(AgentStatus::Waiting));
        assert_eq!(tracker.pending_count, 1);

        // Need another waiting to commit
        tracker.update(ProcessStatus::Unknown, None, "? Still waiting");
        assert_eq!(tracker.current_status(), AgentStatus::Waiting);
    }

    #[test]
    fn test_force_status() {
        let mut tracker = DebouncedStatusTracker::new();
        tracker.force_status(AgentStatus::Running);
        assert_eq!(tracker.current_status(), AgentStatus::Running);
        assert_eq!(tracker.pending_status(), None);
    }

    #[test]
    fn test_tracker_reset() {
        let mut tracker = DebouncedStatusTracker::new();
        tracker.update(ProcessStatus::Unknown, None, "AI is thinking");
        tracker.update(ProcessStatus::Unknown, None, "AI is thinking");
        assert_eq!(tracker.current_status(), AgentStatus::Running);

        tracker.reset();
        assert_eq!(tracker.current_status(), AgentStatus::Unknown);
        assert_eq!(tracker.pending_status(), None);
    }
}
