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
}
