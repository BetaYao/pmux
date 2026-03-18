# E2E Testing for Scheduled Tasks via Keyboard Shortcuts + Screenshot OCR

**Date:** 2026-03-18
**Status:** Approved

## Summary

Add keyboard shortcuts for all scheduled task operations and build an end-to-end test framework that drives pmux via AppleScript keystrokes, captures screenshots, and verifies UI state with macOS Vision OCR.

## Motivation

pmux uses GPUI (GPU-accelerated native UI) which lacks test harness utilities like `TestAppContext` or simulated clicks. To achieve true end-to-end UI testing, we use macOS-native tools: `osascript` for keyboard input and `screencapture` + Vision framework for visual verification.

## Part 1: Keyboard Shortcuts for Scheduled Tasks

### New ShortcutActions

| Shortcut | Action | Behavior |
|----------|--------|----------|
| `Cmd+Shift+L` | `ToggleTaskList` | Expand/collapse task list section in sidebar, set focus to task list area |
| `Cmd+Shift+T` | `NewTask` | Open TaskDialog modal for creating a new scheduled task |
| `Cmd+Shift+Backspace` | `DeleteTask` | Delete the currently selected task (with confirmation prompt) |

Note: `Cmd+T` was avoided because it conflicts with the standard macOS "New Tab" shortcut.

### ShortcutCategory

New actions belong to a new `Tasks` category in `ShortcutCategory` enum, keeping them organized separately from General/Navigation/Workspace/View.

### State Ownership (AppRoot, not Sidebar)

`Sidebar` is a transient `RenderOnce` component rebuilt each render cycle. Task selection and focus state must live on `AppRoot` and be passed to `Sidebar` via builder methods (consistent with the existing `with_tasks_expanded()` pattern):

Add to `AppRoot`:
- `selected_task_index: Option<usize>` — index of the currently highlighted task
- `task_list_focused: bool` — whether the task list area has keyboard focus

Passed to Sidebar via:
- `.with_selected_task_index(self.selected_task_index)`
- `.with_task_list_focused(self.task_list_focused)`

### Task Navigation

When `task_list_focused` is `true`:
- **Up/Down arrows** navigate between tasks, updating `selected_task_index`
- Selected task gets a highlighted background (`rgb(0x3a3a3a)`)
- `Cmd+Shift+Backspace` deletes the task at `selected_task_index` (with confirmation)
- `Escape` exits task list focus (`task_list_focused = false`)

### Key Dispatch Chain

Arrow key navigation must be intercepted in `AppRoot::handle_key_down()` **before** terminal forwarding. Insert a new check after the existing modal/search/diff checks:

```
1. Modal checks (settings dialog, new-branch dialog, task dialog)
2. Alt+Cmd+arrows (pane focus switch)
3. Search mode
4. Cmd+key → handle_shortcut()
5. ** NEW: task_list_focused → arrow keys navigate tasks **
6. Diff view scroll keys
7. Forward to terminal
```

This follows the same pattern as search mode and diff view mode.

### Deletion Confirmation

When `Cmd+Shift+Backspace` is pressed with a selected task:
1. Store the task ID pending deletion
2. Show a minimal confirmation in the sidebar (e.g., task row changes to red background with "Delete? Enter/Esc")
3. `Enter` confirms deletion, `Escape` cancels
4. This avoids a full modal dialog while preventing accidental deletion

### Integration Points

**keyboard_shortcuts.rs:**
- Add `Tasks` to `ShortcutCategory` enum
- Add `ToggleTaskList`, `NewTask`, `DeleteTask` to `ShortcutAction` enum
- Add default bindings in `KeyBinding::all_defaults()`

**app_root.rs:**
- Add `selected_task_index: Option<usize>`, `task_list_focused: bool`, `task_pending_delete: Option<Uuid>` fields
- In `handle_key_down()`: add task-list-focused arrow key dispatch before terminal forwarding

**app_root_render.rs `handle_shortcut()`:**
- `"l"` (with shift) → toggle task list + focus
- `"t"` (with shift) → open TaskDialog
- `"backspace"` (with shift) → initiate delete of selected task

Note: `handle_shortcut()` currently matches raw key strings rather than using `ShortcutRegistry::lookup()`. This is existing tech debt; new shortcuts follow the same pattern for consistency.

**sidebar.rs:**
- Add `selected_task_index: Option<usize>` and `task_list_focused: bool` as builder fields
- Add `on_delete_task: Option<Arc<dyn Fn(Uuid, &mut Window, &mut App) + Send + Sync>>` callback
- Add `task_pending_delete: Option<Uuid>` for confirmation UI
- Render selected task with highlight background
- Render pending-delete task with red background + "Delete? Enter/Esc" text

## Part 2: E2E Test Infrastructure

### Prerequisites

- macOS desktop environment (not headless)
- Terminal/test runner must have **Accessibility permissions** (System Settings → Privacy & Security → Accessibility). First-time users will see a permission dialog.
- Xcode command-line tools (for `swiftc`)

### Directory Structure

```
tests/e2e/
├── ocr_tool.swift          # macOS Vision OCR CLI tool
├── helpers.sh              # Shared functions: send_key, screenshot, ocr, assert_text
├── test_scheduled_tasks.sh # Scheduled task create + delete test
└── results/                # Screenshot output directory (gitignored)
```

### ocr_tool.swift

A minimal Swift CLI that uses the Vision framework for text recognition:
- **Input:** image file path as argument
- **Output:** recognized text lines to stdout, one per line
- **Languages:** `["en-US", "zh-Hans"]`
- **Recognition level:** `.accurate`
- **Exit codes:** 0 = success, 1 = file not found, 2 = recognition failed
- **Staleness check:** test script compares `.swift` mtime vs binary mtime, recompiles if source is newer

### helpers.sh

Core helper functions for test scripts:

```bash
send_key "cmd shift" "l"        # Send Cmd+Shift+L to pmux window via osascript
send_key "cmd shift" "t"        # Send Cmd+Shift+T
send_text "E2E_Test_Task"       # Type text with 50ms inter-character delay
take_screenshot "step_name"     # screencapture -l <window_id> to results/
ocr "image.png"                 # Run ocr_tool, return recognized text
assert_contains "step" "text"   # Screenshot + OCR + assert expected text present
assert_not_contains "step" "t"  # Screenshot + OCR + assert text absent
wait_for_window "pmux"          # Poll until pmux window appears (timeout 10s)
cleanup                         # Kill pmux process, report results
```

Key implementation details:
- `send_key` uses `osascript -e 'tell application "System Events" to keystroke ...'`
- `send_text` types character-by-character with 50ms delay between keystrokes to prevent drops under CPU load
- `take_screenshot` uses `screencapture -l <window_id>` to capture only the pmux window
- Window ID obtained via: `osascript -e 'tell app "System Events" to get id of first window of process "pmux"'` combined with `CGWindowListCopyWindowInfo` query. Fallback: use process name matching via `screencapture` with window title search.

### Retry Logic

Each `assert_contains` / `assert_not_contains` call:
1. Take screenshot
2. Run OCR
3. Check for expected text
4. If assertion fails, wait 1 second and retry (max 3 retries)
5. Fail after 3 retries with diagnostic output (OCR text dump + screenshot path)

## Part 3: Test Scenario

### test_scheduled_tasks.sh

```
Step 1:  Compile ocr_tool.swift (if binary missing or stale)
Step 2:  Build and launch pmux (cargo run &)
Step 3:  Wait for pmux window to appear (wait_for_window, timeout 30s for first build)
Step 4:  Cmd+Shift+L → expand task list + focus
         → assert_contains "Scheduled Tasks"
Step 5:  Cmd+Shift+T → open TaskDialog
         → assert_contains "New Scheduled Task"
Step 6:  Type "E2E_Test_Task" → Tab → Tab → Type "echo hello" → Enter
         (Tab x2 skips Cron field which defaults to "0 2 * * *")
         → assert_contains "E2E_Test_Task"
Step 7:  Arrow keys to select "E2E_Test_Task" → Cmd+Shift+Backspace → Enter to confirm
         → assert_not_contains "E2E_Test_Task"
Step 8:  Cleanup: kill pmux, report results
```

### Expected Output

```
[PASS] Task list visible after Cmd+Shift+L
[PASS] TaskDialog opened after Cmd+Shift+T
[PASS] Task "E2E_Test_Task" created successfully
[PASS] Task "E2E_Test_Task" deleted successfully
=== 4/4 tests passed ===
```

### Error Handling

- Each operation followed by 1 second sleep for UI render
- Screenshot retry: up to 3 attempts per assertion
- Test always kills pmux on exit (trap EXIT)
- All screenshots saved to `tests/e2e/results/` for post-mortem inspection
- Non-zero exit code on any assertion failure

## Scope Exclusions

- No CI integration (requires macOS desktop environment with accessibility permissions)
- No testing of task execution (only CRUD via UI)
- No testing of cron scheduling behavior
- No notification testing
