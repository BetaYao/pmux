# Main Interface Responsiveness Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Eliminate main-thread blocking in click handlers and throttle terminal output updates so hover and click feel instant (Zed-level responsiveness).

**Architecture:** Phase 1 moves all sync I/O (git, tmux) out of click callbacks into `cx.spawn` + `blocking::unblock`. Phase 2 adds a 16ms throttle to terminal output notify loops so the main thread is not flooded.

**Tech Stack:** Rust, GPUI, `cx.spawn`, `blocking::unblock`, `std::time::Instant`

**Verification:** `cargo run --release` — manually test: click worktree/delete/orphan, hover buttons; confirm no lag.

---

## Phase 1: Click Path Zero Blocking

### Task 1.1: Remove sync refresh from process_pending_worktree_selection

**Files:**
- Modify: `src/ui/app_root.rs:1624-1645`

**Step 1: Remove the blocking call**

In `process_pending_worktree_selection`, delete the line that calls `refresh_worktrees_for_repo` and use `cached_worktrees` directly. The cache is already refreshed on tab switch, branch create/delete, and workspace init.

**Before (lines 1629-1640):**
```rust
        let (repo_path, path, branch) = {
            let tab = match self.workspace_manager.active_tab() {
                Some(t) => t,
                None => return,
            };
            let repo_path = tab.path.clone();
            self.refresh_worktrees_for_repo(&repo_path);
            let worktree = match self.cached_worktrees.get(idx) {
```

**After:**
```rust
        let (repo_path, path, branch) = {
            let tab = match self.workspace_manager.active_tab() {
                Some(t) => t,
                None => return,
            };
            let repo_path = tab.path.clone();
            // Use cached worktrees; no sync git in click path
            let worktree = match self.cached_worktrees.get(idx) {
```

**Step 2: Verify**

```bash
cargo run --release
```

Click a worktree — selection should update immediately with no freeze.

**Step 3: Commit**

```bash
git add src/ui/app_root.rs
git commit -m "perf: remove sync refresh_worktrees from worktree selection click"
```

---

### Task 1.2: Defer on_delete to async (refresh + show dialog)

**Files:**
- Modify: `src/ui/app_root.rs:3160-3168`

**Step 1: Replace sync on_delete with spawn**

The current `on_delete` callback calls `refresh_worktrees_for_repo` (sync git) and `show_delete_dialog` (which calls `has_uncommitted_changes` — sync git). Move both into `cx.spawn` + `blocking::unblock`.

**Before:**
```rust
        sidebar.on_delete(move |idx, _window, cx| {
            let _ = cx.update_entity(&app_root_entity_for_delete, |this: &mut AppRoot, cx| {
                this.sidebar_context_menu_index = None;
                this.refresh_worktrees_for_repo(&repo_path_for_delete);
                if let Some(wt) = this.cached_worktrees.get(idx) {
                    this.show_delete_dialog(wt.clone(), cx);
                }
            });
        });
```

**After:**
```rust
        sidebar.on_delete(move |idx, _window, cx| {
            let _ = cx.update_entity(&app_root_entity_for_delete, |this: &mut AppRoot, cx| {
                this.sidebar_context_menu_index = None;
                cx.notify();
            });
            let repo_path = repo_path_for_delete.clone();
            let entity = app_root_entity_for_delete.clone();
            cx.spawn(async move |_entity, cx| {
                let result = blocking::unblock(move || {
                    let worktrees = crate::worktree::discover_worktrees(&repo_path).ok()?;
                    let worktree = worktrees.get(idx).cloned()?;
                    let has_uncommitted = crate::worktree::has_uncommitted_changes(&worktree.path);
                    Some((worktrees, worktree, has_uncommitted))
                }).await;
                if let Some((worktrees, worktree, has_uncommitted)) = result {
                    let _ = cx.update_entity(&entity, |this: &mut AppRoot, cx| {
                        this.cached_worktrees = worktrees;
                        this.cached_worktrees_repo = Some(repo_path);
                        this.delete_worktree_dialog.open(worktree, has_uncommitted);
                        cx.notify();
                    });
                }
            }).detach();
        });
```

**Step 2: Verify**

```bash
cargo run --release
```

Right-click worktree → Delete → dialog should appear without freezing.

**Step 3: Commit**

```bash
git add src/ui/app_root.rs
git commit -m "perf: defer delete dialog (refresh + has_uncommitted) to async"
```

---

### Task 1.3: Defer on_close_orphan to async (kill_tmux_window)

**Files:**
- Modify: `src/ui/app_root.rs:3169-3175`

**Step 1: Replace sync on_close_orphan with spawn**

**Before:**
```rust
        sidebar.on_close_orphan(move |window_name, _window, cx: &mut App| {
            let _ = cx.update_entity(&app_root_entity_for_close_orphan, |this: &mut AppRoot, cx| {
                let _ = kill_tmux_window(&repo_path_for_close_orphan, window_name);
                this.refresh_sidebar(cx);
                cx.notify();
            });
        });
```

**After:**
```rust
        sidebar.on_close_orphan(move |window_name, _window, cx: &mut App| {
            let repo_path = repo_path_for_close_orphan.clone();
            let entity = app_root_entity_for_close_orphan.clone();
            let window_name = window_name.to_string();
            cx.spawn(async move |_entity, cx| {
                let _ = blocking::unblock(move || kill_tmux_window(&repo_path, &window_name)).await;
                let _ = cx.update_entity(&entity, |this: &mut AppRoot, cx| {
                    this.cached_tmux_windows = None;
                    cx.notify();
                });
            }).detach();
        });
```

**Step 2: Verify**

Create an orphan window (e.g. remove worktree externally), then click close. No freeze.

**Step 3: Commit**

```bash
git add src/ui/app_root.rs
git commit -m "perf: defer kill_tmux_window to async in on_close_orphan"
```

---

### Task 1.4: (Optional) Defer refresh in on_view_diff

**Files:** `src/ui/app_root.rs`

**Rationale:** Only defer `refresh_worktrees_for_repo`; `open_review`/`switch_to_worktree` stay sync. Skip if time-constrained — Tasks 1.1–1.3 give the largest benefit.

**Step 1:** Add `repo_path_for_view_diff = repo_path.clone()` before `sidebar.on_view_diff`.

**Step 2:** Replace `on_view_diff` to spawn discover, then call `open_diff_view_for_worktree_with_cache`:

```rust
        let repo_path_for_view_diff = repo_path.clone();
        sidebar.on_view_diff(move |idx, _window, cx| {
            let _ = cx.update_entity(&app_root_entity_for_view_diff, |this: &mut AppRoot, cx| {
                this.sidebar_context_menu_index = None;
                cx.notify();
            });
            let entity = app_root_entity_for_view_diff.clone();
            let repo_path = repo_path_for_view_diff.clone();
            cx.spawn(async move |_entity, cx| {
                let result = blocking::unblock(move || crate::worktree::discover_worktrees(&repo_path)).await;
                let _ = cx.update_entity(&entity, |this: &mut AppRoot, cx| {
                    if let Ok(wt) = result {
                        this.cached_worktrees = wt;
                        this.cached_worktrees_repo = Some(repo_path);
                    }
                    this.open_diff_view_for_worktree_with_cache(idx, cx);
                });
            }).detach();
        });
```

**Step 3:** Extract the body of `open_diff_view_for_worktree` (without the initial `refresh_worktrees_for_repo`) into `open_diff_view_for_worktree_with_cache(&mut self, idx: usize, cx: &mut Context<Self>)`. Have `open_diff_view_for_worktree` call it for other callers.

**Step 4: Verify and commit**

```bash
cargo run --release
git add src/ui/app_root.rs && git commit -m "perf: defer refresh in on_view_diff to async"
```

---

## Phase 2: Terminal Output Throttling

### Task 2.1: Add 16ms throttle to terminal output loops

**Files:** `src/ui/app_root.rs:706-766` (setup_local_terminal), `src/ui/app_root.rs:858-916` (setup_pane_terminal_output)

**Step 1:** Add `last_notify` and 16ms throttle before `update_entity` in both loops. Only call `cx.update_entity(tae, ...)` when `now.duration_since(last_notify) >= Duration::from_millis(16)`.

**Step 2: Verify and commit**

```bash
cargo run --release
git add src/ui/app_root.rs && git commit -m "perf: throttle terminal output notify to 60fps (16ms)"
```

---

## Execution Options

1. **Subagent-Driven (this session)** — Dispatch fresh subagent per task, review between tasks.
2. **Parallel Session (separate)** — Open new session with executing-plans, batch execution with checkpoints.

