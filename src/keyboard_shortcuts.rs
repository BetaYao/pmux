// keyboard_shortcuts.rs - Keyboard shortcuts system for pmux
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Shortcut categories
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ShortcutCategory {
    General,
    Navigation,
    Workspace,
    View,
    Tasks,
}

/// Shortcut actions
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ShortcutAction {
    // General
    NewWorkspace,
    NewBranch,
    CloseTab,
    ToggleSidebar,
    OpenNotifications,
    ShowHelp,
    OpenSettings,

    // Navigation
    SwitchTab1,
    SwitchTab2,
    SwitchTab3,
    SwitchTab4,
    SwitchTab5,
    SwitchTab6,
    SwitchTab7,
    SwitchTab8,
    JumpToUnread,
    NextTab,
    PrevTab,
    NextWorktree,
    PrevWorktree,

    // Workspace
    VerticalSplit,
    HorizontalSplit,
    ToggleMute,
    FocusPaneLeft,
    FocusPaneRight,
    FocusPaneUp,
    FocusPaneDown,

    // View
    Refresh,
    ZoomIn,
    ZoomOut,
    ResetZoom,
    ViewDiff,

    // Tasks
    ToggleTaskList,
    NewTask,
    DeleteTask,
}

/// Key binding structure
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct KeyBinding {
    pub action: ShortcutAction,
    pub name: String,
    pub shortcut: String,
    pub description: String,
    pub category: ShortcutCategory,
}

impl KeyBinding {
    /// Create a new key binding
    pub fn new(
        action: ShortcutAction,
        name: impl Into<String>,
        shortcut: impl Into<String>,
        description: impl Into<String>,
        category: ShortcutCategory,
    ) -> Self {
        Self {
            action,
            name: name.into(),
            shortcut: shortcut.into(),
            description: description.into(),
            category,
        }
    }

    /// Get all default key bindings
    pub fn all_defaults() -> Vec<Self> {
        vec![
            // General
            Self::new(
                ShortcutAction::NewWorkspace,
                "New Workspace",
                "⌘N",
                "Create a new workspace",
                ShortcutCategory::General,
            ),
            Self::new(
                ShortcutAction::NewBranch,
                "New Branch",
                "⌘⇧N",
                "Create a new branch with worktree",
                ShortcutCategory::General,
            ),
            Self::new(
                ShortcutAction::CloseTab,
                "Close Tab",
                "⌘W",
                "Close the current tab",
                ShortcutCategory::General,
            ),
            Self::new(
                ShortcutAction::ToggleSidebar,
                "Toggle Sidebar",
                "⌘B",
                "Show or hide the sidebar",
                ShortcutCategory::General,
            ),
            Self::new(
                ShortcutAction::OpenNotifications,
                "Open Notifications",
                "⌘I",
                "Open the notifications panel",
                ShortcutCategory::General,
            ),
            Self::new(
                ShortcutAction::ShowHelp,
                "Show Help",
                "⌘?",
                "Show keyboard shortcuts help",
                ShortcutCategory::General,
            ),
            Self::new(
                ShortcutAction::OpenSettings,
                "Open Settings",
                "⌘,",
                "Open or close the settings panel",
                ShortcutCategory::General,
            ),
            // Navigation
            Self::new(
                ShortcutAction::SwitchTab1,
                "Switch to Tab 1",
                "⌘1",
                "Switch to the first tab",
                ShortcutCategory::Navigation,
            ),
            Self::new(
                ShortcutAction::SwitchTab2,
                "Switch to Tab 2",
                "⌘2",
                "Switch to the second tab",
                ShortcutCategory::Navigation,
            ),
            Self::new(
                ShortcutAction::SwitchTab3,
                "Switch to Tab 3",
                "⌘3",
                "Switch to the third tab",
                ShortcutCategory::Navigation,
            ),
            Self::new(
                ShortcutAction::SwitchTab4,
                "Switch to Tab 4",
                "⌘4",
                "Switch to the fourth tab",
                ShortcutCategory::Navigation,
            ),
            Self::new(
                ShortcutAction::SwitchTab5,
                "Switch to Tab 5",
                "⌘5",
                "Switch to the fifth tab",
                ShortcutCategory::Navigation,
            ),
            Self::new(
                ShortcutAction::SwitchTab6,
                "Switch to Tab 6",
                "⌘6",
                "Switch to the sixth tab",
                ShortcutCategory::Navigation,
            ),
            Self::new(
                ShortcutAction::SwitchTab7,
                "Switch to Tab 7",
                "⌘7",
                "Switch to the seventh tab",
                ShortcutCategory::Navigation,
            ),
            Self::new(
                ShortcutAction::SwitchTab8,
                "Switch to Tab 8",
                "⌘8",
                "Switch to the eighth tab",
                ShortcutCategory::Navigation,
            ),
            Self::new(
                ShortcutAction::JumpToUnread,
                "Jump to Unread",
                "⌘⇧U",
                "Jump to the most recent unread notification",
                ShortcutCategory::Navigation,
            ),
            Self::new(
                ShortcutAction::NextTab,
                "Next Tab",
                "⌘]",
                "Switch to the next workspace tab",
                ShortcutCategory::Navigation,
            ),
            Self::new(
                ShortcutAction::PrevTab,
                "Previous Tab",
                "⌘[",
                "Switch to the previous workspace tab",
                ShortcutCategory::Navigation,
            ),
            Self::new(
                ShortcutAction::NextWorktree,
                "Next Worktree",
                "⌘↓",
                "Switch to the next worktree in current workspace",
                ShortcutCategory::Navigation,
            ),
            Self::new(
                ShortcutAction::PrevWorktree,
                "Previous Worktree",
                "⌘↑",
                "Switch to the previous worktree in current workspace",
                ShortcutCategory::Navigation,
            ),
            // Workspace
            Self::new(
                ShortcutAction::VerticalSplit,
                "Vertical Split",
                "⌘D",
                "Split pane vertically",
                ShortcutCategory::Workspace,
            ),
            Self::new(
                ShortcutAction::HorizontalSplit,
                "Horizontal Split",
                "⌘⇧D",
                "Split pane horizontally",
                ShortcutCategory::Workspace,
            ),
            Self::new(
                ShortcutAction::ToggleMute,
                "Toggle Mute",
                "⌘M",
                "Mute or unmute notifications for current worktree",
                ShortcutCategory::Workspace,
            ),
            Self::new(
                ShortcutAction::FocusPaneLeft,
                "Focus Pane Left",
                "⌥⌘←",
                "Move focus to the pane on the left",
                ShortcutCategory::Workspace,
            ),
            Self::new(
                ShortcutAction::FocusPaneRight,
                "Focus Pane Right",
                "⌥⌘→",
                "Move focus to the pane on the right",
                ShortcutCategory::Workspace,
            ),
            Self::new(
                ShortcutAction::FocusPaneUp,
                "Focus Pane Up",
                "⌥⌘↑",
                "Move focus to the pane above",
                ShortcutCategory::Workspace,
            ),
            Self::new(
                ShortcutAction::FocusPaneDown,
                "Focus Pane Down",
                "⌥⌘↓",
                "Move focus to the pane below",
                ShortcutCategory::Workspace,
            ),
            // View
            Self::new(
                ShortcutAction::Refresh,
                "Refresh",
                "⌘R",
                "Refresh the current view",
                ShortcutCategory::View,
            ),
            Self::new(
                ShortcutAction::ZoomIn,
                "Zoom In",
                "⌘+",
                "Increase zoom level",
                ShortcutCategory::View,
            ),
            Self::new(
                ShortcutAction::ZoomOut,
                "Zoom Out",
                "⌘-",
                "Decrease zoom level",
                ShortcutCategory::View,
            ),
            Self::new(
                ShortcutAction::ResetZoom,
                "Reset Zoom",
                "⌘0",
                "Reset zoom to default",
                ShortcutCategory::View,
            ),
            Self::new(
                ShortcutAction::ViewDiff,
                "View Diff",
                "⌘⇧R",
                "Open diff view for current branch",
                ShortcutCategory::View,
            ),
            // Tasks
            Self::new(
                ShortcutAction::ToggleTaskList,
                "Toggle Task List",
                "⌘⇧L",
                "Show/hide and focus the scheduled tasks list",
                ShortcutCategory::Tasks,
            ),
            Self::new(
                ShortcutAction::NewTask,
                "New Task",
                "⌘⇧T",
                "Create a new scheduled task",
                ShortcutCategory::Tasks,
            ),
            Self::new(
                ShortcutAction::DeleteTask,
                "Delete Task",
                "⌘⇧⌫",
                "Delete the selected scheduled task",
                ShortcutCategory::Tasks,
            ),
        ]
    }
}

/// Keyboard shortcut registry
#[derive(Clone, Debug, Default)]
pub struct ShortcutRegistry {
    bindings: HashMap<ShortcutAction, KeyBinding>,
    shortcuts: HashMap<String, ShortcutAction>,
}

impl ShortcutRegistry {
    /// Create new registry with default bindings
    pub fn new() -> Self {
        let mut registry = Self::default();
        for binding in KeyBinding::all_defaults() {
            registry.register(binding);
        }
        registry
    }

    /// Register a key binding
    pub fn register(&mut self, binding: KeyBinding) {
        let shortcut = binding.shortcut.clone();
        let action = binding.action;
        self.bindings.insert(action, binding);
        self.shortcuts.insert(shortcut, action);
    }

    /// Unregister a binding
    pub fn unregister(&mut self, action: ShortcutAction) {
        if let Some(binding) = self.bindings.remove(&action) {
            self.shortcuts.remove(&binding.shortcut);
        }
    }

    /// Look up action by shortcut string
    pub fn lookup(&self, shortcut: &str) -> Option<ShortcutAction> {
        self.shortcuts.get(shortcut).copied()
    }

    /// Get binding for an action
    pub fn get_binding(&self, action: ShortcutAction) -> Option<&KeyBinding> {
        self.bindings.get(&action)
    }

    /// Check if shortcut is registered
    pub fn is_registered(&self, shortcut: &str) -> bool {
        self.shortcuts.contains_key(shortcut)
    }

    /// Check for conflicts with existing bindings
    pub fn check_conflict(&self, shortcut: &str) -> Option<ShortcutAction> {
        self.shortcuts.get(shortcut).copied()
    }

    /// Get all bindings
    pub fn all_bindings(&self) -> Vec<&KeyBinding> {
        self.bindings.values().collect()
    }

    /// Update a shortcut
    pub fn update_shortcut(
        &mut self,
        action: ShortcutAction,
        new_shortcut: impl Into<String>,
    ) -> Result<(), ShortcutError> {
        let new_shortcut = new_shortcut.into();

        // Check for conflicts
        if let Some(conflict) = self.check_conflict(&new_shortcut) {
            if conflict != action {
                return Err(ShortcutError::Conflict {
                    shortcut: new_shortcut,
                    existing_action: conflict,
                });
            }
        }

        // Remove old shortcut
        if let Some(binding) = self.bindings.get_mut(&action) {
            self.shortcuts.remove(&binding.shortcut);
            binding.shortcut = new_shortcut.clone();
            self.shortcuts.insert(new_shortcut, action);
        }

        Ok(())
    }
}

/// Shortcut errors
#[derive(Debug, thiserror::Error)]
pub enum ShortcutError {
    #[error("Shortcut '{shortcut}' is already bound to '{existing_action:?}'")]
    Conflict {
        shortcut: String,
        existing_action: ShortcutAction,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_binding_creation() {
        let binding = KeyBinding::new(
            ShortcutAction::NewWorkspace,
            "Test",
            "⌘T",
            "Test description",
            ShortcutCategory::General,
        );
        assert_eq!(binding.name, "Test");
        assert_eq!(binding.shortcut, "⌘T");
        assert_eq!(binding.category, ShortcutCategory::General);
    }

    #[test]
    fn test_default_bindings_count() {
        let bindings = KeyBinding::all_defaults();
        // General(7) + Navigation(13) + Workspace(7) + View(5) + Tasks(3) = 35
        assert_eq!(bindings.len(), 35, "Expected 35 default bindings");
    }

    #[test]
    fn test_shortcut_registry_new() {
        let registry = ShortcutRegistry::new();
        assert!(!registry.all_bindings().is_empty());
    }

    #[test]
    fn test_shortcut_lookup() {
        let registry = ShortcutRegistry::new();
        assert_eq!(
            registry.lookup("⌘N"),
            Some(ShortcutAction::NewWorkspace)
        );
        assert_eq!(
            registry.lookup("⌘B"),
            Some(ShortcutAction::ToggleSidebar)
        );
    }

    #[test]
    fn test_new_shortcut_lookups() {
        let registry = ShortcutRegistry::new();
        assert_eq!(registry.lookup("⌘]"), Some(ShortcutAction::NextTab));
        assert_eq!(registry.lookup("⌘["), Some(ShortcutAction::PrevTab));
        assert_eq!(registry.lookup("⌘,"), Some(ShortcutAction::OpenSettings));
        assert_eq!(registry.lookup("⌘↓"), Some(ShortcutAction::NextWorktree));
        assert_eq!(registry.lookup("⌘↑"), Some(ShortcutAction::PrevWorktree));
    }

    #[test]
    fn test_get_binding() {
        let registry = ShortcutRegistry::new();
        let binding = registry.get_binding(ShortcutAction::NewWorkspace);
        assert!(binding.is_some());
        assert_eq!(binding.unwrap().shortcut, "⌘N");
    }

    #[test]
    fn test_is_registered() {
        let registry = ShortcutRegistry::new();
        assert!(registry.is_registered("⌘N"));
        assert!(!registry.is_registered("⌘X"));
    }

    #[test]
    fn test_check_conflict_existing() {
        let registry = ShortcutRegistry::new();
        let conflict = registry.check_conflict("⌘N");
        assert_eq!(conflict, Some(ShortcutAction::NewWorkspace));
    }

    #[test]
    fn test_check_conflict_none() {
        let registry = ShortcutRegistry::new();
        let conflict = registry.check_conflict("⌘X");
        assert_eq!(conflict, None);
    }

    #[test]
    fn test_update_shortcut_success() {
        let mut registry = ShortcutRegistry::new();
        assert!(registry.update_shortcut(ShortcutAction::NewWorkspace, "⌘X").is_ok());
        assert_eq!(
            registry.lookup("⌘X"),
            Some(ShortcutAction::NewWorkspace)
        );
        assert!(!registry.is_registered("⌘N")); // Old shortcut removed
    }

    #[test]
    fn test_update_shortcut_conflict() {
        let mut registry = ShortcutRegistry::new();
        let result = registry.update_shortcut(ShortcutAction::NewWorkspace, "⌘B");
        assert!(result.is_err());
    }

    #[test]
    fn test_unregister() {
        let mut registry = ShortcutRegistry::new();
        registry.unregister(ShortcutAction::NewWorkspace);
        assert!(!registry.is_registered("⌘N"));
        assert_eq!(registry.lookup("⌘N"), None);
    }

    #[test]
    fn test_category_variants() {
        // Ensure all categories are covered
        let cats = vec![
            ShortcutCategory::General,
            ShortcutCategory::Navigation,
            ShortcutCategory::Workspace,
            ShortcutCategory::View,
            ShortcutCategory::Tasks,
        ];
        assert_eq!(cats.len(), 5);
    }

    #[test]
    fn test_action_variants() {
        // Spot check some actions
        let actions = vec![
            ShortcutAction::NewWorkspace,
            ShortcutAction::ToggleSidebar,
            ShortcutAction::SwitchTab1,
            ShortcutAction::VerticalSplit,
            ShortcutAction::ZoomIn,
        ];
        assert_eq!(actions.len(), 5);
    }
}
