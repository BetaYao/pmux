//! TerminalManager - manages terminal buffers, resize, focus, IME, and search.
//!
//! Extracted from AppRoot Phase 4.
//! Observes RuntimeManager for runtime reference.

use crate::ui::terminal_controller::ResizeController;
use crate::ui::terminal_view::TerminalBuffer;
use crate::ui::terminal_area_entity::TerminalAreaEntity;
use gpui::*;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::sync::atomic::AtomicBool;

pub struct TerminalManager {
    pub buffers: Arc<Mutex<HashMap<String, TerminalBuffer>>>,
    pub focus: Option<FocusHandle>,
    pub needs_focus: bool,
    pub resize_controller: ResizeController,
    pub preferred_dims: Option<(u16, u16)>,
    pub shared_dims: Arc<Mutex<Option<(u16, u16)>>>,
    pub ime_pending_enter: Arc<AtomicBool>,
    pub area_entity: Option<Entity<TerminalAreaEntity>>,
    // Search
    pub search_active: bool,
    pub search_query: String,
    pub search_current_match: usize,
}

impl TerminalManager {
    pub fn new() -> Self {
        Self {
            buffers: Arc::new(Mutex::new(HashMap::new())),
            focus: None,
            needs_focus: false,
            resize_controller: ResizeController::new(),
            preferred_dims: None,
            shared_dims: Arc::new(Mutex::new(None)),
            ime_pending_enter: Arc::new(AtomicBool::new(false)),
            area_entity: None,
            search_active: false,
            search_query: String::new(),
            search_current_match: 0,
        }
    }

    pub fn ensure_focus(&mut self, cx: &mut Context<Self>) {
        if self.focus.is_none() {
            self.focus = Some(cx.focus_handle());
        }
    }

    /// Clean up terminal buffers for a workspace prefix (on switch/close).
    pub fn cleanup_buffers_for_prefix(&mut self, prefix: &str) {
        if let Ok(mut buffers) = self.buffers.lock() {
            let colon_prefix = format!("{}:", prefix);
            buffers.retain(|k, _| k != prefix && !k.starts_with(&colon_prefix));
        }
    }

    /// Toggle search bar visibility.
    pub fn toggle_search(&mut self) {
        self.search_active = !self.search_active;
        if !self.search_active {
            self.search_query.clear();
            self.search_current_match = 0;
        }
    }

    /// Activate search mode.
    pub fn start_search(&mut self) {
        self.search_active = true;
        self.search_query.clear();
        self.search_current_match = 0;
    }

    /// Deactivate search mode.
    pub fn stop_search(&mut self) {
        self.search_active = false;
        self.search_query.clear();
        self.search_current_match = 0;
    }
}
