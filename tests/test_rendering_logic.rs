//! Tests for terminal rendering logic that don't require GPUI context.
//! These tests run independently from the main crate to avoid the gpui_macros SIGBUS issue.

/// Inline copy of is_zero_width_or_combining for testing without GPUI dependency
fn is_zero_width_or_combining(c: char) -> bool {
    matches!(c,
        '\u{200B}'  // zero-width space
        | '\u{200C}' // zero-width non-joiner
        | '\u{200D}' // zero-width joiner
        | '\u{FEFF}' // zero-width no-break space / BOM
        | '\u{FE00}'..='\u{FE0F}' // variation selectors
        | '\u{0300}'..='\u{036F}' // combining diacritical marks
        | '\u{20D0}'..='\u{20FF}' // combining diacritical marks for symbols
        | '\u{1AB0}'..='\u{1AFF}' // combining diacritical marks extended
        | '\u{1DC0}'..='\u{1DFF}' // combining diacritical marks supplement
        | '\u{FE20}'..='\u{FE2F}' // combining half marks
        | '\u{E0100}'..='\u{E01EF}' // variation selectors supplement
    )
}

/// Inline copy of split_text_into_cells for testing without GPUI dependency
fn split_text_into_cells(text: &str) -> Vec<String> {
    let mut cells = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        let mut cell_text = String::new();
        cell_text.push(c);
        while let Some(&next) = chars.peek() {
            if is_zero_width_or_combining(next) {
                cell_text.push(chars.next().unwrap());
            } else {
                break;
            }
        }
        cells.push(cell_text);
    }
    cells
}

// --- Zero-width detection tests ---

#[test]
fn test_is_zero_width_or_combining() {
    assert!(is_zero_width_or_combining('\u{200D}')); // ZWJ
    assert!(is_zero_width_or_combining('\u{0301}')); // combining acute accent
    assert!(is_zero_width_or_combining('\u{FE0F}')); // variation selector 16
    assert!(is_zero_width_or_combining('\u{200B}')); // zero-width space
    assert!(is_zero_width_or_combining('\u{FEFF}')); // BOM / ZWNBSP
    assert!(!is_zero_width_or_combining('a'));
    assert!(!is_zero_width_or_combining('─'));
    assert!(!is_zero_width_or_combining('⏺'));
    assert!(!is_zero_width_or_combining(' '));
    assert!(!is_zero_width_or_combining('R'));
}

// --- Per-cell splitting tests ---

#[test]
fn test_split_ascii_text() {
    let cells = split_text_into_cells("Read");
    assert_eq!(cells, vec!["R", "e", "a", "d"]);
}

#[test]
fn test_split_box_drawing() {
    let cells = split_text_into_cells("──────");
    assert_eq!(cells.len(), 6);
    for cell in &cells {
        assert_eq!(cell, "─");
    }
}

#[test]
fn test_split_unicode_symbols() {
    let cells = split_text_into_cells("⏺ Read");
    assert_eq!(cells, vec!["⏺", " ", "R", "e", "a", "d"]);
}

#[test]
fn test_split_mixed_content() {
    let cells = split_text_into_cells("⏺Read(packages/app/src)");
    assert_eq!(cells.len(), 23);
    assert_eq!(cells[0], "⏺");
    assert_eq!(cells[1], "R");
    assert_eq!(cells[2], "e");
    assert_eq!(cells[3], "a");
    assert_eq!(cells[4], "d");
    assert_eq!(cells[5], "(");
}

#[test]
fn test_split_with_combining_char() {
    // 'e' followed by combining acute accent (é)
    let text = "e\u{0301}x";
    let cells = split_text_into_cells(text);
    assert_eq!(cells.len(), 2);
    assert_eq!(cells[0], "e\u{0301}"); // base + combining grouped together
    assert_eq!(cells[1], "x");
}

#[test]
fn test_split_with_variation_selector() {
    // ⏺ followed by VS16 (emoji presentation selector)
    let text = "⏺\u{FE0F}x";
    let cells = split_text_into_cells(text);
    assert_eq!(cells.len(), 2);
    assert_eq!(cells[0], "⏺\u{FE0F}");
    assert_eq!(cells[1], "x");
}

#[test]
fn test_split_with_zwj() {
    // base + ZWJ + next char
    let text = "a\u{200D}bx";
    let cells = split_text_into_cells(text);
    assert_eq!(cells.len(), 3);
    assert_eq!(cells[0], "a\u{200D}"); // ZWJ grouped with preceding base
    assert_eq!(cells[1], "b");
    assert_eq!(cells[2], "x");
}

#[test]
fn test_split_empty_text() {
    let cells = split_text_into_cells("");
    assert!(cells.is_empty());
}

#[test]
fn test_split_single_char() {
    let cells = split_text_into_cells("R");
    assert_eq!(cells, vec!["R"]);
}

#[test]
fn test_split_claude_code_like_output() {
    let test_cases = vec![
        ("Read(packages/app/src/stores/knowledge.ts)", 42),
        ("────────────────────", 20),
        ("BaseSearch", 10),
        ("✓ Write(CLAUDE.md)", 18),
        ("abcdefghijklmnopqrstuvwxyz", 26),
        ("ABCDEFGHIJKLMNOPQRSTUVWXYZ", 26),
        ("0123456789", 10),
    ];
    for (text, expected_len) in test_cases {
        let cells = split_text_into_cells(text);
        assert_eq!(
            cells.len(), expected_len,
            "Failed for {:?}: got {} cells, expected {}",
            text, cells.len(), expected_len
        );
        // Verify each cell has at least one visible character
        for (i, cell) in cells.iter().enumerate() {
            assert!(!cell.is_empty(), "Cell {} is empty for text {:?}", i, text);
            let first = cell.chars().next().unwrap();
            assert!(
                !is_zero_width_or_combining(first),
                "Cell {} starts with zero-width char for text {:?}",
                i, text
            );
        }
    }
}

#[test]
fn test_split_multiple_combining_marks() {
    // Base char + two combining marks
    let text = "a\u{0300}\u{0301}b";
    let cells = split_text_into_cells(text);
    assert_eq!(cells.len(), 2);
    assert_eq!(cells[0], "a\u{0300}\u{0301}"); // both marks grouped with 'a'
    assert_eq!(cells[1], "b");
}

#[test]
fn test_split_consecutive_base_with_combiners() {
    // Two chars, each with a combining mark
    let text = "a\u{0301}e\u{0300}";
    let cells = split_text_into_cells(text);
    assert_eq!(cells.len(), 2);
    assert_eq!(cells[0], "a\u{0301}"); // á
    assert_eq!(cells[1], "e\u{0300}"); // è
}

#[test]
fn test_cell_count_matches_split_for_ascii_batches() {
    // Simulates building a batch then verifying split matches cell_count
    let test_texts = vec![
        "Hello",
        "Read(packages/app/src/stores/knowledge.ts)",
        "git:(main)",
        "────────────────────────────────────────",
        "⏺ BaseSearch for pattern in codebase",
    ];
    for text in test_texts {
        let cells = split_text_into_cells(text);
        let char_count = text.chars().filter(|c| !is_zero_width_or_combining(*c)).count();
        assert_eq!(
            cells.len(), char_count,
            "Cell count mismatch for {:?}: split={}, visible_chars={}",
            text, cells.len(), char_count
        );
    }
}
