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

    /// Paint this run using GPUI's shape_line + paint
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
        let shaped = window
            .text_system()
            .shape_line(self.text.clone().into(), font_size, &[run_style], Some(cell_width));
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
}
