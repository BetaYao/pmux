# Terminal I/O + Tab Switch State Preservation — E2E Test Design

Verify that terminal output is preserved when switching between workspace tabs and back.

## Problem

Tab switching is a core workflow. If terminal content is lost after switching away and back, the product is broken. Currently there are no automated tests covering this critical path.

## Test Strategy: Two Layers

### Layer 1: GPUI State Check (fast, deterministic, CI-safe)

Validates the **state management logic** of tab switching — that `active_worktree_index` is saved to the departing tab and restored when returning. Runs in-process via `TestAppContext`, no real tmux/PTY.

**Important constraint:** `handle_workspace_tab_switch()` has filesystem side effects (`save_config`, `refresh_worktrees_for_repo`, `try_recover_then_switch`). Tests must NOT call it directly. Instead, we test the **save/restore logic on `WorkspaceManager`** and `active_worktree_index` through a test-only method that bypasses I/O.

### Layer 2: macOS E2E Screenshot + OCR (slow, realistic, local-only)

Validates the **actual user experience** — type a command, see output, switch tabs, switch back, confirm output is still visible. Uses `screencapture` + Swift Vision OCR against a real running pmux instance.

## Layer 1: GPUI State Check

### Approach: Test-Only Tab Switch Method

Create `handle_workspace_tab_switch_for_test()` that only exercises the save/restore logic:
1. Save `active_worktree_index` to departing tab's `last_worktree_index`
2. Call `workspace_manager.switch_to_tab(idx)`
3. Read incoming tab's `last_worktree_index` and restore to `active_worktree_index`

This mirrors the state transitions in `handle_workspace_tab_switch` (lines 1425-1428, 1436, 1443-1450) without calling `save_config`, `stop_current_session`, `refresh_worktrees_for_repo`, or `try_recover_then_switch`.

### Test: `test_tab_switch_preserves_worktree_index`

```
1. Create AppRoot with 2 workspace tabs (via add_workspace_for_test)
2. Inject fake worktrees for tab 0 (set_cached_worktrees_for_test)
3. Set active_worktree_index = Some(1)
4. Call handle_workspace_tab_switch_for_test(1)
5. Assert: tab 0's last_worktree_index == Some(1) (saved)
6. Inject fake worktrees for tab 1
7. Call handle_workspace_tab_switch_for_test(0)
8. Assert: active_worktree_index == Some(1) (restored)
```

### Test: `test_tab_switch_round_trip_three_tabs`

```
1. Create AppRoot with 3 workspace tabs, inject worktrees for each
2. On tab 0: set active_worktree_index = Some(2)
3. Switch to tab 1: set active_worktree_index = Some(0)
4. Switch to tab 2
5. Switch back to tab 0
6. Assert: active_worktree_index == Some(2)
7. Switch to tab 1
8. Assert: active_worktree_index == Some(0)
```

### Test: `test_tab_switch_to_same_tab_is_noop`

```
1. Create AppRoot with 2 tabs
2. Set active_worktree_index = Some(1)
3. Call handle_workspace_tab_switch_for_test(0) (same tab)
4. Assert: active_worktree_index unchanged (still Some(1))
```

### Test: `test_tab_switch_with_no_worktrees`

```
1. Create AppRoot with 2 tabs
2. Do NOT inject worktrees (cached_worktrees is empty)
3. active_worktree_index = None
4. Switch to tab 1, switch back
5. Assert: active_worktree_index == None (no crash, graceful)
```

### New Test Accessors / Helpers on AppRoot

```rust
// Read current active worktree index
pub fn active_worktree_index_for_test(&self) -> Option<usize> {
    self.active_worktree_index
}

// Read workspace tab count
pub fn workspace_tab_count(&self) -> usize {
    self.workspace_manager.tab_count()
}

// Read active tab index
pub fn active_tab_index_for_test(&self) -> Option<usize> {
    self.workspace_manager.active_tab_index()
}

// Add a workspace tab for testing (bypasses file dialog)
pub fn add_workspace_for_test(&mut self, path: PathBuf) -> usize {
    self.workspace_manager.add_workspace(path)
}

// Set active worktree index for testing
pub fn set_active_worktree_index_for_test(&mut self, index: Option<usize>) {
    self.active_worktree_index = index;
}

// Inject fake worktrees for testing (bypasses git discovery)
pub fn set_cached_worktrees_for_test(&mut self, worktrees: Vec<WorktreeInfo>) {
    self.cached_worktrees = worktrees;
}

// Tab switch that only exercises save/restore logic, no I/O
pub fn handle_workspace_tab_switch_for_test(&mut self, idx: usize) {
    if idx >= self.workspace_manager.tab_count() {
        return;
    }
    // Same-tab: noop
    if self.workspace_manager.active_tab_index() == Some(idx) {
        return;
    }
    // Save worktree index to departing tab
    if let Some(current_tab) = self.workspace_manager.active_tab_mut() {
        current_tab.save_worktree_index(self.active_worktree_index);
    }
    // Switch
    self.workspace_manager.switch_to_tab(idx);
    // Restore worktree index from incoming tab
    if let Some(tab) = self.workspace_manager.active_tab() {
        self.active_worktree_index = tab.last_worktree_index();
    }
}
```

### File Changes

| File | Change |
|------|--------|
| `src/ui/app_root.rs` | Add 7 test accessor/helper methods |
| `tests/gpui_keystroke_poc.rs` | Add 4 new tab-switch state tests |

## Layer 2: macOS E2E Screenshot + OCR

### Primary Approach: Pre-configured Multi-Tab

Requires pmux config to already have 2+ workspace tabs. This is the recommended approach because file dialog automation via AppleScript is unreliable with GPUI's native dialogs.

### Test: `test_terminal_output_preserved_after_tab_switch`

```bash
#!/bin/bash
# test_terminal_tab_switch.sh
source helpers.sh
trap cleanup EXIT

ensure_ocr_tool
start_pmux
wait_for_window 30
sleep 3  # let workspace fully load

# Step 1: Type a unique marker command
send_text "echo PMUX_E2E_MARKER_1234"
send_special_key 36  # Return
sleep 1

# Step 2: Verify output appears
assert_contains "marker_visible" "PMUX_E2E_MARKER_1234"

# Step 3: Switch to tab 2 (⌘2)
send_key "cmd" "2"
sleep 2  # wait for tab 2 to load

# Step 4: Verify marker is NOT visible on tab 2
assert_not_contains "tab2_no_marker" "PMUX_E2E_MARKER_1234"

# Step 5: Switch back to tab 1 (⌘1)
send_key "cmd" "1"
sleep 2  # wait for tmux session recovery

# Step 6: Verify marker is still visible
assert_contains "marker_preserved" "PMUX_E2E_MARKER_1234"

report
```

### Fallback: Single-Tab Config

If only 1 tab exists, the test should skip with a clear message:

```bash
# At start of test, after wait_for_window:
TAB_COUNT=$(detect_tab_count)  # OCR the topbar for tab indicators
if [ "$TAB_COUNT" -lt 2 ]; then
    echo "SKIP: Need 2+ workspace tabs in pmux config for this test"
    exit 0
fi
```

### New E2E Helper

Add `detect_tab_count` to `helpers.sh` — takes a screenshot of the topbar area and counts tab-like elements via OCR, or reads from `~/.config/pmux/config.json`.

### Prerequisites

- macOS desktop with Accessibility permissions
- pmux config with 2+ workspace tabs (each pointing to a valid git repo)
- `swiftc` available (Xcode CLI tools)

### File Changes

| File | Change |
|------|--------|
| `tests/e2e/test_terminal_tab_switch.sh` | New E2E test script |
| `tests/e2e/helpers.sh` | Add `detect_tab_count` helper |

## Known Limitations

- File dialog automation (⌘N + typing path) is unreliable with GPUI native dialogs — avoided in primary E2E approach
- E2E tests require macOS desktop, cannot run in headless CI
- OCR may miss text if terminal font is too small or output scrolled off-screen

## What We Are NOT Doing

- Not testing terminal resize behavior during tab switch
- Not testing worktree switching within a single tab (separate test)
- Not testing split pane state preservation
- Not pixel-diffing screenshots (OCR text match only)
- Not automating file dialog interactions

## Success Criteria

- GPUI tests: `cargo test --test gpui_keystroke_poc` passes with 4 new tab-switch tests
- E2E test: `test_terminal_tab_switch.sh` reports all assertions PASS
- Terminal output (`echo PMUX_E2E_MARKER_1234`) survives a tab round-trip
