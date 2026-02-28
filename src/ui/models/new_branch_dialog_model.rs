// ui/models/new_branch_dialog_model.rs - Shared model for new branch dialog state
/// Shared model for NewBranchDialog. Entity observes this; AppRoot delegates state to model.
/// Does NOT implement Render.
pub struct NewBranchDialogModel {
    pub is_open: bool,
    pub branch_name: String,
    pub error: String,
    pub is_creating: bool,
}

impl NewBranchDialogModel {
    pub fn new() -> Self {
        Self {
            is_open: false,
            branch_name: String::new(),
            error: String::new(),
            is_creating: false,
        }
    }

    pub fn open(&mut self) {
        self.is_open = true;
    }

    pub fn close(&mut self) {
        self.is_open = false;
    }

    pub fn set_branch_name(&mut self, name: &str) {
        self.branch_name = name.to_string();
    }

    pub fn set_error(&mut self, err: &str) {
        self.error = err.to_string();
    }

    pub fn start_creating(&mut self) {
        self.is_creating = true;
    }

    pub fn complete_creating(&mut self, _success: bool) {
        self.is_creating = false;
    }

    /// Validate branch name and set error if invalid
    pub fn validate(&mut self) {
        match crate::new_branch_dialog::validate_branch_name(self.branch_name.trim()) {
            Ok(()) => self.error.clear(),
            Err(e) => self.error = e.message,
        }
    }

    /// Returns true if Create button should be enabled (non-empty valid branch name, not creating)
    pub fn is_create_enabled(&self) -> bool {
        !self.is_creating && !self.branch_name.trim().is_empty() && self.error.is_empty()
    }
}

impl Default for NewBranchDialogModel {
    fn default() -> Self {
        Self::new()
    }
}
