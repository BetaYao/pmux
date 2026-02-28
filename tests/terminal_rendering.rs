//! Terminal rendering integration tests (Phase 5)
//!
//! Tests that use TerminalEngine with real VT sequences for:
//! - Plain text and colored output (ANSI escape sequences)
//! - Alternate screen (TUI apps like vim, htop)
//! - content_for_status_detection with Term buffer
//! - Status detection (Task 5.4)
//!

use pmux::agent_status::AgentStatus;
use pmux::status_detector::StatusDetector;
use pmux::terminal::TerminalEngine;
use pmux::ui::terminal_view::TerminalBuffer;
use std::sync::Arc;

fn make_engine_with_bytes(bytes: &[u8]) -> Arc<TerminalEngine> {
    let (tx, rx) = flume::unbounded();
    let engine = Arc::new(TerminalEngine::new(80, 24, rx));
    tx.send(bytes.to_vec()).unwrap();
    engine.advance_bytes();
    drop(tx);
    engine
}

#[test]
fn test_term_buffer_content_plain_text() {
    let engine = make_engine_with_bytes(b"hello world\r\n");
    let buf = TerminalBuffer::new_term_with_cache_size(engine, 200);
    let content = buf.content_for_status_detection();
    assert!(content.is_some());
    let text = content.unwrap();
    assert!(text.contains("hello world"), "content should contain 'hello world', got: {:?}", text);
}

#[test]
fn test_term_buffer_content_with_ansi_colors() {
    // \x1b[32m = green, \x1b[0m = reset
    let engine = make_engine_with_bytes(b"\x1b[32mgreen text\x1b[0m\r\n");
    let buf = TerminalBuffer::new_term_with_cache_size(engine, 200);
    let content = buf.content_for_status_detection();
    assert!(content.is_some());
    let text = content.unwrap();
    // extract_text_from_display_iter yields plain text (ANSI stripped by display)
    assert!(text.contains("green text"), "content should contain 'green text', got: {:?}", text);
}

#[test]
fn test_term_buffer_content_alternate_screen() {
    // Enter alternate screen (vim/htop style)
    let (tx, rx) = flume::unbounded();
    let engine = Arc::new(TerminalEngine::new(80, 24, rx));
    tx.send(b"\x1b[?1049h".to_vec()).unwrap();
    tx.send(b"vim buffer content\r\n".to_vec()).unwrap();
    engine.advance_bytes();
    drop(tx);
    let buf = TerminalBuffer::new_term_with_cache_size(engine, 200);
    let content = buf.content_for_status_detection();
    assert!(content.is_some());
    let text = content.unwrap();
    // Content extraction should still work in alternate screen
    assert!(text.contains("vim buffer content"), "content should contain 'vim buffer content', got: {:?}", text);
}

#[test]
fn test_term_buffer_content_multiline() {
    let engine = make_engine_with_bytes(b"line1\r\nline2\r\nline3\r\n");
    let buf = TerminalBuffer::new_term_with_cache_size(engine, 200);
    let content = buf.content_for_status_detection();
    assert!(content.is_some());
    let text = content.unwrap();
    assert!(text.contains("line1"));
    assert!(text.contains("line2"));
    assert!(text.contains("line3"));
}

// -----------------------------------------------------------------------------
// Task 5.4: Status detection verification
// -----------------------------------------------------------------------------

#[test]
fn test_content_for_status_detection_works_with_status_detector() {
    let engine = make_engine_with_bytes(b"AI is thinking about your request\r\n");
    let buf = TerminalBuffer::new_term_with_cache_size(engine, 200);
    let content = buf.content_for_status_detection();
    assert!(content.is_some());
    let detector = StatusDetector::new();
    let status = detector.detect_from_text(content.as_deref().unwrap());
    assert_eq!(status, AgentStatus::Running, "content should be detected as Running");
}

#[test]
fn test_content_for_status_detection_waiting_pattern() {
    let engine = make_engine_with_bytes(b"? What would you like to do?\r\n");
    let buf = TerminalBuffer::new_term_with_cache_size(engine, 200);
    let content = buf.content_for_status_detection();
    assert!(content.is_some());
    let detector = StatusDetector::new();
    let status = detector.detect_from_text(content.as_deref().unwrap());
    assert_eq!(status, AgentStatus::Waiting);
}

#[test]
fn test_content_for_status_detection_error_pattern() {
    let engine = make_engine_with_bytes(b"Error: file not found\r\n");
    let buf = TerminalBuffer::new_term_with_cache_size(engine, 200);
    let content = buf.content_for_status_detection();
    assert!(content.is_some());
    let detector = StatusDetector::new();
    let status = detector.detect_from_text(content.as_deref().unwrap());
    assert_eq!(status, AgentStatus::Error);
}

#[test]
fn test_content_for_status_detection_idle() {
    let engine = make_engine_with_bytes(b"$ ls -la\r\n");
    let buf = TerminalBuffer::new_term_with_cache_size(engine, 200);
    let content = buf.content_for_status_detection();
    assert!(content.is_some());
    let detector = StatusDetector::new();
    let status = detector.detect_from_text(content.as_deref().unwrap());
    assert_eq!(status, AgentStatus::Idle);
}
