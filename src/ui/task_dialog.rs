use crate::scheduler::{ScheduledTask, TaskTarget, TaskType};
use gpui::prelude::*;
use gpui::{
    div, px, rgb, rgba, App, Context, FocusHandle, FontWeight, InteractiveElement,
    IntoElement, ParentElement, SharedString, Stateful, Styled, Window,
};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq)]
enum FocusedField {
    Name,
    Cron,
    Command,
}

pub struct TaskDialog {
    name_input: String,
    cron_input: String,
    command_input: String,
    focused_field: FocusedField,
    focus_handle: FocusHandle,
    on_save: Option<Arc<dyn Fn(ScheduledTask, &mut Window, &mut App) + Send + Sync>>,
    on_cancel: Option<Arc<dyn Fn(&mut Window, &mut App) + Send + Sync>>,
}

impl TaskDialog {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            name_input: String::new(),
            cron_input: "0 2 * * *".to_string(),
            command_input: String::new(),
            focused_field: FocusedField::Name,
            focus_handle: cx.focus_handle(),
            on_save: None,
            on_cancel: None,
        }
    }

    pub fn set_on_save<F: Fn(ScheduledTask, &mut Window, &mut App) + Send + Sync + 'static>(
        &mut self,
        f: F,
    ) {
        self.on_save = Some(Arc::new(f));
    }

    pub fn set_on_cancel<F: Fn(&mut Window, &mut App) + Send + Sync + 'static>(&mut self, f: F) {
        self.on_cancel = Some(Arc::new(f));
    }

    fn active_input_mut(&mut self) -> &mut String {
        match self.focused_field {
            FocusedField::Name => &mut self.name_input,
            FocusedField::Cron => &mut self.cron_input,
            FocusedField::Command => &mut self.command_input,
        }
    }

    fn handle_key_down(&mut self, event: &gpui::KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event.keystroke.key.as_str() {
            "escape" => {
                if let Some(ref cb) = self.on_cancel {
                    let cb = cb.clone();
                    cb(window, &mut *cx);
                }
                window.prevent_default();
                return;
            }
            "tab" => {
                self.focused_field = match self.focused_field {
                    FocusedField::Name => FocusedField::Cron,
                    FocusedField::Cron => FocusedField::Command,
                    FocusedField::Command => FocusedField::Name,
                };
                cx.notify();
                window.prevent_default();
                return;
            }
            "enter" => {
                if !self.name_input.trim().is_empty() && !self.command_input.trim().is_empty() {
                    if let Some(ref cb) = self.on_save {
                        let task = self.build_task();
                        let cb = cb.clone();
                        cb(task, window, &mut *cx);
                    }
                }
                window.prevent_default();
                return;
            }
            "backspace" => {
                let input = self.active_input_mut();
                input.pop();
                cx.notify();
                window.prevent_default();
                return;
            }
            _ => {}
        }

        if let Some(ref key_char) = event.keystroke.key_char {
            let filtered: String = key_char
                .chars()
                .filter(|c| !c.is_control() && *c != '\n' && *c != '\r')
                .collect();
            if !filtered.is_empty() {
                let input = self.active_input_mut();
                input.push_str(&filtered);
                cx.notify();
            }
        }
        window.prevent_default();
    }

    fn build_task(&self) -> ScheduledTask {
        ScheduledTask::new(
            self.name_input.clone(),
            self.cron_input.clone(),
            TaskType::Shell {
                command: self.command_input.clone(),
            },
            TaskTarget::ExistingWorktree {
                workspace_index: 0,
                worktree_name: "main".to_string(),
            },
        )
    }

    fn render_input_field(
        &self,
        id: &'static str,
        label: &'static str,
        value: &str,
        placeholder: &'static str,
        field: FocusedField,
    ) -> Stateful<gpui::Div> {
        let is_focused = self.focused_field == field;
        let border_color = if is_focused {
            rgb(0x4ade80)
        } else {
            rgb(0x444444)
        };

        let caret = if is_focused {
            div().w(px(2.)).h(px(16.)).bg(rgb(0xffffff)).flex_shrink()
        } else {
            div()
        };

        let text_el = if value.is_empty() {
            div()
                .flex()
                .flex_row()
                .items_center()
                .child(caret)
                .child(
                    div()
                        .text_size(px(14.))
                        .text_color(rgb(0x666666))
                        .child(placeholder),
                )
        } else {
            div()
                .flex()
                .flex_row()
                .items_center()
                .child(
                    div()
                        .text_size(px(14.))
                        .text_color(rgb(0xffffff))
                        .child(SharedString::from(value.to_string())),
                )
                .child(caret)
        };

        div()
            .id(id)
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .text_color(rgb(0x888888))
                    .text_sm()
                    .child(label),
            )
            .child(
                div()
                    .w_full()
                    .h(px(36.))
                    .px(px(12.))
                    .rounded(px(6.))
                    .bg(rgb(0x1e1e1e))
                    .border_1()
                    .border_color(border_color)
                    .flex()
                    .items_center()
                    .cursor(gpui::CursorStyle::IBeam)
                    .child(text_el),
            )
    }
}

impl Render for TaskDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Keep focus on dialog
        window.focus(&self.focus_handle, cx);

        let on_cancel = self.on_cancel.clone();
        let on_save = self.on_save.clone();
        let is_save_enabled =
            !self.name_input.trim().is_empty() && !self.command_input.trim().is_empty();

        let save_bg = if is_save_enabled {
            rgb(0x4ade80)
        } else {
            rgb(0x4a4a4a)
        };
        let save_text_color = if is_save_enabled {
            rgb(0x000000)
        } else {
            rgb(0x888888)
        };
        let save_hover_bg = if is_save_enabled {
            rgb(0x22c55e)
        } else {
            rgb(0x4a4a4a)
        };

        let task_for_save = if is_save_enabled {
            Some(self.build_task())
        } else {
            None
        };

        // Modal overlay
        div()
            .id("task-dialog-overlay")
            .debug_selector(|| "task-dialog".to_string())
            .absolute()
            .inset_0()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x00000099u32))
            .focusable()
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::handle_key_down))
            // Click overlay to cancel
            .on_click(move |_event, window, cx| {
                if let Some(ref cb) = on_cancel {
                    cb(window, cx);
                }
            })
            .child(
                // Dialog card - stop propagation
                div()
                    .id("task-dialog-card")
                    .flex()
                    .flex_col()
                    .gap_3()
                    .p_6()
                    .w(px(450.))
                    .bg(rgb(0x2d2d2d))
                    .rounded_lg()
                    .border_1()
                    .border_color(rgb(0x333333))
                    .shadow_lg()
                    .on_click(|_event, _window, cx| {
                        cx.stop_propagation();
                    })
                    // Title
                    .child(
                        div()
                            .text_color(rgb(0xffffff))
                            .text_lg()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("New Scheduled Task"),
                    )
                    // Input fields
                    .child(self.render_input_field(
                        "task-name-field",
                        "Name:",
                        &self.name_input.clone(),
                        "e.g., Daily backup",
                        FocusedField::Name,
                    ))
                    .child(self.render_input_field(
                        "task-cron-field",
                        "Schedule (cron):",
                        &self.cron_input.clone(),
                        "0 2 * * *",
                        FocusedField::Cron,
                    ))
                    .child(self.render_input_field(
                        "task-command-field",
                        "Command:",
                        &self.command_input.clone(),
                        "e.g., git pull && make build",
                        FocusedField::Command,
                    ))
                    // Hint
                    .child(
                        div()
                            .text_color(rgb(0x666666))
                            .text_xs()
                            .child("Tab to switch fields · Enter to save · Escape to cancel"),
                    )
                    // Buttons
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .mt_2()
                            .child(
                                div()
                                    .id("task-dialog-cancel-btn")
                                    .flex_1()
                                    .px_4()
                                    .py_2()
                                    .bg(rgb(0x3d3d3d))
                                    .rounded_md()
                                    .text_color(rgb(0xcccccc))
                                    .text_center()
                                    .cursor_pointer()
                                    .hover(|style| style.bg(rgb(0x4d4d4d)))
                                    .on_click({
                                        let on_cancel = self.on_cancel.clone();
                                        move |_event, window, cx| {
                                            if let Some(ref cb) = on_cancel {
                                                cb(window, cx);
                                            }
                                        }
                                    })
                                    .child("Cancel"),
                            )
                            .child(
                                div()
                                    .id("task-dialog-save-btn")
                                    .flex_1()
                                    .px_4()
                                    .py_2()
                                    .bg(save_bg)
                                    .rounded_md()
                                    .text_color(save_text_color)
                                    .text_center()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .cursor_pointer()
                                    .hover(move |style| style.bg(save_hover_bg))
                                    .when(is_save_enabled, |el| {
                                        el.on_click(move |_event, window, cx| {
                                            if let Some(ref cb) = on_save {
                                                if let Some(ref task) = task_for_save {
                                                    cb(task.clone(), window, cx);
                                                }
                                            }
                                        })
                                    })
                                    .child("Save"),
                            ),
                    ),
            )
    }
}
