//! GPUI InputHandler for terminal text input.
//! Text characters go through this path (efficient, IME-compatible).
//! Special keys (arrows, function keys, etc.) still use key_to_bytes via on_key_down.

use gpui::*;
use std::ops::Range;
use std::sync::Arc;

pub struct TerminalInputHandler {
    send_input: Arc<dyn Fn(&[u8]) + Send + Sync>,
}

impl TerminalInputHandler {
    pub fn new(send_input: Arc<dyn Fn(&[u8]) + Send + Sync>) -> Self {
        Self { send_input }
    }
}

impl InputHandler for TerminalInputHandler {
    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: 0..0,
            reversed: false,
        })
    }

    fn marked_text_range(&mut self, _window: &mut Window, _cx: &mut App) -> Option<Range<usize>> {
        None
    }

    fn text_for_range(
        &mut self,
        _range_utf16: Range<usize>,
        _adjusted_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Option<String> {
        None
    }

    fn replace_text_in_range(
        &mut self,
        _replacement_range: Option<Range<usize>>,
        text: &str,
        _window: &mut Window,
        _cx: &mut App,
    ) {
        if text.is_empty() {
            return;
        }
        // Filter macOS function key range (U+F700–U+F8FF)
        let filtered: String = text
            .chars()
            .filter(|c| !('\u{F700}'..='\u{F8FF}').contains(c))
            .collect();
        if filtered.is_empty() {
            return;
        }
        let mut bytes = Vec::new();
        for c in filtered.chars() {
            match c {
                '\r' | '\n' => bytes.push(b'\r'),
                '\u{8}' => bytes.push(0x7f),
                _ => {
                    let mut buf = [0u8; 4];
                    let s = c.encode_utf8(&mut buf);
                    bytes.extend_from_slice(s.as_bytes());
                }
            }
        }
        (self.send_input)(&bytes);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _range_utf16: Option<Range<usize>>,
        _new_text: &str,
        _new_selected_range: Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut App,
    ) {
        // IME composing state — do NOT send to PTY.
        // The final committed text arrives via replace_text_in_range.
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut App) {}

    fn bounds_for_range(
        &mut self,
        _range_utf16: Range<usize>,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Option<Bounds<Pixels>> {
        None
    }

    fn character_index_for_point(
        &mut self,
        _point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Option<usize> {
        None
    }
}
