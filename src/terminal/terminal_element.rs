//! Custom GPUI Element for rendering a terminal grid.

use crate::terminal::colors::ColorPalette;
use crate::terminal::terminal_core::{DetectedLink, SearchMatch, Terminal, TerminalSize};
use crate::terminal::terminal_rendering::{BatchedTextRun, LayoutRect, is_default_bg};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line, Point as AlacPoint, Side};
use alacritty_terminal::selection::{SelectionRange, SelectionType};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::TermMode;
use gpui::*;
use std::sync::Arc;

pub struct TerminalElement {
    terminal: Arc<Terminal>,
    focus_handle: FocusHandle,
    palette: ColorPalette,
    on_resize: Option<Box<dyn Fn(u16, u16) + Send + Sync>>,
    on_input: Option<Arc<dyn Fn(&[u8]) + Send + Sync>>,
    style: StyleRefinement,
    search_matches: Vec<SearchMatch>,
    search_current: Option<usize>,
    links: Vec<DetectedLink>,
    hovered_link: Option<usize>,
    focused: bool,
    selection_range: Option<SelectionRange>,
}

impl TerminalElement {
    pub fn new(terminal: Arc<Terminal>, focus_handle: FocusHandle, palette: ColorPalette) -> Self {
        Self {
            terminal,
            focus_handle,
            palette,
            on_resize: None,
            on_input: None,
            style: StyleRefinement::default(),
            search_matches: Vec::new(),
            search_current: None,
            links: Vec::new(),
            hovered_link: None,
            focused: true,
            selection_range: None,
        }
    }

    pub fn with_resize_callback(mut self, cb: impl Fn(u16, u16) + Send + Sync + 'static) -> Self {
        self.on_resize = Some(Box::new(cb));
        self
    }

    pub fn with_search(mut self, matches: Vec<SearchMatch>, current: Option<usize>) -> Self {
        self.search_matches = matches;
        self.search_current = current;
        self
    }

    pub fn with_links(mut self, links: Vec<DetectedLink>, hovered: Option<usize>) -> Self {
        self.links = links;
        self.hovered_link = hovered;
        self
    }

    pub fn with_input_handler(mut self, f: Arc<dyn Fn(&[u8]) + Send + Sync>) -> Self {
        self.on_input = Some(f);
        self
    }

    pub fn with_focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    pub fn with_selection(mut self, range: Option<SelectionRange>) -> Self {
        self.selection_range = range;
        self
    }
}

impl IntoElement for TerminalElement {
    type Element = Self;
    fn into_element(self) -> Self::Element {
        self
    }
}

pub struct TerminalElementState {
    pub cell_width: Pixels,
    pub line_height: Pixels,
    pub font_size: Pixels,
    pub font: Font,
    pub font_bold: Font,
    pub font_italic: Font,
    pub font_bold_italic: Font,
    pub cols: usize,
    pub rows: usize,
}

const FONT_FAMILY: &str = "Menlo";
fn font_size() -> Pixels {
    px(12.0)
}

fn make_font(weight: FontWeight, style: FontStyle) -> Font {
    use gpui::FontFallbacks;
    Font {
        family: FONT_FAMILY.into(),
        features: FontFeatures::default(),
        fallbacks: Some(FontFallbacks::from_fonts(vec![
            "Apple Symbols".into(),
            "Apple Color Emoji".into(),
            "Helvetica Neue".into(),
        ])),
        weight,
        style,
    }
}

fn pixel_to_grid(
    mouse_pos: Point<Pixels>,
    origin: Point<Pixels>,
    cell_width: Pixels,
    line_height: Pixels,
    display_offset: usize,
    cols: usize,
    rows: usize,
) -> (AlacPoint, Side) {
    let rel_x: f32 = (mouse_pos.x - origin.x).into();
    let rel_y: f32 = (mouse_pos.y - origin.y).into();
    let cell_w: f32 = cell_width.into();
    let line_h: f32 = line_height.into();

    let col_f = (rel_x / cell_w).max(0.0);
    let col = (col_f as usize).min(cols.saturating_sub(1));
    let side = if col_f.fract() < 0.5 { Side::Left } else { Side::Right };

    let row_f = (rel_y / line_h).max(0.0);
    let row = (row_f as usize).min(rows.saturating_sub(1));

    let line = Line(row as i32 - display_offset as i32);
    (AlacPoint::new(line, Column(col)), side)
}

fn is_mouse_mode(mode: TermMode) -> bool {
    mode.contains(TermMode::MOUSE_REPORT_CLICK)
        || mode.contains(TermMode::MOUSE_DRAG)
        || mode.contains(TermMode::MOUSE_MOTION)
}

impl Element for TerminalElement {
    type RequestLayoutState = Style;
    type PrepaintState = TerminalElementState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.refine(&self.style);
        let layout_id = window.request_layout(style.clone(), [], cx);
        (layout_id, style)
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        _cx: &mut App,
    ) -> Self::PrepaintState {
        use std::cell::Cell;
        thread_local! {
            static CELL_SIZE_CACHE: Cell<Option<(Pixels, Pixels)>> = const { Cell::new(None) };
        }

        let (cell_width, line_height) = CELL_SIZE_CACHE.with(|cache| {
            if let Some(cached) = cache.get() {
                return cached;
            }
            // Use Menlo without fallbacks for measurement so cell_width is always
            // based on Menlo's monospace advance — fallback fonts may have different metrics.
            let font_no_fallback = Font {
                family: FONT_FAMILY.into(),
                features: FontFeatures::default(),
                fallbacks: None,
                weight: FontWeight::NORMAL,
                style: FontStyle::Normal,
            };
            // Measure 10 identical ASCII chars and divide; more accurate than a single char.
            let text_run = TextRun {
                len: 10,
                font: font_no_fallback.clone(),
                color: gpui::black(),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let fs = font_size();
            let shaped = window
                .text_system()
                .shape_line("MMMMMMMMMM".into(), fs, &[text_run], None);

            let cw = if shaped.width > px(0.0) { shaped.width / 10.0 } else { fs * 0.6 };
            let lh = if shaped.ascent + shaped.descent > px(0.0) {
                (shaped.ascent + shaped.descent).ceil()
            } else {
                fs * 1.4
            };
            cache.set(Some((cw, lh)));
            (cw, lh)
        });

        let width_f32: f32 = bounds.size.width.into();
        let height_f32: f32 = bounds.size.height.into();
        let cell_width_f32: f32 = cell_width.into();
        let line_height_f32: f32 = line_height.into();

        let cols = ((width_f32 / cell_width_f32) as usize).max(1);
        let rows = ((height_f32 / line_height_f32) as usize).max(1);

        let current_size = self.terminal.size();
        if current_size.cols as usize != cols || current_size.rows as usize != rows {
            let new_size = TerminalSize {
                cols: cols as u16,
                rows: rows as u16,
                cell_width: cell_width_f32,
                cell_height: line_height_f32,
            };
            self.terminal.resize(new_size);
            if let Some(ref cb) = self.on_resize {
                cb(cols as u16, rows as u16);
            }
        }

        TerminalElementState {
            cell_width,
            line_height,
            font_size: font_size(),
            font: make_font(FontWeight::NORMAL, FontStyle::Normal),
            font_bold: make_font(FontWeight::BOLD, FontStyle::Normal),
            font_italic: make_font(FontWeight::NORMAL, FontStyle::Italic),
            font_bold_italic: make_font(FontWeight::BOLD, FontStyle::Italic),
            cols,
            rows,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        state: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let origin = bounds.origin;
        let cell_width = state.cell_width;
        let line_height = state.line_height;
        let font_size = state.font_size;


        let default_bg = self.palette.background();
        window.paint_quad(quad(
            bounds,
            px(0.0),
            default_bg,
            Edges::default(),
            transparent_black(),
            Default::default(),
        ));

        let (layout_rects, text_runs) = self.terminal.with_content(|term| {
            let grid = term.grid();
            let num_lines = grid.screen_lines();
            let num_cols = grid.columns();
            let display_offset = grid.display_offset() as i32;
            let colors = term.colors();



            let mut layout_rects: Vec<LayoutRect> = Vec::new();
            let mut text_runs: Vec<BatchedTextRun> = Vec::new();

            for line_idx in 0..num_lines {
                let line = Line(line_idx as i32 - display_offset);

                let mut current_bg: Option<LayoutRect> = None;
                let mut current_run: Option<BatchedTextRun> = None;

                for col_idx in 0..num_cols {
                    let col = Column(col_idx);
                    let point = AlacPoint::new(line, col);
                    let cell = &grid[point];

                    if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                        continue;
                    }

                    // Fast-path: skip blank cells with default bg and no decorations
                    if (cell.c == ' ' || cell.c == '\0')
                        && is_default_bg(&cell.bg)
                        && !cell.flags.contains(Flags::UNDERLINE)
                        && !cell.flags.contains(Flags::STRIKEOUT)
                        && !cell.flags.contains(Flags::INVERSE)
                    {
                        if let Some(run) = current_run.take() {
                            text_runs.push(run);
                        }
                        if let Some(rect) = current_bg.take() {
                            layout_rects.push(rect);
                        }
                        continue;
                    }

                    let mut fg = cell.fg;
                    let mut bg = cell.bg;
                    if cell.flags.contains(Flags::INVERSE) {
                        std::mem::swap(&mut fg, &mut bg);
                    }

                    let fg_hsla = self.palette.resolve(fg, colors);
                    let bg_hsla = self.palette.resolve(bg, colors);

                    let ch = if cell.c == ' ' || cell.c == '\0' {
                        ' '
                    } else {
                        cell.c
                    };

                    if !is_default_bg(&bg) {
                        if let Some(ref mut rect) = current_bg {
                            if rect.color == bg_hsla
                                && rect.start_col + rect.num_cells as i32 == col_idx as i32
                            {
                                rect.extend();
                            } else {
                                layout_rects.push(std::mem::replace(
                                    rect,
                                    LayoutRect::new(line_idx as i32, col_idx as i32, bg_hsla),
                                ));
                            }
                        } else {
                            current_bg = Some(LayoutRect::new(
                                line_idx as i32,
                                col_idx as i32,
                                bg_hsla,
                            ));
                        }
                    } else if let Some(rect) = current_bg.take() {
                        layout_rects.push(rect);
                    }

                    // Wide characters occupy 2 terminal columns; extend background rect to cover the spacer column
                    if cell.flags.contains(Flags::WIDE_CHAR) {
                        if let Some(ref mut rect) = current_bg {
                            rect.extend();
                        }
                    }

                    let has_decorations = cell.flags.contains(Flags::UNDERLINE)
                        || cell.flags.contains(Flags::STRIKEOUT)
                        || !is_default_bg(&bg);

                    let font = match (cell.flags.contains(Flags::BOLD), cell.flags.contains(Flags::ITALIC)) {
                        (true, true) => state.font_bold_italic.clone(),
                        (true, false) => state.font_bold.clone(),
                        (false, true) => state.font_italic.clone(),
                        (false, false) => state.font.clone(),
                    };

                    let text_run = TextRun {
                        len: ch.len_utf8(),
                        font: font.clone(),
                        color: fg_hsla,
                        background_color: None,
                        underline: if cell.flags.contains(Flags::UNDERLINE) {
                            Some(UnderlineStyle {
                                thickness: px(1.0),
                                color: Some(fg_hsla),
                                wavy: false,
                            })
                        } else {
                            None
                        },
                        strikethrough: if cell.flags.contains(Flags::STRIKEOUT) {
                            Some(StrikethroughStyle {
                                thickness: px(1.0),
                                color: Some(fg_hsla),
                            })
                        } else {
                            None
                        },
                    };

                    if ch != ' ' && ch != '\0' || has_decorations {
                        if let Some(ref mut run) = current_run {
                            if run.can_append(&text_run, line_idx as i32, col_idx as i32) {
                                run.append_char(ch);
                            } else {
                                text_runs.push(std::mem::replace(
                                    run,
                                    BatchedTextRun::new(line_idx as i32, col_idx as i32, ch, text_run),
                                ));
                            }
                        } else {
                            current_run = Some(BatchedTextRun::new(
                                line_idx as i32,
                                col_idx as i32,
                                ch,
                                text_run,
                            ));
                        }
                    } else {
                        if let Some(run) = current_run.take() {
                            text_runs.push(run);
                        }
                    }
                }

                if let Some(rect) = current_bg {
                    layout_rects.push(rect);
                }
                if let Some(run) = current_run {
                    text_runs.push(run);
                }
            }

            (layout_rects, text_runs)
        });

        for rect in layout_rects {
            rect.paint(origin, cell_width, line_height, window);
        }

        if let Some(ref sel_range) = self.selection_range {
            let display_offset = self.terminal.display_offset() as i32;
            let sel_color = Hsla { h: 0.58, s: 0.6, l: 0.5, a: 0.35 };
            let cell_w_f: f32 = cell_width.into();
            let line_h_f: f32 = line_height.into();

            for row in 0..state.rows {
                let grid_line = Line(row as i32 - display_offset);
                let row_start = AlacPoint::new(grid_line, Column(0));
                let row_end = AlacPoint::new(grid_line, Column(state.cols.saturating_sub(1)));

                if sel_range.start <= row_end && sel_range.end >= row_start {
                    let start_col = if grid_line == sel_range.start.line {
                        sel_range.start.column.0
                    } else {
                        0
                    };
                    let end_col = if grid_line == sel_range.end.line {
                        sel_range.end.column.0
                    } else {
                        state.cols.saturating_sub(1)
                    };
                    if start_col <= end_col {
                        let sel_x = origin.x + px(start_col as f32 * cell_w_f);
                        let sel_y = origin.y + px(row as f32 * line_h_f);
                        let sel_w = px((end_col - start_col + 1) as f32 * cell_w_f);
                        window.paint_quad(quad(
                            Bounds::new(
                                Point::new(sel_x, sel_y),
                                Size::new(sel_w, line_height),
                            ),
                            px(0.0),
                            sel_color,
                            Edges::default(),
                            transparent_black(),
                            Default::default(),
                        ));
                    }
                }
            }
        }

        for run in text_runs {
            run.paint(origin, cell_width, line_height, font_size, window, cx);
        }

        let cell_width_f: f32 = cell_width.into();
        let line_height_f: f32 = line_height.into();

        // Paint search match overlays
        for (idx, m) in self.search_matches.iter().enumerate() {
            let is_current = self.search_current == Some(idx);
            let color = if is_current {
                Hsla { h: 0.1, s: 0.9, l: 0.6, a: 0.7 }
            } else {
                Hsla { h: 0.15, s: 1.0, l: 0.7, a: 0.4 }
            };
            let match_x = origin.x + px(m.col as f32 * cell_width_f);
            let match_y = origin.y + px(m.line as f32 * line_height_f);
            window.paint_quad(quad(
                Bounds::new(
                    Point::new(match_x, match_y),
                    Size::new(px(m.len as f32 * cell_width_f), line_height),
                ),
                px(0.0),
                color,
                Edges::default(),
                transparent_black(),
                Default::default(),
            ));
        }

        // Paint URL underlines
        for (idx, link) in self.links.iter().enumerate() {
            let is_hovered = self.hovered_link == Some(idx);
            let color = if is_hovered {
                Hsla { h: 0.55, s: 0.8, l: 0.7, a: 1.0 }
            } else {
                Hsla { h: 0.55, s: 0.6, l: 0.5, a: 0.6 }
            };
            let link_x = origin.x + px(link.col as f32 * cell_width_f);
            let link_y = origin.y + px(link.line as f32 * line_height_f) + line_height - px(1.5);
            window.paint_quad(quad(
                Bounds::new(
                    Point::new(link_x, link_y),
                    Size::new(px(link.len as f32 * cell_width_f), px(1.5)),
                ),
                px(0.0),
                color,
                Edges::default(),
                transparent_black(),
                Default::default(),
            ));
        }

        if self.terminal.mode().contains(TermMode::SHOW_CURSOR) {
            let (cursor_x, cursor_y) = self.terminal.with_content(|term| {
                let grid = term.grid();
                let cursor_point = grid.cursor.point;
                let display_offset = grid.display_offset() as i32;
                let visual_line = cursor_point.line.0 + display_offset;
                let cursor_x = origin.x + cell_width * (cursor_point.column.0 as f32);
                let cursor_y = origin.y + line_height * (visual_line as f32);
                (cursor_x, cursor_y)
            });

            let cursor_color = self.palette.cursor();
            let cursor_bounds = Bounds::new(
                Point::new(cursor_x, cursor_y),
                Size::new(cell_width, line_height),
            );
            window.paint_quad(quad(
                cursor_bounds,
                px(0.0),
                cursor_color,
                Edges::default(),
                transparent_black(),
                Default::default(),
            ));
        }

        // Focus fog: dim unfocused panes
        if !self.focused {
            window.paint_quad(quad(
                Bounds::new(origin, bounds.size),
                px(0.0),
                Hsla { h: 0.0, s: 0.0, l: 0.0, a: 0.15 },
                Edges::default(),
                transparent_black(),
                Default::default(),
            ));
        }

        // Mouse scroll handler
        {
            let terminal = self.terminal.clone();
            let on_input = self.on_input.clone();
            let cw = cell_width;
            let lh = line_height;
            let cols = state.cols;
            let rows = state.rows;
            let b = bounds;

            window.on_mouse_event(move |event: &ScrollWheelEvent, phase, window, _cx| {
                if !phase.bubble() || !b.contains(&event.position) {
                    return;
                }
                let mode = terminal.mode();
                let lh_f: f32 = lh.into();

                if is_mouse_mode(mode) {
                    let delta_lines = match event.delta {
                        ScrollDelta::Lines(d) => d.y as i32,
                        ScrollDelta::Pixels(d) => {
                            if lh_f > 0.0 { (f32::from(d.y) / lh_f) as i32 } else { 0 }
                        }
                    };
                    if delta_lines == 0 {
                        return;
                    }
                    if let Some(ref send) = on_input {
                        let display_offset = terminal.display_offset();
                        let (pt, _) = pixel_to_grid(event.position, b.origin, cw, lh, display_offset, cols, rows);
                        let col = pt.column.0;
                        let row = (pt.line.0 + display_offset as i32).max(0) as usize;
                        let up = delta_lines > 0;
                        for _ in 0..delta_lines.unsigned_abs() {
                            send(&crate::terminal::sgr_mouse_scroll(up, col, row));
                        }
                    }
                } else {
                    match event.delta {
                        ScrollDelta::Lines(d) => {
                            let lines = d.y as i32;
                            if lines != 0 {
                                terminal.scroll_display(lines);
                            }
                        }
                        ScrollDelta::Pixels(d) => {
                            terminal.scroll_display_pixels(f32::from(d.y), lh_f);
                        }
                    }
                    window.refresh();
                }
            });
        }

        // Mouse down handler (selection start)
        {
            let terminal = self.terminal.clone();
            let on_input = self.on_input.clone();
            let cw = cell_width;
            let lh = line_height;
            let cols = state.cols;
            let rows = state.rows;
            let b = bounds;

            window.on_mouse_event(move |event: &MouseDownEvent, phase, window, _cx| {
                if !phase.bubble() || event.button != MouseButton::Left || !b.contains(&event.position) {
                    return;
                }
                let mode = terminal.mode();
                let display_offset = terminal.display_offset();
                let (pt, side) = pixel_to_grid(event.position, b.origin, cw, lh, display_offset, cols, rows);

                if is_mouse_mode(mode) {
                    if let Some(ref send) = on_input {
                        let col = pt.column.0;
                        let row = (pt.line.0 + display_offset as i32).max(0) as usize;
                        send(&crate::terminal::sgr_mouse_press(0, col, row));
                    }
                } else {
                    let sel_type = match event.click_count {
                        2 => SelectionType::Semantic,
                        3 => SelectionType::Lines,
                        _ => SelectionType::Simple,
                    };
                    terminal.clear_selection();
                    terminal.start_selection(pt, side, sel_type);
                    window.refresh();
                }
            });
        }

        // Mouse move handler (selection update / motion reporting)
        {
            let terminal = self.terminal.clone();
            let on_input = self.on_input.clone();
            let cw = cell_width;
            let lh = line_height;
            let cols = state.cols;
            let rows = state.rows;
            let b = bounds;

            window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, _cx| {
                if !phase.bubble() || event.pressed_button != Some(MouseButton::Left) {
                    return;
                }
                let mode = terminal.mode();
                let display_offset = terminal.display_offset();
                let (pt, side) = pixel_to_grid(event.position, b.origin, cw, lh, display_offset, cols, rows);

                if is_mouse_mode(mode) {
                    if let Some(ref send) = on_input {
                        let col = pt.column.0;
                        let row = (pt.line.0 + display_offset as i32).max(0) as usize;
                        send(&crate::terminal::sgr_mouse_motion(0, col, row));
                    }
                } else if terminal.has_selection() {
                    terminal.update_selection(pt, side);
                    window.refresh();
                }
            });
        }

        // Mouse up handler (selection finalize + clipboard)
        {
            let terminal = self.terminal.clone();
            let on_input = self.on_input.clone();
            let cw = cell_width;
            let lh = line_height;
            let cols = state.cols;
            let rows = state.rows;
            let b = bounds;

            window.on_mouse_event(move |event: &MouseUpEvent, phase, _window, cx| {
                if !phase.bubble() || event.button != MouseButton::Left {
                    return;
                }
                let mode = terminal.mode();

                if is_mouse_mode(mode) {
                    if let Some(ref send) = on_input {
                        let display_offset = terminal.display_offset();
                        let (pt, _) = pixel_to_grid(event.position, b.origin, cw, lh, display_offset, cols, rows);
                        let col = pt.column.0;
                        let row = (pt.line.0 + display_offset as i32).max(0) as usize;
                        send(&crate::terminal::sgr_mouse_release(0, col, row));
                    }
                } else if let Some(text) = terminal.selection_text() {
                    if !text.is_empty() {
                        cx.write_to_clipboard(ClipboardItem::new_string(text));
                    }
                }
            });
        }

        // Register InputHandler for text input (IME path).
        // Compute cursor screen bounds here (regardless of SHOW_CURSOR mode)
        // so the IME candidate window can be positioned next to the cursor.
        if self.focused {
            if let Some(ref send_fn) = self.on_input {
                let ime_cursor_bounds = self.terminal.with_content(|term| {
                    let grid = term.grid();
                    let cursor_point = grid.cursor.point;
                    let display_offset = grid.display_offset() as i32;
                    let visual_line = cursor_point.line.0 + display_offset;
                    if visual_line < 0 {
                        return None;
                    }
                    let cx = origin.x + cell_width * (cursor_point.column.0 as f32);
                    let cy = origin.y + line_height * (visual_line as f32);
                    Some(Bounds::new(Point::new(cx, cy), Size::new(cell_width, line_height)))
                });
                let handler = crate::terminal::terminal_input_handler::TerminalInputHandler::new(send_fn.clone())
                    .with_cursor_bounds(ime_cursor_bounds);
                window.handle_input(&self.focus_handle, handler, cx);
            }
        }
    }
}

impl Styled for TerminalElement {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}
