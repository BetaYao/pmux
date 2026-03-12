//! Rendering primitives for the terminal grid.
//!
//! BatchedTextRun groups adjacent cells with identical text style.
//! LayoutRect groups adjacent cells with identical background color.

use alacritty_terminal::vte::ansi::{Color, NamedColor};
use gpui::*;

/// A batched text run — adjacent cells with the same text style merged into one shape call
pub struct BatchedTextRun {
    pub start_line: i32,
    pub start_col: i32,
    pub text: String,
    pub cell_count: usize,
    pub style: TextRun,
}

impl BatchedTextRun {
    pub fn new(start_line: i32, start_col: i32, c: char, style: TextRun) -> Self {
        let mut text = String::with_capacity(16);
        text.push(c);
        Self { start_line, start_col, text, cell_count: 1, style }
    }

    /// Create a new run with zero-width combining characters appended after the main char
    pub fn new_with_zerowidth(start_line: i32, start_col: i32, c: char, zerowidth: Option<&[char]>, style: TextRun) -> Self {
        let mut text = String::with_capacity(16);
        text.push(c);
        if let Some(zw) = zerowidth {
            for &zwc in zw {
                text.push(zwc);
            }
        }
        Self { start_line, start_col, text, cell_count: 1, style }
    }

    /// Whether another cell with the given style can be appended to this run
    pub fn can_append(&self, other_style: &TextRun, line: i32, col: i32) -> bool {
        self.start_line == line
            && self.start_col + self.cell_count as i32 == col
            && self.style.font == other_style.font
            && self.style.color == other_style.color
            && self.style.background_color == other_style.background_color
            && self.style.underline == other_style.underline
            && self.style.strikethrough == other_style.strikethrough
    }

    pub fn append_char(&mut self, c: char) {
        self.text.push(c);
        self.cell_count += 1;
        self.style.len += c.len_utf8();
    }

    /// Append a character with its zero-width combining characters
    pub fn append_char_with_zerowidth(&mut self, c: char, zerowidth: Option<&[char]>) {
        self.text.push(c);
        self.cell_count += 1;
        let mut extra_len = c.len_utf8();
        if let Some(zw) = zerowidth {
            for &zwc in zw {
                self.text.push(zwc);
                extra_len += zwc.len_utf8();
            }
        }
        self.style.len += extra_len;
    }

    /// Paint this run using GPUI's shape_line + paint.
    ///
    /// Batches all characters into a single shape_line call with `force_width`
    /// to enforce monospace grid alignment (same approach as Zed's terminal).
    pub fn paint(
        &self,
        origin: Point<Pixels>,
        cell_width: Pixels,
        line_height: Pixels,
        font_size: Pixels,
        window: &mut Window,
        cx: &mut App,
    ) {
        let pos = Point::new(
            origin.x + px(self.start_col as f32 * f32::from(cell_width)),
            origin.y + px(self.start_line as f32 * f32::from(line_height)),
        );
        let run_style = TextRun {
            len: self.text.len(),
            font: self.style.font.clone(),
            color: self.style.color,
            background_color: self.style.background_color,
            underline: self.style.underline.clone(),
            strikethrough: self.style.strikethrough.clone(),
        };
        let shaped = window.text_system().shape_line(
            self.text.clone().into(),
            font_size,
            &[run_style],
            Some(cell_width),
        );
        let _ = shaped.paint(pos, line_height, TextAlign::Left, None, window, cx);
    }
}

/// A background color rectangle — adjacent cells with the same background color merged
pub struct LayoutRect {
    pub line: i32,
    pub start_col: i32,
    pub num_cells: usize,
    pub color: Hsla,
}

impl LayoutRect {
    pub fn new(line: i32, col: i32, color: Hsla) -> Self {
        Self { line, start_col: col, num_cells: 1, color }
    }

    pub fn extend(&mut self) {
        self.num_cells += 1;
    }

    pub fn paint(&self, origin: Point<Pixels>, cell_width: Pixels, line_height: Pixels, window: &mut Window) {
        use gpui::Edges;
        let pos = Point::new(
            origin.x + px(self.start_col as f32 * f32::from(cell_width)),
            origin.y + line_height * self.line as f32,
        );
        let sz = Size::new(
            px(f32::from(cell_width) * self.num_cells as f32),
            line_height,
        );
        let bounds = Bounds::new(pos, sz);
        window.paint_quad(quad(
            bounds,
            px(0.0),
            self.color,
            Edges::default(),
            transparent_black(),
            Default::default(),
        ));
    }
}

/// True if a cell's background is the terminal default (should not generate a LayoutRect)
pub fn is_default_bg(color: &Color) -> bool {
    matches!(color, Color::Named(NamedColor::Background))
}

/// Split a batched text run's text into per-cell strings, grouping base characters
/// with any following zero-width combining characters.
/// Returns (col_offset, cell_text) pairs for each cell.
pub fn split_text_into_cells(text: &str) -> Vec<String> {
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

/// True if the character is zero-width or a Unicode combining mark.
/// Used to group combining characters with their base character for per-cell rendering.
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

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::*;

    #[test]
    fn test_batched_text_run_append() {
        let style = TextRun {
            len: 1,
            font: Font::default(),
            color: Hsla::default(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let mut run = BatchedTextRun::new(0, 0, 'a', style.clone());
        let style2 = TextRun { len: 1, ..style.clone() };
        assert!(run.can_append(&style2, 0, 1));
        run.append_char('b');
        assert_eq!(run.text, "ab");
        assert_eq!(run.cell_count, 2);
    }

    #[test]
    fn test_batched_text_run_no_append_different_line() {
        let style = TextRun {
            len: 1,
            font: Font::default(),
            color: Hsla::default(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let run = BatchedTextRun::new(0, 0, 'a', style.clone());
        assert!(!run.can_append(&style, 1, 1)); // different line
    }

    #[test]
    fn test_layout_rect_extend() {
        let mut rect = LayoutRect::new(0, 0, Hsla::default());
        assert_eq!(rect.num_cells, 1);
        rect.extend();
        assert_eq!(rect.num_cells, 2);
    }

    #[test]
    fn test_is_default_bg() {
        use alacritty_terminal::vte::ansi::{Color, NamedColor};
        assert!(is_default_bg(&Color::Named(NamedColor::Background)));
        assert!(!is_default_bg(&Color::Named(NamedColor::Foreground)));
    }

    #[test]
    fn test_is_zero_width_or_combining() {
        assert!(is_zero_width_or_combining('\u{200D}')); // ZWJ
        assert!(is_zero_width_or_combining('\u{0301}')); // combining acute accent
        assert!(is_zero_width_or_combining('\u{FE0F}')); // variation selector
        assert!(!is_zero_width_or_combining('a'));
        assert!(!is_zero_width_or_combining('─'));
        assert!(!is_zero_width_or_combining('⏺'));
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
        // Simulates Claude Code output: icon + bold text + path
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
        assert_eq!(cells[0], "⏺\u{FE0F}"); // base + VS grouped
        assert_eq!(cells[1], "x");
    }

    #[test]
    fn test_split_with_zwj_sequence() {
        // Simulated ZWJ sequence: base + ZWJ + next char
        let text = "a\u{200D}bx";
        let cells = split_text_into_cells(text);
        assert_eq!(cells.len(), 3);
        assert_eq!(cells[0], "a\u{200D}"); // a + ZWJ grouped (ZWJ is zero-width)
        assert_eq!(cells[1], "b"); // b is NOT zero-width, starts new cell
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
    fn test_split_preserves_cell_count() {
        // For a batched run, the number of cells returned by split_text_into_cells
        // should equal cell_count (assuming no zero-width chars in the text).
        let style = TextRun {
            len: 1,
            font: Font::default(),
            color: Hsla::default(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let mut run = BatchedTextRun::new(0, 0, 'H', style.clone());
        run.append_char('e');
        run.append_char('l');
        run.append_char('l');
        run.append_char('o');
        assert_eq!(run.cell_count, 5);
        let cells = split_text_into_cells(&run.text);
        assert_eq!(cells.len(), run.cell_count);
    }

    #[test]
    fn test_split_claude_code_like_output() {
        // Test content similar to what Claude Code produces
        let test_cases = vec![
            ("Read(packages/app/src/stores/knowledge.ts)", 42),
            ("────────────────────", 20),
            ("BaseSearch", 10),
            ("✓ Write(CLAUDE.md)", 18),
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
                // First char should not be a zero-width char
                let first = cell.chars().next().unwrap();
                assert!(
                    !is_zero_width_or_combining(first),
                    "Cell {} starts with zero-width char for text {:?}",
                    i, text
                );
            }
        }
    }
}
