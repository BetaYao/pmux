# Keyboard Shortcuts Completion Design

Complete keyboard shortcuts so pmux can be fully controlled without a mouse.

## Problem

Several common operations still require mouse interaction:
- No way to cycle through workspace tabs sequentially
- No keyboard shortcut to open settings
- New workspace / new branch shortcuts registered but not implemented
- Worktree switching requires clicking sidebar items

## Approach

Extend the existing `handle_shortcut()` in `app_root_render.rs` (Approach A — minimal change, consistent with current architecture). Register new actions in `ShortcutRegistry` so the help panel stays accurate.

## New Shortcuts

| Shortcut | Action (enum variant) | Description |
|----------|----------------------|-------------|
| `⌘,` | OpenSettings | Toggle settings panel (macOS convention) |
| `⌘N` | NewWorkspace | Open file dialog to add a workspace tab |
| `⌘⇧N` | NewBranch | Open new branch dialog |
| `⌘]` | NextTab | Switch to next workspace tab (wraps) |
| `⌘[` | PrevTab | Switch to previous workspace tab (wraps) |
| `⌘↓` | NextWorktree | Select next worktree in current workspace |
| `⌘↑` | PrevWorktree | Select previous worktree in current workspace |

## Implementation Details

### 1. `src/keyboard_shortcuts.rs`

Add 5 new variants to `ShortcutAction` enum:
```rust
NextTab,
PrevTab,
OpenSettings,
NextWorktree,
PrevWorktree,
```

Note: `NewWorkspace` and `NewBranch` already exist in the enum — they just need implementation.

Add 5 new entries in `all_defaults()`:
- `NextTab` — `⌘]`, Navigation
- `PrevTab` — `⌘[`, Navigation
- `OpenSettings` — `⌘,`, General
- `NextWorktree` — `⌘↓`, Navigation
- `PrevWorktree` — `⌘↑`, Navigation

Update `test_default_bindings_count` assertion from 30 to 35.

### 2. `src/ui/app_root.rs` — Fix task list key handler ordering

**Bug fix required:** The task list `up`/`down` handler in `handle_key_down()` (lines 2427-2451) intercepts arrow keys *before* the `Cmd+key` check (line 2490), which means `⌘↑`/`⌘↓` would be swallowed when `task_list_focused` is true.

Fix: Add platform modifier guard to the task list handler:
```rust
// In handle_key_down, task_list_focused block:
"up" => {
    if !event.keystroke.modifiers.platform {  // <-- add this guard
        // existing task navigation code...
    }
    return;
}
"down" => {
    if !event.keystroke.modifiers.platform {  // <-- add this guard
        // existing task navigation code...
    }
    return;
}
```

This ensures `⌘↑`/`⌘↓` falls through to `handle_shortcut()` even when the task list is focused.

### 3. `src/ui/app_root_render.rs` — `handle_shortcut()`

Add new match arms:

**`⌘,` — Settings toggle:**
```rust
"," => {
    self.show_settings = !self.show_settings;
    if self.show_settings {
        // Sync DialogManager on open (same pattern as sidebar settings button)
        if let Some(ref dm) = self.dialog_mgr {
            let config = Config::load().unwrap_or_default();
            let secrets = Secrets::load().unwrap_or_default();
            dm.update(cx, |dm, cx| dm.open_settings(config, secrets, cx));
        }
    } else {
        // Sync DialogManager on close (same pattern as Escape handler)
        self.settings_draft = None;
        self.settings_secrets_draft = None;
        self.settings_configuring_channel = None;
        self.settings_editing_agent = None;
        self.settings_focused_field = None;
        if let Some(ref dm) = self.dialog_mgr {
            dm.update(cx, |dm, cx| dm.close_settings(cx));
        }
    }
    cx.notify();
}
```

**`⌘N` / `⌘⇧N` — New workspace / branch:**
```rust
"n" => {
    if event.keystroke.modifiers.shift {
        self.open_new_branch_dialog(cx);
    } else {
        // Reuse existing method (same as sidebar "+" button)
        self.handle_add_workspace(cx);
    }
}
```

Note: GPUI delivers `⌘⇧N` as `key: "n"` with `modifiers.shift: true` (lowercase), consistent with how the existing `"d"` handler detects `⌘⇧D`.

**`⌘]` / `⌘[` — Tab cycling:**
```rust
"]" => {
    let count = self.workspace_manager.tab_count();
    if count > 1 {
        let current = self.workspace_manager.active_tab_index();
        let next = (current + 1) % count;
        self.handle_workspace_tab_switch(next, cx);
        let counts = self.compute_per_tab_active_counts();
        if let Some(ref e) = self.topbar_entity {
            let topbar = e.clone();
            let wm = self.workspace_manager.clone();
            let _ = cx.update_entity(&topbar, |t: &mut TopBarEntity, cx| {
                t.set_workspace_manager(wm);
                t.set_per_tab_active_counts(counts);
                cx.notify();
            });
        }
    }
}
"[" => {
    let count = self.workspace_manager.tab_count();
    if count > 1 {
        let current = self.workspace_manager.active_tab_index();
        let prev = (current + count - 1) % count;
        self.handle_workspace_tab_switch(prev, cx);
        let counts = self.compute_per_tab_active_counts();
        if let Some(ref e) = self.topbar_entity {
            let topbar = e.clone();
            let wm = self.workspace_manager.clone();
            let _ = cx.update_entity(&topbar, |t: &mut TopBarEntity, cx| {
                t.set_workspace_manager(wm);
                t.set_per_tab_active_counts(counts);
                cx.notify();
            });
        }
    }
}
```

**`⌘↑` / `⌘↓` — Worktree navigation:**

`⌥⌘+arrows` (pane focus) is already intercepted in `handle_key_down()` before `handle_shortcut()` is called, so `⌘↑/⌘↓` without Alt reaches here without conflict.

```rust
"down" => {
    let wt_count = self.cached_worktrees.len();
    if wt_count > 1 {
        let current = self.active_worktree_index.unwrap_or(0);
        let next = (current + 1) % wt_count;
        self.pending_worktree_selection = Some(next);
        cx.notify();
    }
}
"up" => {
    let wt_count = self.cached_worktrees.len();
    if wt_count > 1 {
        let current = self.active_worktree_index.unwrap_or(0);
        let prev = (current + wt_count - 1) % wt_count;
        self.pending_worktree_selection = Some(prev);
        cx.notify();
    }
}
```

Note: The `!event.keystroke.modifiers.alt` guard is not needed here because Alt+Cmd+arrows are intercepted earlier in `handle_key_down()` and never reach `handle_shortcut()`.

### 4. No new methods needed in `app_root.rs`

`handle_add_workspace(cx)` already exists at `app_root.rs:1334` and is reusable directly. No extraction needed.

## Files Changed

| File | Change |
|------|--------|
| `src/keyboard_shortcuts.rs` | 5 new ShortcutAction variants + 5 default bindings + test count update |
| `src/ui/app_root_render.rs` | 7 new match arms in `handle_shortcut()` |
| `src/ui/app_root.rs` | Add `!platform` guard to task list up/down handler |

## Test Plan

### Registry tests (`keyboard_shortcuts.rs`):
- `test_default_bindings_count` — update assertion to 35
- `test_new_shortcut_lookups` — verify `⌘]`, `⌘[`, `⌘,`, `⌘↑`, `⌘↓` resolve to correct actions

### Behavioral tests (manual, requires GPUI window):
- `⌘]`/`⌘[` cycles tabs and wraps at boundaries
- `⌘↑`/`⌘↓` switches worktrees within current workspace
- `⌘,` opens settings; pressing again closes settings
- `⌘N` opens file dialog (or does nothing if no workspace)
- `⌘⇧N` opens new branch dialog
- `⌘↑` still works when task list is focused (bug fix verification)

## What We Are NOT Doing

- No refactoring of the shortcut dispatch architecture
- No custom keybinding configuration UI
- No changes to existing shortcut behavior
- No changes to terminal key forwarding logic

## Success Criteria

- All 7 new shortcuts work as described
- Help panel (`⌘?`) shows all new shortcuts with correct descriptions
- No conflicts with existing shortcuts or terminal input
- Existing tests pass; new registry tests added
- `⌘↑`/`⌘↓` works even when task list is focused
