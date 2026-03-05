// ui/diff_view/diff_panel.rs - Right panel: side-by-side / unified diff renderer
// Uses uniform_list for virtualized rendering (only visible rows are created).
use crate::git_diff::{DiffLineKind, FileDiff};
use crate::syntax_highlight::HighlightedLine;
use gpui::prelude::*;
use gpui::*;
use std::sync::Arc;

pub type RejectHunkCallback = Arc<dyn Fn(usize, &mut Window, &mut App) + Send + Sync>;

/// A flattened row index into the diff: either a hunk header or a content line.
#[derive(Clone)]
pub enum DiffFlatRow {
    HunkHeader(usize),
    Line {
        hunk_idx: usize,
        line_idx: usize,
        highlight_idx: usize,
    },
}

/// Row height in pixels — must be uniform for uniform_list.
const ROW_HEIGHT: f32 = 22.;

/// Build the flat row index from a FileDiff.
pub fn build_flat_rows(diff: &FileDiff) -> Vec<DiffFlatRow> {
    let mut rows = Vec::new();
    let mut highlight_idx = 0;
    for (hunk_idx, hunk) in diff.hunks.iter().enumerate() {
        rows.push(DiffFlatRow::HunkHeader(hunk_idx));
        for line_idx in 0..hunk.lines.len() {
            rows.push(DiffFlatRow::Line {
                hunk_idx,
                line_idx,
                highlight_idx,
            });
            highlight_idx += 1;
        }
    }
    rows
}

/// Build a monospace Font for TextRun usage.
fn mono_font() -> Font {
    font("monospace")
}

/// Convert a syntax-highlighted line into a single StyledText element with TextRuns.
fn styled_text_from_highlight(highlighted: &HighlightedLine) -> StyledText {
    let mut full_text = String::new();
    let mut runs = Vec::with_capacity(highlighted.spans.len());
    let mono = mono_font();

    for span in &highlighted.spans {
        let byte_len = span.text.len();
        if byte_len == 0 {
            continue;
        }
        full_text.push_str(&span.text);
        let (r, g, b, _a) = span.fg;
        runs.push(TextRun {
            len: byte_len,
            font: mono.clone(),
            color: Rgba {
                r: r as f32 / 255.0,
                g: g as f32 / 255.0,
                b: b as f32 / 255.0,
                a: 1.0,
            }
            .into(),
            background_color: None,
            underline: None,
            strikethrough: None,
        });
    }

    if full_text.is_empty() {
        full_text.push(' ');
        runs.push(TextRun {
            len: 1,
            font: mono,
            color: Rgba {
                r: 0.5,
                g: 0.5,
                b: 0.5,
                a: 1.0,
            }
            .into(),
            background_color: None,
            underline: None,
            strikethrough: None,
        });
    }

    StyledText::new(SharedString::from(full_text)).with_runs(runs)
}

fn styled_text_plain(text: &str, color: Hsla) -> StyledText {
    let s = if text.is_empty() {
        " ".to_string()
    } else {
        text.to_string()
    };
    let len = s.len();
    StyledText::new(SharedString::from(s)).with_runs(vec![TextRun {
        len,
        font: mono_font(),
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    }])
}

fn line_content_element(highlighted: Option<&HighlightedLine>, fallback: &str) -> AnyElement {
    if let Some(hl) = highlighted {
        styled_text_from_highlight(hl).into_any_element()
    } else {
        let default_color: Hsla = rgb(0xcccccc).into();
        styled_text_plain(fallback, default_color).into_any_element()
    }
}

// --- Public render functions (called from uniform_list callback) ---

pub fn render_empty_state() -> Div {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .text_size(px(14.))
                .text_color(rgb(0x888888))
                .child("Select a file to view diff"),
        )
}

pub fn render_binary_notice() -> Div {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .text_size(px(14.))
                .text_color(rgb(0x888888))
                .child("Binary file (cannot display diff)"),
        )
}

pub fn render_no_changes() -> Div {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .text_size(px(14.))
                .text_color(rgb(0x888888))
                .child("No changes"),
        )
}

/// Render a single diff row (hunk header or line) for the virtualized list.
pub fn render_diff_row(
    row: &DiffFlatRow,
    diff: &FileDiff,
    highlighted: Option<&[HighlightedLine]>,
    on_reject: Option<&RejectHunkCallback>,
    side_by_side: bool,
) -> AnyElement {
    match row {
        DiffFlatRow::HunkHeader(hunk_idx) => render_hunk_header(
            *hunk_idx,
            &diff.hunks[*hunk_idx].header,
            on_reject,
        )
        .h(px(ROW_HEIGHT))
        .into_any_element(),
        DiffFlatRow::Line {
            hunk_idx,
            line_idx,
            highlight_idx,
        } => {
            let line = &diff.hunks[*hunk_idx].lines[*line_idx];
            let hl = highlighted.and_then(|h| h.get(*highlight_idx));
            if side_by_side {
                render_side_by_side_line(line, hl)
                    .h(px(ROW_HEIGHT))
                    .into_any_element()
            } else {
                render_unified_line(line, hl)
                    .h(px(ROW_HEIGHT))
                    .into_any_element()
            }
        }
    }
}

// --- Line rendering (unchanged logic, just fixed height) ---

fn render_side_by_side_line(
    line: &crate::git_diff::DiffLine,
    highlighted: Option<&HighlightedLine>,
) -> Div {
    let gutter_width = px(50.);

    match line.kind {
        DiffLineKind::Context => {
            let old_no = line
                .old_line_no
                .map(|n| format!("{}", n))
                .unwrap_or_default();
            let new_no = line
                .new_line_no
                .map(|n| format!("{}", n))
                .unwrap_or_default();

            div()
                .flex()
                .flex_row()
                .child(
                    div()
                        .w(gutter_width)
                        .flex_shrink_0()
                        .text_color(rgb(0x6d6d6d))
                        .text_right()
                        .pr(px(8.))
                        .child(SharedString::from(old_no)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .px(px(4.))
                        .overflow_hidden()
                        .child(line_content_element(highlighted, &line.content)),
                )
                .child(div().w(px(1.)).flex_shrink_0().bg(rgb(0x3d3d3d)))
                .child(
                    div()
                        .w(gutter_width)
                        .flex_shrink_0()
                        .text_color(rgb(0x6d6d6d))
                        .text_right()
                        .pr(px(8.))
                        .child(SharedString::from(new_no)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .px(px(4.))
                        .overflow_hidden()
                        .child(line_content_element(highlighted, &line.content)),
                )
        }
        DiffLineKind::Removed => {
            let old_no = line
                .old_line_no
                .map(|n| format!("{}", n))
                .unwrap_or_default();

            div()
                .flex()
                .flex_row()
                .child(
                    div()
                        .w(gutter_width)
                        .flex_shrink_0()
                        .text_color(rgb(0x6d6d6d))
                        .text_right()
                        .pr(px(8.))
                        .bg(rgba(0xf4433615u32))
                        .child(SharedString::from(old_no)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .px(px(4.))
                        .bg(rgba(0xf4433620u32))
                        .overflow_hidden()
                        .child(line_content_element(highlighted, &line.content)),
                )
                .child(div().w(px(1.)).flex_shrink_0().bg(rgb(0x3d3d3d)))
                .child(div().w(gutter_width).flex_shrink_0())
                .child(div().flex_1().min_w(px(0.)))
        }
        DiffLineKind::Added => {
            let new_no = line
                .new_line_no
                .map(|n| format!("{}", n))
                .unwrap_or_default();

            div()
                .flex()
                .flex_row()
                .child(div().w(gutter_width).flex_shrink_0())
                .child(div().flex_1().min_w(px(0.)))
                .child(div().w(px(1.)).flex_shrink_0().bg(rgb(0x3d3d3d)))
                .child(
                    div()
                        .w(gutter_width)
                        .flex_shrink_0()
                        .text_color(rgb(0x6d6d6d))
                        .text_right()
                        .pr(px(8.))
                        .bg(rgba(0x4caf5015u32))
                        .child(SharedString::from(new_no)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .px(px(4.))
                        .bg(rgba(0x4caf5020u32))
                        .overflow_hidden()
                        .child(line_content_element(highlighted, &line.content)),
                )
        }
    }
}

fn render_unified_line(
    line: &crate::git_diff::DiffLine,
    highlighted: Option<&HighlightedLine>,
) -> Div {
    let gutter_width = px(50.);
    let old_no = line
        .old_line_no
        .map(|n| format!("{}", n))
        .unwrap_or_default();
    let new_no = line
        .new_line_no
        .map(|n| format!("{}", n))
        .unwrap_or_default();

    let (prefix, bg) = match line.kind {
        DiffLineKind::Context => (" ", None),
        DiffLineKind::Added => ("+", Some(rgba(0x4caf5020u32))),
        DiffLineKind::Removed => ("-", Some(rgba(0xf4433620u32))),
    };

    let content_element = if let Some(hl) = highlighted {
        let prefix_color: Hsla = rgb(0x888888).into();
        let mut full_text = prefix.to_string();
        let mut runs = Vec::with_capacity(1 + hl.spans.len());
        let mono = mono_font();

        runs.push(TextRun {
            len: prefix.len(),
            font: mono.clone(),
            color: prefix_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        });

        for span in &hl.spans {
            if span.text.is_empty() {
                continue;
            }
            full_text.push_str(&span.text);
            let (r, g, b, _a) = span.fg;
            runs.push(TextRun {
                len: span.text.len(),
                font: mono.clone(),
                color: Rgba {
                    r: r as f32 / 255.0,
                    g: g as f32 / 255.0,
                    b: b as f32 / 255.0,
                    a: 1.0,
                }
                .into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            });
        }

        StyledText::new(SharedString::from(full_text))
            .with_runs(runs)
            .into_any_element()
    } else {
        let default_color: Hsla = rgb(0xcccccc).into();
        styled_text_plain(&format!("{}{}", prefix, &line.content), default_color)
            .into_any_element()
    };

    let row = div()
        .flex()
        .flex_row()
        .child(
            div()
                .w(gutter_width)
                .flex_shrink_0()
                .text_color(rgb(0x6d6d6d))
                .text_right()
                .pr(px(4.))
                .child(SharedString::from(old_no)),
        )
        .child(
            div()
                .w(gutter_width)
                .flex_shrink_0()
                .text_color(rgb(0x6d6d6d))
                .text_right()
                .pr(px(8.))
                .child(SharedString::from(new_no)),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .px(px(4.))
                .overflow_hidden()
                .child(content_element),
        );

    if let Some(bg_color) = bg {
        row.bg(bg_color)
    } else {
        row
    }
}

fn render_hunk_header(
    hunk_idx: usize,
    header: &str,
    on_reject: Option<&RejectHunkCallback>,
) -> Stateful<Div> {
    let mut row = div()
        .id(ElementId::Integer(hunk_idx as u64 + 10000))
        .flex()
        .flex_row()
        .items_center()
        .px(px(8.))
        .bg(rgb(0x2a2d37))
        .border_b_1()
        .border_color(rgb(0x3d3d3d))
        .child(
            div()
                .flex_1()
                .text_size(px(11.))
                .text_color(rgb(0x888888))
                .font_family("monospace")
                .overflow_hidden()
                .text_ellipsis()
                .child(SharedString::from(header.to_string())),
        );

    if let Some(cb) = on_reject {
        let cb = cb.clone();
        row = row.child(
            div()
                .id(ElementId::Integer(hunk_idx as u64 + 20000))
                .ml(px(8.))
                .px(px(8.))
                .py(px(2.))
                .rounded(px(3.))
                .bg(rgba(0xf4433630u32))
                .text_color(rgb(0xf44336))
                .text_size(px(10.))
                .font_weight(FontWeight::MEDIUM)
                .cursor_pointer()
                .hover(|s: StyleRefinement| s.bg(rgba(0xf4433650u32)))
                .on_click(move |_event, window, cx| {
                    cb(hunk_idx, window, cx);
                })
                .child("Reject"),
        );
    }

    row
}
