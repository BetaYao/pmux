// status_detector.rs - Agent status detection from terminal output
use crate::agent_status::AgentStatus;
use crate::shell_integration::{ShellPhase, ShellPhaseInfo};
use regex::Regex;

/// Detects agent status from terminal content
#[derive(Clone)]
pub struct StatusDetector {
    /// Keywords that indicate Running status
    running_patterns: Vec<Regex>,
    /// Keywords that indicate Waiting status
    waiting_patterns: Vec<Regex>,
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

    /// Add custom error pattern
    pub fn add_error_pattern(mut self, pattern: &str) -> Result<Self, regex::Error> {
        self.error_patterns.push(Regex::new(pattern)?);
        Ok(self)
    }

    /// Detect status from content
    /// Priority: Error > Waiting > Running > Idle > Unknown
    /// Uses text-based detection only (no OSC 133).
    pub fn detect(&self, content: &str) -> AgentStatus {
        self.detect_with_shell_phase(content, None)
    }

    /// Detect status from content, with optional OSC 133 shell phase info.
    /// When shell_phase is available: Running → Running, Output+exit!=0 → Error.
    /// Otherwise falls back to text-based detection (Task 4.4).
    /// Priority: Error > Waiting > Running > Idle > Unknown
    pub fn detect_with_shell_phase(
        &self,
        content: &str,
        shell_info: Option<ShellPhaseInfo>,
    ) -> AgentStatus {
        // Task 4.2: PreExec (Running phase) → AgentStatus::Running
        if let Some(info) = shell_info {
            if info.phase == ShellPhase::Running {
                return AgentStatus::Running;
            }
            // Task 4.3: PostExec with error → AgentStatus::Error
            if info.phase == ShellPhase::Output {
                if let Some(code) = info.last_post_exec_exit_code {
                    if code != 0 {
                        return AgentStatus::Error;
                    }
                }
            }
            // Task 4.4: If phase is Unknown, fall through to text detection
            // For Prompt, Input, Output (success): use text detection for Waiting/Running/Idle
        }

        // Task 4.4: Fallback to text-based detection
        let processed = self.preprocess(content);

        // Check in priority order
        if self.matches_error(&processed) {
            return AgentStatus::Error;
        }

        if self.matches_waiting(&processed) {
            return AgentStatus::Waiting;
        }

        if self.matches_running(&processed) {
            return AgentStatus::Running;
        }

        // If content is not empty but no patterns match, it's Idle
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

    /// Check if content matches any error pattern
    fn matches_error(&self, content: &str) -> bool {
        self.error_patterns.iter().any(|re| re.is_match(content))
    }

    /// Get detection confidence score (0.0 - 1.0)
    /// Higher score means more confident in the detection
    pub fn confidence(&self, content: &str) -> f32 {
        let processed = self.preprocess(content);
        
        // Count matching patterns
        let error_matches = self.error_patterns.iter()
            .filter(|re| re.is_match(&processed))
            .count();
        let waiting_matches = self.waiting_patterns.iter()
            .filter(|re| re.is_match(&processed))
            .count();
        let running_matches = self.running_patterns.iter()
            .filter(|re| re.is_match(&processed))
            .count();
        
        let total_checks = self.error_patterns.len() + 
                          self.waiting_patterns.len() + 
                          self.running_patterns.len();
        
        let max_matches = error_matches.max(waiting_matches).max(running_matches);
        
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

    /// Update with new content, returns true if status changed
    pub fn update(&mut self, content: &str) -> bool {
        let detected = self.detector.detect(content);
        self.update_with_status(detected)
    }

    /// Update with a pre-detected status, returns true if status changed.
    /// Used by StatusPublisher when status is already detected via shell phase.
    pub fn update_with_status(&mut self, detected: AgentStatus) -> bool {
        // Error status always updates immediately
        if detected == AgentStatus::Error {
            if self.current_status != AgentStatus::Error {
                self.current_status = AgentStatus::Error;
                self.pending_status = None;
                self.pending_count = 0;
                return true;
            }
            return false;
        }

        // Check if this matches pending status
        if Some(detected) == self.pending_status {
            self.pending_count += 1;

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
    use crate::shell_integration::{MarkerKind, ParsedMarker, ShellMarker, ShellPhase, ShellPhaseInfo};
    use crate::terminal::TerminalEngine;

    // --- Phase 4: OSC 133 shell phase integration tests ---

    #[test]
    fn test_detect_with_shell_phase_running() {
        let detector = StatusDetector::new();
        let info = ShellPhaseInfo {
            phase: ShellPhase::Running,
            last_post_exec_exit_code: None,
        };
        let status = detector.detect_with_shell_phase("$ ls -la\nsome output", Some(info));
        assert_eq!(status, AgentStatus::Running);
    }

    #[test]
    fn test_detect_with_shell_phase_output_error() {
        let detector = StatusDetector::new();
        let info = ShellPhaseInfo {
            phase: ShellPhase::Output,
            last_post_exec_exit_code: Some(1),
        };
        let status = detector.detect_with_shell_phase("command output", Some(info));
        assert_eq!(status, AgentStatus::Error);
    }

    #[test]
    fn test_detect_with_shell_phase_output_success() {
        let detector = StatusDetector::new();
        let info = ShellPhaseInfo {
            phase: ShellPhase::Output,
            last_post_exec_exit_code: Some(0),
        };
        let status = detector.detect_with_shell_phase("$ echo done\ndone", Some(info));
        assert_eq!(status, AgentStatus::Idle);
    }

    #[test]
    fn test_detect_with_shell_phase_unknown_fallback() {
        let detector = StatusDetector::new();
        let info = ShellPhaseInfo {
            phase: ShellPhase::Unknown,
            last_post_exec_exit_code: None,
        };
        assert_eq!(
            detector.detect_with_shell_phase("AI is thinking", Some(info)),
            AgentStatus::Running
        );
        assert_eq!(
            detector.detect_with_shell_phase("? What next?", Some(info)),
            AgentStatus::Waiting
        );
    }

    #[test]
    fn test_detect_with_shell_phase_none_fallback() {
        let detector = StatusDetector::new();
        assert_eq!(
            detector.detect_with_shell_phase("AI is thinking", None),
            detector.detect("AI is thinking")
        );
    }

    #[test]
    fn test_detect_with_shell_phase_priority_running_over_text() {
        let detector = StatusDetector::new();
        let info = ShellPhaseInfo {
            phase: ShellPhase::Running,
            last_post_exec_exit_code: None,
        };
        let status = detector.detect_with_shell_phase("error in log", Some(info));
        assert_eq!(status, AgentStatus::Running);
    }

    #[test]
    fn test_detect_with_shell_phase_output_exit_2() {
        let detector = StatusDetector::new();
        let info = ShellPhaseInfo {
            phase: ShellPhase::Output,
            last_post_exec_exit_code: Some(2),
        };
        assert_eq!(
            detector.detect_with_shell_phase("any content", Some(info)),
            AgentStatus::Error
        );
    }

    #[test]
    fn test_integration_with_terminal_engine() {
        let (tx, rx) = flume::unbounded();
        let engine = TerminalEngine::new(80, 24, rx);
        drop(tx);

        let detector = StatusDetector::new();

        let info = ShellPhaseInfo {
            phase: engine.shell_phase(),
            last_post_exec_exit_code: engine.last_post_exec_exit_code(),
        };
        assert_eq!(info.phase, ShellPhase::Unknown);
        assert_eq!(
            detector.detect_with_shell_phase("hello", Some(info)),
            AgentStatus::Idle
        );

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
        assert_eq!(
            detector.detect_with_shell_phase("any content", Some(info)),
            AgentStatus::Running
        );

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
        assert_eq!(
            detector.detect_with_shell_phase("output", Some(info)),
            AgentStatus::Error
        );
    }

    #[test]
    fn test_detector_creation() {
        let detector = StatusDetector::new();
        let _ = detector.detect("test content");
    }

    #[test]
    fn test_detect_running() {
        let detector = StatusDetector::new();
        
        assert_eq!(detector.detect("AI is thinking about your request"), AgentStatus::Running);
        assert_eq!(detector.detect("Writing code..."), AgentStatus::Running);
        assert_eq!(detector.detect("Running tool: grep"), AgentStatus::Running);
        assert_eq!(detector.detect("Loading data from API"), AgentStatus::Running);
    }

    #[test]
    fn test_detect_waiting() {
        let detector = StatusDetector::new();
        
        assert_eq!(detector.detect("? What would you like to do?"), AgentStatus::Waiting);
        assert_eq!(detector.detect("> Enter your choice:"), AgentStatus::Waiting);
        assert_eq!(detector.detect("Human: please review"), AgentStatus::Waiting);
        assert_eq!(detector.detect("Press enter to continue"), AgentStatus::Waiting);
    }

    #[test]
    fn test_detect_error() {
        let detector = StatusDetector::new();
        
        assert_eq!(detector.detect("Error: file not found"), AgentStatus::Error);
        assert_eq!(detector.detect("Traceback (most recent call):"), AgentStatus::Error);
        assert_eq!(detector.detect("Command failed with exit code 1"), AgentStatus::Error);
        assert_eq!(detector.detect("Panic: runtime error"), AgentStatus::Error);
    }

    #[test]
    fn test_detect_idle() {
        let detector = StatusDetector::new();
        
        // Content that doesn't match any pattern
        assert_eq!(detector.detect("Just some regular text"), AgentStatus::Idle);
        assert_eq!(detector.detect("Hello world"), AgentStatus::Idle);
        assert_eq!(detector.detect("$ ls -la"), AgentStatus::Idle);
    }

    #[test]
    fn test_detect_unknown() {
        let detector = StatusDetector::new();
        
        // Empty or whitespace-only content
        assert_eq!(detector.detect(""), AgentStatus::Unknown);
        assert_eq!(detector.detect("   "), AgentStatus::Unknown);
        assert_eq!(detector.detect("\n\n\n"), AgentStatus::Unknown);
    }

    #[test]
    fn test_priority_ordering() {
        let detector = StatusDetector::new();
        
        // Error has highest priority - "error" in text should trigger Error status
        let content = "An error occurred while processing";
        assert_eq!(detector.detect(content), AgentStatus::Error);
        
        // Waiting > Running - "awaiting input" should trigger Waiting even if other words suggest Running
        let content = "awaiting input from user";
        assert_eq!(detector.detect(content), AgentStatus::Waiting);
    }

    #[test]
    fn test_ansi_removal() {
        let detector = StatusDetector::new();
        
        let with_ansi = "\x1b[32mAI is\x1b[0m \x1b[1mthinking\x1b[0m";
        assert_eq!(detector.detect(with_ansi), AgentStatus::Running);
    }

    #[test]
    fn test_line_limit() {
        let detector = StatusDetector::with_line_count(StatusDetector::new(), 2);
        
        // Only checks last 2 lines
        let content = "Old content without keywords\nNew content\nAI is thinking";
        assert_eq!(detector.detect(content), AgentStatus::Running);
    }

    #[test]
    fn test_custom_patterns() {
        let detector = StatusDetector::new()
            .add_running_pattern(r"custom_running").unwrap();
        
        assert_eq!(detector.detect("custom_running now"), AgentStatus::Running);
    }

    #[test]
    fn test_confidence() {
        let detector = StatusDetector::new();
        
        let conf = detector.confidence("AI is thinking and writing code");
        assert!(conf > 0.0 && conf <= 1.0);
        
        // No matches
        let conf = detector.confidence("random text without keywords");
        assert_eq!(conf, 0.5);
    }

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
        let changed = tracker.update("AI is thinking");
        assert!(!changed);
        assert_eq!(tracker.current_status(), AgentStatus::Unknown);
        assert_eq!(tracker.pending_status(), Some(AgentStatus::Running));
        
        // Second call with same status commits
        let changed = tracker.update("AI is still thinking");
        assert!(changed);
        assert_eq!(tracker.current_status(), AgentStatus::Running);
        assert_eq!(tracker.pending_status(), None);
    }

    #[test]
    fn test_error_bypasses_debounce() {
        let mut tracker = DebouncedStatusTracker::with_debounce(2);
        
        // Set to running first (need 2 calls with same status)
        tracker.update("AI is thinking");
        let changed = tracker.update("AI is thinking");
        assert!(changed); // Status changed to Running
        assert_eq!(tracker.current_status(), AgentStatus::Running);
        
        // Error should immediately change (bypasses debounce)
        let changed = tracker.update("Error occurred!");
        assert!(changed);
        assert_eq!(tracker.current_status(), AgentStatus::Error);
    }

    #[test]
    fn test_different_status_resets_debounce() {
        let mut tracker = DebouncedStatusTracker::with_debounce(2);
        
        // Start with running
        tracker.update("AI is thinking");
        
        // Different status resets counter
        tracker.update("? What next?");
        assert_eq!(tracker.pending_status(), Some(AgentStatus::Waiting));
        assert_eq!(tracker.pending_count, 1);
        
        // Need another waiting to commit
        tracker.update("? Still waiting");
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
        
        tracker.update("AI is thinking");
        tracker.update("AI is thinking");
        assert_eq!(tracker.current_status(), AgentStatus::Running);
        
        tracker.reset();
        assert_eq!(tracker.current_status(), AgentStatus::Unknown);
        assert_eq!(tracker.pending_status(), None);
    }
}
