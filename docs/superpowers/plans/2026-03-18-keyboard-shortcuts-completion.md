# Keyboard Shortcuts Completion — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add 7 missing keyboard shortcuts so pmux can be fully controlled without a mouse.

**Architecture:** Extend the existing `handle_shortcut()` match statement in `app_root_render.rs` with new arms, register new actions in `ShortcutRegistry`, and fix a task-list key handling bug in `app_root.rs`.

**Tech Stack:** Rust, GPUI (KeyDownEvent, modifiers), existing ShortcutRegistry

**Spec:** `docs/superpowers/specs/2026-03-18-keyboard-shortcuts-completion-design.md`

---

## File Map

| File | Role | Change |
|------|------|--------|
| `src/keyboard_shortcuts.rs` | Shortcut definitions & registry | Add 5 enum variants + 5 default bindings |
| `src/ui/app_root_render.rs` | Shortcut handler | Add 7 match arms in `handle_shortcut()` |
| `src/ui/app_root.rs` | Key event dispatcher | Fix task list `up`/`down` to not swallow `⌘↑`/`⌘↓` |

---

### Task 1: Register new ShortcutAction variants and default bindings

**Files:**
- Modify: `src/keyboard_shortcuts.rs:16-57` (enum) and `src/keyboard_shortcuts.rs:88-306` (defaults)

- [ ] **Step 1: Add 5 new enum variants**

In `src/keyboard_shortcuts.rs`, add after `JumpToUnread` (line 35) in the Navigation section:

```rust
    NextTab,
    PrevTab,
    NextWorktree,
    PrevWorktree,
```

And add after `ShowHelp` (line 24) in the General section:

```rust
    OpenSettings,
```

- [ ] **Step 2: Add 5 new default bindings**

In `KeyBinding::all_defaults()`, add after the `JumpToUnread` entry (before line 197 `// Workspace`):

```rust
            Self::new(
                ShortcutAction::NextTab,
                "Next Tab",
                "⌘]",
                "Switch to the next workspace tab",
                ShortcutCategory::Navigation,
            ),
            Self::new(
                ShortcutAction::PrevTab,
                "Previous Tab",
                "⌘[",
                "Switch to the previous workspace tab",
                ShortcutCategory::Navigation,
            ),
            Self::new(
                ShortcutAction::NextWorktree,
                "Next Worktree",
                "⌘↓",
                "Switch to the next worktree in current workspace",
                ShortcutCategory::Navigation,
            ),
            Self::new(
                ShortcutAction::PrevWorktree,
                "Previous Worktree",
                "⌘↑",
                "Switch to the previous worktree in current workspace",
                ShortcutCategory::Navigation,
            ),
```

Add after the `ShowHelp` entry (before the Navigation section comment):

```rust
            Self::new(
                ShortcutAction::OpenSettings,
                "Open Settings",
                "⌘,",
                "Open or close the settings panel",
                ShortcutCategory::General,
            ),
```

- [ ] **Step 3: Update test assertion**

In `test_default_bindings_count` (line 424-428), change:
```rust
        // General(6) + Navigation(9) + Workspace(7) + View(5) + Tasks(3) = 30
        assert_eq!(bindings.len(), 30, "...");
```
to:
```rust
        // General(7) + Navigation(13) + Workspace(7) + View(5) + Tasks(3) = 35
        assert_eq!(bindings.len(), 35, "Expected 35 default bindings");
```

- [ ] **Step 4: Add new lookup test**

Add after `test_shortcut_lookup` (line 447):

```rust
    #[test]
    fn test_new_shortcut_lookups() {
        let registry = ShortcutRegistry::new();
        assert_eq!(registry.lookup("⌘]"), Some(ShortcutAction::NextTab));
        assert_eq!(registry.lookup("⌘["), Some(ShortcutAction::PrevTab));
        assert_eq!(registry.lookup("⌘,"), Some(ShortcutAction::OpenSettings));
        assert_eq!(registry.lookup("⌘↓"), Some(ShortcutAction::NextWorktree));
        assert_eq!(registry.lookup("⌘↑"), Some(ShortcutAction::PrevWorktree));
    }
```

- [ ] **Step 5: Run tests**

Run: `RUSTUP_TOOLCHAIN=stable cargo test keyboard_shortcuts`
Expected: All tests pass including updated count (35) and new lookups.

- [ ] **Step 6: Commit**

```bash
git add src/keyboard_shortcuts.rs
git commit -m "feat: register 5 new shortcut actions in ShortcutRegistry"
```

---

### Task 2: Fix task list key handler to not swallow ⌘↑/⌘↓

**Files:**
- Modify: `src/ui/app_root.rs:2427-2451`

- [ ] **Step 1: Add platform modifier guard**

In `handle_key_down()`, the `task_list_focused` block at line 2427. Change the `"up"` arm (lines 2429-2437):

```rust
                "up" => {
                    if !event.keystroke.modifiers.platform {
                        if let Some(idx) = self.selected_task_index {
                            if idx > 0 {
                                self.selected_task_index = Some(idx - 1);
                                self.task_pending_delete = None;
                                cx.notify();
                            }
                        }
                        return;
                    }
                }
```

Change the `"down"` arm (lines 2439-2451):

```rust
                "down" => {
                    if !event.keystroke.modifiers.platform {
                        let task_count = self.scheduler_manager.as_ref()
                            .map(|m| m.read(cx).tasks().len())
                            .unwrap_or(0);
                        if let Some(idx) = self.selected_task_index {
                            if idx + 1 < task_count {
                                self.selected_task_index = Some(idx + 1);
                                self.task_pending_delete = None;
                                cx.notify();
                            }
                        }
                        return;
                    }
                }
```

Key change: `return` is inside the `if !platform` block, so `⌘↑`/`⌘↓` falls through to `handle_shortcut()`.

- [ ] **Step 2: Run full test suite**

Run: `RUSTUP_TOOLCHAIN=stable cargo test`
Expected: All tests pass (no behavioral change for non-Cmd arrow keys).

- [ ] **Step 3: Commit**

```bash
git add src/ui/app_root.rs
git commit -m "fix: allow ⌘↑/⌘↓ to pass through task list key handler"
```

---

### Task 3: Implement ⌘N / ⌘⇧N (New Workspace / New Branch)

**Files:**
- Modify: `src/ui/app_root_render.rs:90-197` (`handle_shortcut()`)

- [ ] **Step 1: Add match arm**

In `handle_shortcut()`, add before the `_ => {}` line (line 196):

```rust
            "n" => {
                if event.keystroke.modifiers.shift {
                    self.open_new_branch_dialog(cx);
                } else {
                    self.handle_add_workspace(cx);
                }
            }
```

- [ ] **Step 2: Run cargo check**

Run: `RUSTUP_TOOLCHAIN=stable cargo check`
Expected: Compiles without errors. Both `open_new_branch_dialog` and `handle_add_workspace` are existing `pub(crate)` methods on `AppRoot`.

- [ ] **Step 3: Commit**

```bash
git add src/ui/app_root_render.rs
git commit -m "feat: implement ⌘N (new workspace) and ⌘⇧N (new branch) shortcuts"
```

---

### Task 4: Implement ⌘, (Open Settings)

**Files:**
- Modify: `src/ui/app_root_render.rs:90-197` (`handle_shortcut()`)

- [ ] **Step 1: Add match arm**

In `handle_shortcut()`, add before the `_ => {}` line:

```rust
            "," => {
                self.show_settings = !self.show_settings;
                if self.show_settings {
                    if let Some(ref dm) = self.dialog_mgr {
                        let config = Config::load().unwrap_or_default();
                        let secrets = Secrets::load().unwrap_or_default();
                        dm.update(cx, |dm, cx| dm.open_settings(config, secrets, cx));
                    }
                } else {
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

Note: `Config` and `Secrets` are already imported in `app_root_render.rs` (lines 6, 8).

- [ ] **Step 2: Run cargo check**

Run: `RUSTUP_TOOLCHAIN=stable cargo check`
Expected: Compiles without errors.

- [ ] **Step 3: Commit**

```bash
git add src/ui/app_root_render.rs
git commit -m "feat: implement ⌘, (toggle settings) shortcut"
```

---

### Task 5: Implement ⌘] / ⌘[ (Next/Previous Tab)

**Files:**
- Modify: `src/ui/app_root_render.rs:90-197` (`handle_shortcut()`)

- [ ] **Step 1: Add match arms**

In `handle_shortcut()`, add before the `_ => {}` line:

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

Note: `TopBarEntity` is already imported (line 18). The topbar sync block is identical to the `⌘1-8` handler pattern at lines 141-158.

- [ ] **Step 2: Run cargo check**

Run: `RUSTUP_TOOLCHAIN=stable cargo check`
Expected: Compiles without errors.

- [ ] **Step 3: Commit**

```bash
git add src/ui/app_root_render.rs
git commit -m "feat: implement ⌘]/⌘[ (next/previous tab) shortcuts"
```

---

### Task 6: Implement ⌘↑ / ⌘↓ (Next/Previous Worktree)

**Files:**
- Modify: `src/ui/app_root_render.rs:90-197` (`handle_shortcut()`)

- [ ] **Step 1: Add match arms**

In `handle_shortcut()`, add before the `_ => {}` line:

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

Note: `⌥⌘+arrows` (pane focus) is intercepted in `handle_key_down()` at line 2379 *before* `handle_shortcut()` is called. Only `⌘+arrows` (without Alt) reach this code path.

- [ ] **Step 2: Run full test suite**

Run: `RUSTUP_TOOLCHAIN=stable cargo test`
Expected: All tests pass.

- [ ] **Step 3: Commit**

```bash
git add src/ui/app_root_render.rs
git commit -m "feat: implement ⌘↑/⌘↓ (next/previous worktree) shortcuts"
```

---

### Task 7: Final verification

- [ ] **Step 1: Run full test suite**

Run: `RUSTUP_TOOLCHAIN=stable cargo test`
Expected: All tests pass.

- [ ] **Step 2: Run cargo build**

Run: `RUSTUP_TOOLCHAIN=stable cargo build`
Expected: Compiles without warnings related to our changes.

- [ ] **Step 3: Commit all remaining changes (if any)**

```bash
git status
# If any unstaged changes remain, commit them
```
