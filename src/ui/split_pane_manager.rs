//! SplitPaneManager - manages split layout tree, pane focus, and divider drag.
//!
//! Extracted from AppRoot Phase 5.

use crate::split_tree::SplitNode;
use std::sync::{Arc, Mutex};
use std::sync::atomic::AtomicBool;

pub struct SplitPaneManager {
    pub split_tree: SplitNode,
    pub focused_pane_index: usize,
    pub divider_drag: Option<(Vec<bool>, f32, f32, bool)>,
    pub active_target: Option<String>,
    pub active_target_shared: Arc<Mutex<String>>,
    pub targets_shared: Arc<Mutex<Vec<String>>>,
    pub dragging: Arc<AtomicBool>,
}

impl SplitPaneManager {
    pub fn new() -> Self {
        Self {
            split_tree: SplitNode::pane(""),
            focused_pane_index: 0,
            divider_drag: None,
            active_target: None,
            active_target_shared: Arc::new(Mutex::new(String::new())),
            targets_shared: Arc::new(Mutex::new(Vec::new())),
            dragging: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Get pane count from split tree.
    pub fn pane_count(&self) -> usize {
        self.split_tree.flatten().len()
    }

    /// Focus a specific pane by index.
    pub fn focus_pane(&mut self, index: usize) {
        let panes = self.split_tree.flatten();
        if index < panes.len() {
            self.focused_pane_index = index;
            let (target, _) = &panes[index];
            self.active_target = Some(target.clone());
            if let Ok(mut guard) = self.active_target_shared.lock() {
                *guard = target.clone();
            }
        }
    }

    /// Update shared targets list from current split tree.
    pub fn sync_targets(&self) {
        let targets: Vec<String> = self.split_tree.flatten().into_iter().map(|(t, _)| t).collect();
        if let Ok(mut guard) = self.targets_shared.lock() {
            *guard = targets;
        }
    }
}
