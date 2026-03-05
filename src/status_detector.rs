// status_detector.rs - Agent status detection from terminal output
// Priority: Process lifecycle > OSC 133 markers > Text patterns (fallback)
use crate::agent_status::AgentStatus;
use crate::shell_integration::{ShellPhase, ShellPhaseInfo};
use regex::Regex;
use std::sync::LazyLock;

static ANSI_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\x1b\[[0-9;]*m").unwrap());

static RUNNING_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| vec![
    Regex::new(r"(?i)thinking|analyzing|processing").unwrap(),
    Regex::new(r"(?i)reasoning|streaming").unwrap(),
    Regex::new(r"(?i)writing|generating|creating").unwrap(),
    Regex::new(r"(?i)running tool|executing|performing").unwrap(),
    Regex::new(r"(?i)loading|downloading|uploading").unwrap(),
    Regex::new(r"(?i)in progress|working on|busy").unwrap(),
    Regex::new(r"(?i)esc to (interrupt|cancel)").unwrap(),
    // claude-code spinner verbs (not covered by generic patterns above)
    Regex::new(r"(?i)(conjuring|pondering|ruminating|cogitating|clauding|noodling|percolating)\b").unwrap(),
]);

static WAITING_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| vec![
    Regex::new(r"^\?\s").unwrap(),
    Regex::new(r"^>\s").unwrap(),
    Regex::new(r"(?i)human:|user:|awaiting input").unwrap(),
    Regex::new(r"(?i)press enter|hit enter|continue\\?").unwrap(),
    Regex::new(r"(?i)waiting for|ready for").unwrap(),
    Regex::new(r"(?i)your turn|input required").unwrap(),
]);

static CONFIRM_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| vec![
    Regex::new(r"(?i)(requires approval|needs approval|permission to|don't ask again)").unwrap(),
    Regex::new(r"(?i)(Accept|Reject|Allow|Deny)\s+(all|this)").unwrap(),
    Regex::new(r"(?i)Always allow|Always deny").unwrap(),
    Regex::new(r"(?i)This command requires").unwrap(),
    Regex::new(r"(?i)approval required|approve\s").unwrap(),
    Regex::new(r"(?i)Run without asking").unwrap(),
    Regex::new(r"(?i)Yes, allow (once|always)").unwrap(),                  // gemini
    Regex::new(r"(?i)Approve (Once|This Session)|approve\s*\(y\)").unwrap(), // codex / cursor
]);

static ERROR_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| vec![
    Regex::new(r"(?i)error|exception|failure|failed").unwrap(),
    Regex::new(r"(?i)panic|abort|crash").unwrap(),
    Regex::new(r"(?i)traceback|stack trace").unwrap(),
    Regex::new(r"(?i)syntax error|compile error").unwrap(),
    Regex::new(r"(?i)command not found|exit code [1-9]").unwrap(),
]);

static IDLE_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| vec![
    Regex::new(r"^\s*[❯➜→\$%#>]\s*$").unwrap(),           // bare prompt chars (shell, claude-code, gemini)
    Regex::new(r"git:\([^)]+\)\s*[✗✓×]?\s*$").unwrap(),   // zsh git prompt: git:(main) ✗
    Regex::new(r"^\s*\S+@\S+[\s:~][^$%#]*[\$%#]\s*$").unwrap(), // user@host:~ $
    Regex::new(r"^\s*(ask|architect|help|multi)>\s*$").unwrap(), // aider mode prompts
]);

/// Context about the running process, passed to StatusDetector for fallback logic.
/// Instead of forcing Running when a non-shell process is detected, we pass this
/// context so text detection can still determine the specific status.
#[derive(Debug, Clone, Copy, Default)]
pub struct ProcessContext {
    /// Non-shell process is active (alt_screen or pane_current_command != shell)
    pub process_active: bool,
    /// Terminal is in alternate screen mode (TUI app like opencode, vim)
    pub alt_screen: bool,
}

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
    custom_running: Vec<Regex>,
    custom_waiting: Vec<Regex>,
    custom_confirm: Vec<Regex>,
    custom_error: Vec<Regex>,
    custom_idle: Vec<Regex>,
    check_line_count: usize,
}

impl Default for StatusDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl StatusDetector {
    pub fn new() -> Self {
        Self {
            custom_running: Vec::new(),
            custom_waiting: Vec::new(),
            custom_confirm: Vec::new(),
            custom_error: Vec::new(),
            custom_idle: Vec::new(),
            check_line_count: 15,
        }
    }

    pub fn with_line_count(mut self, count: usize) -> Self {
        self.check_line_count = count;
        self
    }

    pub fn add_running_pattern(mut self, pattern: &str) -> Result<Self, regex::Error> {
        self.custom_running.push(Regex::new(pattern)?);
        Ok(self)
    }

    pub fn add_waiting_pattern(mut self, pattern: &str) -> Result<Self, regex::Error> {
        self.custom_waiting.push(Regex::new(pattern)?);
        Ok(self)
    }

    pub fn add_confirm_pattern(mut self, pattern: &str) -> Result<Self, regex::Error> {
        self.custom_confirm.push(Regex::new(pattern)?);
        Ok(self)
    }

    pub fn add_error_pattern(mut self, pattern: &str) -> Result<Self, regex::Error> {
        self.custom_error.push(Regex::new(pattern)?);
        Ok(self)
    }

    pub fn add_idle_pattern(mut self, pattern: &str) -> Result<Self, regex::Error> {
        self.custom_idle.push(Regex::new(pattern)?);
        Ok(self)
    }

    /// Detect status with full context.
    /// Priority: Process lifecycle > OSC 133 markers > Text patterns + ProcessContext (fallback)
    ///
    /// # Arguments
    /// * `process_status` - Primary status source from process lifecycle
    /// * `shell_info` - OSC 133 shell phase info (secondary source)
    /// * `content` - Terminal content for text-based fallback detection
    /// * `process_ctx` - Context about the running process (for fallback logic)
    pub fn detect(
        &self,
        process_status: ProcessStatus,
        shell_info: Option<ShellPhaseInfo>,
        content: &str,
        process_ctx: ProcessContext,
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
                        return AgentStatus::Idle; // exit 0 → command completed successfully
                    }
                    // No exit code yet - fall through to text detection
                }
                ShellPhase::Unknown => {
                    // Fall through to text detection
                }
            }
        }

        // Priority 3: Text-based detection (fallback)
        // ProcessContext is passed for future use but does not currently alter results.
        // Agent status is determined purely by text patterns (thinking/waiting/prompt/etc.)
        // so that agents like claude-code correctly show Idle on startup.
        let _ = process_ctx;
        self.detect_from_text(content)
    }

    /// Detect status from text patterns only (fallback method).
    /// Priority: Prompt(Idle) > Confirm > Error > Waiting > Running > Idle > Unknown
    ///
    /// Prompt detection checks the last 3 lines for shell prompt patterns.
    /// If a prompt is at the bottom, the shell is idle regardless of older text.
    pub fn detect_from_text(&self, content: &str) -> AgentStatus {
        self.detect_from_text_detailed(content).0
    }

    /// Internal: returns (AgentStatus, prompt_matched).
    /// `prompt_matched` is true when Idle was determined via IDLE_PATTERNS (shell prompt),
    /// false when Idle is a fallback (non-empty content but no pattern matched).
    fn detect_from_text_detailed(&self, content: &str) -> (AgentStatus, bool) {
        let processed = self.preprocess(content);

        // Prompt in last 3 lines → force Idle (overrides stale Running text above)
        if self.last_lines_match_idle(&processed) {
            return (AgentStatus::Idle, true); // prompt-matched
        }

        if self.matches_confirm(&processed) {
            return (AgentStatus::WaitingConfirm, false);
        }

        if self.matches_error(&processed) {
            return (AgentStatus::Error, false);
        }

        if self.matches_waiting(&processed) {
            return (AgentStatus::Waiting, false);
        }

        if self.matches_running(&processed) {
            return (AgentStatus::Running, false);
        }

        if !processed.trim().is_empty() {
            return (AgentStatus::Idle, false); // fallback Idle
        }

        (AgentStatus::Unknown, false)
    }

    /// Check if any of the last 3 non-empty lines match idle (shell prompt) patterns.
    fn last_lines_match_idle(&self, content: &str) -> bool {
        content.lines().rev().take(3).any(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty()
                && IDLE_PATTERNS
                    .iter()
                    .chain(self.custom_idle.iter())
                    .any(|re| re.is_match(trimmed))
        })
    }

    fn preprocess(&self, content: &str) -> String {
        let without_ansi = ANSI_REGEX.replace_all(content, "");
        let lines: Vec<&str> = without_ansi.lines().collect();
        let start = lines.len().saturating_sub(self.check_line_count);
        lines[start..].join("\n")
    }

    fn matches_running(&self, content: &str) -> bool {
        RUNNING_PATTERNS.iter().chain(self.custom_running.iter()).any(|re| re.is_match(content))
    }

    fn matches_waiting(&self, content: &str) -> bool {
        WAITING_PATTERNS.iter().chain(self.custom_waiting.iter()).any(|re| re.is_match(content))
    }

    fn matches_confirm(&self, content: &str) -> bool {
        CONFIRM_PATTERNS.iter().chain(self.custom_confirm.iter()).any(|re| re.is_match(content))
    }

    fn matches_error(&self, content: &str) -> bool {
        ERROR_PATTERNS.iter().chain(self.custom_error.iter()).any(|re| re.is_match(content))
    }

    pub fn confidence(&self, content: &str) -> f32 {
        let processed = self.preprocess(content);

        let error_matches = ERROR_PATTERNS.iter().chain(self.custom_error.iter())
            .filter(|re| re.is_match(&processed)).count();
        let waiting_matches = WAITING_PATTERNS.iter().chain(self.custom_waiting.iter())
            .filter(|re| re.is_match(&processed)).count();
        let confirm_matches = CONFIRM_PATTERNS.iter().chain(self.custom_confirm.iter())
            .filter(|re| re.is_match(&processed)).count();
        let running_matches = RUNNING_PATTERNS.iter().chain(self.custom_running.iter())
            .filter(|re| re.is_match(&processed)).count();

        let total_checks = ERROR_PATTERNS.len() + self.custom_error.len()
            + WAITING_PATTERNS.len() + self.custom_waiting.len()
            + CONFIRM_PATTERNS.len() + self.custom_confirm.len()
            + RUNNING_PATTERNS.len() + self.custom_running.len();

        let max_matches = error_matches
            .max(waiting_matches)
            .max(confirm_matches)
            .max(running_matches);

        if max_matches == 0 {
            return 0.5;
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

    /// Update with full context (process status + shell info + content + process context).
    /// Returns true if status changed.
    pub fn update(
        &mut self,
        process_status: ProcessStatus,
        shell_info: Option<ShellPhaseInfo>,
        content: &str,
        process_ctx: ProcessContext,
    ) -> bool {
        let detected = self.detector.detect(process_status, shell_info, content, process_ctx);
        self.update_with_status(detected)
    }

    /// Update with text content only (uses ProcessStatus::Unknown, no process context).
    /// Returns true if status changed.
    pub fn update_from_text(&mut self, content: &str) -> bool {
        let detected = self.detector.detect(ProcessStatus::Unknown, None, content, ProcessContext::default());
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
    use crate::shell_integration::{ShellPhase, ShellPhaseInfo};

    // --- Process status priority tests ---

    #[test]
    fn test_process_exited_overrides_osc133() {
        let detector = StatusDetector::new();
        let info = ShellPhaseInfo {
            phase: ShellPhase::Running,
            last_post_exec_exit_code: None,
        };
        // Process exited should override OSC 133 Running
        let status = detector.detect(ProcessStatus::Exited, Some(info), "any content", ProcessContext::default());
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
        let status = detector.detect(ProcessStatus::Error, Some(info), "any content", ProcessContext::default());
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
        let status = detector.detect(ProcessStatus::Running, Some(info), "any content", ProcessContext::default());
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
        let status = detector.detect(ProcessStatus::Running, Some(info), "any content", ProcessContext::default());
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
        let status = detector.detect(ProcessStatus::Running, Some(info), "any content", ProcessContext::default());
        assert_eq!(status, AgentStatus::Running);
    }

    #[test]
    fn test_osc133_input_waiting() {
        let detector = StatusDetector::new();
        let info = ShellPhaseInfo {
            phase: ShellPhase::Input,
            last_post_exec_exit_code: None,
        };
        let status = detector.detect(ProcessStatus::Running, Some(info), "any content", ProcessContext::default());
        assert_eq!(status, AgentStatus::Waiting);
    }

    #[test]
    fn test_osc133_prompt_waiting() {
        let detector = StatusDetector::new();
        let info = ShellPhaseInfo {
            phase: ShellPhase::Prompt,
            last_post_exec_exit_code: None,
        };
        let status = detector.detect(ProcessStatus::Running, Some(info), "any content", ProcessContext::default());
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
        let status = detector.detect(ProcessStatus::Running, Some(info), "$ echo done", ProcessContext::default());
        assert_eq!(status, AgentStatus::Idle);
    }

    // --- Text fallback tests ---

    #[test]
    fn test_text_fallback_running() {
        let detector = StatusDetector::new();
        // Process unknown, no OSC 133 -> text detection
        let status = detector.detect(ProcessStatus::Unknown, None, "AI is thinking", ProcessContext::default());
        assert_eq!(status, AgentStatus::Running);
    }

    #[test]
    fn test_text_fallback_waiting() {
        let detector = StatusDetector::new();
        let status = detector.detect(ProcessStatus::Unknown, None, "? What next?", ProcessContext::default());
        assert_eq!(status, AgentStatus::Waiting);
    }

    #[test]
    fn test_text_fallback_error() {
        let detector = StatusDetector::new();
        let status = detector.detect(ProcessStatus::Unknown, None, "Error: file not found", ProcessContext::default());
        assert_eq!(status, AgentStatus::Error);
    }

    #[test]
    fn test_text_fallback_idle() {
        let detector = StatusDetector::new();
        let status = detector.detect(ProcessStatus::Unknown, None, "Just some regular text", ProcessContext::default());
        assert_eq!(status, AgentStatus::Idle);
    }

    #[test]
    fn test_text_fallback_unknown() {
        let detector = StatusDetector::new();
        let status = detector.detect(ProcessStatus::Unknown, None, "", ProcessContext::default());
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
        let status = detector.detect(ProcessStatus::Running, Some(info), "error in log", ProcessContext::default());
        assert_eq!(status, AgentStatus::Running);
    }

    /// StatusDetector with ShellPhaseInfo (from ContentExtractor) - no TerminalEngine.
    #[test]
    fn test_integration_with_shell_phase_info() {
        let detector = StatusDetector::new();

        // Unknown phase -> Idle
        let info = ShellPhaseInfo {
            phase: ShellPhase::Unknown,
            last_post_exec_exit_code: None,
        };
        assert_eq!(
            detector.detect(ProcessStatus::Running, Some(info), "hello", ProcessContext::default()),
            AgentStatus::Idle
        );

        // Running phase -> Running
        let info = ShellPhaseInfo {
            phase: ShellPhase::Running,
            last_post_exec_exit_code: None,
        };
        assert_eq!(
            detector.detect(ProcessStatus::Running, Some(info), "any content", ProcessContext::default()),
            AgentStatus::Running
        );

        // Output + exit 1 -> Error
        let info = ShellPhaseInfo {
            phase: ShellPhase::Output,
            last_post_exec_exit_code: Some(1),
        };
        assert_eq!(
            detector.detect(ProcessStatus::Running, Some(info), "output", ProcessContext::default()),
            AgentStatus::Error
        );
    }

    #[test]
    fn test_detector_creation() {
        let detector = StatusDetector::new();
        let _ = detector.detect(ProcessStatus::Unknown, None, "test content", ProcessContext::default());
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
        let changed = tracker.update(ProcessStatus::Unknown, None, "AI is thinking", ProcessContext::default());
        assert!(!changed);
        assert_eq!(tracker.current_status(), AgentStatus::Unknown);
        assert_eq!(tracker.pending_status(), Some(AgentStatus::Running));

        // Second call with same status commits
        let changed = tracker.update(ProcessStatus::Unknown, None, "AI is still thinking", ProcessContext::default());
        assert!(changed);
        assert_eq!(tracker.current_status(), AgentStatus::Running);
        assert_eq!(tracker.pending_status(), None);
    }

    #[test]
    fn test_error_bypasses_debounce() {
        let mut tracker = DebouncedStatusTracker::with_debounce(2);

        // Set to running first
        tracker.update(ProcessStatus::Unknown, None, "AI is thinking", ProcessContext::default());
        let changed = tracker.update(ProcessStatus::Unknown, None, "AI is thinking", ProcessContext::default());
        assert!(changed);
        assert_eq!(tracker.current_status(), AgentStatus::Running);

        // Error should immediately change (bypasses debounce)
        let changed = tracker.update(ProcessStatus::Unknown, None, "Error occurred!", ProcessContext::default());
        assert!(changed);
        assert_eq!(tracker.current_status(), AgentStatus::Error);
    }

    #[test]
    fn test_exited_bypasses_debounce() {
        let mut tracker = DebouncedStatusTracker::with_debounce(2);

        // Set to running first
        tracker.update(ProcessStatus::Running, None, "AI is thinking", ProcessContext::default());
        tracker.update(ProcessStatus::Running, None, "AI is thinking", ProcessContext::default());
        assert_eq!(tracker.current_status(), AgentStatus::Running);

        // Exited should immediately change (bypasses debounce)
        let changed = tracker.update(ProcessStatus::Exited, None, "any content", ProcessContext::default());
        assert!(changed);
        assert_eq!(tracker.current_status(), AgentStatus::Exited);
    }

    #[test]
    fn test_different_status_resets_debounce() {
        let mut tracker = DebouncedStatusTracker::with_debounce(2);

        // Start with running
        tracker.update(ProcessStatus::Unknown, None, "AI is thinking", ProcessContext::default());

        // Different status resets counter
        tracker.update(ProcessStatus::Unknown, None, "? What next?", ProcessContext::default());
        assert_eq!(tracker.pending_status(), Some(AgentStatus::Waiting));
        assert_eq!(tracker.pending_count, 1);

        // Need another waiting to commit
        tracker.update(ProcessStatus::Unknown, None, "? Still waiting", ProcessContext::default());
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
        tracker.update(ProcessStatus::Unknown, None, "AI is thinking", ProcessContext::default());
        tracker.update(ProcessStatus::Unknown, None, "AI is thinking", ProcessContext::default());
        assert_eq!(tracker.current_status(), AgentStatus::Running);

        tracker.reset();
        assert_eq!(tracker.current_status(), AgentStatus::Unknown);
        assert_eq!(tracker.pending_status(), None);
    }

    // --- Idle prompt detection tests ---

    #[test]
    fn test_prompt_overrides_stale_running_text() {
        let detector = StatusDetector::new();
        // Old "thinking" text above, but prompt at the bottom → Idle
        let content = "AI is thinking about your request\nProcessing files...\n❯ ";
        assert_eq!(detector.detect_from_text(content), AgentStatus::Idle);
    }

    #[test]
    fn test_prompt_dollar_sign_idle() {
        let detector = StatusDetector::new();
        assert_eq!(detector.detect_from_text("some output\n$ "), AgentStatus::Idle);
    }

    #[test]
    fn test_prompt_git_zsh_idle() {
        let detector = StatusDetector::new();
        let content = "Writing code...\ngit:(main) ✗";
        assert_eq!(detector.detect_from_text(content), AgentStatus::Idle);
    }

    #[test]
    fn test_prompt_user_at_host_idle() {
        let detector = StatusDetector::new();
        let content = "AI is thinking\nuser@host:~ $";
        assert_eq!(detector.detect_from_text(content), AgentStatus::Idle);
    }

    #[test]
    fn test_no_prompt_still_detects_running() {
        let detector = StatusDetector::new();
        // No prompt at bottom → normal Running detection
        let content = "AI is thinking about your request";
        assert_eq!(detector.detect_from_text(content), AgentStatus::Running);
    }

    #[test]
    fn test_custom_idle_pattern() {
        let detector = StatusDetector::new()
            .add_idle_pattern(r"^myshell>").unwrap();
        let content = "thinking about stuff\nmyshell>";
        assert_eq!(detector.detect_from_text(content), AgentStatus::Idle);
    }

    #[test]
    fn test_osc133_output_exit0_returns_idle() {
        let detector = StatusDetector::new();
        let info = ShellPhaseInfo {
            phase: ShellPhase::Output,
            last_post_exec_exit_code: Some(0),
        };
        // Even with "thinking" in content, OSC 133 exit 0 → Idle
        let status = detector.detect(ProcessStatus::Running, Some(info), "AI is thinking", ProcessContext::default());
        assert_eq!(status, AgentStatus::Idle);
    }

    #[test]
    fn test_osc133_output_no_exit_code_falls_through() {
        let detector = StatusDetector::new();
        let info = ShellPhaseInfo {
            phase: ShellPhase::Output,
            last_post_exec_exit_code: None,
        };
        // No exit code → falls through to text detection
        let status = detector.detect(ProcessStatus::Running, Some(info), "AI is thinking", ProcessContext::default());
        assert_eq!(status, AgentStatus::Running);
    }

    #[test]
    fn test_reduced_window_15_lines() {
        let detector = StatusDetector::new();
        // "thinking" is on line 1, followed by 15 blank lines → outside window
        let mut content = String::from("AI is thinking\n");
        for _ in 0..15 {
            content.push_str("normal output line\n");
        }
        // With 15-line window, "thinking" is no longer visible
        assert_eq!(detector.detect_from_text(&content), AgentStatus::Idle);
    }

    // --- ProcessContext tests ---

    #[test]
    fn test_process_active_tui_static_screen_idle() {
        let detector = StatusDetector::new();
        let ctx = ProcessContext { process_active: true, alt_screen: true };
        // TUI app with non-matching content (fallback Idle) → Idle
        let status = detector.detect(ProcessStatus::Unknown, None, "some TUI content", ctx);
        assert_eq!(status, AgentStatus::Idle);
    }

    #[test]
    fn test_process_active_tui_thinking_running() {
        let detector = StatusDetector::new();
        let ctx = ProcessContext { process_active: true, alt_screen: true };
        // TUI app with "thinking" text → Running
        let status = detector.detect(ProcessStatus::Unknown, None, "AI is thinking...", ctx);
        assert_eq!(status, AgentStatus::Running);
    }

    #[test]
    fn test_process_active_tui_confirm() {
        let detector = StatusDetector::new();
        let ctx = ProcessContext { process_active: true, alt_screen: true };
        let status = detector.detect(ProcessStatus::Unknown, None, "requires approval", ctx);
        assert_eq!(status, AgentStatus::WaitingConfirm);
    }

    #[test]
    fn test_process_active_cli_no_match_idle() {
        let detector = StatusDetector::new();
        let ctx = ProcessContext { process_active: true, alt_screen: false };
        // CLI tool with non-matching content → Idle (text detection only, process_active doesn't force Running)
        let status = detector.detect(ProcessStatus::Unknown, None, "Compiling foo v1.0", ctx);
        assert_eq!(status, AgentStatus::Idle);
    }

    #[test]
    fn test_process_active_cli_prompt_idle() {
        let detector = StatusDetector::new();
        let ctx = ProcessContext { process_active: true, alt_screen: false };
        // CLI tool showing prompt → Idle (prompt-matched)
        let status = detector.detect(ProcessStatus::Unknown, None, "> ", ctx);
        assert_eq!(status, AgentStatus::Idle);
    }

    #[test]
    fn test_process_active_cli_thinking_running() {
        let detector = StatusDetector::new();
        let ctx = ProcessContext { process_active: true, alt_screen: false };
        let status = detector.detect(ProcessStatus::Unknown, None, "thinking about code", ctx);
        assert_eq!(status, AgentStatus::Running);
    }

    #[test]
    fn test_process_active_empty_content_unknown() {
        let detector = StatusDetector::new();
        let ctx = ProcessContext { process_active: true, alt_screen: false };
        // Empty content with process_active → Unknown (no new data)
        let status = detector.detect(ProcessStatus::Unknown, None, "", ctx);
        assert_eq!(status, AgentStatus::Unknown);
    }

    #[test]
    fn test_process_inactive_fallback_idle() {
        let detector = StatusDetector::new();
        let ctx = ProcessContext { process_active: false, alt_screen: false };
        // No process active → normal text detection (fallback Idle)
        let status = detector.detect(ProcessStatus::Unknown, None, "Compiling foo v1.0", ctx);
        assert_eq!(status, AgentStatus::Idle);
    }

    #[test]
    fn test_process_active_cli_interrupted_then_prompt() {
        let detector = StatusDetector::new();
        let ctx = ProcessContext { process_active: true, alt_screen: false };
        // claude-code after cancel: shows interrupted message + prompt → Idle
        let status = detector.detect(
            ProcessStatus::Unknown, None,
            "[Request interrupted by user]\n> ", ctx,
        );
        assert_eq!(status, AgentStatus::Idle);
    }

    #[test]
    fn test_process_context_does_not_override_osc133() {
        let detector = StatusDetector::new();
        let ctx = ProcessContext { process_active: true, alt_screen: false };
        let info = ShellPhaseInfo {
            phase: ShellPhase::Running,
            last_post_exec_exit_code: None,
        };
        // OSC 133 Running takes priority over ProcessContext
        let status = detector.detect(ProcessStatus::Running, Some(info), "any", ctx);
        assert_eq!(status, AgentStatus::Running);
    }
}
