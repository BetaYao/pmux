//! ContentExtractor parses terminal output to extract shell phase (OSC 133) and visible text.
//!
//! Used in the status pipeline: PTY output → ContentExtractor::feed → StatusPublisher.

use crate::shell_integration::{MarkerKind, Osc133Parser, ShellPhase};

/// Extracts shell phase from OSC 133 markers and visible text from terminal output.
/// Filters out CSI, OSC, and other escape sequences from the visible text.
pub struct ContentExtractor {
    osc133: Osc133Parser,
    phase: ShellPhase,
    /// Exit code from last PostExec marker (OSC 133;D;N). Cleared on PreExec.
    last_exit_code: Option<u8>,
    text_buf: Vec<u8>,
    text_state: TextState,
    /// UTF-8 multi-byte accumulation buffer (max 4 bytes per codepoint)
    utf8_buf: [u8; 4],
    /// Number of continuation bytes still expected
    utf8_expected: usize,
    /// Number of bytes accumulated so far in utf8_buf
    utf8_len: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum TextState {
    #[default]
    Normal,
    AfterEsc,
    InCsi,
    InOsc,
    InOscAfterEsc,
}

const ESC: u8 = 0x1b;
const CSI_START: u8 = b'[';
const OSC_START: u8 = b']';
const ST_SECOND: u8 = b'\\';
const BEL: u8 = 0x07;

impl ContentExtractor {
    pub fn new() -> Self {
        Self {
            osc133: Osc133Parser::new(),
            phase: ShellPhase::Unknown,
            last_exit_code: None,
            text_buf: Vec::new(),
            text_state: TextState::Normal,
            utf8_buf: [0; 4],
            utf8_expected: 0,
            utf8_len: 0,
        }
    }

    /// Feed terminal output bytes. Updates shell phase from OSC 133 markers and
    /// accumulates visible text (filtering escape sequences).
    pub fn feed(&mut self, bytes: &[u8]) {
        // Parse OSC 133 for phase
        let markers = self.osc133.feed(bytes);
        for m in markers {
            self.phase = match m.kind {
                MarkerKind::PromptStart => ShellPhase::Prompt,
                MarkerKind::PromptEnd => ShellPhase::Input,
                MarkerKind::PreExec => {
                    self.last_exit_code = None; // new command starting, clear old exit code
                    ShellPhase::Running
                }
                MarkerKind::PostExec => {
                    self.last_exit_code = m.exit_code; // save exit code from D marker
                    ShellPhase::Output
                }
            };
        }

        // Extract visible text (filter CSI, OSC, etc.)
        for &b in bytes {
            self.advance_text(b);
        }
    }

    fn advance_text(&mut self, b: u8) {
        // If we're accumulating a UTF-8 multi-byte sequence, handle continuation bytes first.
        // An ESC byte (0x1b) inside a UTF-8 sequence means the sequence was broken/invalid;
        // discard the partial UTF-8 and switch to escape handling.
        if self.utf8_expected > 0 {
            if b == ESC {
                // Broken UTF-8 sequence interrupted by ESC — discard partial and handle ESC
                self.utf8_expected = 0;
                self.utf8_len = 0;
                self.text_state = TextState::AfterEsc;
                return;
            }
            if (0x80..=0xBF).contains(&b) {
                // Valid continuation byte
                self.utf8_buf[self.utf8_len] = b;
                self.utf8_len += 1;
                self.utf8_expected -= 1;
                if self.utf8_expected == 0 {
                    self.text_buf
                        .extend_from_slice(&self.utf8_buf[..self.utf8_len]);
                    self.utf8_len = 0;
                }
            } else {
                // Invalid continuation — discard partial UTF-8, re-process this byte
                self.utf8_expected = 0;
                self.utf8_len = 0;
                self.advance_text(b);
            }
            return;
        }

        match self.text_state {
            TextState::Normal => {
                if b == ESC {
                    self.text_state = TextState::AfterEsc;
                } else if b >= 0xC2 && b <= 0xF4 {
                    // UTF-8 multi-byte lead byte (C2-DF = 2-byte, E0-EF = 3-byte, F0-F4 = 4-byte)
                    self.utf8_buf[0] = b;
                    self.utf8_len = 1;
                    self.utf8_expected = if b < 0xE0 {
                        1
                    } else if b < 0xF0 {
                        2
                    } else {
                        3
                    };
                } else if matches!(b, 0x20..=0x7e | b'\n' | b'\r' | b'\t') {
                    self.text_buf.push(b);
                }
                // C0 controls (except \n\r\t), C0/C1 bytes (0x80-0xBF alone), overlong
                // lead bytes (0xC0-0xC1, 0xF5-0xFF) are silently dropped.
            }
            TextState::AfterEsc => {
                if b == CSI_START {
                    self.text_state = TextState::InCsi;
                } else if b == OSC_START {
                    self.text_state = TextState::InOsc;
                } else if b == ST_SECOND {
                    // ESC \ - ST without preceding OSC, discard
                    self.text_state = TextState::Normal;
                } else {
                    self.text_state = TextState::Normal;
                    // Re-process: standalone ESC typically doesn't emit, but a non-sequence
                    // like ESC X might; we skip ESC and the byte (conservative)
                }
            }
            TextState::InCsi => {
                // CSI ends with byte in 0x40..0x7e (e.g. m, H, A)
                if (0x40..=0x7e).contains(&b) {
                    self.text_state = TextState::Normal;
                }
            }
            TextState::InOsc => {
                if b == BEL {
                    self.text_state = TextState::Normal;
                } else if b == ESC {
                    self.text_state = TextState::InOscAfterEsc;
                }
            }
            TextState::InOscAfterEsc => {
                if b == ST_SECOND {
                    self.text_state = TextState::Normal;
                } else {
                    self.text_state = TextState::InOsc;
                }
            }
        }
    }

    /// Current shell phase derived from OSC 133 markers.
    pub fn shell_phase(&self) -> ShellPhase {
        self.phase
    }

    /// Exit code from last PostExec (OSC 133;D;N). None if no PostExec yet or no code provided.
    pub fn last_exit_code(&self) -> Option<u8> {
        self.last_exit_code
    }

    /// Returns accumulated visible text and clears the internal buffer.
    /// Second element is () for now (reserved for future use).
    pub fn take_content(&mut self) -> (String, ()) {
        let s = String::from_utf8_lossy(&self.text_buf).into_owned();
        self.text_buf.clear();
        (s, ())
    }

    /// Returns a view of accumulated visible text for status detection WITHOUT clearing.
    /// The buffer is capped at `max_len` bytes (keeping the tail) to prevent unbounded growth.
    pub fn content_for_status(&mut self, max_len: usize) -> String {
        // Cap buffer to max_len to prevent unbounded growth
        if self.text_buf.len() > max_len {
            // Find a safe UTF-8 boundary to truncate from
            let start = self.text_buf.len() - max_len;
            // Walk forward to find the start of a valid UTF-8 codepoint
            let start = self.text_buf[start..]
                .iter()
                .position(|&b| (b & 0xC0) != 0x80) // not a continuation byte
                .map(|off| start + off)
                .unwrap_or(self.text_buf.len());
            self.text_buf.drain(..start);
        }
        String::from_utf8_lossy(&self.text_buf).into_owned()
    }
}

impl Default for ContentExtractor {
    fn default() -> Self {
        Self::new()
    }
}

/// Check if a character is a box-drawing, block element, or simple separator character.
fn is_box_or_separator(c: char) -> bool {
    matches!(c,
        '-' | '=' | '*' | '_' | '~' | '.' | ' '
        // Box Drawing (U+2500-U+257F)
        | '\u{2500}'..='\u{257F}'
        // Block Elements (U+2580-U+259F)
        | '\u{2580}'..='\u{259F}'
    )
}

/// Check if a character is a TUI frame start character (vertical bars, corners, etc.).
fn is_frame_start_char(c: char) -> bool {
    matches!(c,
        '┃' | '│' | '╹' | '╻' | '┌' | '┐' | '└' | '┘'
        | '├' | '┤' | '╭' | '╮' | '╯' | '╰'
        | '║' | '╔' | '╗' | '╚' | '╝'
    )
}

/// Detect if a line is TUI chrome (borders, separators, single-char prompts, etc.).
fn is_chrome_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return true;
    }
    // Use char count (not byte len) so multi-byte single chars like ❯ are correctly detected
    let visible_chars = trimmed.chars().count();
    if visible_chars <= 1 {
        return true;
    }
    // All box-drawing / border / block characters + spaces
    if trimmed.chars().all(|c| is_box_or_separator(c)) {
        return true;
    }
    // Starts with a frame character (e.g. ┃ text, ╹▀▀▀)
    if let Some(first) = trimmed.chars().next() {
        if is_frame_start_char(first) {
            return true;
        }
    }
    false
}

/// Truncate a string to `max_len` characters, appending "..." if truncated.
fn truncate_with_ellipsis(s: &str, max_len: usize) -> String {
    let char_count = s.chars().count();
    if char_count > max_len {
        let truncated: String = s.chars().take(max_len).collect();
        format!("{}...", truncated)
    } else {
        s.to_string()
    }
}

/// Extract the last meaningful line from terminal content.
/// Skips empty lines and separator-only lines (e.g. "---", "===").
/// Truncates to `max_len` characters with "..." suffix if needed.
pub fn extract_last_line(content: &str, max_len: usize) -> String {
    extract_last_line_filtered(content, max_len, &[])
}

/// Extract the last meaningful line from terminal content, with two-layer filtering:
/// 1. Default filtering: skip chrome lines (box-drawing, single-char, separators)
/// 2. Per-agent filtering: skip lines matching any of `skip_patterns` (case-insensitive)
///
/// Used by `AgentDef::extract_last_message()` to skip TUI chrome specific to each agent.
pub fn extract_last_line_filtered(
    content: &str,
    max_len: usize,
    skip_patterns: &[String],
) -> String {
    content
        .lines()
        .rev()
        .map(|l| l.trim())
        .find(|l| {
            if is_chrome_line(l) {
                return false;
            }
            // Per-agent skip patterns (case-insensitive substring match)
            if !skip_patterns.is_empty() {
                let lower = l.to_lowercase();
                if skip_patterns
                    .iter()
                    .any(|p| lower.contains(&p.to_lowercase()))
                {
                    return false;
                }
            }
            true
        })
        .map(|l| truncate_with_ellipsis(l, max_len))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_last_line_empty() {
        assert_eq!(extract_last_line("", 80), "");
        assert_eq!(extract_last_line("   \n  \n", 80), "");
    }

    #[test]
    fn test_extract_last_line_separator_only() {
        assert_eq!(extract_last_line("---\n===\n***\n...\n", 80), "");
    }

    #[test]
    fn test_extract_last_line_truncation() {
        let long = "a".repeat(100);
        let result = extract_last_line(&long, 20);
        assert_eq!(result, format!("{}...", "a".repeat(20)));
    }

    #[test]
    fn test_extract_last_line_normal() {
        let content = "first line\nsecond line\n---\n\n";
        assert_eq!(extract_last_line(content, 80), "second line");
    }

    #[test]
    fn test_extract_last_line_skips_single_char() {
        let content = "hello world\n?\n";
        assert_eq!(extract_last_line(content, 80), "hello world");
    }

    #[test]
    fn test_extract_last_line_utf8_truncation() {
        // Truncating multi-byte chars must not panic
        let content = "café résumé naïve";
        let result = extract_last_line(content, 5);
        assert_eq!(result, "café ...");

        // CJK characters
        let content = "错误：编译失败了";
        let result = extract_last_line(content, 4);
        assert_eq!(result, "错误：编...");

        // Emoji
        let content = "✓ Done 🎉 完成";
        let result = extract_last_line(content, 6);
        assert_eq!(result, "✓ Done...");
    }

    #[test]
    fn test_content_extractor_utf8() {
        let mut ext = ContentExtractor::new();

        // Chinese text
        ext.feed("你好世界".as_bytes());
        let (text, _) = ext.take_content();
        assert_eq!(text, "你好世界");

        // Mixed ASCII + UTF-8
        ext.feed("Error: 编译失败".as_bytes());
        let (text, _) = ext.take_content();
        assert_eq!(text, "Error: 编译失败");

        // Emoji
        ext.feed("✓ Done".as_bytes());
        let (text, _) = ext.take_content();
        assert_eq!(text, "✓ Done");
    }

    #[test]
    fn test_content_extractor_utf8_with_escapes() {
        let mut ext = ContentExtractor::new();

        // UTF-8 text mixed with CSI color codes
        let mut data = Vec::new();
        data.extend_from_slice(b"\x1b[31m"); // red
        data.extend_from_slice("错误".as_bytes());
        data.extend_from_slice(b"\x1b[0m"); // reset
        data.extend_from_slice(b": failed");
        ext.feed(&data);
        let (text, _) = ext.take_content();
        assert_eq!(text, "错误: failed");
    }

    #[test]
    fn test_content_extractor_broken_utf8() {
        let mut ext = ContentExtractor::new();

        // Broken UTF-8: lead byte followed by ESC (not a continuation byte)
        ext.feed(&[0xC3, ESC, b'[', b'm', b'x']);
        let (text, _) = ext.take_content();
        // The partial UTF-8 (0xC3) is discarded, ESC[m is consumed, 'x' is kept
        assert_eq!(text, "x");
    }

    #[test]
    fn test_content_extractor_invalid_continuation() {
        let mut ext = ContentExtractor::new();

        // Lead byte (0xC3) followed by non-continuation ASCII byte
        ext.feed(&[0xC3, b'A']);
        let (text, _) = ext.take_content();
        // 0xC3 is discarded (invalid sequence), 'A' is re-processed as printable
        assert_eq!(text, "A");
    }

    // --- Tests for two-layer filtering ---

    #[test]
    fn test_is_chrome_line_box_drawing() {
        // Lines composed entirely of box-drawing characters are chrome
        assert!(is_chrome_line("─────────────────"));
        assert!(is_chrome_line("═══════════════"));
        assert!(is_chrome_line("▀▀▀▀▀▀▀▀▀▀▀▀▀"));
        assert!(is_chrome_line("────── ──────")); // box drawing + spaces
        assert!(is_chrome_line("  "));
        assert!(is_chrome_line(""));
    }

    #[test]
    fn test_is_chrome_line_frame_start() {
        // Lines starting with frame characters are chrome
        assert!(is_chrome_line("┃ some content"));
        assert!(is_chrome_line("│ sidebar text"));
        assert!(is_chrome_line("╹▀▀▀▀▀▀▀▀▀"));
        assert!(is_chrome_line("╭─────────────╮"));
        assert!(is_chrome_line("╰─────────────╯"));
        assert!(is_chrome_line("║ double frame"));
    }

    #[test]
    fn test_is_chrome_line_single_char_utf8() {
        // Single UTF-8 char (like ❯) should be detected as single-char and skipped
        assert!(is_chrome_line("❯"));
        assert!(is_chrome_line("$"));
        assert!(is_chrome_line("▶"));
        // But multi-char content is NOT chrome
        assert!(!is_chrome_line("❯ hello"));
        assert!(!is_chrome_line("hello world"));
    }

    #[test]
    fn test_is_chrome_line_normal_content() {
        // Normal content should NOT be detected as chrome
        assert!(!is_chrome_line("I've updated the file successfully"));
        assert!(!is_chrome_line("Error: compilation failed"));
        assert!(!is_chrome_line("Done: 3 files changed"));
        assert!(!is_chrome_line("已提交并推送到 main"));
    }

    #[test]
    fn test_extract_filtered_claude_code() {
        // Simulated Claude Code bottom of screen
        let content = "I've updated the configuration file.\n\
                       \n\
                       ─────────────────────────────────\n\
                       > accept edits on (shift+tab to cycle)\n\
                       \n\
                       ╹ press esc to interrupt";
        let skip = vec![
            "shift+tab".to_string(),
            "accept edits".to_string(),
            "to interrupt".to_string(),
        ];
        let result = extract_last_line_filtered(content, 80, &skip);
        assert_eq!(result, "I've updated the configuration file.");
    }

    #[test]
    fn test_extract_filtered_opencode() {
        // Simulated opencode bottom of screen
        let content = "Successfully compiled the project\n\
                       ───────────────────────────────\n\
                       │ tab agents  ctrl+p commands\n\
                       ╹▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀";
        let skip = vec![
            "tab agents".to_string(),
            "ctrl+p commands".to_string(),
        ];
        let result = extract_last_line_filtered(content, 80, &skip);
        assert_eq!(result, "Successfully compiled the project");
    }

    #[test]
    fn test_extract_filtered_skip_patterns_case_insensitive() {
        let content = "Real message here\nShift+Tab to cycle\n";
        let skip = vec!["shift+tab".to_string()];
        let result = extract_last_line_filtered(content, 80, &skip);
        assert_eq!(result, "Real message here");
    }

    #[test]
    fn test_extract_last_line_backward_compat() {
        // Original extract_last_line behavior should be preserved
        assert_eq!(extract_last_line("", 80), "");
        assert_eq!(extract_last_line("---\n===\n", 80), "");
        assert_eq!(
            extract_last_line("first line\nsecond line\n---\n\n", 80),
            "second line"
        );
        // New: box-drawing lines are also skipped (enhancement)
        assert_eq!(
            extract_last_line("real content\n──────────\n", 80),
            "real content"
        );
    }

    #[test]
    fn test_extract_filtered_empty_skip_patterns() {
        // With empty skip patterns, behaves like enhanced extract_last_line
        let content = "message\n┃ chrome line\n───────\n";
        let result = extract_last_line_filtered(content, 80, &[]);
        assert_eq!(result, "message");
    }

    #[test]
    fn test_extract_filtered_all_chrome() {
        // When all lines are chrome, return empty
        let content = "───────\n┃ frame\n╹▀▀▀▀\n";
        let result = extract_last_line_filtered(content, 80, &[]);
        assert_eq!(result, "");
    }
}
