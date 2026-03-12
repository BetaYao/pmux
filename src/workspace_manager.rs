// workspace_manager.rs - Multi-workspace tab management
use std::path::{Path, PathBuf};
use serde::{Serialize, Deserialize};

/// Represents a single workspace tab
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceTab {
    pub path: PathBuf,
    pub name: String,
    pub display_name: String,
    pub is_modified: bool,
    /// Last selected worktree index for this tab (preserved across tab switches)
    #[serde(default)]
    pub last_worktree_index: Option<usize>,
}

impl WorkspaceTab {
    /// Create a new workspace tab
    pub fn new(path: PathBuf) -> Self {
        let name = path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unnamed".to_string());
        
        Self {
            path: path.clone(),
            name: name.clone(),
            display_name: name,
            is_modified: false,
            last_worktree_index: None,
        }
    }

    /// Get the workspace path
    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    /// Get the workspace name (directory name)
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the display name (may include disambiguation)
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    /// Check if workspace has unsaved changes
    pub fn is_modified(&self) -> bool {
        self.is_modified
    }

    /// Mark as modified
    pub fn mark_modified(&mut self) {
        self.is_modified = true;
    }

    /// Mark as saved
    pub fn mark_saved(&mut self) {
        self.is_modified = false;
    }

    /// Update display name with disambiguation
    pub fn set_display_name(&mut self, name: String) {
        self.display_name = name;
    }

    /// Save the last selected worktree index for this tab
    pub fn save_worktree_index(&mut self, index: Option<usize>) {
        self.last_worktree_index = index;
    }

    /// Get the last selected worktree index for this tab
    pub fn last_worktree_index(&self) -> Option<usize> {
        self.last_worktree_index
    }
}

/// Manages multiple workspace tabs
#[derive(Debug, Default, Clone)]
pub struct WorkspaceManager {
    tabs: Vec<WorkspaceTab>,
    active_index: Option<usize>,
}

impl WorkspaceManager {
    /// Create a new empty workspace manager
    pub fn new() -> Self {
        Self {
            tabs: Vec::new(),
            active_index: None,
        }
    }

    /// Get the number of open tabs
    pub fn tab_count(&self) -> usize {
        self.tabs.len()
    }

    /// Check if there are any tabs
    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    /// Get the currently active tab index
    pub fn active_tab_index(&self) -> Option<usize> {
        self.active_index
    }

    /// Get the currently active tab
    pub fn active_tab(&self) -> Option<&WorkspaceTab> {
        self.active_index.and_then(|i| self.tabs.get(i))
    }

    /// Get mutable reference to active tab
    pub fn active_tab_mut(&mut self) -> Option<&mut WorkspaceTab> {
        self.active_index.and_then(|i| self.tabs.get_mut(i))
    }

    /// Get a tab by index
    pub fn get_tab(&self, index: usize) -> Option<&WorkspaceTab> {
        self.tabs.get(index)
    }

    /// Get mutable reference to a tab
    pub fn get_tab_mut(&mut self, index: usize) -> Option<&mut WorkspaceTab> {
        self.tabs.get_mut(index)
    }

    /// Iterate over all tabs
    pub fn tabs(&self) -> impl Iterator<Item = &WorkspaceTab> {
        self.tabs.iter()
    }

    /// Add a new workspace tab
    /// Returns the index of the new tab
    pub fn add_workspace(&mut self, path: PathBuf) -> usize {
        let tab = WorkspaceTab::new(path);
        let index = self.tabs.len();
        self.tabs.push(tab);
        
        // Update display names to ensure uniqueness
        self.update_display_names();
        
        // If this is the first tab, make it active
        if self.active_index.is_none() {
            self.active_index = Some(index);
        }
        
        index
    }

    /// Switch to a specific tab by index
    pub fn switch_to_tab(&mut self, index: usize) -> bool {
        if index < self.tabs.len() {
            self.active_index = Some(index);
            true
        } else {
            false
        }
    }

    /// Switch to the next tab
    pub fn next_tab(&mut self) -> bool {
        if let Some(current) = self.active_index {
            let next = (current + 1) % self.tabs.len();
            self.switch_to_tab(next)
        } else {
            false
        }
    }

    /// Switch to the previous tab
    pub fn prev_tab(&mut self) -> bool {
        if let Some(current) = self.active_index {
            let prev = if current == 0 {
                self.tabs.len() - 1
            } else {
                current - 1
            };
            self.switch_to_tab(prev)
        } else {
            false
        }
    }

    /// Close a tab by index
    /// Returns true if the tab was closed successfully
    pub fn close_tab(&mut self, index: usize) -> bool {
        if index >= self.tabs.len() {
            return false;
        }

        self.tabs.remove(index);
        
        // Update active index
        if let Some(active) = self.active_index {
            match active.cmp(&index) {
                std::cmp::Ordering::Equal => {
                    // Closed the active tab
                    if self.tabs.is_empty() {
                        self.active_index = None;
                    } else if active >= self.tabs.len() {
                        self.active_index = Some(self.tabs.len() - 1);
                    }
                    // else keep same index (next tab slides in)
                }
                std::cmp::Ordering::Greater => {
                    // Closed a tab before the active one
                    self.active_index = Some(active - 1);
                }
                std::cmp::Ordering::Less => {
                    // Closed a tab after the active one, no change needed
                }
            }
        }
        
        // Update display names after removal
        self.update_display_names();
        
        true
    }

    /// Close the currently active tab
    pub fn close_active_tab(&mut self) -> bool {
        if let Some(index) = self.active_index {
            self.close_tab(index)
        } else {
            false
        }
    }

    /// Check if a workspace is already open
    pub fn is_workspace_open(&self, path: &Path) -> bool {
        self.tabs.iter().any(|tab| tab.path == path)
    }

    /// Find the index of a workspace by path
    pub fn find_workspace_index(&self, path: &Path) -> Option<usize> {
        self.tabs.iter().position(|tab| tab.path == path)
    }

    /// Update display names to handle name collisions
    fn update_display_names(&mut self) {
        // Group tabs by base name
        let mut name_counts: std::collections::HashMap<String, Vec<usize>> = 
            std::collections::HashMap::new();
        
        for (index, tab) in self.tabs.iter().enumerate() {
            name_counts
                .entry(tab.name.clone())
                .or_default()
                .push(index);
        }
        
        // Update display names for duplicates
        for (name, indices) in name_counts {
            if indices.len() > 1 {
                // Multiple tabs with same name, add parent directory
                for &index in &indices {
                    if let Some(tab) = self.tabs.get(index) {
                        if let Some(parent) = tab.path.parent() {
                            if let Some(parent_name) = parent.file_name() {
                                let new_display = format!(
                                    "{} ({}",
                                    name,
                                    parent_name.to_string_lossy()
                                );
                                if let Some(tab) = self.tabs.get_mut(index) {
                                    tab.set_display_name(new_display);
                                }
                            }
                        }
                    }
                }
            } else {
                // Unique name, use base name
                if let Some(&index) = indices.first() {
                    if let Some(tab) = self.tabs.get_mut(index) {
                        tab.set_display_name(name.clone());
                    }
                }
            }
        }
    }

    /// Get all workspace paths for persistence
    pub fn workspace_paths(&self) -> Vec<PathBuf> {
        self.tabs.iter().map(|tab| tab.path.clone()).collect()
    }

    /// Get the active workspace path
    pub fn active_workspace_path(&self) -> Option<&PathBuf> {
        self.active_tab().map(|tab| &tab.path)
    }

    /// Set the active tab by workspace path
    pub fn set_active_tab(&mut self, path: PathBuf) -> Result<(), WorkspaceError> {
        if let Some(index) = self.find_workspace_index(&path) {
            self.switch_to_tab(index);
            Ok(())
        } else {
            Err(WorkspaceError::WorkspaceNotFound)
        }
    }

    /// Close a tab by workspace path
    pub fn close_tab_by_path(&mut self, path: &Path) -> Result<(), WorkspaceError> {
        if let Some(index) = self.find_workspace_index(path) {
            self.close_tab(index);
            Ok(())
        } else {
            Err(WorkspaceError::WorkspaceNotFound)
        }
    }

    /// Remove a tab by workspace path
    pub fn remove_tab(&mut self, path: PathBuf) -> Result<(), WorkspaceError> {
        if let Some(index) = self.find_workspace_index(&path) {
            self.close_tab(index);
            Ok(())
        } else {
            Err(WorkspaceError::WorkspaceNotFound)
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum WorkspaceError {
    WorkspaceNotFound,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test: WorkspaceTab creation
    #[test]
    fn test_workspace_tab_creation() {
        let tab = WorkspaceTab::new(PathBuf::from("/home/user/project1"));
        
        assert_eq!(tab.name(), "project1");
        assert_eq!(tab.path(), &PathBuf::from("/home/user/project1"));
        assert!(!tab.is_modified());
    }

    /// Test: WorkspaceManager creation
    #[test]
    fn test_workspace_manager_creation() {
        let manager = WorkspaceManager::new();
        assert_eq!(manager.tab_count(), 0);
        assert!(manager.active_tab().is_none());
        assert!(manager.is_empty());
    }

    /// Test: Add workspace tab
    #[test]
    fn test_add_workspace_tab() {
        let mut manager = WorkspaceManager::new();
        let path = PathBuf::from("/tmp/project1");
        
        let index = manager.add_workspace(path.clone());
        
        assert_eq!(manager.tab_count(), 1);
        assert_eq!(manager.active_tab_index(), Some(0));
        assert_eq!(manager.get_tab(index).unwrap().path(), &path);
    }

    /// Test: Switch active tab
    #[test]
    fn test_switch_active_tab() {
        let mut manager = WorkspaceManager::new();
        manager.add_workspace(PathBuf::from("/tmp/project1"));
        manager.add_workspace(PathBuf::from("/tmp/project2"));
        
        assert!(manager.switch_to_tab(1));
        assert_eq!(manager.active_tab_index(), Some(1));
        
        // Invalid index should fail
        assert!(!manager.switch_to_tab(5));
    }

    /// Test: Next/previous tab navigation
    #[test]
    fn test_tab_navigation() {
        let mut manager = WorkspaceManager::new();
        manager.add_workspace(PathBuf::from("/tmp/project1"));
        manager.add_workspace(PathBuf::from("/tmp/project2"));
        manager.add_workspace(PathBuf::from("/tmp/project3"));
        
        // Start at index 0
        assert_eq!(manager.active_tab_index(), Some(0));
        
        // Next should go to 1
        manager.next_tab();
        assert_eq!(manager.active_tab_index(), Some(1));
        
        // Next should go to 2
        manager.next_tab();
        assert_eq!(manager.active_tab_index(), Some(2));
        
        // Next should wrap to 0
        manager.next_tab();
        assert_eq!(manager.active_tab_index(), Some(0));
        
        // Previous should wrap to 2
        manager.prev_tab();
        assert_eq!(manager.active_tab_index(), Some(2));
    }

    /// Test: Close tab
    #[test]
    fn test_close_tab() {
        let mut manager = WorkspaceManager::new();
        manager.add_workspace(PathBuf::from("/tmp/project1"));
        manager.add_workspace(PathBuf::from("/tmp/project2"));
        
        assert!(manager.close_tab(0));
        assert_eq!(manager.tab_count(), 1);
        assert_eq!(manager.active_tab_index(), Some(0));
        
        // Invalid index should fail
        assert!(!manager.close_tab(5));
    }

    /// Test: Close active tab switches to another
    #[test]
    fn test_close_active_tab() {
        let mut manager = WorkspaceManager::new();
        manager.add_workspace(PathBuf::from("/tmp/project1"));
        manager.add_workspace(PathBuf::from("/tmp/project2"));
        manager.add_workspace(PathBuf::from("/tmp/project3"));
        
        // Switch to middle tab
        manager.switch_to_tab(1);
        assert_eq!(manager.active_tab_index(), Some(1));
        
        // Close it, should switch to tab 1 (was tab 2)
        manager.close_tab(1);
        assert_eq!(manager.active_tab_index(), Some(1));
        assert_eq!(manager.get_tab(1).unwrap().name(), "project3");
    }

    /// Test: Close last remaining tab
    #[test]
    fn test_close_last_tab() {
        let mut manager = WorkspaceManager::new();
        manager.add_workspace(PathBuf::from("/tmp/project1"));
        
        manager.close_tab(0);
        
        assert_eq!(manager.tab_count(), 0);
        assert!(manager.active_tab().is_none());
        assert!(manager.is_empty());
    }

    /// Test: Check if workspace is open
    #[test]
    fn test_is_workspace_open() {
        let mut manager = WorkspaceManager::new();
        let path = PathBuf::from("/tmp/project1");
        
        assert!(!manager.is_workspace_open(&path));
        
        manager.add_workspace(path.clone());
        
        assert!(manager.is_workspace_open(&path));
        assert!(!manager.is_workspace_open(&PathBuf::from("/tmp/other")));
    }

    /// Test: Find workspace index
    #[test]
    fn test_find_workspace_index() {
        let mut manager = WorkspaceManager::new();
        let path1 = PathBuf::from("/tmp/project1");
        let path2 = PathBuf::from("/tmp/project2");
        
        manager.add_workspace(path1.clone());
        manager.add_workspace(path2.clone());
        
        assert_eq!(manager.find_workspace_index(&path1), Some(0));
        assert_eq!(manager.find_workspace_index(&path2), Some(1));
        assert_eq!(manager.find_workspace_index(&PathBuf::from("/tmp/other")), None);
    }

    /// Test: Modified state tracking
    #[test]
    fn test_modified_state() {
        let mut tab = WorkspaceTab::new(PathBuf::from("/tmp/project"));
        
        assert!(!tab.is_modified());
        
        tab.mark_modified();
        assert!(tab.is_modified());
        
        tab.mark_saved();
        assert!(!tab.is_modified());
    }

    /// Test: Workspace paths collection
    #[test]
    fn test_workspace_paths() {
        let mut manager = WorkspaceManager::new();
        manager.add_workspace(PathBuf::from("/tmp/project1"));
        manager.add_workspace(PathBuf::from("/tmp/project2"));
        
        let paths = manager.workspace_paths();
        assert_eq!(paths.len(), 2);
        assert!(paths.contains(&PathBuf::from("/tmp/project1")));
        assert!(paths.contains(&PathBuf::from("/tmp/project2")));
    }

    /// Test: Active workspace path
    #[test]
    fn test_active_workspace_path() {
        let mut manager = WorkspaceManager::new();
        
        assert!(manager.active_workspace_path().is_none());
        
        manager.add_workspace(PathBuf::from("/tmp/project1"));
        
        assert_eq!(
            manager.active_workspace_path(),
            Some(&PathBuf::from("/tmp/project1"))
        );
    }

    /// Test: set_active_tab selects tab by workspace path
    #[test]
    fn test_set_active_tab_by_path() {
        let mut manager = WorkspaceManager::new();
        let path1 = PathBuf::from("/tmp/project1");
        let path2 = PathBuf::from("/tmp/project2");

        manager.add_workspace(path1.clone());
        manager.add_workspace(path2.clone());

        // Initially first tab is active
        assert_eq!(manager.active_tab_index(), Some(0));

        // When setting active tab by second path, active index should change
        manager
            .set_active_tab(PathBuf::from("/tmp/project2"))
            .expect("should find workspace");
        assert_eq!(manager.active_tab_index(), Some(1));

        // Non-existing path should return an error and keep active index unchanged
        let err = manager
            .set_active_tab(PathBuf::from("/tmp/other"))
            .expect_err("should fail for missing workspace");
        match err {
            WorkspaceError::WorkspaceNotFound => {}
        }
        assert_eq!(manager.active_tab_index(), Some(1));
    }

    /// Test: close_tab_by_path closes tab by workspace path
    #[test]
    fn test_close_tab_by_path() {
        let mut manager = WorkspaceManager::new();
        let path1 = PathBuf::from("/tmp/project1");
        let path2 = PathBuf::from("/tmp/project2");

        manager.add_workspace(path1.clone());
        manager.add_workspace(path2.clone());

        assert_eq!(manager.tab_count(), 2);
        assert_eq!(manager.active_tab_index(), Some(0));

        // Close non-active tab
        manager
            .close_tab_by_path(Path::new("/tmp/project2"))
            .expect("should close existing workspace");
        assert_eq!(manager.tab_count(), 1);

        // Closing non-existing path should error
        let err = manager
            .close_tab_by_path(Path::new("/tmp/other"))
            .expect_err("should fail for missing workspace");
        match err {
            WorkspaceError::WorkspaceNotFound => {}
        }
    }

    /// Test: remove_tab removes tab by workspace path
    #[test]
    fn test_remove_tab_by_path() {
        let mut manager = WorkspaceManager::new();
        let path1 = PathBuf::from("/tmp/project1");
        let path2 = PathBuf::from("/tmp/project2");

        manager.add_workspace(path1.clone());
        manager.add_workspace(path2.clone());

        assert_eq!(manager.tab_count(), 2);
        assert_eq!(manager.active_tab_index(), Some(0));

        // Remove non-active tab
        manager
            .remove_tab(PathBuf::from("/tmp/project2"))
            .expect("should remove existing workspace");
        assert_eq!(manager.tab_count(), 1);
        assert_eq!(manager.active_tab_index(), Some(0));
        assert_eq!(
            manager.active_workspace_path(),
            Some(&PathBuf::from("/tmp/project1"))
        );

        // Removing non-existing workspace should error
        let err = manager
            .remove_tab(PathBuf::from("/tmp/other"))
            .expect_err("should fail for missing workspace");
        match err {
            WorkspaceError::WorkspaceNotFound => {}
        }

        // Remove remaining (active) tab
        manager
            .remove_tab(PathBuf::from("/tmp/project1"))
            .expect("should remove remaining workspace");
        assert_eq!(manager.tab_count(), 0);
        assert!(manager.active_tab().is_none());
    }
}
