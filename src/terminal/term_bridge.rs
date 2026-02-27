// terminal/term_bridge.rs - Bridge to alacritty_terminal::Term for VT parsing
use alacritty_terminal::event::VoidListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::{self, Color, NamedColor, Rgb};
use std::sync::Mutex;

/// Zed One Dark terminal colors (from Zed's built-in theme)
const DEFAULT_FG: Rgb = Rgb { r: 0xab, g: 0xb2, b: 0xbf }; // terminal.foreground
const DEFAULT_BG: Rgb = Rgb { r: 0x28, g: 0x2c, b: 0x34 }; // terminal.background

/// Styled cell: (char, fg_rgb, bg_rgb) for rendering with ANSI colors.
pub type StyledCell = (char, [u8; 3], [u8; 3]);

/// Zed One Dark ANSI 16-color palette (terminal.ansi.*)
const ZED_ONE_DARK_16: [Rgb; 16] = [
    Rgb { r: 0x28, g: 0x2c, b: 0x34 }, // black
    Rgb { r: 0xe0, g: 0x6c, b: 0x75 }, // red
    Rgb { r: 0x98, g: 0xc3, b: 0x79 }, // green
    Rgb { r: 0xe5, g: 0xc0, b: 0x7b }, // yellow
    Rgb { r: 0x61, g: 0xaf, b: 0xef }, // blue
    Rgb { r: 0xc6, g: 0x78, b: 0xdd }, // magenta
    Rgb { r: 0x56, g: 0xb6, b: 0xc2 }, // cyan
    Rgb { r: 0xab, g: 0xb2, b: 0xbf }, // white
    Rgb { r: 0x63, g: 0x6d, b: 0x83 }, // bright black
    Rgb { r: 0xea, g: 0x85, b: 0x8b }, // bright red
    Rgb { r: 0xaa, g: 0xd5, b: 0x81 }, // bright green
    Rgb { r: 0xff, g: 0xd8, b: 0x85 }, // bright yellow
    Rgb { r: 0x85, g: 0xc1, b: 0xff }, // bright blue
    Rgb { r: 0xd3, g: 0x98, b: 0xeb }, // bright magenta
    Rgb { r: 0x6e, g: 0xd5, b: 0xde }, // bright cyan
    Rgb { r: 0xfa, g: 0xfa, b: 0xfa }, // bright white
];

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

    /// Resize the terminal to new dimensions. PTY should be resized separately via runtime.resize().
    pub fn resize(&self, columns: usize, screen_lines: usize) {
        let size = TermDimensions { columns, screen_lines };
        if let Ok(mut term) = self.term.lock() {
            term.resize(size);
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

    /// Resolve Color to Rgb using term's color table, with fallbacks for unset colors.
    fn resolve_color(color: Color, colors: &alacritty_terminal::term::color::Colors) -> Rgb {
        match color {
            Color::Spec(rgb) => rgb,
            Color::Named(n) => {
                let idx = n as usize;
                colors[idx]
                    .or_else(|| {
                        if idx < 16 {
                            Some(ZED_ONE_DARK_16[idx])
                        } else if n == NamedColor::Foreground || n == NamedColor::BrightForeground {
                            Some(DEFAULT_FG)
                        } else if n == NamedColor::Background {
                            Some(DEFAULT_BG)
                        } else {
                            None
                        }
                    })
                    .unwrap_or(DEFAULT_FG)
            }
            Color::Indexed(i) => colors[i as usize].unwrap_or_else(|| {
                if i < 16 {
                    ZED_ONE_DARK_16[i as usize]
                } else {
                    DEFAULT_FG
                }
            }),
        }
    }

    /// Extract visible lines as plain text for rendering. Skips WIDE_CHAR_SPACER cells.
    /// display_iter yields (display_offset+1 + screen_lines) lines; we allocate screen_lines+1
    /// to capture all (avoids dropping last line which caused cursor misalignment).
    pub fn visible_lines(&self) -> Vec<String> {
        let term = self.term.lock().unwrap();
        let grid = term.grid();
        let cols = grid.columns();
        let screen_lines = grid.screen_lines();
        let capacity = screen_lines + 1;
        let mut lines: Vec<String> = (0..capacity).map(|_| String::with_capacity(cols)).collect();
        for indexed in grid.display_iter() {
            if !indexed.cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                let row = indexed.point.line.0;
                let display_start = -(grid.display_offset() as i32) - 1;
                let row_idx = (row - display_start) as usize;
                if row_idx < capacity {
                    lines[row_idx].push(indexed.cell.c);
                }
            }
        }
        lines.iter().map(|s| s.trim_end().to_string()).collect()
    }

    /// Extract visible lines with per-cell colors for TUI rendering.
    /// Allocates screen_lines+1 to match display_iter output (fixes cursor alignment).
    pub fn visible_lines_with_colors(&self) -> Vec<Vec<StyledCell>> {
        let term = self.term.lock().unwrap();
        let grid = term.grid();
        let colors = term.colors();
        let cols = grid.columns();
        let screen_lines = grid.screen_lines();
        let capacity = screen_lines + 1;
        let mut lines: Vec<Vec<StyledCell>> = (0..capacity).map(|_| Vec::with_capacity(cols)).collect();
        for indexed in grid.display_iter() {
            if !indexed.cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                let row = indexed.point.line.0;
                let display_start = -(grid.display_offset() as i32) - 1;
                let row_idx = (row - display_start) as usize;
                if row_idx < capacity {
                    let fg = Self::resolve_color(indexed.cell.fg, colors);
                    let bg = Self::resolve_color(indexed.cell.bg, colors);
                    lines[row_idx].push((
                        indexed.cell.c,
                        [fg.r, fg.g, fg.b],
                        [bg.r, bg.g, bg.b],
                    ));
                }
            }
        }
        for row in &mut lines {
            while row.last().map(|(c, _, _)| *c == ' ').unwrap_or(false) {
                row.pop();
            }
        }
        lines
    }

    /// Get cursor position (row, col) matching visible_lines row indexing.
    /// visible_lines uses row_idx = grid_row - display_start (display_start = -display_offset-1).
    /// So row_idx = grid_row + display_offset + 1. Cursor at grid_row has viewport_line = grid_row + display_offset.
    /// Thus row_idx = viewport_line + 1. We return row in 0..(screen_lines+1) to match.
    pub fn cursor_position(&self) -> Option<(usize, usize)> {
        let term = self.term.lock().unwrap();
        let grid = term.grid();
        let display_offset = grid.display_offset();
        let cursor = grid.cursor.point;
        let screen_lines = grid.screen_lines();
        let viewport_line = (cursor.line.0 + display_offset as i32) as usize;
        let row = (viewport_line + 1).min(screen_lines); // +1 to match visible_lines row_idx
        let col = cursor.column.0;
        Some((row, col))
    }
}
