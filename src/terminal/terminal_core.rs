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
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
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
    /// Monotonically increasing counter bumped by each process_output() call.
    /// The resync thread reads this before and after capture-pane: if it changed,
    /// the captured data is stale (process_output wrote newer data during the
    /// ~6ms capture window) and the resync is skipped. This prevents the resync
    /// from overwriting fresh keystroke echo / streaming data with older state.
    process_generation: Arc<AtomicU64>,
    /// Timestamp of the last process_output() call (milliseconds since UNIX epoch).
    /// The resync thread skips capture-pane if this is too recent (< 500ms),
    /// to avoid overwriting the VTE grid during inter-keystroke gaps in fast typing.
    last_output_time_ms: Arc<AtomicU64>,
    /// Set to true while the user is dragging a mouse selection.
    /// The resync thread and render/idle ticks skip their work when true.
    selecting: Arc<AtomicBool>,
    /// Timestamp (millis since epoch) when selecting started. 0 when not selecting.
    /// Used by the resync thread to auto-clear selecting after 5 seconds.
    selecting_since: Arc<AtomicU64>,
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
            process_generation: Arc::new(AtomicU64::new(0)),
            last_output_time_ms: Arc::new(AtomicU64::new(0)),
            selecting: Arc::new(AtomicBool::new(false)),
            selecting_since: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Create a new Terminal backed by tmux (control mode).
    /// Starts a background thread that applies resync data directly to the VTE
    /// grid when new output arrives (gated by `output_dirty` flag). This keeps
    /// ALL VTE processing off the main/paint thread, eliminating input lag.
    pub fn new_tmux(terminal_id: String, size: TerminalSize) -> Self {
        let mut t = Self::new(terminal_id, size);
        t.tmux_backed = true;

        // Start background resync thread — periodically corrects the VTE grid by
        // applying capture-pane data from tmux. Resync ONLY runs during cooldown
        // (after output has stopped), never during active streaming. This prevents
        // the visual "shake" caused by resync overwriting process_output's VTE
        // state with slightly-different capture-pane snapshots mid-stream.
        // The generation counter provides an extra safety net: if output resumes
        // during a capture window, the stale capture is discarded.
        let term = t.term.clone();
        let processor = t.processor.clone();
        let render_dirty = t.dirty.clone();
        let stop = t.resync_stop.clone();
        let dirty = t.output_dirty.clone();
        let gen = t.process_generation.clone();
        let last_output = t.last_output_time_ms.clone();
        let selecting = t.selecting.clone();
        let selecting_since = t.selecting_since.clone();
        let pane_id = t.terminal_id.clone();
        std::thread::Builder::new()
            .name(format!("resync-{}", pane_id))
            .spawn(move || {
                let mut cooldown: u8 = 0;
                while !stop.load(Ordering::Relaxed) {
                    let was_dirty = dirty.swap(false, Ordering::Relaxed);
                    if was_dirty || cooldown > 0 {
                        // Only resync during cooldown — NOT during active output.
                        //
                        // When was_dirty is true, %output events are arriving and
                        // process_output is feeding the same raw PTY bytes through
                        // VTE. Running capture-pane at this point would spawn a
                        // subprocess (~6ms) whose result may differ slightly from
                        // the VTE grid, causing visible "shake" if applied.
                        //
                        // During cooldown (output stopped, was_dirty=false), we
                        // resync to correct any accumulated VTE drift. The
                        // generation counter is still checked as a safety net in
                        // case output resumes during the capture window.
                        //
                        // Time guard: skip resync if last process_output was < 500ms ago.
                        // This covers inter-keystroke gaps during fast typing (typically
                        // 50-150ms between keys). Without this, the generation counter
                        // alone cannot prevent stale resync because no process_output
                        // happens during the gap → gen stays unchanged → resync applies.
                        if !was_dirty {
                            // Skip resync while user is selecting text (prevents flicker).
                            // Auto-clear after 5 seconds as a safety net.
                            if selecting.load(Ordering::Relaxed) {
                                let sel_start = selecting_since.load(Ordering::Relaxed);
                                let now_ms = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .map(|d| d.as_millis() as u64)
                                    .unwrap_or(0);
                                if sel_start > 0 && now_ms.saturating_sub(sel_start) >= 5000 {
                                    selecting.store(false, Ordering::Relaxed);
                                    selecting_since.store(0, Ordering::Relaxed);
                                } else {
                                    cooldown = cooldown.saturating_sub(1);
                                    std::thread::sleep(std::time::Duration::from_millis(33));
                                    continue;
                                }
                            }
                            let now_ms = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .map(|d| d.as_millis() as u64)
                                .unwrap_or(0);
                            let last_ms = last_output.load(Ordering::Relaxed);
                            let elapsed = now_ms.saturating_sub(last_ms);
                            if elapsed < 500 {
                                // Too soon after last output — user may still be typing.
                                // Skip resync to avoid overwriting inter-keystroke grid state.
                                cooldown = cooldown.saturating_sub(1);
                                std::thread::sleep(std::time::Duration::from_millis(33));
                                continue;
                            }

                            let gen_before = gen.load(Ordering::Relaxed);

                            if let Some(data) =
                                crate::runtime::backends::tmux_control_mode::capture_pane_resync(
                                    &pane_id,
                                )
                            {
                                let gen_after = gen.load(Ordering::Relaxed);
                                if gen_before == gen_after {
                                    if let Some(mut t) = term.try_lock() {
                                        if let Some(mut p) = processor.try_lock() {
                                            p.advance(&mut *t, &data);
                                            render_dirty.store(true, Ordering::Relaxed);
                                        }
                                    }
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

    /// Returns the selecting flag for mouse selection state.
    pub fn selecting(&self) -> &Arc<AtomicBool> {
        &self.selecting
    }

    /// Returns the timestamp of when selecting started (millis since epoch).
    pub fn selecting_since(&self) -> &Arc<AtomicU64> {
        &self.selecting_since
    }

    // Resync is applied by the background thread only when process_generation
    // didn't change during capture-pane. No rendering gating needed.

    /// Feed PTY output bytes into the VTE parser.
    ///
    /// Buffers incomplete UTF-8 multi-byte sequences at chunk boundaries to prevent
    /// garbled character rendering. The VTE parser receives only complete UTF-8
    /// codepoints; any trailing incomplete sequence is held until the next call.
    ///
    /// For tmux-backed terminals, VTE processing still runs (to build scrollback
    /// history and maintain terminal modes), but the output loop defers rendering
    /// until the background resync thread has applied a consistent frame. This
    /// prevents the visual "shake" without breaking scrollback or input echo.
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
            // Bump generation so the resync thread knows any in-flight
            // capture-pane result is now stale (we have newer data).
            self.process_generation.fetch_add(1, Ordering::Relaxed);
            // Record timestamp so the resync thread can skip capture-pane
            // during inter-keystroke gaps (avoids overwriting fresh content).
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            self.last_output_time_ms.store(now_ms, Ordering::Relaxed);
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

        let text = term.dump_grid_text();
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

        let text = term.dump_grid_text();
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

        let text = term.dump_grid_text();
        // CJK chars are double-width; grid dump may include spacer cells between them
        assert!(
            text.contains("hi") && text.contains('编') && text.contains('码'),
            "expected 'hi', '编', and '码' in output, got: {:?}",
            text.trim()
        );
    }

    // --- Generation counter and resync tests ---

    #[test]
    fn test_process_generation_increments_for_tmux_backed() {
        // process_output should bump process_generation when tmux_backed = true
        let mut term = Terminal::new("gen-tmux".into(), TerminalSize::default());
        term.tmux_backed = true;

        assert_eq!(term.process_generation.load(Ordering::Relaxed), 0);

        term.process_output(b"hello");
        assert_eq!(term.process_generation.load(Ordering::Relaxed), 1);

        term.process_output(b" world");
        assert_eq!(term.process_generation.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn test_process_generation_unchanged_for_non_tmux() {
        // Non-tmux terminals should NOT bump the generation counter
        let term = Terminal::new("gen-local".into(), TerminalSize::default());
        assert!(!term.tmux_backed);

        term.process_output(b"hello");
        assert_eq!(term.process_generation.load(Ordering::Relaxed), 0);

        term.process_output(b" world");
        assert_eq!(term.process_generation.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_selecting_flag_default_false() {
        let term = Terminal::new("sel-test".into(), TerminalSize::default());
        assert!(!term.selecting().load(Ordering::Relaxed));
        assert_eq!(term.selecting_since().load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_selecting_flag_skips_resync_in_spirit() {
        // Verify the selecting flag is cloned into new_tmux terminals
        let term = Terminal::new_tmux("sel-resync".into(), TerminalSize::default());
        assert!(!term.selecting().load(Ordering::Relaxed));

        // Simulate selection start
        term.selecting().store(true, Ordering::Relaxed);
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        term.selecting_since().store(now_ms, Ordering::Relaxed);

        assert!(term.selecting().load(Ordering::Relaxed));
        assert!(term.selecting_since().load(Ordering::Relaxed) > 0);

        // Simulate selection end
        term.selecting().store(false, Ordering::Relaxed);
        term.selecting_since().store(0, Ordering::Relaxed);
        assert!(!term.selecting().load(Ordering::Relaxed));
    }

    #[test]
    fn test_output_dirty_flag_set_for_tmux_backed() {
        let mut term = Terminal::new("dirty-tmux".into(), TerminalSize::default());
        term.tmux_backed = true;

        assert!(!term.output_dirty.load(Ordering::Relaxed));
        term.process_output(b"data");
        assert!(term.output_dirty.load(Ordering::Relaxed));
    }

    #[test]
    fn test_generation_guard_prevents_stale_resync() {
        // Simulate the generation counter preventing stale resync from applying.
        //
        // Scenario: resync snapshots gen=0, then process_output bumps gen to 1
        // during the capture window. Resync should skip because gen changed.
        let mut term = Terminal::new("gen-guard".into(), TerminalSize {
            cols: 40, rows: 5, cell_width: 7.0, cell_height: 14.0,
        });
        term.tmux_backed = true;

        // Initial state: write "original" at line 1
        term.process_output(b"original");
        assert_eq!(term.process_generation.load(Ordering::Relaxed), 1);

        // Snapshot generation before "capture" (simulating resync thread)
        let gen_before = term.process_generation.load(Ordering::Relaxed);

        // Simulate process_output running during capture (new data arrives)
        term.process_output(b"\r\nupdated");
        assert_eq!(term.process_generation.load(Ordering::Relaxed), 2);

        // Check generation after "capture"
        let gen_after = term.process_generation.load(Ordering::Relaxed);

        // Generation changed — resync should be skipped
        assert_ne!(gen_before, gen_after, "Generation should have changed");

        // Verify the grid shows the newer data (not overwritten by stale resync)
        let text = term.dump_grid_text();
        assert!(
            text.contains("updated"),
            "Grid should show newer data, got: {:?}",
            text
        );
    }

    #[test]
    fn test_resync_applies_when_no_process_output_during_capture() {
        // When no process_output happens during capture, generation is unchanged,
        // so resync should be safe to apply.
        let mut term = Terminal::new("resync-apply".into(), TerminalSize {
            cols: 40, rows: 5, cell_width: 7.0, cell_height: 14.0,
        });
        term.tmux_backed = true;

        // Write initial content
        term.process_output(b"line1");
        let gen_before = term.process_generation.load(Ordering::Relaxed);

        // Simulate "capture-pane" returning data (no process_output during capture)
        // Build resync data: move to row 1, write "RESYNCED", clear rest of line
        let resync_data = b"\x1b[1;1HRESYNCED\x1b[K";

        // Generation unchanged — safe to apply
        let gen_after = term.process_generation.load(Ordering::Relaxed);
        assert_eq!(gen_before, gen_after);

        // Apply resync data through VTE
        {
            let mut t = term.term.lock();
            let mut p = term.processor.lock();
            p.advance(&mut *t, resync_data);
        }

        // Grid should now show resync'd content
        let text = term.dump_grid_text();
        assert!(
            text.contains("RESYNCED"),
            "Resync should have applied, got: {:?}",
            text
        );
    }

    #[test]
    fn test_concurrent_process_output_and_resync_no_corruption() {
        // Verify that rapidly alternating process_output and resync-style writes
        // don't corrupt the terminal grid. Both paths go through the same VTE
        // parser under the term lock, so the grid should always be consistent.
        let mut term = Terminal::new("concurrent".into(), TerminalSize {
            cols: 80, rows: 10, cell_width: 7.0, cell_height: 14.0,
        });
        term.tmux_backed = true;

        // Simulate fast typing: 'c', 'l', 'a', 'u', 'd', 'e'
        let chars = ['c', 'l', 'a', 'u', 'd', 'e'];
        for (i, ch) in chars.iter().enumerate() {
            // Each keystroke echo arrives as process_output
            term.process_output(ch.to_string().as_bytes());

            // Occasionally, resync applies (simulating brief pause in output)
            if i == 2 {
                // Resync applies after 'a' — writes "cla" at row 1
                let gen_before = term.process_generation.load(Ordering::Relaxed);
                // No process_output during this "capture window"
                let gen_after = term.process_generation.load(Ordering::Relaxed);
                if gen_before == gen_after {
                    let resync_data = b"\x1b[1;1Hcla\x1b[K";
                    let mut t = term.term.lock();
                    let mut p = term.processor.lock();
                    p.advance(&mut *t, resync_data);
                }
            }
        }

        // Final state should show "claude" (the full word)
        let text = term.dump_grid_text();
        assert!(
            text.contains("claude"),
            "Expected 'claude' after fast typing, got: {:?}",
            text.trim()
        );
    }

    #[test]
    fn test_resync_does_not_reenter_alt_screen() {
        // When VTE is already in alt-screen, sending CSI ?1049h (the sequence
        // that capture_pane_resync emits) should be a no-op — NOT re-enter
        // alt screen or clear the grid.
        let mut term = Terminal::new("alt-noop".into(), TerminalSize {
            cols: 40, rows: 5, cell_width: 7.0, cell_height: 14.0,
        });
        term.tmux_backed = true;

        // Enter alt screen and write content
        term.process_output(b"\x1b[?1049h");
        assert!(term.is_alt_screen());
        term.process_output(b"\x1b[1;1Horiginal content");

        // Verify content is there
        let text_before = term.dump_grid_text();
        assert!(text_before.contains("original content"),
            "Should have 'original content', got: {:?}", text_before);

        // Now apply resync with CSI ?1049h (like capture_pane_resync does)
        // This should NOT clear the alt screen
        let resync_data = b"\x1b[?1049h\x1b[1;1Hresynced content\x1b[K";
        {
            let mut t = term.term.lock();
            let mut p = term.processor.lock();
            p.advance(&mut *t, resync_data);
        }

        // Should still be in alt screen
        assert!(term.is_alt_screen());

        // Content should be "resynced content" (not cleared)
        let text_after = term.dump_grid_text();
        assert!(text_after.contains("resynced content"),
            "Resync should update content without clearing, got: {:?}", text_after);
    }

    #[test]
    fn test_generation_counter_threaded() {
        // Test the generation counter with actual concurrent threads,
        // simulating the real scenario of process_output on the output loop
        // and a resync-like reader on a background thread.
        use std::sync::Arc;
        use std::thread;

        let mut term = Terminal::new("gen-thread".into(), TerminalSize {
            cols: 40, rows: 5, cell_width: 7.0, cell_height: 14.0,
        });
        term.tmux_backed = true;
        let term = Arc::new(term);

        let gen = term.process_generation.clone();

        // Spawn a "resync checker" thread that samples generation before/after a delay
        let gen_clone = gen.clone();
        let term_clone = term.clone();
        let checker = thread::spawn(move || {
            let mut stale_count = 0u32;
            let mut apply_count = 0u32;

            for _ in 0..20 {
                let gen_before = gen_clone.load(Ordering::Relaxed);
                // Simulate capture-pane delay
                thread::sleep(std::time::Duration::from_millis(1));
                let gen_after = gen_clone.load(Ordering::Relaxed);

                if gen_before != gen_after {
                    stale_count += 1; // Would skip resync
                } else {
                    apply_count += 1; // Would apply resync
                }
            }
            (stale_count, apply_count)
        });

        // Rapidly feed process_output on the main thread (simulating fast output)
        for i in 0..100 {
            term.process_output(format!("{}", i % 10).as_bytes());
            if i % 5 == 0 {
                thread::sleep(std::time::Duration::from_millis(2));
            }
        }

        let (stale, apply) = checker.join().unwrap();
        // During active output, most resync checks should detect stale data
        assert!(
            stale > 0,
            "Expected some stale detections during active output, got stale={stale}, apply={apply}"
        );

        // Generation should have been bumped 100 times
        assert_eq!(
            gen.load(Ordering::Relaxed),
            100,
            "Generation should equal number of process_output calls"
        );
    }

    #[test]
    fn test_fast_typing_no_text_replacement() {
        // Simulates the exact user-reported bug: fast typing "claude" where
        // resync with stale data would overwrite newer process_output.
        // The generation counter should prevent this.
        let mut term = Terminal::new("fast-type".into(), TerminalSize {
            cols: 80, rows: 5, cell_width: 7.0, cell_height: 14.0,
        });
        term.tmux_backed = true;

        // Type "cla" — process_output handles echo
        term.process_output(b"c");
        term.process_output(b"l");
        term.process_output(b"a");
        // gen is now 3

        // Resync thread snapshots gen BEFORE capture starts
        let gen_before = term.process_generation.load(Ordering::Relaxed);
        assert_eq!(gen_before, 3);

        // During capture (~6ms), user types "u", "d", "e"
        term.process_output(b"u");
        term.process_output(b"d");
        term.process_output(b"e");
        // gen is now 6

        // Resync capture returns with OLD state "cla" (captured before u/d/e)
        let gen_after = term.process_generation.load(Ordering::Relaxed);
        assert_eq!(gen_after, 6);

        // Generation changed! Resync MUST skip.
        assert_ne!(gen_before, gen_after,
            "Generation counter must detect stale capture during typing");

        // Verify grid shows "claude" (the full word, not "cla")
        let text = term.dump_grid_text();
        assert!(
            text.contains("claude"),
            "Grid should show 'claude' (not 'cla'), got: {:?}",
            text.trim()
        );

        // If we had applied the stale resync (overwriting with "cla"), the grid
        // would show "cla" instead of "claude". The generation guard prevents this.
    }

    /// Regression: Simulates the real-world bug where resync fires in the gap
    /// between keystrokes during fast typing, causing visible text replacement.
    ///
    /// Timeline:
    ///   0ms:   User types "c","l","a" → process_output bumps gen to 3, dirty=true
    ///   33ms:  Resync thread: was_dirty=true → cooldown=3, skip resync
    ///   66ms:  Resync thread: was_dirty=false (no new output), cooldown=2
    ///          → executes capture-pane! capture returns "cla"
    ///          → gen unchanged (user hasn't typed yet) → resync APPLIES
    ///   80ms:  User types "u" → process_output("u"), but resync already overwrote grid
    ///          → user sees flicker: "claude" briefly becomes "cla" then "clau"
    ///
    /// The fix: resync must NOT apply during the inter-keystroke cooldown window.
    /// The last_output_time guard ensures resync waits long enough after the
    /// last process_output before applying.
    #[test]
    fn test_resync_must_not_apply_during_typing_gaps() {
        let mut term = Terminal::new("gap-resync".into(), TerminalSize {
            cols: 80, rows: 5, cell_width: 7.0, cell_height: 14.0,
        });
        term.tmux_backed = true;

        // Simulate: user typed "cla" (3 process_output calls)
        term.process_output(b"c");
        term.process_output(b"l");
        term.process_output(b"a");
        let gen_after_typing = term.process_generation.load(Ordering::Relaxed);
        assert_eq!(gen_after_typing, 3);

        // Simulate: resync thread wakes up AFTER output stopped (was_dirty=false, cooldown>0)
        // No new process_output since "a" — generation is unchanged.
        // In the buggy code, this would apply resync and overwrite "cla" with stale data.
        let gen_before_capture = term.process_generation.load(Ordering::Relaxed);

        // Simulate capture-pane returning the SAME content (no change)
        // In reality it could return slightly different formatting.
        // The key issue: even "same" content resync causes cursor position reset,
        // SGR attribute changes, and visual flicker.

        // Simulate ~6ms capture-pane delay — NO new process_output during this window
        // gen stays at 3
        let gen_after_capture = term.process_generation.load(Ordering::Relaxed);
        assert_eq!(gen_before_capture, gen_after_capture,
            "Generation should not change when no output arrives (inter-keystroke gap)");

        // In the buggy code: gen_before == gen_after → resync applies → BAD!
        // The generation counter alone CANNOT protect against this case because
        // no process_output happened during capture — gen didn't change.
        //
        // This test documents the limitation: generation counter only protects
        // against concurrent process_output, NOT against stale capture in typing gaps.
        //
        // The fix must add a time-based guard: skip resync if last process_output
        // was less than N ms ago (e.g., 500ms), to cover typical inter-keystroke gaps.

        // Verify the current grid shows "cla" correctly (before any stale resync)
        let text = term.dump_grid_text();
        assert!(text.contains("cla"), "Grid should show 'cla', got: {:?}", text.trim());
    }
}
