// ui/models/notification_panel_model.rs - Shared model for notification panel state
/// Shared model for notification panel. TopBar bell + NotificationPanel observe this.
/// Does NOT implement Render.
pub struct NotificationPanelModel {
    pub show_panel: bool,
    pub unread_count: usize,
}

impl NotificationPanelModel {
    pub fn new() -> Self {
        Self {
            show_panel: false,
            unread_count: 0,
        }
    }

    pub fn set_show_panel(&mut self, show: bool) {
        self.show_panel = show;
    }

    pub fn toggle_panel(&mut self) {
        self.show_panel = !self.show_panel;
    }

    pub fn set_unread_count(&mut self, count: usize) {
        self.unread_count = count;
    }
}

impl Default for NotificationPanelModel {
    fn default() -> Self {
        Self::new()
    }
}
