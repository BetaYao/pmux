// src/terminal/terminal_core.rs
//! Core terminal state wrapping alacritty_terminal directly.

use alacritty_terminal::event::{Event as TermEvent, EventListener};
use alacritty_terminal::term::test::TermSize;
use alacritty_terminal::term::{Config as TermConfig, Term, TermMode};
use alacritty_terminal::vte::ansi::Processor;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Terminal size in cells and pixels
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerminalSize {
    pub cols: u16,
    pub rows: u16,
    pub cell_width: f32,
    pub cell_height: f32,
}

impl Default for TerminalSize {
    fn default() -> Self {
        Self { cols: 80, rows: 24, cell_width: 8.0, cell_height: 16.0 }
    }
}

/// Event listener bridge: captures title changes, bell, and PTY write-back
pub struct TermEventProxy {
    title: Arc<Mutex<Option<String>>>,
    has_bell: Arc<Mutex<bool>>,
    pty_write_tx: flume::Sender<Vec<u8>>,
}

impl TermEventProxy {
    pub fn new(
        title: Arc<Mutex<Option<String>>>,
        has_bell: Arc<Mutex<bool>>,
        pty_write_tx: flume::Sender<Vec<u8>>,
    ) -> Self {
        Self { title, has_bell, pty_write_tx }
    }
}

impl EventListener for TermEventProxy {
    fn send_event(&self, event: TermEvent) {
        match event {
            TermEvent::Title(t) => { *self.title.lock() = Some(t); }
            TermEvent::ResetTitle => { *self.title.lock() = None; }
            TermEvent::Bell => { *self.has_bell.lock() = true; }
            TermEvent::PtyWrite(data) => {
                let _ = self.pty_write_tx.send(data.into_bytes());
            }
            _ => {}
        }
    }
}

/// Core terminal wrapping alacritty_terminal::Term
pub struct Terminal {
    term: Arc<Mutex<Term<TermEventProxy>>>,
    processor: Mutex<Processor>,
    pub terminal_id: String,
    size: Mutex<TerminalSize>,
    title: Arc<Mutex<Option<String>>>,
    has_bell: Arc<Mutex<bool>>,
    dirty: AtomicBool,
    pub pty_write_rx: flume::Receiver<Vec<u8>>,
    /// Keep sender to maintain channel liveness; TermEventProxy uses a clone.
    #[allow(dead_code)]
    pty_write_tx: flume::Sender<Vec<u8>>,
}

impl Terminal {
    pub fn new(terminal_id: String, size: TerminalSize) -> Self {
        let config = TermConfig::default();
        let term_size = TermSize::new(size.cols as usize, size.rows as usize);
        let title = Arc::new(Mutex::new(None));
        let has_bell = Arc::new(Mutex::new(false));
        let (pty_write_tx, pty_write_rx) = flume::unbounded();

        let event_proxy = TermEventProxy::new(
            title.clone(),
            has_bell.clone(),
            pty_write_tx.clone(),
        );
        let term = Term::new(config, &term_size, event_proxy);

        Self {
            term: Arc::new(Mutex::new(term)),
            processor: Mutex::new(Processor::new()),
            terminal_id,
            size: Mutex::new(size),
            title,
            has_bell,
            dirty: AtomicBool::new(false),
            pty_write_rx,
            pty_write_tx,
        }
    }

    /// Feed PTY output bytes into the VTE parser
    pub fn process_output(&self, data: &[u8]) {
        let mut term = self.term.lock();
        let mut processor = self.processor.lock();
        processor.advance(&mut *term, data);
        self.dirty.store(true, Ordering::Relaxed);
    }

    /// Check and clear dirty flag
    pub fn take_dirty(&self) -> bool {
        self.dirty.swap(false, Ordering::Relaxed)
    }

    /// Read-only access to the terminal grid for rendering
    pub fn with_content<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&Term<TermEventProxy>) -> R,
    {
        let term = self.term.lock();
        f(&term)
    }

    /// Resize the terminal grid (does NOT resize PTY — caller must do that)
    pub fn resize(&self, new_size: TerminalSize) {
        *self.size.lock() = new_size;
        let mut term = self.term.lock();
        let term_size = TermSize::new(new_size.cols as usize, new_size.rows as usize);
        term.resize(term_size);
    }

    /// Current terminal size
    pub fn size(&self) -> TerminalSize {
        *self.size.lock()
    }

    /// Terminal title from OSC sequences
    pub fn title(&self) -> Option<String> {
        self.title.lock().clone()
    }

    /// Check and clear bell flag
    pub fn take_bell(&self) -> bool {
        let mut bell = self.has_bell.lock();
        let had = *bell;
        *bell = false;
        had
    }

    /// Current terminal mode flags (cursor visibility, app cursor, etc.)
    pub fn mode(&self) -> TermMode {
        *self.term.lock().mode()
    }

    /// Search the visible terminal grid for `query`. Returns all matches.
    /// Coordinates are in visual screen space (line 0 = top, col = column index).
    pub fn search(&self, query: &str) -> Vec<SearchMatch> {
        if query.is_empty() {
            return vec![];
        }
        self.with_content(|term| {
            use alacritty_terminal::grid::Dimensions;
            use alacritty_terminal::index::{Column, Line, Point};
            let grid = term.grid();
            let screen_lines = grid.screen_lines();
            let cols = grid.columns();
            let display_offset = grid.display_offset() as i32;
            let mut matches = Vec::new();

            for row in 0..screen_lines {
                let mut line_text = String::with_capacity(cols);
                let line = Line(row as i32 - display_offset);
                for col in 0..cols {
                    let cell = &grid[Point { line, column: Column(col) }];
                    line_text.push(cell.c);
                }
                let mut start = 0;
                while let Some(pos) = line_text[start..].find(query) {
                    matches.push(SearchMatch {
                        line: row as i32,
                        col: start + pos,
                        len: query.len(),
                    });
                    start += pos + query.len();
                    if start >= line_text.len() {
                        break;
                    }
                }
            }
            matches
        })
    }
}

/// A text match in the terminal grid (visual coordinates)
#[derive(Debug, Clone)]
pub struct SearchMatch {
    pub line: i32,
    pub col: usize,
    pub len: usize,
}

/// A detected URL in the terminal grid
#[derive(Debug, Clone)]
pub struct DetectedLink {
    pub line: i32,
    pub col: usize,
    pub len: usize,
    pub url: String,
}

impl Terminal {
    /// Detect http/https URLs in the visible terminal grid.
    pub fn detect_links(&self) -> Vec<DetectedLink> {
        self.with_content(|term| {
            use alacritty_terminal::grid::Dimensions;
            use alacritty_terminal::index::{Column, Line, Point};
            let grid = term.grid();
            let screen_lines = grid.screen_lines();
            let cols = grid.columns();
            let display_offset = grid.display_offset() as i32;
            let mut links = Vec::new();

            for row in 0..screen_lines {
                let mut line_text = String::with_capacity(cols);
                let line = Line(row as i32 - display_offset);
                for col in 0..cols {
                    let cell = &grid[Point { line, column: Column(col) }];
                    line_text.push(cell.c);
                }
                let mut start = 0;
                while let Some(pos) = line_text[start..].find("http") {
                    let abs = start + pos;
                    if line_text[abs..].starts_with("http://") || line_text[abs..].starts_with("https://") {
                        let url_end = line_text[abs..]
                            .find(|c: char| c.is_whitespace())
                            .map(|p| abs + p)
                            .unwrap_or(line_text.len());
                        let url = line_text[abs..url_end].to_string();
                        links.push(DetectedLink {
                            line: row as i32,
                            col: abs,
                            len: url.len(),
                            url,
                        });
                        start = url_end;
                    } else {
                        start = abs + 4;
                    }
                    if start >= line_text.len() {
                        break;
                    }
                }
            }
            links
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_terminal_new_default_size() {
        let term = Terminal::new("test-1".into(), TerminalSize::default());
        let size = term.size();
        assert_eq!(size.cols, 80);
        assert_eq!(size.rows, 24);
    }

    #[test]
    fn test_terminal_process_output_sets_dirty() {
        let term = Terminal::new("test-2".into(), TerminalSize::default());
        assert!(!term.take_dirty());
        term.process_output(b"hello");
        assert!(term.take_dirty());
        // take_dirty clears the flag
        assert!(!term.take_dirty());
    }

    #[test]
    fn test_terminal_resize() {
        let term = Terminal::new("test-3".into(), TerminalSize::default());
        term.resize(TerminalSize { cols: 120, rows: 40, cell_width: 8.0, cell_height: 16.0 });
        let size = term.size();
        assert_eq!(size.cols, 120);
        assert_eq!(size.rows, 40);
    }

    #[test]
    fn test_terminal_with_content() {
        let term = Terminal::new("test-4".into(), TerminalSize::default());
        let cols = term.with_content(|t| {
            use alacritty_terminal::grid::Dimensions;
            t.grid().columns()
        });
        assert_eq!(cols, 80);
    }

    #[test]
    fn test_terminal_title() {
        let term = Terminal::new("test-5".into(), TerminalSize::default());
        assert!(term.title().is_none());
        // Title is set via OSC sequences — just test None initial state
    }

    #[test]
    fn test_search_empty_query() {
        let term = Terminal::new("t".into(), TerminalSize::default());
        assert!(term.search("").is_empty());
    }

    #[test]
    fn test_search_no_match() {
        let term = Terminal::new("t".into(), TerminalSize::default());
        let matches = term.search("zzz_no_match");
        assert!(matches.is_empty());
    }
}
