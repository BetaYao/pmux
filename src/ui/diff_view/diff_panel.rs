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
    /// Side-by-side paired row: removed line on left, added line on right.
    /// Either side can be None (extra removed or extra added with no counterpart).
    PairedLine {
        hunk_idx: usize,
        left_line_idx: Option<usize>,
        left_highlight_idx: Option<usize>,
        right_line_idx: Option<usize>,
        right_highlight_idx: Option<usize>,
    },
}

/// Row height in pixels — must be uniform for uniform_list.
const ROW_HEIGHT: f32 = 20.;
/// Gutter width for line numbers.
const GUTTER_W: f32 = 44.;
/// Separator color between left/right panels.
const SEP_COLOR: u32 = 0x3d3d3d;

// --- Diff line background colors ---
const REMOVED_LINE_BG: u32 = 0xff555530;
const ADDED_LINE_BG: u32 = 0x2ea04330;
const REMOVED_GUTTER_BG: u32 = 0xff555520;
const ADDED_GUTTER_BG: u32 = 0x2ea04320;
const REMOVED_EMPTY_BG: u32 = 0xff555512;
const ADDED_EMPTY_BG: u32 = 0x2ea04312;
const REMOVED_INLINE_HL: u32 = 0xff555570;
const ADDED_INLINE_HL: u32 = 0x2ea04370;

/// Build the flat row index from a FileDiff (unified mode: one row per diff line).
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

/// Build flat rows for side-by-side mode: pairs consecutive removed/added lines
/// so they appear on the same row (removed on left, added on right).
pub fn build_flat_rows_sbs(diff: &FileDiff) -> Vec<DiffFlatRow> {
    let mut rows = Vec::new();
    let mut highlight_idx = 0;

    for (hunk_idx, hunk) in diff.hunks.iter().enumerate() {
        rows.push(DiffFlatRow::HunkHeader(hunk_idx));
        let mut i = 0;
        while i < hunk.lines.len() {
            match hunk.lines[i].kind {
                DiffLineKind::Context => {
                    rows.push(DiffFlatRow::Line { hunk_idx, line_idx: i, highlight_idx });
                    highlight_idx += 1;
                    i += 1;
                }
                DiffLineKind::Removed | DiffLineKind::Added => {
                    // Collect consecutive removed lines
                    let mut removed: Vec<(usize, usize)> = Vec::new();
                    while i < hunk.lines.len() && hunk.lines[i].kind == DiffLineKind::Removed {
                        removed.push((i, highlight_idx));
                        highlight_idx += 1;
                        i += 1;
                    }
                    // Collect consecutive added lines
                    let mut added: Vec<(usize, usize)> = Vec::new();
                    while i < hunk.lines.len() && hunk.lines[i].kind == DiffLineKind::Added {
                        added.push((i, highlight_idx));
                        highlight_idx += 1;
                        i += 1;
                    }
                    // Pair them: removed[j] with added[j], filling None for the shorter side
                    let max = removed.len().max(added.len());
                    for j in 0..max {
                        rows.push(DiffFlatRow::PairedLine {
                            hunk_idx,
                            left_line_idx: removed.get(j).map(|(idx, _)| *idx),
                            left_highlight_idx: removed.get(j).map(|(_, hi)| *hi),
                            right_line_idx: added.get(j).map(|(idx, _)| *idx),
                            right_highlight_idx: added.get(j).map(|(_, hi)| *hi),
                        });
                    }
                }
            }
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

/// Compute inline diff between old and new text using common prefix/suffix.
/// Returns (old_changed_ranges, new_changed_ranges) as byte offset pairs.
fn compute_inline_diff(old: &str, new: &str) -> (Vec<(usize, usize)>, Vec<(usize, usize)>) {
    if old == new {
        return (vec![], vec![]);
    }
    if old.is_empty() {
        return (vec![], if new.is_empty() { vec![] } else { vec![(0, new.len())] });
    }
    if new.is_empty() {
        return (vec![(0, old.len())], vec![]);
    }

    let old_bytes = old.as_bytes();
    let new_bytes = new.as_bytes();

    // Common prefix length (bytes)
    let prefix_len = old_bytes
        .iter()
        .zip(new_bytes.iter())
        .take_while(|(a, b)| a == b)
        .count();

    // Common suffix length (not overlapping prefix)
    let old_rest = old.len() - prefix_len;
    let new_rest = new.len() - prefix_len;
    let suffix_len = old_bytes[prefix_len..]
        .iter()
        .rev()
        .zip(new_bytes[prefix_len..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count()
        .min(old_rest)
        .min(new_rest);

    let old_end = old.len() - suffix_len;
    let new_end = new.len() - suffix_len;

    if prefix_len >= old_end && prefix_len >= new_end {
        return (vec![], vec![]);
    }

    let mut old_ranges = Vec::new();
    let mut new_ranges = Vec::new();
    if prefix_len < old_end {
        old_ranges.push((prefix_len, old_end));
    }
    if prefix_len < new_end {
        new_ranges.push((prefix_len, new_end));
    }
    (old_ranges, new_ranges)
}

/// Build StyledText with inline highlight backgrounds on changed byte ranges.
fn styled_text_with_inline_hl(
    highlighted: &HighlightedLine,
    changed_ranges: &[(usize, usize)],
    hl_bg: Hsla,
) -> StyledText {
    let mut full_text = String::new();
    let mut runs = Vec::new();
    let mono = mono_font();
    let mut byte_offset: usize = 0;

    for span in &highlighted.spans {
        if span.text.is_empty() {
            continue;
        }
        let span_start = byte_offset;
        let span_end = byte_offset + span.text.len();
        full_text.push_str(&span.text);

        let (r, g, b, _) = span.fg;
        let fg: Hsla = Rgba {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
            a: 1.0,
        }
        .into();

        // Mark span as highlighted if it overlaps any changed range
        let has_overlap = changed_ranges
            .iter()
            .any(|&(rs, re)| span_start < re && span_end > rs);

        runs.push(TextRun {
            len: span.text.len(),
            font: mono.clone(),
            color: fg,
            background_color: if has_overlap { Some(hl_bg) } else { None },
            underline: None,
            strikethrough: None,
        });
        byte_offset = span_end;
    }

    if full_text.is_empty() {
        full_text.push(' ');
        runs.push(TextRun {
            len: 1,
            font: mono,
            color: Rgba { r: 0.5, g: 0.5, b: 0.5, a: 1.0 }.into(),
            background_color: None,
            underline: None,
            strikethrough: None,
        });
    }

    StyledText::new(SharedString::from(full_text)).with_runs(runs)
}

/// Line content element with inline diff highlights.
fn line_content_with_inline_hl(
    highlighted: Option<&HighlightedLine>,
    fallback: &str,
    changed_ranges: &[(usize, usize)],
    hl_bg: Hsla,
) -> AnyElement {
    if let Some(hl) = highlighted {
        if changed_ranges.is_empty() {
            styled_text_from_highlight(hl).into_any_element()
        } else {
            styled_text_with_inline_hl(hl, changed_ranges, hl_bg).into_any_element()
        }
    } else {
        let default_color: Hsla = rgb(0xcccccc).into();
        styled_text_plain(fallback, default_color).into_any_element()
    }
}

// --- Side-by-side layout helpers ---
// Each SBS row has exactly TWO flex children (left half + right half).
// The separator is a border_r on the left half — no third element.
// Both halves use flex_1 + min_w(0) for guaranteed equal split.

/// Left half container: flex_1, border_r as separator.
fn sbs_left() -> Div {
    div()
        .flex_1()
        .min_w(px(0.))
        .flex()
        .flex_row()
        .overflow_hidden()
        .border_r_1()
        .border_color(rgb(SEP_COLOR))
}

/// Right half container: flex_1, no border.
fn sbs_right() -> Div {
    div()
        .flex_1()
        .min_w(px(0.))
        .flex()
        .flex_row()
        .overflow_hidden()
}

/// Line-number gutter.
fn sbs_gutter(line_no: String) -> Div {
    div()
        .w(px(GUTTER_W))
        .flex_shrink_0()
        .text_color(rgb(0x636d83))
        .text_size(px(11.))
        .text_right()
        .pr(px(6.))
        .child(SharedString::from(line_no))
}

/// Content area (fills remaining width, clips overflow).
fn sbs_content(hl: Option<&HighlightedLine>, text: &str) -> Div {
    div()
        .flex_1()
        .min_w(px(0.))
        .pl(px(6.))
        .pr(px(4.))
        .overflow_hidden()
        .child(line_content_element(hl, text))
}

/// Content area with inline diff highlights.
fn sbs_content_hl(
    hl: Option<&HighlightedLine>,
    text: &str,
    changed_ranges: &[(usize, usize)],
    hl_bg: Hsla,
) -> Div {
    div()
        .flex_1()
        .min_w(px(0.))
        .pl(px(6.))
        .pr(px(4.))
        .overflow_hidden()
        .child(line_content_with_inline_hl(hl, text, changed_ranges, hl_bg))
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
                render_sbs_context(line, hl)
                    .h(px(ROW_HEIGHT))
                    .into_any_element()
            } else {
                render_unified_line(line, hl)
                    .h(px(ROW_HEIGHT))
                    .into_any_element()
            }
        }
        DiffFlatRow::PairedLine {
            hunk_idx,
            left_line_idx,
            left_highlight_idx,
            right_line_idx,
            right_highlight_idx,
        } => {
            let hunk = &diff.hunks[*hunk_idx];
            render_sbs_paired(
                hunk,
                *left_line_idx,
                left_highlight_idx.and_then(|hi| highlighted.and_then(|h| h.get(hi))),
                *right_line_idx,
                right_highlight_idx.and_then(|hi| highlighted.and_then(|h| h.get(hi))),
            )
            .h(px(ROW_HEIGHT))
            .into_any_element()
        }
    }
}

// --- Side-by-side row renderers ---

/// Context line in SBS mode: same content on both sides.
fn render_sbs_context(
    line: &crate::git_diff::DiffLine,
    highlighted: Option<&HighlightedLine>,
) -> Div {
    let old_no = line.old_line_no.map(|n| format!("{}", n)).unwrap_or_default();
    let new_no = line.new_line_no.map(|n| format!("{}", n)).unwrap_or_default();

    div()
        .w_full()
        .flex()
        .flex_row()
        .overflow_hidden()
        .child(
            sbs_left()
                .child(sbs_gutter(old_no))
                .child(sbs_content(highlighted, &line.content)),
        )
        .child(
            sbs_right()
                .child(sbs_gutter(new_no))
                .child(sbs_content(highlighted, &line.content)),
        )
}

/// Paired line in SBS mode: removed on left, added on right (either can be None).
fn render_sbs_paired(
    hunk: &crate::git_diff::DiffHunk,
    left_line_idx: Option<usize>,
    left_hl: Option<&HighlightedLine>,
    right_line_idx: Option<usize>,
    right_hl: Option<&HighlightedLine>,
) -> Div {
    // Compute inline diff if both sides present
    let (left_ranges, right_ranges) = match (left_line_idx, right_line_idx) {
        (Some(li), Some(ri)) => {
            compute_inline_diff(&hunk.lines[li].content, &hunk.lines[ri].content)
        }
        _ => (vec![], vec![]),
    };

    let removed_inline_bg: Hsla = rgba(REMOVED_INLINE_HL).into();
    let added_inline_bg: Hsla = rgba(ADDED_INLINE_HL).into();

    // Left half: removed line or empty
    let left = if let Some(idx) = left_line_idx {
        let line = &hunk.lines[idx];
        let no = line.old_line_no.map(|n| format!("{}", n)).unwrap_or_default();
        sbs_left()
            .child(sbs_gutter(no).bg(rgba(REMOVED_GUTTER_BG)))
            .child(
                sbs_content_hl(left_hl, &line.content, &left_ranges, removed_inline_bg)
                    .bg(rgba(REMOVED_LINE_BG)),
            )
    } else {
        sbs_left().bg(rgba(REMOVED_EMPTY_BG))
    };

    // Right half: added line or empty
    let right = if let Some(idx) = right_line_idx {
        let line = &hunk.lines[idx];
        let no = line.new_line_no.map(|n| format!("{}", n)).unwrap_or_default();
        sbs_right()
            .child(sbs_gutter(no).bg(rgba(ADDED_GUTTER_BG)))
            .child(
                sbs_content_hl(right_hl, &line.content, &right_ranges, added_inline_bg)
                    .bg(rgba(ADDED_LINE_BG)),
            )
    } else {
        sbs_right().bg(rgba(ADDED_EMPTY_BG))
    };

    div().w_full().flex().flex_row().overflow_hidden().child(left).child(right)
}

// --- Unified mode ---

fn render_unified_line(
    line: &crate::git_diff::DiffLine,
    highlighted: Option<&HighlightedLine>,
) -> Div {
    let gutter_width = px(GUTTER_W);
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
        DiffLineKind::Added => ("+", Some(rgba(ADDED_LINE_BG))),
        DiffLineKind::Removed => ("-", Some(rgba(REMOVED_LINE_BG))),
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
        .overflow_hidden()
        .child(
            div()
                .w(gutter_width)
                .flex_shrink_0()
                .text_color(rgb(0x636d83))
                .text_size(px(11.))
                .text_right()
                .pr(px(4.))
                .child(SharedString::from(old_no)),
        )
        .child(
            div()
                .w(gutter_width)
                .flex_shrink_0()
                .text_color(rgb(0x636d83))
                .text_size(px(11.))
                .text_right()
                .pr(px(6.))
                .child(SharedString::from(new_no)),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .pl(px(6.))
                .pr(px(4.))
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
        .px(px(10.))
        .bg(rgb(0x1e2d3d))
        .border_b_1()
        .border_color(rgb(0x2a3a4a))
        .child(
            div()
                .flex_1()
                .text_size(px(11.))
                .text_color(rgb(0x7c8599))
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
