// ui/app_root_test.rs - Tests for AppRoot keyboard handling and workspace restoration

use crate::ui::app_root::AppRoot;
use gpui::*;

#[test]
fn test_app_root_initialization() {
    // Test that AppRoot can be created without crashing
    let app_root = AppRoot::new();
    let _ = app_root.sidebar_visible;
}

#[test]
fn test_sidebar_toggle_state() {
    // Test that sidebar_visible starts as true
    let app_root = AppRoot::new();
    assert!(app_root.sidebar_visible);
}

#[test]
fn test_has_workspaces_false_initially() {
    let app_root = AppRoot::new();
    assert!(!app_root.has_workspaces());
}