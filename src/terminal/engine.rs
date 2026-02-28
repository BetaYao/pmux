// terminal/engine.rs - Terminal Engine core: Term + VTE Processor + byte stream
use crate::shell_integration::{Osc133Parser, ShellMarker, ShellPhase, ShellState};
use alacritty_terminal::event::VoidListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::term::cell::Cell;
use alacritty_terminal::term::TermMode;
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::Processor;
use std::sync::{Arc, Mutex};

/// Fixed terminal dimensions for engine display.
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

/// Terminal Engine: owns alacritty_terminal::Term, VTE Processor, and byte stream receiver.
pub struct TerminalEngine {
    terminal: Arc<Mutex<Term<VoidListener>>>,
    processor: Arc<Mutex<Processor>>,
    osc133_parser: Arc<Mutex<Osc133Parser>>,
    byte_rx: flume::Receiver<Vec<u8>>,
    shell_state: Arc<Mutex<ShellState>>,
}

impl TerminalEngine {
    /// Create a new TerminalEngine with the given dimensions and byte stream receiver.
    pub fn new(
        columns: usize,
        screen_lines: usize,
        byte_rx: flume::Receiver<Vec<u8>>,
    ) -> Self {
        let size = TermDimensions {
            columns,
            screen_lines,
        };
        let term = Term::new(Config::default(), &size, VoidListener);
        Self {
            terminal: Arc::new(Mutex::new(term)),
            processor: Arc::new(Mutex::new(Processor::new())),
            osc133_parser: Arc::new(Mutex::new(Osc133Parser::new())),
            byte_rx,
            shell_state: Arc::new(Mutex::new(ShellState::new())),
        }
    }

    /// Process all pending bytes from the PTY channel.
    /// Drains byte_rx with try_recv() until empty, advancing each chunk through the VTE processor.
    pub fn advance_bytes(&self) {
        let mut term = self.terminal.lock().unwrap();
        let mut processor = self.processor.lock().unwrap();
        while let Ok(bytes) = self.byte_rx.try_recv() {
            processor.advance(&mut *term, &bytes);
        }
    }

    /// Process bytes through Osc133Parser and VTE, updating shell markers and phase.
    /// Processes byte-by-byte so cursor position is correct when each OSC 133 marker is received.
    pub fn advance_with_osc133(&self) {
        let mut term = self.terminal.lock().unwrap();
        let mut processor = self.processor.lock().unwrap();
        let mut parser = self.osc133_parser.lock().unwrap();
        let mut markers_to_add: Vec<ShellMarker> = Vec::new();

        while let Ok(bytes) = self.byte_rx.try_recv() {
            for b in bytes {
                processor.advance(&mut *term, &[b]);
                let parsed = parser.feed(&[b]);
                for p in parsed {
                    let cursor = term.grid().cursor.point;
                    let line = cursor.line.0.max(0) as usize;
                    let col = cursor.column.0.max(0) as usize;
                    markers_to_add.push(ShellMarker::from_parsed(p, line, col));
                }
            }
        }

        drop(term);
        drop(processor);
        drop(parser);

        if !markers_to_add.is_empty() {
            let mut shell_state = self.shell_state.lock().unwrap();
            for m in markers_to_add {
                shell_state.add_marker(m);
            }
        }
    }

    /// Get a lock guard to the terminal for rendering.
    pub fn terminal(&self) -> std::sync::MutexGuard<'_, Term<VoidListener>> {
        self.terminal.lock().unwrap()
    }

    /// Resize the terminal to new dimensions.
    /// PTY resize should be handled separately by the runtime.
    /// Adjusts marker line numbers and removes markers that scroll off the buffer.
    pub fn resize(&self, columns: usize, screen_lines: usize) {
        let size = TermDimensions {
            columns,
            screen_lines,
        };
        if let Ok(mut term) = self.terminal.lock() {
            term.resize(size);
            drop(term);
            // Update markers: remove those with line >= new screen_lines (scrolled off)
            if let Ok(mut shell_state) = self.shell_state.lock() {
                shell_state.markers.retain(|m| m.line < screen_lines);
            }
        }
    }

    /// Get renderable content for frame loop rendering.
    /// Calls `f` with (content, display_iter, screen_lines). Use the iterator for cells; content provides colors and cursor.
    pub fn with_renderable_content<F, R>(&self, f: F) -> R
    where
        F: FnOnce(
            &alacritty_terminal::term::RenderableContent<'_>,
            alacritty_terminal::grid::GridIterator<'_, Cell>,
            usize,
        ) -> R,
    {
        let term = self.terminal.lock().unwrap();
        let content = term.renderable_content();
        let display_iter = term.grid().display_iter();
        let screen_lines = term.grid().screen_lines();
        f(&content, display_iter, screen_lines)
    }

    /// Returns true when a TUI app (vim, neovim, Claude Code, etc.) is active.
    pub fn is_tui_active(&self) -> bool {
        let term = self.terminal.lock().unwrap();
        term.mode().contains(TermMode::ALT_SCREEN)
    }

    /// Current shell phase from OSC 133 markers (Unknown if not available).
    pub fn shell_phase(&self) -> ShellPhase {
        self.shell_state.lock().unwrap().phase
    }

    /// Line number of current prompt (if known from OSC 133).
    ///
    /// Returns `Some(line)` when a PromptStart marker has been received; the line is the grid row
    /// where the prompt was emitted. Returns `None` when no prompt marker is available.
    pub fn prompt_line(&self) -> Option<usize> {
        self.shell_state.lock().unwrap().prompt_line
    }

    /// Cursor position for clicking at a specific column in the prompt line.
    ///
    /// Returns `Some((line, col))` when a prompt line is known, suitable for cursor positioning
    /// (e.g., sending input to tmux to move the cursor). Returns `None` when no PromptStart marker
    /// has been received.
    ///
    /// # Example
    ///
    /// When the user Alt+Clicks at column 5 of the prompt line:
    /// ```ignore
    /// if let Some((line, col)) = engine.click_to_prompt(5) {
    ///     // Send cursor positioning to tmux at (line, col)
    /// }
    /// ```
    pub fn click_to_prompt(&self, col: usize) -> Option<(usize, usize)> {
        self.shell_state
            .lock()
            .unwrap()
            .prompt_line
            .map(|line| (line, col))
    }

    /// Exit code from last PostExec marker (OSC 133;D;N). None if not yet seen.
    pub fn last_post_exec_exit_code(&self) -> Option<u8> {
        self.shell_state.lock().unwrap().last_post_exec_exit_code
    }

    /// Markers whose grid line falls within the visible viewport [start_line, end_line).
    /// start_line and end_line are viewport row indices (0 = top of visible area).
    pub fn visible_markers(&self, start_line: usize, end_line: usize) -> Vec<ShellMarker> {
        let term = self.terminal.lock().unwrap();
        let grid = term.grid();
        let mut grid_lines_by_viewport: Vec<i32> = Vec::new();
        let mut current_line = i32::MIN;
        for indexed in grid.display_iter() {
            if indexed.point.line.0 != current_line {
                if current_line != i32::MIN {
                    grid_lines_by_viewport.push(current_line);
                }
                current_line = indexed.point.line.0;
            }
        }
        if current_line != i32::MIN {
            grid_lines_by_viewport.push(current_line);
        }
        drop(term);

        let grid_lines_in_range: std::collections::HashSet<i32> = grid_lines_by_viewport
            .into_iter()
            .enumerate()
            .filter(|(vp, _)| *vp >= start_line && *vp < end_line)
            .map(|(_, gl)| gl)
            .collect();

        let shell_state = self.shell_state.lock().unwrap();
        shell_state
            .markers
            .iter()
            .filter(|m| grid_lines_in_range.contains(&(m.line as i32)))
            .cloned()
            .collect()
    }

    /// Access shell state for OSC 133 marker processing (e.g. advance_with_osc133).
    pub fn shell_state(&self) -> std::sync::MutexGuard<'_, ShellState> {
        self.shell_state.lock().unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell_integration::ShellPhase;
    use alacritty_terminal::term::cell::Flags;

    fn osc133_st(kind: char, exit_code: Option<u8>) -> Vec<u8> {
        let mut s = format!("\x1b]133;{}", kind);
        if let Some(code) = exit_code {
            s.push_str(&format!(";{}", code));
        }
        s.push_str("\x1b\\");
        s.into_bytes()
    }

    #[test]
    fn test_engine_shell_state_initialized() {
        let (tx, rx) = flume::unbounded();
        let engine = TerminalEngine::new(80, 24, rx);
        drop(tx);
        assert_eq!(engine.shell_phase(), ShellPhase::Unknown);
        assert_eq!(engine.prompt_line(), None);
        assert_eq!(engine.last_post_exec_exit_code(), None);
        assert!(engine.shell_state().markers.is_empty());
    }

    #[test]
    fn test_engine_creation() {
        let (tx, rx) = flume::unbounded();
        let engine = TerminalEngine::new(80, 24, rx);
        drop(tx);
        let _term = engine.terminal();
    }

    #[test]
    fn test_advance_bytes() {
        let (tx, rx) = flume::unbounded();
        let engine = TerminalEngine::new(80, 24, rx);
        tx.send(b"hello".to_vec()).unwrap();
        drop(tx);
        engine.advance_bytes();
        let term = engine.terminal();
        let grid = term.grid();
        let mut chars: Vec<char> = Vec::new();
        for indexed in grid.display_iter() {
            if !indexed.cell.flags.contains(Flags::WIDE_CHAR_SPACER) && indexed.cell.c != ' ' {
                chars.push(indexed.cell.c);
            }
        }
        let s: String = chars.into_iter().collect();
        assert!(
            s.contains("hello"),
            "terminal should contain 'hello' after advance_bytes, got: {:?}",
            s
        );
    }

    #[test]
    fn test_click_to_prompt() {
        let (tx, rx) = flume::unbounded();
        let engine = TerminalEngine::new(80, 24, rx);

        // No prompt yet
        assert_eq!(engine.click_to_prompt(5), None);

        // After PromptStart, click_to_prompt returns (prompt_line, col)
        tx.send(osc133_st('A', None)).unwrap();
        engine.advance_with_osc133();
        let line = engine.prompt_line().unwrap();
        assert_eq!(engine.click_to_prompt(5), Some((line, 5)));
        assert_eq!(engine.click_to_prompt(0), Some((line, 0)));
        assert_eq!(engine.click_to_prompt(42), Some((line, 42)));

        drop(tx);
    }

    #[test]
    fn test_advance_with_osc133_phase_transitions() {
        let (tx, rx) = flume::unbounded();
        let engine = TerminalEngine::new(80, 24, rx);

        // Unknown -> Prompt (A)
        tx.send(osc133_st('A', None)).unwrap();
        engine.advance_with_osc133();
        assert_eq!(engine.shell_phase(), ShellPhase::Prompt);
        assert!(engine.prompt_line().is_some());

        // Prompt -> Input (B)
        tx.send(osc133_st('B', None)).unwrap();
        engine.advance_with_osc133();
        assert_eq!(engine.shell_phase(), ShellPhase::Input);

        // Input -> Running (C)
        tx.send(osc133_st('C', None)).unwrap();
        engine.advance_with_osc133();
        assert_eq!(engine.shell_phase(), ShellPhase::Running);

        // Running -> Output (D)
        tx.send(osc133_st('D', Some(0))).unwrap();
        engine.advance_with_osc133();
        assert_eq!(engine.shell_phase(), ShellPhase::Output);
        assert_eq!(engine.last_post_exec_exit_code(), Some(0));

        drop(tx);
    }

    #[test]
    fn test_advance_with_osc133_markers_stored() {
        use crate::shell_integration::MarkerKind;

        let (tx, rx) = flume::unbounded();
        let engine = TerminalEngine::new(80, 24, rx);
        tx.send(osc133_st('A', None)).unwrap();
        tx.send(osc133_st('B', None)).unwrap();
        engine.advance_with_osc133();
        drop(tx);

        let state = engine.shell_state();
        assert_eq!(state.markers.len(), 2);
        assert_eq!(state.markers[0].kind, MarkerKind::PromptStart);
        assert_eq!(state.markers[1].kind, MarkerKind::PromptEnd);
    }

    #[test]
    fn test_is_tui_active_alt_screen() {
        let (tx, rx) = flume::unbounded();
        let engine = TerminalEngine::new(80, 24, rx);
        // Enter alternate screen (vim, neovim, etc. use this)
        tx.send(b"\x1b[?1049h".to_vec()).unwrap();
        engine.advance_bytes();
        assert!(engine.is_tui_active(), "ALT_SCREEN mode should make is_tui_active true");
        drop(tx);
    }

    #[test]
    fn test_is_tui_active_normal_shell() {
        let (tx, rx) = flume::unbounded();
        let engine = TerminalEngine::new(80, 24, rx);
        tx.send(b"$ ls".to_vec()).unwrap();
        engine.advance_bytes();
        assert!(!engine.is_tui_active(), "normal shell output should not trigger TUI mode");
        drop(tx);
    }

    #[test]
    fn test_resize_removes_off_screen_markers() {
        let (tx, rx) = flume::unbounded();
        let engine = TerminalEngine::new(80, 24, rx);
        tx.send(osc133_st('A', None)).unwrap();
        engine.advance_with_osc133();
        drop(tx);

        // Resize to fewer rows - marker at line 0 should remain (line 0 < 12)
        engine.resize(80, 12);
        assert!(!engine.shell_state().markers.is_empty());

        // Resize to 1 row - marker at line 0 remains (line 0 < 1)
        engine.resize(80, 1);
        assert!(!engine.shell_state().markers.is_empty());
    }
}
