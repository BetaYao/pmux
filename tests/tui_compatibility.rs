//! TUI Application Test Suite
//!
//! Tests for TerminalEngine compatibility with TUI applications.
//! Design reference: docs/plans/2026-02-terminal-engine-phase4-integration.md T4.
//!
//! Phase 1-3 completed: TerminalEngine, 60fps frame tick, SIGWINCH resize, direct PTY write.

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::term::cell::Flags;
use pmux::terminal::{spawn_pty_reader_with_handle, TerminalEngine};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::io::Write;
use std::time::Duration;

/// Extract visible text from engine via renderable_content.
fn engine_content_text(engine: &TerminalEngine) -> String {
    engine
        .try_renderable_content(|_content, display_iter, _screen_lines| {
            let mut chars = Vec::new();
            for indexed in display_iter {
                if !indexed.cell.flags.contains(Flags::WIDE_CHAR_SPACER) && indexed.cell.c != '\0' {
                    chars.push(indexed.cell.c);
                }
            }
            chars.into_iter().collect::<String>()
        })
        .unwrap_or_default()
}

/// Get cursor position (row, col) from engine.
fn engine_cursor_position(engine: &TerminalEngine) -> Option<(usize, usize)> {
    let term = engine.terminal();
    let grid = term.grid();
    let display_offset = grid.display_offset();
    let cursor = term.renderable_content().cursor;
    let cursor_row = (cursor.point.line.0 + display_offset as i32) as usize;
    let cursor_col = cursor.point.column.0;
    Some((cursor_row, cursor_col))
}

// =============================================================================
// Basic terminal test (requires PTY - slow, may be flaky in CI)
// =============================================================================

#[test]
#[ignore = "Requires PTY; slow (process spawn); run with: cargo test test_terminal_basic_output -- --ignored"]
fn test_terminal_basic_output() {
    // 1. Create PTY
    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("openpty");

    // 2. Spawn shell in slave
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "sh".to_string());
    let mut cmd = CommandBuilder::new(shell);
    cmd.cwd(temp_dir.path());
    let _child = pair.slave.spawn_command(cmd).expect("spawn shell");

    // 3. Setup engine with channel
    let (tx, rx) = flume::unbounded();
    let engine = TerminalEngine::new(80, 24, rx);

    // 4. Spawn reader (need fd before take_writer)
    let fd = pair
        .master
        .as_raw_fd()
        .expect("PTY master raw fd not available");
    let handle = spawn_pty_reader_with_handle(fd, tx);

    // 5. Write command
    let mut writer = pair.master.take_writer().expect("take_writer");
    writer.write_all(b"echo 'hello world'\n").expect("write");
    writer.flush().expect("flush");

    // 6. Wait and process
    std::thread::sleep(Duration::from_millis(500));
    engine.advance_bytes();

    // 7. Verify output
    let text = engine_content_text(&engine);
    assert!(
        text.contains("hello world"),
        "terminal should contain 'hello world', got: {:?}",
        text
    );

    // Cleanup
    drop(writer);
    drop(pair);
    handle.shutdown();
}

// =============================================================================
// TUI mode detection (no PTY - fast)
// =============================================================================

#[test]
fn test_tui_mode_detection() {
    let (tx, rx) = flume::unbounded();
    let engine = TerminalEngine::new(80, 24, rx);

    // Normal mode - not TUI
    assert!(!engine.is_tui_active());

    // Enter alternate screen (ESC[?1049h)
    tx.send(b"\x1b[?1049h".to_vec()).unwrap();
    engine.advance_bytes();
    assert!(engine.is_tui_active());

    // Exit alternate screen (ESC[?1049l)
    tx.send(b"\x1b[?1049l".to_vec()).unwrap();
    engine.advance_bytes();
    assert!(!engine.is_tui_active());
}

// =============================================================================
// Resize test (no PTY - fast)
// =============================================================================

#[test]
fn test_terminal_resize() {
    let (tx, rx) = flume::unbounded();
    let engine = TerminalEngine::new(80, 24, rx);

    // Write content
    tx.send(b"line1\nline2\n".to_vec()).unwrap();
    engine.advance_bytes();

    // Resize
    engine.resize(40, 12);

    // Verify dimensions updated
    let term = engine.terminal();
    let grid = term.grid();
    assert_eq!(grid.columns(), 40, "columns should be 40 after resize");
    assert_eq!(grid.screen_lines(), 12, "screen_lines should be 12 after resize");
    // Lock is released here when term is dropped
    drop(term);

    // Verify content still accessible after resize
    let term2 = engine.terminal();
    let has_content = term2.grid().display_iter().any(|i| i.cell.c == 'l');
    assert!(has_content, "should have some content after resize");
}

// =============================================================================
// Cursor position test (no PTY - fast)
// =============================================================================

#[test]
fn test_cursor_position() {
    let (tx, rx) = flume::unbounded();
    let engine = TerminalEngine::new(80, 24, rx);

    // Write some content
    tx.send(b"hello\n".to_vec()).unwrap();
    engine.advance_bytes();

    // Check cursor position is available and content contains "hello"
    let pos = engine_cursor_position(&engine);
    let text = engine_content_text(&engine);
    assert!(text.contains("hello"), "content should contain 'hello', got: {:?}", text);
    assert!(pos.is_some(), "cursor position should be available, got {:?}", pos);
    // After "hello\n": cursor should be at start of next line (col 0) or end of "hello" (col 5)
    let (row, col) = pos.unwrap();
    assert!(col == 0 || col == 5, "cursor col should be 0 or 5 after 'hello\\n', got ({}, {})", row, col);
}
