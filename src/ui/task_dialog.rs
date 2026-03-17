use gpui::prelude::*;
use gpui::{div, px, rgb};

use crate::scheduler::{TaskTarget, TaskType};

pub struct TaskDialog {
    name: String,
    cron: String,
    task_type: TaskType,
    target: TaskTarget,
}

impl TaskDialog {
    pub fn new() -> Self {
        Self {
            name: String::new(),
            cron: "0 2 * * *".to_string(),
            task_type: TaskType::Shell {
                command: String::new(),
            },
            target: TaskTarget::ExistingWorktree {
                workspace_index: 0,
                worktree_name: "main".to_string(),
            },
        }
    }

    pub fn build(self) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_2()
            .p_4()
            .w(px(400.))
            .bg(rgb(0x1a1a1a))
            .rounded_md()
            .child(
                div()
                    .text_color(rgb(0xffffff))
                    .text_lg()
                    .child("New Scheduled Task"),
            )
            .child(div().text_color(rgb(0x888888)).text_sm().child("Name:"))
            .child(div().text_color(rgb(0xffffff)).child(self.name.clone()))
            .child(
                div()
                    .text_color(rgb(0x888888))
                    .text_sm()
                    .child("Schedule (cron):"),
            )
            .child(div().text_color(rgb(0xffffff)).child(self.cron.clone()))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .mt_2()
                    .child(
                        div()
                            .px_3()
                            .py_1()
                            .bg(rgb(0x333333))
                            .rounded_md()
                            .text_color(rgb(0x888888))
                            .child("Cancel"),
                    )
                    .child(
                        div()
                            .px_3()
                            .py_1()
                            .bg(rgb(0x4ade80))
                            .rounded_md()
                            .text_color(rgb(0x000000))
                            .child("Save"),
                    ),
            )
    }
}
