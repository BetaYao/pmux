// ui/new_branch_dialog_entity.rs - NewBranchDialog Entity that observes NewBranchDialogModel
// Phase 3.2: Entity with observe; re-renders only when model notifies
use crate::ui::models::NewBranchDialogModel;
use crate::ui::new_branch_dialog_ui::NewBranchDialogUi;
use gpui::prelude::*;
use gpui::*;
use std::sync::Arc;

/// NewBranchDialog Entity - observes NewBranchDialogModel; re-renders when model notifies.
pub struct NewBranchDialogEntity {
    model: Entity<NewBranchDialogModel>,
    is_open: bool,
    branch_name: String,
    error: String,
    is_creating: bool,
    input_focus: FocusHandle,
    on_create: Arc<dyn Fn(&mut Window, &mut App) + Send + Sync>,
    on_close: Arc<dyn Fn(&mut Window, &mut App) + Send + Sync>,
    on_branch_name_change: Arc<dyn Fn(String, &mut Window, &mut App) + Send + Sync>,
}

impl NewBranchDialogEntity {
    pub fn new(
        model: Entity<NewBranchDialogModel>,
        input_focus: FocusHandle,
        on_create: Arc<dyn Fn(&mut Window, &mut App) + Send + Sync>,
        on_close: Arc<dyn Fn(&mut Window, &mut App) + Send + Sync>,
        on_branch_name_change: Arc<dyn Fn(String, &mut Window, &mut App) + Send + Sync>,
        cx: &mut Context<Self>,
    ) -> Self {
        let m = model.read(cx);
        let is_open = m.is_open;
        let branch_name = m.branch_name.clone();
        let error = m.error.clone();
        let is_creating = m.is_creating;
        cx.observe(&model, |this, observed, cx| {
            let m = observed.read(cx);
            this.is_open = m.is_open;
            this.branch_name = m.branch_name.clone();
            this.error = m.error.clone();
            this.is_creating = m.is_creating;
            cx.notify();
        })
        .detach();
        Self {
            model,
            is_open,
            branch_name,
            error,
            is_creating,
            input_focus,
            on_create,
            on_close,
            on_branch_name_change,
        }
    }
}

impl Render for NewBranchDialogEntity {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        if !self.is_open {
            return div().into_any_element();
        }

        let on_close = self.on_close.clone();
        let on_create = self.on_create.clone();
        let on_branch_name_change = self.on_branch_name_change.clone();
        let branch_name = self.branch_name.clone();
        let error = self.error.clone();
        let is_creating = self.is_creating;
        let is_create_enabled = !is_creating && !branch_name.trim().is_empty() && error.is_empty();
        let input_focus = self.input_focus.clone();

        let mut ui = NewBranchDialogUi::new()
            .with_focus_handle(input_focus)
            .on_close(move |w, cx| on_close(w, cx))
            .on_create(move |w, cx| on_create(w, cx))
            .on_branch_name_change(move |v, w, cx| on_branch_name_change(v, w, cx));
        ui.open();
        ui.set_branch_name(&branch_name);
        if !error.is_empty() {
            ui.set_error(&error);
        }
        if is_creating {
            ui.start_creating();
        }

        div().child(ui).into_any_element()
    }
}
