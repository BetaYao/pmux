// ui/new_branch_dialog_ui.rs - New branch dialog UI component with GPUI rendering
use gpui::prelude::*;
use gpui::*;
use std::sync::Arc;
use crate::new_branch_dialog::NewBranchDialog;

/// Callback type for creating a new branch
pub type CreateBranchCallback = Arc<dyn Fn(&mut Window, &mut App) + Send + 'static>;

/// Callback type for closing the dialog
pub type CloseDialogCallback = Arc<dyn Fn(&mut Window, &mut App) + Send + 'static>;

/// Callback type for when the branch name input changes
pub type BranchNameChangeCallback = Arc<dyn Fn(String, &mut Window, &mut App) + Send + 'static>;

/// New Branch Dialog UI component - manages the dialog UI state and rendering
pub struct NewBranchDialogUi {
    dialog: NewBranchDialog,
    on_create: Option<CreateBranchCallback>,
    on_close: Option<CloseDialogCallback>,
    on_branch_name_change: Option<BranchNameChangeCallback>,
    /// Focus handle for the branch name input - when set, used for track_focus and initial focus
    input_focus_handle: Option<gpui::FocusHandle>,
}

impl NewBranchDialogUi {
    /// Creates a new dialog UI component in closed state
    pub fn new() -> Self {
        Self {
            dialog: NewBranchDialog::new(),
            on_create: None,
            on_close: None,
            on_branch_name_change: None,
            input_focus_handle: None,
        }
    }

    /// Sets the focus handle for the branch name input (for track_focus and initial focus on open)
    pub fn with_focus_handle(mut self, handle: gpui::FocusHandle) -> Self {
        self.input_focus_handle = Some(handle);
        self
    }

    /// Sets the callback for when the user clicks "Create"
    pub fn on_create<F: Fn(&mut Window, &mut App) + Send + 'static>(mut self, callback: F) -> Self {
        self.on_create = Some(Arc::new(callback));
        self
    }

    /// Sets the callback for when the user clicks "Cancel" or closes the dialog
    pub fn on_close<F: Fn(&mut Window, &mut App) + Send + 'static>(mut self, callback: F) -> Self {
        self.on_close = Some(Arc::new(callback));
        self
    }

    /// Sets the callback for when the branch name input changes (e.g. user types)
    pub fn on_branch_name_change<F: Fn(String, &mut Window, &mut App) + Send + 'static>(mut self, callback: F) -> Self {
        self.on_branch_name_change = Some(Arc::new(callback));
        self
    }

    /// Opens the dialog
    pub fn open(&mut self) {
        self.dialog.open();
    }

    /// Closes the dialog (only if not creating)
    pub fn close(&mut self) {
        self.dialog.close();
    }

    /// Returns true if the dialog is currently open
    pub fn is_open(&self) -> bool {
        self.dialog.is_open()
    }

    /// Returns true if the dialog is currently creating a worktree
    pub fn is_creating(&self) -> bool {
        self.dialog.is_creating()
    }

    /// Gets the current branch name input
    pub fn branch_name(&self) -> &str {
        self.dialog.branch_name()
    }

    /// Sets the branch name input
    pub fn set_branch_name(&mut self, name: &str) {
        self.dialog.set_branch_name(name);
    }

    /// Sets an error message
    pub fn set_error(&mut self, error: &str) {
        self.dialog.set_error(error);
    }

    /// Returns true if there is currently a validation error
    pub fn has_error(&self) -> bool {
        self.dialog.has_error()
    }

    /// Returns the error message, or empty string if no error
    pub fn error_message(&self) -> &str {
        self.dialog.error_message()
    }

    /// Returns true if the Create button should be enabled
    pub fn is_create_enabled(&self) -> bool {
        self.dialog.is_create_enabled()
    }

    /// Starts the branch creation process
    pub fn start_creating(&mut self) {
        self.dialog.start_creating();
    }

    /// Completes the creation process
    pub fn complete_creating(&mut self, success: bool) {
        self.dialog.complete_creating(success);
    }

    /// Validates the current branch name
    pub fn validate(&mut self) {
        self.dialog.validate();
    }
}

impl Default for NewBranchDialogUi {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoElement for NewBranchDialogUi {
    type Element = AnyElement;

    fn into_element(self) -> Self::Element {
        if !self.is_open() {
            // Don't render anything if dialog is closed
            return div().into_any_element();
        }

        let branch_name = self.branch_name().to_string();
        let has_error = self.has_error();
        let error_message = self.error_message().to_string();
        let is_creating = self.is_creating();
        let is_create_enabled = self.is_create_enabled();
        let on_close = self.on_close.clone();
        let on_branch_name_change = self.on_branch_name_change.clone();
        let input_focus_handle = self.input_focus_handle.clone();

        // Modal overlay - covers entire screen with semi-transparent background
        let modal_overlay = div()
            .id("new-branch-modal-overlay")
            .absolute()
            .inset(px(0.))
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x00000099u32)) // Semi-transparent black
            // Click outside to close (only if not creating)
            .when(!is_creating, |el| {
                el.on_click(move |_event, window, cx| {
                    if let Some(ref close_cb) = on_close {
                        close_cb(window, cx);
                    }
                })
            });

        // Dialog card - stop propagation so clicks on dialog don't close it
        let dialog_card = div()
            .id("new-branch-dialog-card")
            .w(px(400.))
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

        // Title
        let title = div()
            .text_size(px(18.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgb(0xffffff))
            .child("Create New Branch");

        // Description
        let description = div()
            .text_size(px(13.))
            .text_color(rgb(0x999999))
            .child("Enter a name for the new branch and worktree:");

        // Input field container
        let input_container = div()
            .flex()
            .flex_col()
            .gap(px(8.));

        // Label
        let input_label = div()
            .text_size(px(12.))
            .font_weight(FontWeight::MEDIUM)
            .text_color(rgb(0xcccccc))
            .child("Branch Name");

        // Editable text input - focusable div with on_key_down for branch name entry
        let input_text: SharedString = if branch_name.is_empty() {
            "e.g., feature/my-new-feature".into()
        } else {
            branch_name.clone().into()
        };
        let mut input_field = div()
            .id("new-branch-input")
            .w_full()
            .h(px(40.))
            .px(px(12.))
            .rounded(px(6.))
            .bg(rgb(0x1e1e1e))
            .border(if has_error { px(1.) } else { px(0.) })
            .border_color(rgb(0xf44336))
            .flex()
            .items_center()
            .cursor(gpui::CursorStyle::IBeam)
            .child(
                div()
                    .text_size(px(14.))
                    .text_color(if branch_name.is_empty() { rgb(0x666666) } else { rgb(0xffffff) })
                    .child(input_text)
            );
        // Make input editable when not creating and callback is set
        if !is_creating {
            if let Some(ref on_change) = on_branch_name_change {
                let on_change = Arc::clone(on_change);
                let branch_name_for_keys = branch_name.clone();
                input_field = input_field
                    .focusable();
                if let Some(ref handle) = input_focus_handle {
                    input_field = input_field.track_focus(handle);
                }
                input_field = input_field
                    .on_key_down(move |event, window, cx| {
                        let new_value = match event.keystroke.key.as_str() {
                            "backspace" => {
                                let mut s = branch_name_for_keys.clone();
                                if s.pop().is_some() {
                                    s
                                } else {
                                    branch_name_for_keys.clone()
                                }
                            }
                            "delete" => {
                                // Forward delete - with cursor-at-end model, no change
                                branch_name_for_keys.clone()
                            }
                            _ => {
                                if let Some(ref key_char) = event.keystroke.key_char {
                                    // Filter out newlines and other control chars
                                    let filtered: String = key_char.chars()
                                        .filter(|c| !c.is_control() && *c != '\n' && *c != '\r')
                                        .collect();
                                    if !filtered.is_empty() {
                                        format!("{}{}", branch_name_for_keys, filtered)
                                    } else {
                                        branch_name_for_keys.clone()
                                    }
                                } else {
                                    branch_name_for_keys.clone()
                                }
                            }
                        };
                        if new_value != branch_name_for_keys {
                            on_change(new_value, window, cx);
                        }
                        window.prevent_default();
                    });
            }
        }

        // Error message
        let error_display = if has_error {
            div()
                .text_size(px(12.))
                .text_color(rgb(0xf44336))
                .child(error_message.clone())
        } else {
            div()
        };

        // Hint text
        let hint_text = div()
            .text_size(px(11.))
            .text_color(rgb(0x666666))
            .child("Use letters, numbers, hyphens, underscores, and forward slashes. No spaces or special characters.");

        // Buttons row
        let buttons_row = div()
            .flex()
            .flex_row()
            .justify_end()
            .gap(px(12.));

        // Cancel button
        let cancel_button = div()
            .id("cancel-btn")
            .px(px(16.))
            .py(px(8.))
            .rounded(px(6.))
            .bg(rgb(0x3d3d3d))
            .text_color(rgb(0xcccccc))
            .text_size(px(14.))
            .font_weight(FontWeight::MEDIUM)
            .cursor_pointer()
            .hover(|style: StyleRefinement| style.bg(rgb(0x4d4d4d)))
            .when(!is_creating, |el| {
                let close_cb = self.on_close.clone();
                el.on_click(move |_event, window, cx| {
                    if let Some(ref cb) = close_cb {
                        cb(window, cx);
                    }
                })
            })
            .child("Cancel");

        // Create button
        let create_button_text = if is_creating { "Creating..." } else { "Create" };
        let create_button_bg = if is_create_enabled && !is_creating {
            rgb(0x0066cc)
        } else {
            rgb(0x4a4a4a)
        };
        let create_button_hover_bg = if is_create_enabled && !is_creating {
            rgb(0x0077dd)
        } else {
            rgb(0x4a4a4a)
        };

        let create_button = div()
            .id("create-btn")
            .px(px(16.))
            .py(px(8.))
            .rounded(px(6.))
            .bg(create_button_bg)
            .text_color(if is_create_enabled || is_creating { rgb(0xffffff) } else { rgb(0x888888) })
            .text_size(px(14.))
            .font_weight(FontWeight::MEDIUM)
            .cursor_pointer()
            .when(is_create_enabled && !is_creating, |el| {
                let create_cb = self.on_create.clone();
                el.hover(|style: StyleRefinement| style.bg(create_button_hover_bg))
                    .on_click(move |_event, window, cx| {
                        if let Some(ref cb) = create_cb {
                            cb(window, cx);
                        }
                    })
            })
            .child(create_button_text);

        // Assemble buttons
        let buttons = buttons_row.child(cancel_button).child(create_button);

        // Assemble input container
        let input_section = input_container
            .child(input_label)
            .child(input_field)
            .child(error_display)
            .child(hint_text);

        // Assemble dialog card
        let dialog_content = dialog_card
            .child(title)
            .child(description)
            .child(input_section)
            .child(buttons);

        // Return the complete modal
        modal_overlay.child(dialog_content).into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test: Dialog starts in closed state
    #[test]
    fn test_new_branch_dialog_ui_starts_closed() {
        let dialog = NewBranchDialogUi::new();
        assert!(!dialog.is_open());
        assert!(!dialog.is_creating());
        assert_eq!(dialog.branch_name(), "");
        assert!(!dialog.has_error());
    }

    /// Test: Dialog can be opened
    #[test]
    fn test_new_branch_dialog_ui_can_be_opened() {
        let mut dialog = NewBranchDialogUi::new();
        dialog.open();
        assert!(dialog.is_open());
        assert!(!dialog.is_creating());
    }

    /// Test: Dialog can be closed
    #[test]
    fn test_new_branch_dialog_ui_can_be_closed() {
        let mut dialog = NewBranchDialogUi::new();
        dialog.open();
        dialog.close();
        assert!(!dialog.is_open());
    }

    /// Test: Dialog clears state when closed
    #[test]
    fn test_new_branch_dialog_ui_clears_state_on_close() {
        let mut dialog = NewBranchDialogUi::new();
        dialog.open();
        dialog.set_branch_name("feature/test");
        dialog.close();
        assert_eq!(dialog.branch_name(), "");
    }

    /// Test: Create button is disabled for empty input
    #[test]
    fn test_create_button_disabled_for_empty_input() {
        let mut dialog = NewBranchDialogUi::new();
        dialog.open();
        assert!(!dialog.is_create_enabled());
    }

    /// Test: Create button is enabled for valid input
    #[test]
    fn test_create_button_enabled_for_valid_input() {
        let mut dialog = NewBranchDialogUi::new();
        dialog.open();
        dialog.set_branch_name("feature/test");
        assert!(dialog.is_create_enabled());
    }

    /// Test: Create button is disabled for invalid input (spaces)
    #[test]
    fn test_create_button_disabled_for_invalid_input() {
        let mut dialog = NewBranchDialogUi::new();
        dialog.open();
        dialog.set_branch_name("feature test");
        dialog.validate();
        assert!(!dialog.is_create_enabled());
        assert!(dialog.has_error());
    }

    /// Test: Dialog can enter creating state
    #[test]
    fn test_dialog_can_enter_creating_state() {
        let mut dialog = NewBranchDialogUi::new();
        dialog.open();
        dialog.set_branch_name("feature/test");
        dialog.start_creating();
        assert!(dialog.is_creating());
        assert!(!dialog.is_create_enabled());
    }

    /// Test: Dialog exits creating state on success
    #[test]
    fn test_dialog_exits_creating_on_success() {
        let mut dialog = NewBranchDialogUi::new();
        dialog.open();
        dialog.set_branch_name("feature/test");
        dialog.start_creating();
        dialog.complete_creating(true);
        assert!(!dialog.is_creating());
        assert!(!dialog.is_open()); // Should close on success
    }

    /// Test: Dialog exits creating state on failure but stays open
    #[test]
    fn test_dialog_exits_creating_on_failure_stays_open() {
        let mut dialog = NewBranchDialogUi::new();
        dialog.open();
        dialog.set_branch_name("feature/test");
        dialog.start_creating();
        dialog.complete_creating(false);
        assert!(!dialog.is_creating());
        assert!(dialog.is_open()); // Should stay open on failure
    }

    /// Test: Callbacks can be set
    #[test]
    fn test_callbacks_can_be_set() {
        let dialog = NewBranchDialogUi::new()
            .on_create(|_: &mut Window, _: &mut App| {})
            .on_close(|_: &mut Window, _: &mut App| {})
            .on_branch_name_change(|_: String, _: &mut Window, _: &mut App| {});
        // Just verify it compiles and doesn't panic
        assert!(!dialog.is_open());
    }
}
