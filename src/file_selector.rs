// file_selector.rs - Cross-platform file selector integration
use std::path::PathBuf;

/// Show a folder picker dialog
/// Returns Some(PathBuf) if user selected a folder, None if cancelled
pub fn show_folder_picker() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Select Git Repository")
        .pick_folder()
}

/// Show a folder picker with a default starting path
pub fn show_folder_picker_with_default(start_path: &PathBuf) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Select Git Repository")
        .set_directory(start_path)
        .pick_folder()
}

/// Show an async folder picker dialog (for use inside gpui event loop)
/// Returns Some(PathBuf) if user selected a folder, None if cancelled
pub async fn show_folder_picker_async() -> Option<PathBuf> {
    let handle = rfd::AsyncFileDialog::new()
        .set_title("Select Git Repository")
        .pick_folder()
        .await;
    handle.map(|h| h.path().to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test: Module can be imported and functions exist
    #[test]
    fn test_module_exports() {
        // This test just verifies the module compiles and exports correctly
        // Actual UI testing requires integration tests or manual testing
        let _ = show_folder_picker as fn() -> Option<PathBuf>;
        let _ = show_folder_picker_with_default as fn(&PathBuf) -> Option<PathBuf>;
    }
}

// Note: UI tests for file dialogs are typically done via integration tests
// or manual testing since they require user interaction.
//
// For automated testing of the full flow, consider:
// 1. Mocking the file selector in test builds
// 2. Using headless testing frameworks
// 3. Integration tests with predefined paths
