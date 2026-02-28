//! New branch dialog and worktree creation logic.
//!
//! **Tmux backend principle**: One worktree = one session (`pmux-{worktree_folder}`). New worktrees
//! get new sessions; each session has a main window.
use std::path::PathBuf;

/// Validation error for branch names
#[derive(Debug, Clone)]
pub struct ValidationError {
    pub message: String,
}

impl ValidationError {
    pub fn new(message: &str) -> Self {
        Self {
            message: message.to_string(),
        }
    }
}

/// Validates a Git branch name according to Git branch naming rules
///
/// Valid branch names:
/// - Cannot be empty
/// - Cannot contain spaces
/// - Cannot contain special characters: ~ ^ : ? * [ . .. (leading or consecutive)
/// - Cannot start with a dash
/// - Can contain: alphanumeric, hyphen, underscore, forward slash
pub fn validate_branch_name(name: &str) -> Result<(), ValidationError> {
    let trimmed = name.trim();
    
    // Check empty
    if trimmed.is_empty() {
        return Err(ValidationError::new("Branch name cannot be empty"));
    }
    
    // Check for spaces
    if trimmed.contains(' ') {
        return Err(ValidationError::new("Branch name cannot contain spaces"));
    }
    
    // Check for invalid characters
    let invalid_chars = ['~', '^', ':', '?', '*', '[', ']'];
    for ch in invalid_chars {
        if trimmed.contains(ch) {
            return Err(ValidationError::new(&format!(
                "Branch name cannot contain '{}' character", ch
            )));
        }
    }
    
    // Check for leading dash
    if trimmed.starts_with('-') {
        return Err(ValidationError::new("Branch name cannot start with a dash"));
    }
    
    // Check for .. (double dot)
    if trimmed.contains("..") {
        return Err(ValidationError::new("Branch name cannot contain '..'"));
    }
    
    // Check for leading dot
    if trimmed.starts_with('.') {
        return Err(ValidationError::new("Branch name cannot start with a dot"));
    }
    
    Ok(())
}

/// Generates a worktree path from the repository path and branch name
/// Replaces forward slashes with hyphens to create a valid directory name
pub fn generate_worktree_path(repo_path: &PathBuf, branch_name: &str) -> PathBuf {
    let safe_branch_name = branch_name.replace('/', "-");
    repo_path.join(&safe_branch_name)
}

/// Dialog state for creating a new branch and worktree
#[derive(Debug, Clone)]
pub struct NewBranchDialog {
    is_open: bool,
    is_creating: bool,
    branch_name: String,
    error: Option<String>,
}

impl NewBranchDialog {
    /// Creates a new dialog in closed state
    pub fn new() -> Self {
        Self {
            is_open: false,
            is_creating: false,
            branch_name: String::new(),
            error: None,
        }
    }

    /// Returns true if the dialog is currently open
    pub fn is_open(&self) -> bool {
        self.is_open
    }

    /// Returns true if the dialog is currently creating a worktree
    pub fn is_creating(&self) -> bool {
        self.is_creating
    }

    /// Returns the current branch name input
    pub fn branch_name(&self) -> &str {
        &self.branch_name
    }

    /// Returns true if there is currently a validation error
    pub fn has_error(&self) -> bool {
        self.error.is_some()
    }

    /// Returns the error message, or empty string if no error
    pub fn error_message(&self) -> &str {
        self.error.as_deref().unwrap_or("")
    }

    /// Returns true if the Create button should be enabled
    pub fn is_create_enabled(&self) -> bool {
        !self.is_creating() && 
        !self.has_error() && 
        !self.branch_name().trim().is_empty()
    }

    /// Opens the dialog
    pub fn open(&mut self) {
        self.is_open = true;
        self.clear_state();
    }

    /// Closes the dialog if not in creating state
    pub fn close(&mut self) {
        if !self.is_creating {
            self.is_open = false;
            self.clear_state();
        }
    }

    /// Sets the branch name input
    pub fn set_branch_name(&mut self, name: &str) {
        self.branch_name = name.to_string();
    }

    /// Sets an error message
    pub fn set_error(&mut self, error: &str) {
        self.error = Some(error.to_string());
    }

    /// Validates the current branch name and sets error if invalid
    pub fn validate(&mut self) {
        match validate_branch_name(&self.branch_name) {
            Ok(()) => self.error = None,
            Err(e) => self.error = Some(e.message),
        }
    }

    /// Performs validation without modifying state - returns error message if invalid
    pub fn check_validation(&self) -> Option<String> {
        match validate_branch_name(&self.branch_name) {
            Ok(()) => None,
            Err(e) => Some(e.message),
        }
    }

    /// Starts the creation process, entering creating state
    pub fn start_creating(&mut self) {
        self.validate();
        if !self.has_error() {
            self.is_creating = true;
        }
    }

    /// Completes the creation process
    pub fn complete_creating(&mut self, success: bool) {
        self.is_creating = false;
        if success {
            self.close();
        }
    }

    /// Clears dialog state (branch name, error)
    fn clear_state(&mut self) {
        self.branch_name = String::new();
        self.error = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test: Branch name validation accepts valid Git branch names
    #[test]
    fn test_validate_branch_name_accepts_valid_names() {
        assert!(validate_branch_name("feature/test").is_ok());
        assert!(validate_branch_name("fix-bug-123").is_ok());
        assert!(validate_branch_name("main").is_ok());
        assert!(validate_branch_name("develop").is_ok());
        assert!(validate_branch_name("user/john/do-something").is_ok());
    }

    /// Test: Branch name validation rejects names with spaces
    #[test]
    fn test_validate_branch_name_rejects_spaces() {
        assert!(validate_branch_name("feature test").is_err());
        assert!(validate_branch_name("my branch").is_err());
        assert!(validate_branch_name(" test").is_err());
        assert!(validate_branch_name("test ").is_err());
    }

    /// Test: Branch name validation rejects invalid special characters
    #[test]
    fn test_validate_branch_name_rejects_special_characters() {
        assert!(validate_branch_name("branch~name").is_err());
        assert!(validate_branch_name("branch^name").is_err());
        assert!(validate_branch_name("branch:name").is_err());
        assert!(validate_branch_name("branch?name").is_err());
        assert!(validate_branch_name("branch*name").is_err());
        assert!(validate_branch_name("branch[").is_err());
        assert!(validate_branch_name("branch]").is_err());
        assert!(validate_branch_name("branch..name").is_err());
    }

    /// Test: Branch name validation rejects empty names
    #[test]
    fn test_validate_branch_name_rejects_empty() {
        assert!(validate_branch_name("").is_err());
        assert!(validate_branch_name("   ").is_err());
    }

    /// Test: Branch name validation rejects names starting with dash
    #[test]
    fn test_validate_branch_name_rejects_leading_dash() {
        assert!(validate_branch_name("-branch").is_err());
    }

    /// Test: Worktree path generation
    #[test]
    fn test_generate_worktree_path() {
        let repo_path = PathBuf::from("/home/user/myproject");
        let branch_name = "feature/new-function";
        let worktree_path = generate_worktree_path(&repo_path, branch_name);
        assert_eq!(worktree_path, PathBuf::from("/home/user/myproject/feature-new-function"));
    }

    /// Test: Worktree path generation with special characters in branch name
    #[test]
    fn test_generate_worktree_path_replaces_special_chars() {
        let repo_path = PathBuf::from("/home/user/myproject");
        let branch_name = "feature/new_function";
        let worktree_path = generate_worktree_path(&repo_path, branch_name);
        assert_eq!(worktree_path, PathBuf::from("/home/user/myproject/feature-new_function"));
    }

    /// Test: Worktree path generation with nested branch
    #[test]
    fn test_generate_worktree_path_nested_branch() {
        let repo_path = PathBuf::from("/home/user/myproject");
        let branch_name = "user/john/feature/awesome";
        let worktree_path = generate_worktree_path(&repo_path, branch_name);
        assert_eq!(worktree_path, PathBuf::from("/home/user/myproject/user-john-feature-awesome"));
    }

    /// Test: NewBranchDialog initial state
    #[test]
    fn test_new_branch_dialog_initial_state() {
        let dialog = NewBranchDialog::new();
        assert!(!dialog.is_open());
        assert!(!dialog.is_creating());
        assert_eq!(dialog.branch_name(), "");
        assert!(!dialog.has_error());
    }

    /// Test: NewBranchDialog can be opened
    #[test]
    fn test_new_branch_dialog_open() {
        let mut dialog = NewBranchDialog::new();
        dialog.open();
        assert!(dialog.is_open());
        assert!(!dialog.is_creating());
    }

    /// Test: NewBranchDialog can be closed
    #[test]
    fn test_new_branch_dialog_close() {
        let mut dialog = NewBranchDialog::new();
        dialog.open();
        dialog.close();
        assert!(!dialog.is_open());
        assert!(!dialog.is_creating());
        assert_eq!(dialog.branch_name(), "");
    }

    /// Test: NewBranchDialog clears state on close
    #[test]
    fn test_new_branch_dialog_clears_state_on_close() {
        let mut dialog = NewBranchDialog::new();
        dialog.open();
        dialog.set_branch_name("feature/test");
        dialog.set_error("Some error");
        dialog.close();
        assert_eq!(dialog.branch_name(), "");
        assert!(!dialog.has_error());
    }

    /// Test: NewBranchDialog validates branch name
    #[test]
    fn test_new_branch_dialog_validates_branch_name() {
        let mut dialog = NewBranchDialog::new();
        dialog.open();
        dialog.set_branch_name("feature/test");
        dialog.validate();
        assert!(!dialog.has_error());
        assert!(dialog.is_create_enabled());
    }

    /// Test: NewBranchDialog rejects invalid branch name with spaces
    #[test]
    fn test_new_branch_dialog_rejects_branch_with_spaces() {
        let mut dialog = NewBranchDialog::new();
        dialog.open();
        dialog.set_branch_name("feature test");
        dialog.validate();
        assert!(dialog.has_error());
        assert!(!dialog.is_create_enabled());
        assert!(dialog.error_message().contains("space"));
    }

    /// Test: NewBranchDialog enters creating state
    #[test]
    fn test_new_branch_dialog_enter_creating_state() {
        let mut dialog = NewBranchDialog::new();
        dialog.open();
        dialog.set_branch_name("feature/test");
        dialog.start_creating();
        assert!(dialog.is_open());
        assert!(dialog.is_creating());
        assert!(!dialog.is_create_enabled());
    }

    /// Test: NewBranchDialog cannot be closed while creating
    #[test]
    fn test_new_branch_dialog_cannot_close_while_creating() {
        let mut dialog = NewBranchDialog::new();
        dialog.open();
        dialog.start_creating();
        dialog.close();
        assert!(dialog.is_open()); // Should remain open
    }

    /// Test: NewBranchDialog exits creating state on error
    #[test]
    fn test_new_branch_dialog_exits_creating_on_error() {
        let mut dialog = NewBranchDialog::new();
        dialog.open();
        dialog.start_creating();
        dialog.set_error("Failed to create");
        assert!(!dialog.is_creating());
        assert!(dialog.has_error());
    }
}