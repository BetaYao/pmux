//! Render layer tests: verify that UI elements are actually rendered
//! when state changes via keyboard shortcuts.
//!
//! Uses GPUI's debug_selector + debug_bounds to check element presence in rendered frames.
//! Run with: RUSTUP_TOOLCHAIN=stable cargo test --test gpui_render_tests

use gpui::{TestAppContext, VisualTestContext};
use pmux::ui::app_root::AppRoot;

/// Helper: create AppRoot window and return VisualTestContext for render assertions.
fn setup_visual(cx: &mut TestAppContext) -> (gpui::WindowHandle<AppRoot>, VisualTestContext) {
    let window = cx.add_window(|_window, cx| {
        let mut app = AppRoot::new();
        app.ensure_entities_for_test(cx);
        app
    });
    cx.run_until_parked();
    window.update(cx, |app_root, win, cx| {
        app_root.focus_for_test(win, cx);
    }).unwrap();
    cx.run_until_parked();

    let vcx = VisualTestContext::from_window(window.into(), cx);
    (window, vcx)
}

/// Helper: update AppRoot state through the window handle.
fn with_app<R>(
    window: &gpui::WindowHandle<AppRoot>,
    vcx: &mut VisualTestContext,
    f: impl FnOnce(&mut AppRoot, &mut gpui::Window, &mut gpui::Context<AppRoot>) -> R,
) -> R {
    window.update(&mut vcx.cx, |app, win, cx| f(app, win, cx)).unwrap()
}

/// Helper: refocus the terminal so on_key_down works after state changes.
fn refocus(window: &gpui::WindowHandle<AppRoot>, vcx: &mut VisualTestContext) {
    with_app(window, vcx, |app, win, cx| {
        app.focus_for_test(win, cx);
    });
    vcx.run_until_parked();
}

/// Helper: create a task via keyboard and return to focused state.
fn create_task(window: &gpui::WindowHandle<AppRoot>, vcx: &mut VisualTestContext, name: &str, cmd: &str) {
    vcx.simulate_keystrokes("cmd-shift-t");
    vcx.run_until_parked();
    vcx.simulate_input(name);
    vcx.simulate_keystrokes("tab tab");
    vcx.simulate_input(cmd);
    vcx.simulate_keystrokes("enter");
    vcx.run_until_parked();
    // Re-focus after dialog closes (render cycle changes focus)
    refocus(window, vcx);
}

// ─── Tests ───────────────────────────────────────────────────────────

#[gpui::test]
fn test_tasks_section_header_rendered(cx: &mut TestAppContext) {
    let (_window, mut vcx) = setup_visual(cx);

    let bounds = vcx.debug_bounds("tasks-section-header");
    assert!(bounds.is_some(), "Tasks section header should be rendered");
}

#[gpui::test]
fn test_task_dialog_rendered_on_cmd_shift_t(cx: &mut TestAppContext) {
    let (_window, mut vcx) = setup_visual(cx);

    assert!(vcx.debug_bounds("task-dialog").is_none(), "No dialog initially");

    vcx.simulate_keystrokes("cmd-shift-t");
    vcx.run_until_parked();

    assert!(vcx.debug_bounds("task-dialog").is_some(),
        "Task dialog should be rendered after Cmd+Shift+T");
}

#[gpui::test]
fn test_task_item_rendered_after_creation(cx: &mut TestAppContext) {
    let (window, mut vcx) = setup_visual(cx);

    let idx = with_app(&window, &mut vcx, |app, _w, cx| app.task_count(cx));

    create_task(&window, &mut vcx, "RenderTestTask", "echo render");

    // Verify the new task item element exists in rendered frame
    let selector = format!("task-item-{}", idx);
    assert!(vcx.debug_bounds(Box::leak(selector.into_boxed_str())).is_some(),
        "Task item at index {} should be rendered", idx);

    // Clean up
    with_app(&window, &mut vcx, |app, _w, cx| {
        app.set_task_list_focused_for_test(true, idx, cx);
    });
    refocus(&window, &mut vcx);
    vcx.simulate_keystrokes("cmd-shift-backspace");
    vcx.run_until_parked();
    vcx.simulate_keystrokes("enter");
    vcx.run_until_parked();
}

#[gpui::test]
fn test_selected_task_highlight_rendered(cx: &mut TestAppContext) {
    let (window, mut vcx) = setup_visual(cx);

    let idx = with_app(&window, &mut vcx, |app, _w, cx| app.task_count(cx));
    create_task(&window, &mut vcx, "SelectRenderTask", "echo sel");

    // Before focusing: no selection highlight
    let sel = format!("task-selected-{}", idx);
    assert!(vcx.debug_bounds(Box::leak(sel.clone().into_boxed_str())).is_none(),
        "No selection highlight before focus");

    // Focus and select
    with_app(&window, &mut vcx, |app, _w, cx| {
        app.set_task_list_focused_for_test(true, idx, cx);
    });
    vcx.run_until_parked();

    // Selection marker should now be rendered
    assert!(vcx.debug_bounds(Box::leak(sel.into_boxed_str())).is_some(),
        "Selection highlight should render for selected task");

    // Clean up
    refocus(&window, &mut vcx);
    vcx.simulate_keystrokes("cmd-shift-backspace");
    vcx.run_until_parked();
    vcx.simulate_keystrokes("enter");
    vcx.run_until_parked();
}

#[gpui::test]
fn test_pending_delete_confirmation_rendered(cx: &mut TestAppContext) {
    let (window, mut vcx) = setup_visual(cx);

    let idx = with_app(&window, &mut vcx, |app, _w, cx| app.task_count(cx));
    create_task(&window, &mut vcx, "DeleteRenderTask", "echo del");

    // No confirmation initially
    let del = format!("task-pending-delete-{}", idx);
    assert!(vcx.debug_bounds(Box::leak(del.clone().into_boxed_str())).is_none(),
        "No delete confirmation initially");

    // Select and initiate delete
    with_app(&window, &mut vcx, |app, win, cx| {
        app.set_task_list_focused_for_test(true, idx, cx);
        app.focus_for_test(win, cx);
    });
    vcx.run_until_parked();
    vcx.simulate_keystrokes("cmd-shift-backspace");
    vcx.run_until_parked();

    // "Delete? Enter/Esc" should be rendered
    assert!(vcx.debug_bounds(Box::leak(del.into_boxed_str())).is_some(),
        "Delete confirmation should be rendered");

    // Confirm to clean up
    vcx.simulate_keystrokes("enter");
    vcx.run_until_parked();
}
