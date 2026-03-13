// src/terminal/terminal_core.rs
//! Core terminal state wrapping alacritty_terminal directly.

use alacritty_terminal::event::{Event as TermEvent, EventListener};
use alacritty_terminal::grid::Scroll;
use alacritty_terminal::index::Side;
use alacritty_terminal::selection::{Selection, SelectionRange, SelectionType};
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

/// Event listener bridge: captures title changes, bell, PTY write-back, and OSC 52 clipboard
pub struct TermEventProxy {
    title: Arc<Mutex<Option<String>>>,
    has_bell: Arc<Mutex<bool>>,
    pty_write_tx: flume::Sender<Vec<u8>>,
    clipboard_store_tx: flume::Sender<String>,
}

impl TermEventProxy {
    pub fn new(
        title: Arc<Mutex<Option<String>>>,
        has_bell: Arc<Mutex<bool>>,
        pty_write_tx: flume::Sender<Vec<u8>>,
        clipboard_store_tx: flume::Sender<String>,
    ) -> Self {
        Self { title, has_bell, pty_write_tx, clipboard_store_tx }
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
            // OSC 52: terminal app requests clipboard write (e.g. opencode, tmux copy-mode)
            TermEvent::ClipboardStore(_ty, text) => {
                let _ = self.clipboard_store_tx.send(text);
            }
            _ => {}
        }
    }
}

/// Find the largest prefix length of `data` that doesn't split a UTF-8 multi-byte sequence.
///
/// Scans backward from the end to check if the trailing bytes form an incomplete
/// UTF-8 character. Returns `data.len()` if the data ends on a complete boundary.
fn find_utf8_boundary(data: &[u8]) -> usize {
    if data.is_empty() {
        return 0;
    }
    // Check trailing bytes: walk backward to find the last lead byte or ASCII byte.
    // UTF-8 encoding:
    //   0xxxxxxx (00-7F) = 1-byte (ASCII) — always a complete boundary
    //   110xxxxx (C0-DF) = 2-byte lead, needs 1 continuation
    //   1110xxxx (E0-EF) = 3-byte lead, needs 2 continuations
    //   11110xxx (F0-F7) = 4-byte lead, needs 3 continuations
    //   10xxxxxx (80-BF) = continuation byte
    let len = data.len();
    // Walk back up to 3 bytes (max UTF-8 sequence is 4 bytes)
    let check_from = len.saturating_sub(3);
    for i in (check_from..len).rev() {
        let b = data[i];
        if b < 0x80 {
            // ASCII byte — everything up to and including this is safe
            return len;
        }
        if b >= 0xC2 {
            // Lead byte found — check if the sequence is complete
            let expected = if b < 0xE0 {
                2
            } else if b < 0xF0 {
                3
            } else {
                4
            };
            let available = len - i;
            if available >= expected {
                // Complete sequence — all of `data` is safe
                return len;
            } else {
                // Incomplete sequence — split before this lead byte
                return i;
            }
        }
        // else: continuation byte (0x80-0xBF), keep walking back
    }
    // All trailing bytes are continuations with no lead byte in range — likely
    // corrupted data. Feed it all through and let the VTE parser handle it.
    len
}

/// Core terminal wrapping alacritty_terminal::Term
pub struct Terminal {
    term: Arc<Mutex<Term<TermEventProxy>>>,
    processor: Arc<Mutex<Processor>>,
    pub terminal_id: String,
    size: Mutex<TerminalSize>,
    title: Arc<Mutex<Option<String>>>,
    has_bell: Arc<Mutex<bool>>,
    dirty: Arc<AtomicBool>,
    pub pty_write_rx: flume::Receiver<Vec<u8>>,
    /// Keep sender to maintain channel liveness; TermEventProxy uses a clone.
    #[allow(dead_code)]
    pty_write_tx: flume::Sender<Vec<u8>>,
    /// OSC 52 clipboard store requests from terminal apps (e.g. opencode, tmux).
    pub clipboard_store_rx: flume::Receiver<String>,
    #[allow(dead_code)]
    clipboard_store_tx: flume::Sender<String>,
    cached_links: Mutex<Option<Vec<DetectedLink>>>,
    cached_search: Mutex<Option<(String, Vec<SearchMatch>)>>,
    scroll_pixel_remainder: Mutex<f32>,
    /// Buffer for incomplete UTF-8 sequences split across chunks.
    /// Holds trailing bytes that form an incomplete multi-byte character,
    /// to be prepended to the next chunk before feeding to the VTE parser.
    utf8_remainder: Mutex<Vec<u8>>,
    /// IME composition state — persists across input handler recreations.
    /// The InputHandler is recreated every paint frame, so marked text must
    /// live here to survive between frames.
    ime_marked_text: Mutex<Option<String>>,
    /// Frozen cursor position (column, visual_line) captured when IME composition starts.
    /// While composing, the terminal may receive output that moves the grid cursor away
    /// from where the user is typing. This field locks the cursor position so the
    /// preedit overlay and candidate window stay at the original input location.
    ime_cursor_frozen: Mutex<Option<(usize, i32)>>,
    /// Cursor position (column, visual_line) saved at each paint frame.
    /// Used as a stable reference for IME cursor freezing: TUI apps (like Claude Code)
    /// may move the grid cursor between paint frames and key events, but the cursor
    /// position at paint time reflects what the user actually sees on screen.
    ime_last_paint_cursor: Mutex<(usize, i32)>,
    /// True during a CSI ?2026h synchronized-output block.
    /// Detected in process_output() by scanning for CSI ?2026h/l sequences.
    /// Used to gate cx.notify() in the output loop so that renders only happen
    /// outside sync blocks (relevant for local PTY mode; tmux strips these).
    synchronized_output: AtomicBool,
    /// True when this terminal is backed by tmux (control mode).
    /// When true, paint-time `capture_pane_resync` is used to sync the VTE grid
    /// from tmux's frame-consistent screen state before rendering, eliminating
    /// ghosting caused by mid-frame %output processing.
    tmux_backed: bool,
    /// Stop signal for the background resync thread.
    resync_stop: Arc<AtomicBool>,
    /// Set by process_output() when new data arrives. The background resync
    /// thread only calls capture_pane_resync when this is true, so zero
    /// subprocess overhead when the terminal is idle.
    output_dirty: Arc<AtomicBool>,
}

impl Terminal {
    pub fn new(terminal_id: String, size: TerminalSize) -> Self {
        let config = TermConfig::default();
        let term_size = TermSize::new(size.cols as usize, size.rows as usize);
        let title = Arc::new(Mutex::new(None));
        let has_bell = Arc::new(Mutex::new(false));
        let (pty_write_tx, pty_write_rx) = flume::unbounded();
        let (clipboard_store_tx, clipboard_store_rx) = flume::unbounded();

        let event_proxy = TermEventProxy::new(
            title.clone(),
            has_bell.clone(),
            pty_write_tx.clone(),
            clipboard_store_tx.clone(),
        );
        let term = Term::new(config, &term_size, event_proxy);

        Self {
            term: Arc::new(Mutex::new(term)),
            processor: Arc::new(Mutex::new(Processor::new())),
            terminal_id,
            size: Mutex::new(size),
            title,
            has_bell,
            dirty: Arc::new(AtomicBool::new(false)),
            pty_write_rx,
            pty_write_tx,
            clipboard_store_rx,
            clipboard_store_tx,
            cached_links: Mutex::new(None),
            cached_search: Mutex::new(None),
            scroll_pixel_remainder: Mutex::new(0.0),
            utf8_remainder: Mutex::new(Vec::new()),
            ime_marked_text: Mutex::new(None),
            ime_cursor_frozen: Mutex::new(None),
            ime_last_paint_cursor: Mutex::new((0, 0)),
            synchronized_output: AtomicBool::new(false),
            tmux_backed: false,
            resync_stop: Arc::new(AtomicBool::new(false)),
            output_dirty: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Create a new Terminal backed by tmux (control mode).
    /// Starts a background thread that applies resync data directly to the VTE
    /// grid when new output arrives (gated by `output_dirty` flag). This keeps
    /// ALL VTE processing off the main/paint thread, eliminating input lag.
    pub fn new_tmux(terminal_id: String, size: TerminalSize) -> Self {
        let mut t = Self::new(terminal_id, size);
        t.tmux_backed = true;

        // Start background resync thread — applies capture-pane data directly
        // to the VTE grid so paint() never touches the VTE processor.
        let term = t.term.clone();
        let processor = t.processor.clone();
        let render_dirty = t.dirty.clone();
        let stop = t.resync_stop.clone();
        let dirty = t.output_dirty.clone();
        let pane_id = t.terminal_id.clone();
        std::thread::Builder::new()
            .name(format!("resync-{}", pane_id))
            .spawn(move || {
                let mut cooldown: u8 = 0;
                while !stop.load(Ordering::Relaxed) {
                    let was_dirty = dirty.swap(false, Ordering::Relaxed);
                    if was_dirty || cooldown > 0 {
                        if let Some(data) =
                            crate::runtime::backends::tmux_control_mode::capture_pane_resync(
                                &pane_id,
                            )
                        {
                            // Apply resync to VTE grid — use try_lock to NEVER
                            // block the main thread. If paint() or process_output()
                            // holds the lock, skip this cycle and retry in 33ms.
                            if let Some(mut t) = term.try_lock() {
                                if let Some(mut p) = processor.try_lock() {
                                    p.advance(&mut *t, &data);
                                    render_dirty.store(true, Ordering::Relaxed);
                                }
                            }
                        }
                        if was_dirty {
                            cooldown = 3;
                        } else {
                            cooldown -= 1;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(33));
                    } else {
                        std::thread::sleep(std::time::Duration::from_millis(200));
                    }
                }
            })
            .ok();

        t
    }

    /// Returns true if this terminal is backed by tmux control mode.
    pub fn is_tmux_backed(&self) -> bool {
        self.tmux_backed
    }

    // Resync is now applied directly to the VTE grid by the background thread.
    // Paint just calls with_content() — zero VTE processing overhead.

    /// Feed PTY output bytes into the VTE parser.
    ///
    /// Buffers incomplete UTF-8 multi-byte sequences at chunk boundaries to prevent
    /// garbled character rendering. The VTE parser receives only complete UTF-8
    /// codepoints; any trailing incomplete sequence is held until the next call.
    pub fn process_output(&self, data: &[u8]) {
        // Detect CSI ?2026h/l (synchronized output) in the buffer.
        // In tmux mode these sequences are consumed by tmux and never arrive,
        // so this is effectively only active for local PTY mode.
        let sync_start = b"\x1b[?2026h";
        let sync_end = b"\x1b[?2026l";

        let mut found_start = false;
        let mut found_end = false;

        if data.len() >= sync_start.len() {
            for window in data.windows(sync_start.len()) {
                if window == sync_start {
                    found_start = true;
                }
                if window == sync_end {
                    found_end = true;
                }
            }
        }

        // If we see start but not end, we're entering a sync block.
        // If we see end (regardless of start), the sync block is done.
        if found_start && !found_end {
            self.synchronized_output.store(true, Ordering::Relaxed);
        }
        if found_end {
            self.synchronized_output.store(false, Ordering::Relaxed);
        }

        let mut remainder = self.utf8_remainder.lock();
        // Build the full buffer: previous remainder + new data
        let buf = if remainder.is_empty() {
            data.to_vec()
        } else {
            let mut combined = std::mem::take(&mut *remainder);
            combined.extend_from_slice(data);
            combined
        };
        if buf.is_empty() {
            return;
        }
        // Find the safe UTF-8 boundary (don't split multi-byte sequences)
        let safe_len = find_utf8_boundary(&buf);
        // Save any trailing incomplete sequence for next call
        if safe_len < buf.len() {
            *remainder = buf[safe_len..].to_vec();
        }
        if safe_len == 0 {
            // Only incomplete bytes — nothing to feed yet
            if remainder.is_empty() {
                *remainder = buf;
            }
            return;
        }
        let to_process = &buf[..safe_len];
        let mut term = self.term.lock();
        let was_alt = term.mode().contains(TermMode::ALT_SCREEN);
        let mut processor = self.processor.lock();
        processor.advance(&mut *term, to_process);

        // Give alt screen scrollback when a TUI app enters alternate screen mode.
        // alacritty_terminal creates the alt screen grid with 0 history by default,
        // so content that scrolls off the top is lost. Enable scrollback so users
        // can scroll up through TUI output (e.g. Claude Code analysis results).
        if !was_alt && term.mode().contains(TermMode::ALT_SCREEN) {
            term.grid_mut().update_history(10_000);
        }
        self.dirty.store(true, Ordering::Relaxed);
        // Signal the background resync thread that new data arrived.
        // It will call capture_pane_resync on its next ~33ms cycle.
        if self.tmux_backed {
            self.output_dirty.store(true, Ordering::Relaxed);
        }
    }

    /// Check and clear dirty flag. Invalidates search/link caches when dirty.
    pub fn take_dirty(&self) -> bool {
        let was_dirty = self.dirty.swap(false, Ordering::Relaxed);
        if was_dirty {
            *self.cached_links.lock() = None;
            *self.cached_search.lock() = None;
        }
        was_dirty
    }

    /// Read-only access to the terminal grid for rendering
    pub fn with_content<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&Term<TermEventProxy>) -> R,
    {
        let term = self.term.lock();
        f(&term)
    }

    // NOTE: maybe_resync_and_with_content() removed — resync is now applied
    // directly by the background thread. Paint uses with_content() only.

    /// Resize the terminal grid (does NOT resize PTY — caller must do that)
    pub fn resize(&self, new_size: TerminalSize) {
        *self.size.lock() = new_size;
        let mut term = self.term.lock();
        let term_size = TermSize::new(new_size.cols as usize, new_size.rows as usize);
        term.resize(term_size);
        // Mark dirty so link/search caches are invalidated (they reference old grid coords)
        self.dirty.store(true, Ordering::Relaxed);
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

    /// Returns true if the terminal is in alternate screen mode.
    pub fn is_alt_screen(&self) -> bool {
        self.mode().contains(TermMode::ALT_SCREEN)
    }

    /// Returns true during a CSI ?2026h synchronized-output block.
    pub fn is_synchronized_output(&self) -> bool {
        self.synchronized_output.load(Ordering::Relaxed)
    }

    /// Dump visible grid content as plain text (for debugging).
    /// Each line is terminated by '\n'. Trailing spaces per line are trimmed.
    pub fn dump_grid_text(&self) -> String {
        use alacritty_terminal::grid::Dimensions;
        use alacritty_terminal::index::{Column, Line, Point};
        let term = self.term.lock();
        let grid = term.grid();
        let rows = grid.screen_lines();
        let cols = grid.columns();
        let display_offset = grid.display_offset() as i32;
        let mut out = String::new();
        for row in 0..rows {
            let line = Line(row as i32 - display_offset);
            let mut line_text = String::new();
            for col in 0..cols {
                let point = Point::new(line, Column(col));
                let cell = &grid[point];
                let c = cell.c;
                if c == '\0' { line_text.push(' '); } else { line_text.push(c); }
            }
            let trimmed = line_text.trim_end();
            out.push_str(trimmed);
            out.push('\n');
        }
        out
    }

    /// Sync cursor visibility from an external source (e.g. tmux `cursor_flag`).
    /// Feeds a synthetic DECTCEM sequence through the VTE parser so that
    /// alacritty_terminal properly updates its internal mode flags.
    /// This is needed when reconnecting to an existing tmux session where the
    /// TUI app already hid the cursor before we started listening.
    pub fn sync_cursor_visibility(&self, visible: bool) {
        if visible {
            self.process_output(b"\x1b[?25h"); // DECTCEM show
        } else {
            self.process_output(b"\x1b[?25l"); // DECTCEM hide
        }
    }

    // ── IME state ─────────────────────────────────────────────────────

    /// Set IME marked (composing) text. Called from InputHandler.
    pub fn set_ime_marked_text(&self, text: Option<String>) {
        *self.ime_marked_text.lock() = text;
    }

    /// Get current IME marked text (for rendering and marked_text_range).
    pub fn ime_marked_text(&self) -> Option<String> {
        self.ime_marked_text.lock().clone()
    }

    /// Freeze cursor position (column, visual_line) when IME composition starts.
    /// This prevents terminal output from moving the preedit overlay mid-composition.
    pub fn set_ime_cursor_frozen(&self, pos: Option<(usize, i32)>) {
        *self.ime_cursor_frozen.lock() = pos;
    }

    /// Get frozen cursor position for IME preedit rendering.
    pub fn ime_cursor_frozen(&self) -> Option<(usize, i32)> {
        *self.ime_cursor_frozen.lock()
    }

    /// Save cursor position at paint time for later IME freezing.
    pub fn set_ime_last_paint_cursor(&self, pos: (usize, i32)) {
        *self.ime_last_paint_cursor.lock() = pos;
    }

    /// Get the cursor position from the most recent paint frame.
    pub fn ime_last_paint_cursor(&self) -> (usize, i32) {
        *self.ime_last_paint_cursor.lock()
    }

    /// Find the visual cursor drawn by TUI apps when SHOW_CURSOR is off.
    /// TUI apps (like Claude Code) hide the real cursor and draw their own
    /// using reverse-video cells. Returns the LAST INVERSE cell found
    /// (bottom-most, right-most), because when multiple TUI sessions are
    /// visible (e.g. exited Claude Code + active Cursor Agent), the active
    /// input cursor is always the last one rendered on screen.
    pub fn find_visual_cursor(&self) -> Option<(usize, i32)> {
        use alacritty_terminal::grid::Dimensions;
        use alacritty_terminal::index::{Column, Line, Point};
        use alacritty_terminal::term::cell::Flags;
        self.with_content(|term| {
            let grid = term.grid();
            let num_lines = grid.screen_lines();
            let num_cols = grid.columns();
            let display_offset = grid.display_offset() as i32;

            let mut last: Option<(usize, i32)> = None;
            for row in 0..num_lines {
                let line = Line(row as i32 - display_offset);
                for col in 0..num_cols {
                    let cell = &grid[Point { line, column: Column(col) }];
                    if cell.flags.contains(Flags::INVERSE) {
                        last = Some((col, row as i32));
                    }
                }
            }
            last
        })
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

    /// Get links using cache when content hasn't changed.
    pub fn detect_links_cached(&self) -> Vec<DetectedLink> {
        if !self.dirty.load(Ordering::Relaxed) {
            if let Some(ref cached) = *self.cached_links.lock() {
                return cached.clone();
            }
        }
        let links = self.detect_links();
        *self.cached_links.lock() = Some(links.clone());
        links
    }

    /// Get search results using cache when query and content unchanged.
    pub fn search_cached(&self, query: &str) -> Vec<SearchMatch> {
        if !self.dirty.load(Ordering::Relaxed) {
            if let Some((ref cached_q, ref cached_r)) = *self.cached_search.lock() {
                if cached_q == query {
                    return cached_r.clone();
                }
            }
        }
        let results = self.search(query);
        *self.cached_search.lock() = Some((query.to_string(), results.clone()));
        results
    }
}

impl Terminal {
    pub fn scroll_display(&self, delta: i32) {
        if delta == 0 {
            return;
        }
        let mut term = self.term.lock();
        term.scroll_display(Scroll::Delta(delta));
        self.dirty.store(true, Ordering::Relaxed);
    }

    /// Accumulate pixel scroll delta and return the whole-line delta to apply.
    /// Keeps sub-line remainder for next call, ensuring smooth trackpad scrolling.
    pub fn scroll_display_pixels(&self, pixels: f32, line_height: f32) -> i32 {
        if line_height <= 0.0 {
            return 0;
        }
        let mut remainder = self.scroll_pixel_remainder.lock();
        *remainder += pixels;
        let lines = (*remainder / line_height) as i32;
        if lines != 0 {
            *remainder -= lines as f32 * line_height;
            self.scroll_display(lines);
        }
        lines
    }

    pub fn scroll_to_bottom(&self) {
        let mut term = self.term.lock();
        term.scroll_display(Scroll::Bottom);
        *self.scroll_pixel_remainder.lock() = 0.0;
        self.dirty.store(true, Ordering::Relaxed);
    }

    pub fn display_offset(&self) -> usize {
        self.term.lock().grid().display_offset()
    }

    pub fn start_selection(
        &self,
        point: alacritty_terminal::index::Point,
        side: Side,
        ty: SelectionType,
    ) {
        let mut term = self.term.lock();
        term.selection = Some(Selection::new(ty, point, side));
    }

    pub fn update_selection(&self, point: alacritty_terminal::index::Point, side: Side) {
        let mut term = self.term.lock();
        if let Some(ref mut sel) = term.selection {
            sel.update(point, side);
        }
    }

    pub fn clear_selection(&self) {
        self.term.lock().selection = None;
    }

    pub fn has_selection(&self) -> bool {
        self.term.lock().selection.is_some()
    }

    pub fn selection_text(&self) -> Option<String> {
        self.term.lock().selection_to_string()
    }

    /// Extract the last `n` lines of visible text from the terminal screen.
    /// Used for agent status detection (pattern matching against on-screen content).
    pub fn screen_tail_text(&self, n: usize) -> String {
        self.with_content(|term| {
            use alacritty_terminal::grid::Dimensions;
            use alacritty_terminal::index::{Column, Line, Point};
            let grid = term.grid();
            let screen_lines = grid.screen_lines();
            let cols = grid.columns();
            let display_offset = grid.display_offset() as i32;
            let start_row = screen_lines.saturating_sub(n);
            let mut text = String::new();
            for row in start_row..screen_lines {
                let line = Line(row as i32 - display_offset);
                for col in 0..cols {
                    let cell = &grid[Point { line, column: Column(col) }];
                    text.push(cell.c);
                }
                text.push('\n');
            }
            text
        })
    }

    pub fn selection_range(&self) -> Option<SelectionRange> {
        let term = self.term.lock();
        term.selection.as_ref().and_then(|s| s.to_range(&term))
    }

    /// Select all content in the terminal (screen + scrollback).
    pub fn select_all(&self) {
        use alacritty_terminal::grid::Dimensions;
        use alacritty_terminal::index::{Column, Line, Point};
        let mut term = self.term.lock();
        let grid = term.grid();
        let total_lines = grid.total_lines();
        let cols = grid.columns();
        // Start at the top of scrollback, end at bottom-right of screen
        let start = Point {
            line: Line(-(total_lines as i32 - grid.screen_lines() as i32)),
            column: Column(0),
        };
        let end = Point {
            line: Line(grid.screen_lines() as i32 - 1),
            column: Column(cols.saturating_sub(1)),
        };
        let mut sel = Selection::new(SelectionType::Simple, start, Side::Left);
        sel.update(end, Side::Right);
        term.selection = Some(sel);
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        // Signal the background resync thread to stop
        self.resync_stop.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::index::{Column, Line, Point, Side};
    use alacritty_terminal::selection::SelectionType;

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

    #[test]
    fn test_scroll_display_up_down() {
        let term = Terminal::new(
            "scroll-1".into(),
            TerminalSize { cols: 80, rows: 24, cell_width: 8.0, cell_height: 16.0 },
        );
        let mut data = Vec::new();
        for i in 0..50 {
            data.extend_from_slice(format!("line {}\r\n", i).as_bytes());
        }
        term.process_output(&data);

        assert_eq!(term.display_offset(), 0);
        term.scroll_display(5);
        assert_eq!(term.display_offset(), 5);
        term.scroll_display(-3);
        assert_eq!(term.display_offset(), 2);
        term.scroll_to_bottom();
        assert_eq!(term.display_offset(), 0);
    }

    #[test]
    fn test_scroll_display_clamps() {
        let term = Terminal::new("scroll-2".into(), TerminalSize::default());
        term.scroll_display(100);
        assert_eq!(term.display_offset(), 0);
        term.scroll_display(-100);
        assert_eq!(term.display_offset(), 0);
    }

    #[test]
    fn test_dectcem_hide_show_cursor() {
        let term = Terminal::new("cursor-vis".into(), TerminalSize::default());
        // Initially cursor should be visible
        assert!(
            term.mode().contains(TermMode::SHOW_CURSOR),
            "cursor should be visible by default"
        );
        // Send DECTCEM hide cursor: ESC [ ? 25 l
        term.process_output(b"\x1b[?25l");
        assert!(
            !term.mode().contains(TermMode::SHOW_CURSOR),
            "cursor should be hidden after \\x1b[?25l"
        );
        // Send DECTCEM show cursor: ESC [ ? 25 h
        term.process_output(b"\x1b[?25h");
        assert!(
            term.mode().contains(TermMode::SHOW_CURSOR),
            "cursor should be visible after \\x1b[?25h"
        );
    }

    #[test]
    fn test_selection_basic() {
        let term = Terminal::new("sel-1".into(), TerminalSize::default());
        term.process_output(b"Hello World\r\n");

        assert!(!term.has_selection());
        term.start_selection(
            Point::new(Line(0), Column(0)),
            Side::Left,
            SelectionType::Simple,
        );
        assert!(term.has_selection());
        term.update_selection(Point::new(Line(0), Column(4)), Side::Right);
        let text = term.selection_text();
        assert!(text.is_some());
        assert_eq!(text.unwrap(), "Hello");

        term.clear_selection();
        assert!(!term.has_selection());
    }

    // --- UTF-8 boundary and split-chunk tests ---

    #[test]
    fn test_find_utf8_boundary_ascii_only() {
        assert_eq!(find_utf8_boundary(b"hello"), 5);
        assert_eq!(find_utf8_boundary(b""), 0);
    }

    #[test]
    fn test_find_utf8_boundary_complete_utf8() {
        // "编" = E7 BC 96
        assert_eq!(find_utf8_boundary(&[0xE7, 0xBC, 0x96]), 3);
        // "编a" = E7 BC 96 61
        assert_eq!(find_utf8_boundary(&[0xE7, 0xBC, 0x96, 0x61]), 4);
    }

    #[test]
    fn test_find_utf8_boundary_incomplete_2byte() {
        // 2-byte lead (C3) without continuation
        assert_eq!(find_utf8_boundary(&[0x61, 0xC3]), 1);
    }

    #[test]
    fn test_find_utf8_boundary_incomplete_3byte() {
        // 3-byte lead (E7) + 1 continuation — needs 2
        assert_eq!(find_utf8_boundary(&[0x61, 0xE7, 0xBC]), 1);
        // 3-byte lead (E7) alone
        assert_eq!(find_utf8_boundary(&[0x61, 0xE7]), 1);
    }

    #[test]
    fn test_find_utf8_boundary_incomplete_4byte() {
        // 4-byte lead (F0) + 2 continuations — needs 3
        assert_eq!(find_utf8_boundary(&[0xF0, 0x9F, 0x98]), 0);
        // After ASCII: split before F0
        assert_eq!(find_utf8_boundary(&[0x61, 0xF0, 0x9F, 0x98]), 1);
    }

    #[test]
    fn test_process_output_split_utf8_renders_correctly() {
        let term = Terminal::new("utf8-split".into(), TerminalSize::default());
        // "编" = E7 BC 96 — split across two chunks
        term.process_output(&[0xE7, 0xBC]); // incomplete
        term.process_output(&[0x96]);        // completes "编"

        let text = term.screen_tail_text(1);
        assert!(
            text.contains('编'),
            "expected '编' in screen output, got: {:?}",
            text.trim()
        );
    }

    #[test]
    fn test_process_output_split_utf8_3way() {
        let term = Terminal::new("utf8-3way".into(), TerminalSize::default());
        // "编" = E7 BC 96 — each byte in a separate chunk
        term.process_output(&[0xE7]);
        term.process_output(&[0xBC]);
        term.process_output(&[0x96]);

        let text = term.screen_tail_text(1);
        assert!(
            text.contains('编'),
            "expected '编' in screen output after 3-way split, got: {:?}",
            text.trim()
        );
    }

    #[test]
    fn test_dump_grid_text_with_sgr_sequences() {
        // Simulate Claude Code-like output with bold, colors, and special chars
        let term = Terminal::new("grid-dump".into(), TerminalSize {
            cols: 80, rows: 24, cell_width: 7.0, cell_height: 14.0,
        });
        // ESC[1m = bold, ESC[0m = reset, ESC[38;2;R;G;Bm = 24-bit fg color
        let data = b"\x1b[38;2;255;255;255m\xe2\x8f\xba\x1b[39m \x1b[1mRead\x1b[0m(packages/app/src/stores/knowledge.ts)\r\n";
        term.process_output(data);
        let grid = term.dump_grid_text();
        let first_line = grid.lines().next().unwrap_or("");
        eprintln!("GRID LINE: {:?}", first_line);
        assert!(
            first_line.contains("Read(packages/app/src/stores/knowledge.ts)"),
            "Grid should contain 'Read(packages/app/src/stores/knowledge.ts)' but got: {:?}",
            first_line,
        );
    }

    #[test]
    fn test_dump_grid_text_with_horizontal_lines() {
        let term = Terminal::new("hlines".into(), TerminalSize {
            cols: 80, rows: 24, cell_width: 7.0, cell_height: 14.0,
        });
        // 20 horizontal box drawing characters
        let data = "────────────────────\r\n".as_bytes();
        term.process_output(data);
        let grid = term.dump_grid_text();
        let first_line = grid.lines().next().unwrap_or("");
        eprintln!("GRID HLINE: {:?}", first_line);
        // Each ─ is 1 column wide; should have 20 of them
        let dash_count = first_line.chars().filter(|&c| c == '─').count();
        assert_eq!(dash_count, 20, "Expected 20 horizontal lines, got {}: {:?}", dash_count, first_line);
    }

    #[test]
    fn test_dump_grid_text_alt_screen_with_history() {
        let term = Terminal::new("alt-hist".into(), TerminalSize {
            cols: 80, rows: 5, cell_width: 7.0, cell_height: 14.0,
        });
        // Enter alt screen
        term.process_output(b"\x1b[?1049h");
        // Write some content
        term.process_output(b"Line1\r\nLine2\r\nLine3\r\nLine4\r\nLine5");
        let grid = term.dump_grid_text();
        eprintln!("ALT GRID:\n{}", grid);
        assert!(grid.contains("Line1"), "Grid should contain Line1: {:?}", grid);
        assert!(grid.contains("Line5"), "Grid should contain Line5: {:?}", grid);
    }

    #[test]
    fn test_process_output_mixed_ascii_and_split_utf8() {
        let term = Terminal::new("utf8-mixed".into(), TerminalSize::default());
        // "hi编码" split: "hi" + E7 BC in chunk 1, then 96 + E7 A0 81 in chunk 2
        let mut chunk1 = b"hi".to_vec();
        chunk1.extend_from_slice(&[0xE7, 0xBC]); // incomplete "编"
        term.process_output(&chunk1);

        let mut chunk2 = vec![0x96]; // completes "编"
        chunk2.extend_from_slice(&[0xE7, 0xA0, 0x81]); // complete "码" (E7 A0 81)
        term.process_output(&chunk2);

        let text = term.screen_tail_text(1);
        assert!(
            text.contains("hi编码"),
            "expected 'hi编码' in output, got: {:?}",
            text.trim()
        );
    }
}
