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
            // Use char count for truncation to avoid panicking on multi-byte UTF-8 boundaries
            let char_count = l.chars().count();
            if char_count > max_len {
                let truncated: String = l.chars().take(max_len).collect();
                format!("{}...", truncated)
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
}
