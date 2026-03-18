// ui/close_tab_dialog_ui.rs - Close workspace tab confirmation dialog
use gpui::prelude::*;
use gpui::{AnyElement, App, FontWeight, StyleRefinement, Window, div, px, rgb, rgba};
use std::path::PathBuf;
use std::sync::Arc;

/// Callback: (tab_index, kill_tmux, window, cx)
pub type ConfirmCloseCallback = Arc<dyn Fn(usize, bool, &mut Window, &mut App) + Send + Sync>;
pub type CancelCloseCallback = Arc<dyn Fn(&mut Window, &mut App) + Send + Sync>;
pub type ToggleKillTmuxCallback = Arc<dyn Fn(&mut Window, &mut App) + Send + Sync>;

pub struct CloseTabDialogUi {
    tab_index: Option<usize>,
    workspace_path: Option<PathBuf>,
    workspace_name: Option<String>,
    show: bool,
    kill_tmux: bool,
    on_confirm: Option<ConfirmCloseCallback>,
    on_cancel: Option<CancelCloseCallback>,
    on_toggle_kill_tmux: Option<ToggleKillTmuxCallback>,
}

impl CloseTabDialogUi {
    pub fn new() -> Self {
        Self {
            tab_index: None,
            workspace_path: None,
            workspace_name: None,
            show: false,
            kill_tmux: true, // default: kill tmux session
            on_confirm: None,
            on_cancel: None,
            on_toggle_kill_tmux: None,
        }
    }

    pub fn on_confirm<F: Fn(usize, bool, &mut Window, &mut App) + Send + Sync + 'static>(
        mut self,
        callback: F,
    ) -> Self {
        self.on_confirm = Some(Arc::new(callback));
        self
    }

    pub fn on_cancel<F: Fn(&mut Window, &mut App) + Send + Sync + 'static>(
        mut self,
        callback: F,
    ) -> Self {
        self.on_cancel = Some(Arc::new(callback));
        self
    }

    pub fn on_toggle_kill_tmux<F: Fn(&mut Window, &mut App) + Send + Sync + 'static>(
        mut self,
        callback: F,
    ) -> Self {
        self.on_toggle_kill_tmux = Some(Arc::new(callback));
        self
    }

    pub fn open(&mut self, tab_index: usize, workspace_path: PathBuf, workspace_name: String) {
        self.tab_index = Some(tab_index);
        self.workspace_path = Some(workspace_path);
        self.workspace_name = Some(workspace_name);
        self.show = true;
        self.kill_tmux = true;
    }

    pub fn close(&mut self) {
        self.show = false;
        self.tab_index = None;
        self.workspace_path = None;
        self.workspace_name = None;
    }

    pub fn is_open(&self) -> bool {
        self.show
    }

    pub fn toggle_kill_tmux(&mut self) {
        self.kill_tmux = !self.kill_tmux;
    }

    pub fn kill_tmux(&self) -> bool {
        self.kill_tmux
    }

    pub fn tab_index(&self) -> Option<usize> {
        self.tab_index
    }

    pub fn workspace_name(&self) -> Option<&str> {
        self.workspace_name.as_deref()
    }

    pub fn workspace_path(&self) -> Option<&PathBuf> {
        self.workspace_path.as_ref()
    }
}

impl Default for CloseTabDialogUi {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoElement for CloseTabDialogUi {
    type Element = AnyElement;

    fn into_element(self) -> Self::Element {
        if !self.show {
            return div().into_any_element();
        }

        let tab_index = match self.tab_index {
            Some(idx) => idx,
            None => return div().into_any_element(),
        };
        let workspace_name = self
            .workspace_name
            .clone()
            .unwrap_or_else(|| "Unknown".to_string());
        let workspace_path = self
            .workspace_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        let kill_tmux = self.kill_tmux;
        let on_confirm = self.on_confirm.clone();
        let on_cancel_overlay = self.on_cancel.clone();
        let on_cancel_btn = self.on_cancel.clone();
        let on_toggle = self.on_toggle_kill_tmux.clone();

        let modal_overlay = div()
            .id("close-tab-modal-overlay")
            .absolute()
            .inset(px(0.))
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x00000099u32))
            .on_click(move |_event, window, cx| {
                if let Some(ref cb) = on_cancel_overlay {
                    cb(window, cx);
                }
            });

        let dialog_card = div()
            .id("close-tab-dialog-card")
            .w(px(440.))
            .flex()
            .flex_col()
            .gap(px(16.))
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
            .child(format!("Close \"{}\"?", workspace_name));

        let path_label = div()
            .text_size(px(12.))
            .text_color(rgb(0x888888))
            .child(workspace_path);

        // Checkbox row for kill tmux option
        let checkbox_icon = if kill_tmux { "☑" } else { "☐" };
        let checkbox_row = div()
            .id("close-tab-kill-tmux-toggle")
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.))
            .cursor_pointer()
            .on_click(move |_event, window, cx| {
                if let Some(ref cb) = on_toggle {
                    cb(window, cx);
                }
            })
            .child(
                div()
                    .text_size(px(16.))
                    .text_color(if kill_tmux {
                        rgb(0x4fc3f7)
                    } else {
                        rgb(0x888888)
                    })
                    .child(checkbox_icon),
            )
            .child(
                div()
                    .text_size(px(13.))
                    .text_color(rgb(0xcccccc))
                    .child("同时关闭后台 tmux 会话"),
            );

        let hint = div()
            .text_size(px(11.))
            .text_color(rgb(0x999999))
            .child("关闭后台会话将终止该工作区下所有运行中的 Agent 进程");

        // Buttons
        let buttons_row = div()
            .flex()
            .flex_row()
            .justify_end()
            .gap(px(12.))
            .mt(px(4.));

        let cancel_button = div()
            .id("close-tab-cancel-btn")
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

        let confirm_button = div()
            .id("close-tab-confirm-btn")
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
                    cb(tab_index, kill_tmux, window, cx);
                }
            })
            .child("Close");

        let buttons = buttons_row.child(cancel_button).child(confirm_button);

        let dialog_content = dialog_card
            .child(title)
            .child(path_label)
            .child(checkbox_row)
            .child(hint)
            .child(buttons);

        modal_overlay.child(dialog_content).into_any_element()
    }
}
