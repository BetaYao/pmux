// ui/terminal_controller.rs - Debounced window bounds → terminal resize
// Resize is driven from window bounds observer, NOT from layout/prepaint/paint.

/// Debounced resize controller. Computes (cols, rows) from window bounds;
/// maybe_resize returns Some when dimensions changed (debounced).
pub struct ResizeController {
    last_cols: Option<u16>,
    last_rows: Option<u16>,
    pending: bool,
}

impl ResizeController {
    pub fn new() -> Self {
        Self {
            last_cols: None,
            last_rows: None,
            pending: false,
        }
    }

    /// Compute terminal dimensions from window bounds.
    /// Assumes 8px char width, 16px char height (configurable later).
    pub fn compute_dims_from_bounds(
        w: f32,
        h: f32,
        sidebar_visible: bool,
        sidebar_w: f32,
    ) -> (u16, u16) {
        let content_w = if sidebar_visible {
            (w - sidebar_w).max(80.0)
        } else {
            w.max(80.0)
        };
        let char_w = 8.0;
        let char_h = 16.0;
        let cols = (content_w / char_w).floor() as u16;
        let rows = (h / char_h).floor() as u16;
        (cols.max(10), rows.max(3))
    }

    /// If (cols, rows) differ from last, return Some((cols, rows)) and update last.
    /// Otherwise return None.
    pub fn maybe_resize(&mut self, cols: u16, rows: u16) -> Option<(u16, u16)> {
        let changed = self
            .last_cols
            .map_or(true, |c| c != cols)
            || self.last_rows.map_or(true, |r| r != rows);
        if changed {
            self.last_cols = Some(cols);
            self.last_rows = Some(rows);
            Some((cols, rows))
        } else {
            None
        }
    }

    pub fn set_pending(&mut self, pending: bool) {
        self.pending = pending;
    }

    pub fn is_pending(&self) -> bool {
        self.pending
    }
}

impl Default for ResizeController {
    fn default() -> Self {
        Self::new()
    }
}
