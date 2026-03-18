// help_panel.rs - Keyboard shortcuts help panel for pmux
use crate::keyboard_shortcuts::{KeyBinding, ShortcutCategory};
use serde::{Deserialize, Serialize};

/// Help panel state
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum HelpPanelState {
    Closed,
    Open { search_query: String, selected_category: Option<ShortcutCategory> },
}

impl Default for HelpPanelState {
    fn default() -> Self {
        HelpPanelState::Closed
    }
}

/// Help panel component
#[derive(Clone, Debug)]
pub struct HelpPanel {
    pub state: HelpPanelState,
    pub all_bindings: Vec<KeyBinding>,
}

impl Default for HelpPanel {
    fn default() -> Self {
        Self {
            state: HelpPanelState::default(),
            all_bindings: KeyBinding::all_defaults(),
        }
    }
}

impl HelpPanel {
    /// Create new help panel
    pub fn new() -> Self {
        Self::default()
    }

    /// Open the help panel
    pub fn open(&mut self) {
        self.state = HelpPanelState::Open {
            search_query: String::new(),
            selected_category: None,
        };
    }

    /// Close the help panel
    pub fn close(&mut self) {
        self.state = HelpPanelState::Closed;
    }

    /// Toggle open/closed
    pub fn toggle(&mut self) {
        match self.state {
            HelpPanelState::Closed => self.open(),
            HelpPanelState::Open { .. } => self.close(),
        }
    }

    /// Check if panel is open
    pub fn is_open(&self) -> bool {
        matches!(self.state, HelpPanelState::Open { .. })
    }

    /// Set search query
    pub fn set_search(&mut self, query: impl Into<String>) {
        if let HelpPanelState::Open { ref mut search_query, .. } = self.state {
            *search_query = query.into();
        }
    }

    /// Clear search query
    pub fn clear_search(&mut self) {
        self.set_search("");
    }

    /// Select category filter
    pub fn select_category(&mut self, category: Option<ShortcutCategory>) {
        if let HelpPanelState::Open { ref mut selected_category, .. } = self.state {
            *selected_category = category;
        }
    }

    /// Get filtered bindings based on search and category
    pub fn filtered_bindings(&self) -> Vec<&KeyBinding> {
        let (search, category) = match &self.state {
            HelpPanelState::Closed => return vec![],
            HelpPanelState::Open { search_query, selected_category } => {
                (search_query.to_lowercase(), *selected_category)
            }
        };

        self.all_bindings
            .iter()
            .filter(|binding| {
                // Category filter
                if let Some(cat) = category {
                    if binding.category != cat {
                        return false;
                    }
                }

                // Search filter
                if search.is_empty() {
                    return true;
                }

                binding.name.to_lowercase().contains(&search)
                    || binding.description.to_lowercase().contains(&search)
                    || binding.shortcut.to_lowercase().contains(&search)
            })
            .collect()
    }

    /// Get bindings grouped by category
    pub fn bindings_by_category(&self) -> Vec<(ShortcutCategory, Vec<&KeyBinding>)> {
        use ShortcutCategory::*;
        let categories = vec![General, Navigation, Workspace, View];

        categories
            .into_iter()
            .map(|cat| {
                let bindings: Vec<_> = self.filtered_bindings()
                    .into_iter()
                    .filter(|b| b.category == cat)
                    .collect();
                (cat, bindings)
            })
            .filter(|(_, bindings)| !bindings.is_empty())
            .collect()
    }

    /// Handle ESC key press
    pub fn handle_escape(&mut self) -> bool {
        if self.is_open() {
            self.close();
            true
        } else {
            false
        }
    }

    /// Get current search query
    pub fn search_query(&self) -> Option<&str> {
        match &self.state {
            HelpPanelState::Open { search_query, .. } => Some(search_query),
            _ => None,
        }
    }

    /// Get current selected category
    pub fn selected_category(&self) -> Option<ShortcutCategory> {
        match &self.state {
            HelpPanelState::Open { selected_category, .. } => *selected_category,
            _ => None,
        }
    }
}

/// Help panel renderer
pub struct HelpPanelRenderer;

impl HelpPanelRenderer {
    /// Render help panel as text
    pub fn render(panel: &HelpPanel) -> String {
        if !panel.is_open() {
            return String::new();
        }

        let mut output = String::new();
        output.push_str("╔══════════════════════════════════════════════════════════════╗\n");
        output.push_str("║                 Keyboard Shortcuts Help (⌘?)                 ║\n");
        output.push_str("╠══════════════════════════════════════════════════════════════╣\n");

        // Search bar
        if let Some(query) = panel.search_query() {
            output.push_str(&format!("║  Search: {}                                                  ║\n", query));
            output.push_str("╠══════════════════════════════════════════════════════════════╣\n");
        }

        // Categories
        for (category, bindings) in panel.bindings_by_category() {
            output.push_str(&format!("║  {}:\n", category_name(category)));
            output.push_str("║  ─────────────────────────────────────────────────────────── ║\n");

            for binding in bindings {
                output.push_str(&format!(
                    "║    {:20} {:30} {}\n",
                    binding.shortcut,
                    binding.name,
                    binding.description
                ));
            }
            output.push_str("║                                                              ║\n");
        }

        output.push_str("╚══════════════════════════════════════════════════════════════╝\n");
        output.push_str("  Press ESC to close | Type to search\n");

        output
    }

    /// Render compact help hint
    pub fn render_hint() -> &'static str {
        "Press ⌘? for keyboard shortcuts"
    }
}

fn category_name(category: ShortcutCategory) -> &'static str {
    use ShortcutCategory::*;
    match category {
        General => "General",
        Navigation => "Navigation",
        Workspace => "Workspace",
        View => "View",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keyboard_shortcuts::ShortcutAction;

    #[test]
    fn test_help_panel_default() {
        let panel = HelpPanel::new();
        assert!(!panel.is_open());
        assert_eq!(panel.all_bindings.len(), 27); // Default bindings count
    }

    #[test]
    fn test_help_panel_toggle() {
        let mut panel = HelpPanel::new();
        assert!(!panel.is_open());

        panel.toggle();
        assert!(panel.is_open());

        panel.toggle();
        assert!(!panel.is_open());
    }

    #[test]
    fn test_help_panel_open_close() {
        let mut panel = HelpPanel::new();
        panel.open();
        assert!(panel.is_open());

        panel.close();
        assert!(!panel.is_open());
    }

    #[test]
    fn test_search_filtering() {
        let mut panel = HelpPanel::new();
        panel.open();

        // Without search, should return all bindings
        let all = panel.filtered_bindings();
        assert_eq!(all.len(), 27);

        // Search for "new"
        panel.set_search("new");
        let filtered = panel.filtered_bindings();
        assert!(filtered.iter().any(|b| b.action == ShortcutAction::NewWorkspace));
        assert!(filtered.iter().any(|b| b.action == ShortcutAction::NewBranch));
    }

    #[test]
    fn test_category_filtering() {
        let mut panel = HelpPanel::new();
        panel.open();

        panel.select_category(Some(ShortcutCategory::General));
        let filtered = panel.filtered_bindings();
        assert!(filtered.iter().all(|b| b.category == ShortcutCategory::General));
    }

    #[test]
    fn test_combined_filters() {
        let mut panel = HelpPanel::new();
        panel.open();

        panel.select_category(Some(ShortcutCategory::Workspace));
        panel.set_search("toggle");

        let filtered = panel.filtered_bindings();
        assert!(filtered.iter().all(|b| b.category == ShortcutCategory::Workspace));
        assert!(filtered.iter().all(|b| b.name.to_lowercase().contains("toggle")));
    }

    #[test]
    fn test_bindings_by_category() {
        let mut panel = HelpPanel::new();
        panel.open();

        let by_cat = panel.bindings_by_category();
        assert!(!by_cat.is_empty());

        // Each category should have at least one binding
        for (_, bindings) in &by_cat {
            assert!(!bindings.is_empty());
        }
    }

    #[test]
    fn test_handle_escape() {
        let mut panel = HelpPanel::new();
        panel.open();
        assert!(panel.is_open());

        let handled = panel.handle_escape();
        assert!(handled);
        assert!(!panel.is_open());

        // Should not handle when already closed
        let handled = panel.handle_escape();
        assert!(!handled);
    }

    #[test]
    fn test_clear_search() {
        let mut panel = HelpPanel::new();
        panel.open();
        panel.set_search("test query");
        assert_eq!(panel.search_query(), Some("test query"));

        panel.clear_search();
        assert_eq!(panel.search_query(), Some(""));
    }

    #[test]
    fn test_render_not_empty_when_open() {
        let mut panel = HelpPanel::new();
        panel.open();

        let rendered = HelpPanelRenderer::render(&panel);
        assert!(!rendered.is_empty());
        assert!(rendered.contains("Keyboard Shortcuts"));
    }

    #[test]
    fn test_render_empty_when_closed() {
        let panel = HelpPanel::new();
        let rendered = HelpPanelRenderer::render(&panel);
        assert!(rendered.is_empty());
    }
}
