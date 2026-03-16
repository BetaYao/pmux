// ui/delete_worktree_dialog_ui.rs - Delete worktree confirmation dialog
use gpui::prelude::*;
use gpui::*;
use std::sync::Arc;
use crate::worktree::WorktreeInfo;

/// Callback type for confirming deletion
pub type ConfirmDeleteCallback = Arc<dyn Fn(WorktreeInfo, &mut Window, &mut App) + Send + Sync>;

/// Callback type for canceling
pub type CancelDeleteCallback = Arc<dyn Fn(&mut Window, &mut App) + Send + Sync>;

/// Delete Worktree Dialog UI - modal confirmation for worktree removal
pub struct DeleteWorktreeDialogUi {
    worktree: Option<WorktreeInfo>,
    show: bool,
    has_uncommitted: bool,
    error: Option<String>,
    on_confirm: Option<ConfirmDeleteCallback>,
    on_cancel: Option<CancelDeleteCallback>,
}

impl DeleteWorktreeDialogUi {
    pub fn new() -> Self {
        Self {
            worktree: None,
            show: false,
            has_uncommitted: false,
            error: None,
            on_confirm: None,
            on_cancel: None,
        }
    }

    pub fn on_confirm<F: Fn(WorktreeInfo, &mut Window, &mut App) + Send + Sync + 'static>(
        mut self,
        callback: F,
    ) -> Self {
        self.on_confirm = Some(Arc::new(callback));
        self
    }

    pub fn on_cancel<F: Fn(&mut Window, &mut App) + Send + Sync + 'static>(mut self, callback: F) -> Self {
        self.on_cancel = Some(Arc::new(callback));
        self
    }

    pub fn open(&mut self, worktree: WorktreeInfo, has_uncommitted: bool) {
        self.worktree = Some(worktree);
        self.show = true;
        self.has_uncommitted = has_uncommitted;
        self.error = None;
    }

    pub fn close(&mut self) {
        self.show = false;
        self.worktree = None;
        self.error = None;
    }

    pub fn set_error(&mut self, error: &str) {
        self.error = Some(error.to_string());
    }

    pub fn clear_error(&mut self) {
        self.error = None;
    }

    pub fn is_open(&self) -> bool {
        self.show
    }

    pub fn worktree(&self) -> Option<&WorktreeInfo> {
        self.worktree.as_ref()
    }

    pub fn has_uncommitted(&self) -> bool {
        self.has_uncommitted
    }

    pub fn error_message(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

impl Default for DeleteWorktreeDialogUi {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoElement for DeleteWorktreeDialogUi {
    type Element = AnyElement;

    fn into_element(self) -> Self::Element {
        if !self.show {
            return div().into_any_element();
        }

        let worktree = match &self.worktree {
            Some(w) => w.clone(),
            None => return div().into_any_element(),
        };
        let has_uncommitted = self.has_uncommitted;
        let error_message = self.error.clone();
        let on_confirm = self.on_confirm.clone();
        let on_cancel_overlay = self.on_cancel.clone();
        let on_cancel_btn = self.on_cancel.clone();

        let branch_name = worktree.short_branch_name().to_string();
        let path_display = worktree.display_path();

        let modal_overlay = div()
            .id("delete-worktree-modal-overlay")
            .absolute()
            .inset(px(0.))
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x00000099u32))
            .occlude()
            .on_click(move |_event, window, cx| {
                if let Some(ref cb) = on_cancel_overlay {
                    cb(window, cx);
                }
            });

        let dialog_card = div()
            .id("delete-worktree-dialog-card")
            .w(px(420.))
            .flex()
            .flex_col()
            .gap(px(20.))
            .px(px(24.))
            .py(px(24.))
            .rounded(px(8.))
            .bg(rgb(0x2d2d2d))
            .shadow_lg()
            .on_click(|_event, _window, cx| {
                cx.stop_propagation();
            });

        let title = div()
            .text_size(px(18.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgb(0xffffff))
            .child("Remove Worktree?");

        let branch_label = div()
            .text_size(px(13.))
            .text_color(rgb(0x999999))
            .child(format!("Branch: {}", branch_name));

        let path_label = div()
            .text_size(px(12.))
            .text_color(rgb(0x888888))
            .child(format!("Path: {}", path_display));

        let uncommitted_warning = if has_uncommitted {
            div()
                .text_size(px(12.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(0xffc107))
                .child("Warning: This worktree has uncommitted changes. They will be discarded.")
        } else {
            div()
        };

        let error_display = if let Some(ref err) = error_message {
            div()
                .text_size(px(12.))
                .text_color(rgb(0xf44336))
                .child(err.clone())
        } else {
            div()
        };

        let buttons_row = div()
            .flex()
            .flex_row()
            .justify_end()
            .gap(px(12.));

        let cancel_button = div()
            .id("delete-worktree-cancel-btn")
            .px(px(16.))
            .py(px(8.))
            .rounded(px(6.))
            .bg(rgb(0x3d3d3d))
            .text_color(rgb(0xcccccc))
            .text_size(px(14.))
            .font_weight(FontWeight::MEDIUM)
            .cursor_pointer()
            .hover(|s: StyleRefinement| s.bg(rgb(0x4d4d4d)))
            .on_click(move |_event, window, cx| {
                if let Some(ref cb) = on_cancel_btn {
                    cb(window, cx);
                }
            })
            .child("Cancel");

        let delete_button = div()
            .id("delete-worktree-confirm-btn")
            .px(px(16.))
            .py(px(8.))
            .rounded(px(6.))
            .bg(rgb(0xc62828))
            .text_color(rgb(0xffffff))
            .text_size(px(14.))
            .font_weight(FontWeight::MEDIUM)
            .cursor_pointer()
            .hover(|s: StyleRefinement| s.bg(rgb(0xd32f2f)))
            .on_click(move |_event, window, cx| {
                if let Some(ref cb) = on_confirm {
                    cb(worktree.clone(), window, cx);
                }
            })
            .child("Delete");

        let buttons = buttons_row.child(cancel_button).child(delete_button);

        let dialog_content = dialog_card
            .child(title)
            .child(branch_label)
            .child(path_label)
            .child(uncommitted_warning)
            .child(error_display)
            .child(buttons);

        modal_overlay.child(dialog_content).into_any_element()
    }
}
