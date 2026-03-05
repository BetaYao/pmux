//! ContentExtractor parses terminal output to extract shell phase (OSC 133) and visible text.
//!
//! Used in the status pipeline: PTY output → ContentExtractor::feed → StatusPublisher.

use crate::shell_integration::{MarkerKind, Osc133Parser, ShellPhase};

/// Extracts shell phase from OSC 133 markers and visible text from terminal output.
/// Filters out CSI, OSC, and other escape sequences from the visible text.
pub struct ContentExtractor {
    osc133: Osc133Parser,
    phase: ShellPhase,
    text_buf: Vec<u8>,
    text_state: TextState,
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
            text_buf: Vec::new(),
            text_state: TextState::Normal,
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
                MarkerKind::PreExec => ShellPhase::Running,
                MarkerKind::PostExec => ShellPhase::Output,
            };
        }

        // Extract visible text (filter CSI, OSC, etc.)
        for &b in bytes {
            self.advance_text(b);
        }
    }

    fn advance_text(&mut self, b: u8) {
        match self.text_state {
            TextState::Normal => {
                if b == ESC {
                    self.text_state = TextState::AfterEsc;
                } else if Self::is_printable(b) {
                    self.text_buf.push(b);
                }
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

    fn is_printable(b: u8) -> bool {
        matches!(b, 0x20..=0x7e | b'\n' | b'\r' | b'\t')
    }

    /// Current shell phase derived from OSC 133 markers.
    pub fn shell_phase(&self) -> ShellPhase {
        self.phase
    }

    /// Returns accumulated visible text and clears the internal buffer.
    /// Second element is () for now (reserved for future use).
    pub fn take_content(&mut self) -> (String, ()) {
        let s = String::from_utf8_lossy(&self.text_buf).into_owned();
        self.text_buf.clear();
        (s, ())
    }
}

impl Default for ContentExtractor {
    fn default() -> Self {
        Self::new()
    }
}

/// Extract the last meaningful line from terminal content.
/// Skips empty lines and separator-only lines (e.g. "---", "===").
/// Truncates to `max_len` characters with "..." suffix if needed.
pub fn extract_last_line(content: &str, max_len: usize) -> String {
    content
        .lines()
        .rev()
        .map(|l| l.trim())
        .find(|l| {
            !l.is_empty()
                && l.len() > 1
                && !l.chars().all(|c| matches!(c, '-' | '=' | '*' | '_' | '~' | '.' | ' '))
        })
        .map(|l| {
            if l.len() > max_len {
                format!("{}...", &l[..max_len])
            } else {
                l.to_string()
            }
        })
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
}
