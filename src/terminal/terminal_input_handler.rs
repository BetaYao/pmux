//! GPUI InputHandler for terminal text input.
//! Text characters go through this path (efficient, IME-compatible).
//! Special keys (arrows, function keys, etc.) still use key_to_bytes via on_key_down.

use gpui::*;
use std::ops::Range;
use std::sync::Arc;

use crate::terminal::Terminal;

pub struct TerminalInputHandler {
    terminal: Arc<Terminal>,
    send_input: Arc<dyn Fn(&[u8]) + Send + Sync>,
    /// Screen-space bounds of the terminal cursor, computed at paint time.
    /// Used to position the IME candidate window.
    cursor_bounds: Option<Bounds<Pixels>>,
}

impl TerminalInputHandler {
    pub fn new(
        terminal: Arc<Terminal>,
        send_input: Arc<dyn Fn(&[u8]) + Send + Sync>,
    ) -> Self {
        Self {
            terminal,
            send_input,
            cursor_bounds: None,
        }
    }

    pub fn with_cursor_bounds(mut self, bounds: Option<Bounds<Pixels>>) -> Self {
        self.cursor_bounds = bounds;
        self
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
        // Read from persistent Terminal state (survives handler recreation each frame).
        self.terminal.ime_marked_text().map(|text| {
            let len_utf16: usize = text.encode_utf16().count();
            0..len_utf16
        })
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
        window: &mut Window,
        _cx: &mut App,
    ) {
        // Committed text — clear IME composition and frozen cursor in persistent state.
        self.terminal.set_ime_marked_text(None);
        self.terminal.set_ime_cursor_frozen(None);
        // Trigger repaint so the preedit overlay is cleared from the terminal surface.
        window.refresh();

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
        new_text: &str,
        _new_selected_range: Option<Range<usize>>,
        window: &mut Window,
        _cx: &mut App,
    ) {
        // IME composing state — store in persistent Terminal state.
        // Do NOT send to PTY; the final committed text arrives via replace_text_in_range.
        if new_text.is_empty() {
            self.terminal.set_ime_marked_text(None);
            self.terminal.set_ime_cursor_frozen(None);
        } else {
            // Freeze cursor position from the most recent paint frame.
            // We use the paint-time cursor instead of reading the grid directly
            // because TUI apps (like Claude Code) move the grid cursor during
            // background redraws. Between the last paint and this key event,
            // the cursor may have been moved to a status bar or output area.
            // The paint-time position reflects what the user actually saw on screen.
            if self.terminal.ime_cursor_frozen().is_none() {
                let cursor_pos = self.terminal.ime_last_paint_cursor();
                self.terminal.set_ime_cursor_frozen(Some(cursor_pos));
            }
            self.terminal.set_ime_marked_text(Some(new_text.to_string()));
        }
        // Trigger repaint so the preedit text overlay is rendered on the terminal surface.
        window.refresh();
    }

    fn unmark_text(&mut self, window: &mut Window, _cx: &mut App) {
        self.terminal.set_ime_marked_text(None);
        self.terminal.set_ime_cursor_frozen(None);
        window.refresh();
    }

    fn bounds_for_range(
        &mut self,
        _range_utf16: Range<usize>,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Option<Bounds<Pixels>> {
        self.cursor_bounds
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
