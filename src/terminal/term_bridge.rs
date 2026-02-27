// terminal/term_bridge.rs - Bridge to alacritty_terminal::Term for VT parsing
use alacritty_terminal::event::VoidListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi;
use std::sync::Mutex;

/// Fixed terminal dimensions for tmux pane display.
#[derive(Debug, Clone, Copy)]
struct TermDimensions {
    columns: usize,
    screen_lines: usize,
}

impl Dimensions for TermDimensions {
    fn total_lines(&self) -> usize {
        self.screen_lines
    }

    fn screen_lines(&self) -> usize {
        self.screen_lines
    }

    fn columns(&self) -> usize {
        self.columns
    }
}

/// Bridge to alacritty_terminal::Term for parsing VT sequences from tmux control mode.
pub struct TermBridge {
    term: Mutex<Term<VoidListener>>,
    parser: Mutex<ansi::Processor>,
}

impl TermBridge {
    /// Create a new TermBridge with the given dimensions (columns, lines).
    pub fn new(columns: usize, screen_lines: usize) -> Self {
        let size = TermDimensions { columns, screen_lines };
        let term = Term::new(Config::default(), &size, VoidListener);
        Self {
            term: Mutex::new(term),
            parser: Mutex::new(ansi::Processor::new()),
        }
    }

    /// Feed raw bytes (VT sequences) to the terminal. Call this with output from tmux control mode.
    pub fn advance(&self, bytes: &[u8]) {
        if let (Ok(mut term), Ok(mut parser)) = (self.term.lock(), self.parser.lock()) {
            parser.advance(&mut *term, bytes);
        }
    }

    /// Access the underlying Term for rendering.
    pub fn term(&self) -> std::sync::MutexGuard<'_, Term<VoidListener>> {
        self.term.lock().unwrap()
    }

    /// Extract visible lines as plain text for rendering. Skips WIDE_CHAR_SPACER cells.
    /// Uses display_iter for correct visible region ordering (handles grid ring buffer).
    pub fn visible_lines(&self) -> Vec<String> {
        let term = self.term.lock().unwrap();
        let grid = term.grid();
        let cols = grid.columns();
        let screen_lines = grid.screen_lines();
        let mut lines: Vec<String> = (0..screen_lines).map(|_| String::with_capacity(cols)).collect();
        for indexed in grid.display_iter() {
            if !indexed.cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                let row = indexed.point.line.0;
                let display_start = -(grid.display_offset() as i32) - 1;
                let row_idx = (row - display_start) as usize;
                if row_idx < screen_lines {
                    lines[row_idx].push(indexed.cell.c);
                }
            }
        }
        lines.iter().map(|s| s.trim_end().to_string()).collect()
    }

    /// Get cursor position (row, col) in visible coordinates.
    pub fn cursor_position(&self) -> Option<(usize, usize)> {
        let term = self.term.lock().unwrap();
        let grid = term.grid();
        let display_offset = grid.display_offset();
        let cursor = grid.cursor.point;
        let row = (cursor.line.0 + display_offset as i32) as usize;
        let col = cursor.column.0;
        Some((row, col))
    }
}
