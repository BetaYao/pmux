// tmux/mod.rs - Tmux integration module
pub mod control_mode;
pub mod pane;
pub mod session;
pub mod window;

pub use control_mode::{attach as control_mode_attach, build_pane_map, ControlModeError, ControlModeHandle};
pub use pane::{PaneInfo, capture_pane, create_pane, get_pane_dimensions, list_panes, select_pane, send_keys, PaneError};
pub use session::{Session, SessionError};
pub use window::{WindowInfo, create_window, list_windows, rename_window, WindowError};

#[cfg(test)]
mod tests {
    use super::*;

    /// Test: Module exports are available
    #[test]
    fn test_module_exports() {
        // Verify all public items are accessible
        let _: fn(&str) -> Session = Session::new;
        let _: fn(&str) -> bool = Session::exists;
    }
}
