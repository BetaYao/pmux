//! E2E tests for Scheduled Tasks via GPUI simulate_keystrokes.
//! Tests the full keyboard-driven CRUD flow: create task, verify, delete task, verify.
//!
//! Run with: RUSTUP_TOOLCHAIN=stable cargo test --test gpui_keystroke_poc

use gpui::TestAppContext;
use pmux::ui::app_root::AppRoot;

/// Helper: create an AppRoot window with entities initialized and focused.
fn setup_app(cx: &mut TestAppContext) -> gpui::WindowHandle<AppRoot> {
    let window = cx.add_window(|_window, cx| {
        let mut app = AppRoot::new();
        app.ensure_entities_for_test(cx);
        app
    });
    cx.run_until_parked();
    window.update(cx, |app_root, window, cx| {
        app_root.focus_for_test(window, cx);
    }).unwrap();
    cx.run_until_parked();
    window
}

/// Helper: re-focus after a render cycle that might have changed focus.
fn refocus(window: &gpui::WindowHandle<AppRoot>, cx: &mut TestAppContext) {
    window.update(cx, |app_root, window, cx| {
        app_root.focus_for_test(window, cx);
    }).unwrap();
    cx.run_until_parked();
}

// ─── Individual shortcut tests ───────────────────────────────────────

#[gpui::test]
fn test_cmd_shift_t_opens_task_dialog(cx: &mut TestAppContext) {
    let window = setup_app(cx);

    window.update(cx, |app, _w, _cx| {
        assert!(!app.has_task_dialog(), "dialog should be closed initially");
    }).unwrap();

    cx.simulate_keystrokes(window.into(), "cmd-shift-t");
    cx.run_until_parked();

    window.update(cx, |app, _w, _cx| {
        assert!(app.has_task_dialog(), "Cmd+Shift+T should open TaskDialog");
    }).unwrap();
}

#[gpui::test]
fn test_cmd_shift_l_toggles_task_list(cx: &mut TestAppContext) {
    let window = setup_app(cx);

    window.update(cx, |app, _w, _cx| {
        assert!(app.is_tasks_expanded());
        assert!(!app.is_task_list_focused());
    }).unwrap();

    // Collapse
    cx.simulate_keystrokes(window.into(), "cmd-shift-l");
    cx.run_until_parked();

    window.update(cx, |app, _w, _cx| {
        assert!(!app.is_tasks_expanded());
    }).unwrap();

    // Re-focus (render may have changed focus), then expand
    refocus(&window, cx);
    cx.simulate_keystrokes(window.into(), "cmd-shift-l");
    cx.run_until_parked();

    window.update(cx, |app, _w, _cx| {
        assert!(app.is_tasks_expanded());
        assert!(app.is_task_list_focused());
    }).unwrap();
}

// ─── Full CRUD E2E test ──────────────────────────────────────────────

#[gpui::test]
fn test_create_and_delete_scheduled_task_via_keyboard(cx: &mut TestAppContext) {
    let window = setup_app(cx);

    // Step 1: Record initial task count
    let initial_count = window.update(cx, |app, _w, cx| {
        app.task_count(cx)
    }).unwrap();

    // Step 2: Cmd+Shift+T to open TaskDialog
    cx.simulate_keystrokes(window.into(), "cmd-shift-t");
    cx.run_until_parked();

    window.update(cx, |app, _w, _cx| {
        assert!(app.has_task_dialog(), "TaskDialog should be open");
    }).unwrap();

    // Step 3: Type task name
    cx.simulate_input(window.into(), "E2E_Test_Task");
    cx.run_until_parked();

    // Step 4: Tab past Cron, Tab to Command
    cx.simulate_keystrokes(window.into(), "tab");
    cx.run_until_parked();
    cx.simulate_keystrokes(window.into(), "tab");
    cx.run_until_parked();

    // Step 5: Type command
    cx.simulate_input(window.into(), "echo hello");
    cx.run_until_parked();

    // Step 6: Enter to save
    cx.simulate_keystrokes(window.into(), "enter");
    cx.run_until_parked();

    // Step 7: Verify task created
    window.update(cx, |app, _w, cx| {
        assert!(!app.has_task_dialog(), "Dialog should close after save");
        assert_eq!(app.task_count(cx), initial_count + 1, "One more task");
        let names = app.task_names(cx);
        assert!(names.iter().any(|n| n == "E2E_Test_Task"),
            "Should find E2E_Test_Task, got: {:?}", names);
    }).unwrap();

    // Step 8: Programmatically focus task list and select the new task
    // Re-focus terminal first so on_key_down fires
    refocus(&window, cx);

    let target_index = window.update(cx, |app, _w, cx| {
        let idx = app.task_count(cx) - 1;
        app.set_task_list_focused_for_test(true, idx, cx);
        idx
    }).unwrap();
    cx.run_until_parked();

    window.update(cx, |app, _w, _cx| {
        assert!(app.is_task_list_focused(), "Task list should be focused");
        assert_eq!(app.selected_task_index(), Some(target_index));
    }).unwrap();

    // Step 10: Verify count before delete
    let count_before_delete = window.update(cx, |app, _w, cx| {
        app.task_count(cx)
    }).unwrap();

    // Step 11: Cmd+Shift+Backspace to initiate delete
    cx.simulate_keystrokes(window.into(), "cmd-shift-backspace");
    cx.run_until_parked();

    window.update(cx, |app, _w, _cx| {
        assert!(app.has_task_pending_delete(), "Should have pending delete");
    }).unwrap();

    // Step 12: Enter to confirm deletion
    // Note: we need the key event to go through task_list_focused handler,
    // not through handle_shortcut, so just send plain "enter"
    cx.simulate_keystrokes(window.into(), "enter");
    cx.run_until_parked();

    // Step 13: Verify deleted
    window.update(cx, |app, _w, cx| {
        assert!(!app.has_task_pending_delete(), "Pending delete cleared");
        assert_eq!(app.task_count(cx), count_before_delete - 1, "One fewer task");
    }).unwrap();
}

// ─── Delete cancellation test ────────────────────────────────────────

#[gpui::test]
fn test_escape_cancels_pending_delete(cx: &mut TestAppContext) {
    let window = setup_app(cx);

    // Create a task
    cx.simulate_keystrokes(window.into(), "cmd-shift-t");
    cx.run_until_parked();
    cx.simulate_input(window.into(), "TempTask");
    cx.simulate_keystrokes(window.into(), "tab tab");
    cx.simulate_input(window.into(), "echo test");
    cx.simulate_keystrokes(window.into(), "enter");
    cx.run_until_parked();

    let count_after_create = window.update(cx, |app, _w, cx| {
        app.task_count(cx)
    }).unwrap();

    // Focus task list
    refocus(&window, cx);
    cx.simulate_keystrokes(window.into(), "cmd-shift-l");
    cx.run_until_parked();
    refocus(&window, cx);
    cx.simulate_keystrokes(window.into(), "cmd-shift-l");
    cx.run_until_parked();

    // Navigate to last task
    let target = count_after_create - 1;
    for _ in 0..target {
        cx.simulate_keystrokes(window.into(), "down");
        cx.run_until_parked();
    }

    // Initiate delete
    cx.simulate_keystrokes(window.into(), "cmd-shift-backspace");
    cx.run_until_parked();

    window.update(cx, |app, _w, _cx| {
        assert!(app.has_task_pending_delete(), "Should have pending delete");
    }).unwrap();

    // Cancel with Escape
    cx.simulate_keystrokes(window.into(), "escape");
    cx.run_until_parked();

    window.update(cx, |app, _w, cx| {
        assert!(!app.has_task_pending_delete(), "Escape cancels delete");
        assert_eq!(app.task_count(cx), count_after_create, "Task still exists");
    }).unwrap();

    // Clean up: actually delete it
    cx.simulate_keystrokes(window.into(), "cmd-shift-backspace");
    cx.run_until_parked();
    cx.simulate_keystrokes(window.into(), "enter");
    cx.run_until_parked();
}
