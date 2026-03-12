/// Integration test for extract_last_line_filtered and helpers.
/// This test file avoids the gpui_macros SIGBUS by importing only what we need.

// We test the public API: extract_last_line and extract_last_line_filtered
use pmux::terminal::{extract_last_line, extract_last_line_filtered};

#[test]
fn test_extract_last_line_backward_compat() {
    assert_eq!(extract_last_line("", 80), "");
    assert_eq!(extract_last_line("---\n===\n", 80), "");
    assert_eq!(
        extract_last_line("first line\nsecond line\n---\n\n", 80),
        "second line"
    );
    // Box-drawing lines are now also skipped
    assert_eq!(
        extract_last_line("real content\n──────────\n", 80),
        "real content"
    );
}

#[test]
fn test_extract_last_line_truncation() {
    let long = "a".repeat(100);
    let result = extract_last_line(&long, 20);
    assert_eq!(result, format!("{}...", "a".repeat(20)));
}

#[test]
fn test_extract_last_line_utf8() {
    // CJK
    let content = "错误：编译失败了";
    let result = extract_last_line(content, 4);
    assert_eq!(result, "错误：编...");

    // Emoji
    let content = "✓ Done 🎉 完成";
    let result = extract_last_line(content, 6);
    assert_eq!(result, "✓ Done...");
}

#[test]
fn test_box_drawing_lines_skipped() {
    // Pure box-drawing lines
    assert_eq!(extract_last_line("msg\n─────────────────\n", 80), "msg");
    assert_eq!(extract_last_line("msg\n═══════════════\n", 80), "msg");
    assert_eq!(extract_last_line("msg\n▀▀▀▀▀▀▀▀▀▀▀▀▀\n", 80), "msg");
}

#[test]
fn test_frame_start_lines_skipped() {
    // Lines starting with frame characters
    assert_eq!(
        extract_last_line("real msg\n┃ some chrome\n", 80),
        "real msg"
    );
    assert_eq!(
        extract_last_line("real msg\n│ sidebar text\n", 80),
        "real msg"
    );
    assert_eq!(
        extract_last_line("real msg\n╭─────────────╮\n", 80),
        "real msg"
    );
}

#[test]
fn test_single_utf8_char_skipped() {
    // Single UTF-8 character like ❯ should be skipped
    assert_eq!(
        extract_last_line("hello world\n❯\n", 80),
        "hello world"
    );
    assert_eq!(
        extract_last_line("hello world\n$\n", 80),
        "hello world"
    );
}

#[test]
fn test_filtered_claude_code() {
    let content = "I've updated the configuration file.\n\
                   \n\
                   ─────────────────────────────────\n\
                   > accept edits on (shift+tab to cycle)\n\
                   \n\
                   ╹ press esc to interrupt";
    let skip = vec![
        "shift+tab".to_string(),
        "accept edits".to_string(),
        "to interrupt".to_string(),
    ];
    let result = extract_last_line_filtered(content, 80, &skip);
    assert_eq!(result, "I've updated the configuration file.");
}

#[test]
fn test_filtered_opencode() {
    let content = "Successfully compiled the project\n\
                   ───────────────────────────────\n\
                   │ tab agents  ctrl+p commands\n\
                   ╹▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀";
    let skip = vec![
        "tab agents".to_string(),
        "ctrl+p commands".to_string(),
    ];
    let result = extract_last_line_filtered(content, 80, &skip);
    assert_eq!(result, "Successfully compiled the project");
}

#[test]
fn test_filtered_case_insensitive() {
    let content = "Real message here\nShift+Tab to cycle\n";
    let skip = vec!["shift+tab".to_string()];
    let result = extract_last_line_filtered(content, 80, &skip);
    assert_eq!(result, "Real message here");
}

#[test]
fn test_filtered_empty_patterns() {
    let content = "message\n┃ chrome line\n───────\n";
    let result = extract_last_line_filtered(content, 80, &[]);
    assert_eq!(result, "message");
}

#[test]
fn test_filtered_all_chrome() {
    let content = "───────\n┃ frame\n╹▀▀▀▀\n";
    let result = extract_last_line_filtered(content, 80, &[]);
    assert_eq!(result, "");
}
