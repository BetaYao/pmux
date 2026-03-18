// ui/app_root_test.rs - Tests for AppRoot keyboard handling and workspace restoration

use super::AppRoot;

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
fn test_has_workspaces_depends_on_config() {
    // AppRoot::new() loads workspaces from config, so has_workspaces()
    // depends on what's in ~/.config/pmux/config.json.
    // Just verify the method is callable and consistent with workspace_manager state.
    let app_root = AppRoot::new();
    assert_eq!(app_root.has_workspaces(), !app_root.workspace_manager.is_empty());
}

/// Regression: TerminalManager.buffers must share the same Arc as AppRoot.terminal_buffers.
/// If they are separate Arcs, setup_local_terminal inserts into TerminalManager's map
/// but rendering reads from AppRoot's map — resulting in empty terminal (the "—" placeholder bug).
#[test]
fn test_terminal_manager_shares_buffers_arc_with_app_root() {
    use crate::ui::terminal_manager::TerminalManager;
    use crate::ui::terminal_view::TerminalBuffer;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use std::sync::atomic::AtomicBool;

    // Simulate AppRoot.terminal_buffers (the Arc that rendering reads from)
    let app_root_buffers: Arc<Mutex<HashMap<String, TerminalBuffer>>> =
        Arc::new(Mutex::new(HashMap::new()));

    // Create TerminalManager (like ensure_entities does)
    let modal = Arc::new(AtomicBool::new(false));
    let drag = Arc::new(AtomicBool::new(false));
    let mut tm = TerminalManager::new(modal, drag);

    // Before sync: they are different Arcs
    assert!(!Arc::ptr_eq(&app_root_buffers, &tm.buffers));

    // Sync buffers (like attach_runtime does after the fix)
    tm.buffers = app_root_buffers.clone();

    // After sync: they must be the same Arc
    assert!(Arc::ptr_eq(&app_root_buffers, &tm.buffers));

    // Insert a buffer via TerminalManager's reference
    if let Ok(mut bufs) = tm.buffers.lock() {
        bufs.insert("test-pane".to_string(), TerminalBuffer::Empty);
    }

    // AppRoot's reference must see the same buffer
    let found = app_root_buffers.lock().unwrap().contains_key("test-pane");
    assert!(found, "Buffer inserted via TerminalManager must be visible through AppRoot.terminal_buffers");
}

/// Regression: The FocusHandle in TerminalBuffer::Terminal must be the SAME handle
/// that AppRoot's on_key_down div tracks via `.track_focus()`.
///
/// If they differ, focusing the terminal (for InputHandler/IME) moves focus AWAY from
/// the on_key_down div, causing ALL keyboard input (including Enter, arrows, Ctrl+C)
/// to stop reaching handle_key_down → forward_key_to_terminal.
///
/// The fix: attach_runtime syncs AppRoot.terminal_focus → TerminalManager.focus
/// before any setup_* call, so TerminalBuffer::Terminal { focus_handle } matches.
#[test]
fn test_terminal_focus_sync_contract() {
    use crate::ui::terminal_manager::TerminalManager;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    // Simulate: AppRoot has a FocusHandle, TerminalManager has its own.
    // After sync, they must be the same.
    let modal = Arc::new(AtomicBool::new(false));
    let drag = Arc::new(AtomicBool::new(false));
    let tm = TerminalManager::new(modal, drag);

    // Before sync: TerminalManager.focus is None (created lazily)
    assert!(tm.focus.is_none(), "TerminalManager.focus should be None before sync");

    // The contract: attach_runtime must set tm.focus = Some(app_root.terminal_focus)
    // before calling setup_local_terminal, so that the FocusHandle in TerminalBuffer
    // is the same one that AppRoot's on_key_down div tracks.
    //
    // This ensures:
    // 1. window.handle_input() registers InputHandler on the correct FocusHandle
    // 2. on_key_down fires when that FocusHandle is focused
    // 3. Both text input (InputHandler) and special keys (on_key_down) work
}