# Extract attach_runtime and Implement recover() — Implementation Plan

> **For Claude:** Use TDD when implementing. Consider `subagent-driven-development` for parallel tasks.

**Goal:** Extract shared "attach runtime + setup terminal" logic into `attach_runtime()`, then implement `try_recover_then_switch` and `try_recover_then_start` by calling `recover_runtime` + `attach_runtime`.

**Architecture:** 
- `attach_runtime(runtime, pane_target, worktree_path, branch_name, cx)` — single place for runtime wiring, terminal setup, status publisher
- `start_local_session` / `switch_to_worktree` — create runtime → `attach_runtime`
- `try_recover_then_switch` / `try_recover_then_start` — `recover_runtime` → `attach_runtime`

**Tech Stack:** Rust, GPUI, flume, tokio

---

## Background

**Current state:**
- `start_local_session` and `switch_to_worktree` duplicate ~40 lines of identical logic (runtime wiring, pane target, split_tree, status publisher, setup_local_terminal, save_runtime_state)
- `try_recover_then_switch` and `try_recover_then_start` return `false` (stub) — `recover_runtime` exists in backend but is never called
- Impact: tmux sessions are never recovered on pmux restart; users always get new sessions

**Design reference:** `docs/design-gap-analysis.md` Section 二.1 (P0), `openspec/changes/runtime-completion/design.md` Section 3.6

---

## Task 1: Add attach_runtime and Refactor start_local_session

**Files:** `src/ui/app_root.rs`

**Step 1: Add attach_runtime method**

Add a new private method after `setup_local_terminal`:

```rust
/// Attach an existing runtime: wire UI state, terminal, status publisher.
/// Used by start_local_session, switch_to_worktree, and try_recover_*.
fn attach_runtime(
    &mut self,
    runtime: Arc<dyn AgentRuntime>,
    pane_target: String,
    worktree_path: &Path,
    branch_name: &str,
    cx: &mut Context<Self>,
) {
    self.runtime = Some(runtime.clone());
    let _ = runtime.focus_pane(&pane_target);
    self.active_pane_target = Some(pane_target.clone());
    self.split_tree = SplitNode::pane(&pane_target);
    self.focused_pane_index = 0;
    if let Ok(mut guard) = self.active_pane_target_shared.lock() {
        *guard = pane_target.clone();
    }
    if let Ok(mut guard) = self.pane_targets_shared.lock() {
        *guard = vec![pane_target.clone()];
    }
    self.terminal_needs_focus = true;

    self.ensure_event_bus_subscription(cx);

    let status_publisher = StatusPublisher::new(Arc::clone(&self.event_bus));
    status_publisher.register_pane(&pane_target);
    self.status_publisher = Some(status_publisher);

    self.setup_local_terminal(runtime, &pane_target, cx);

    if let Some(tab) = self.workspace_manager.active_tab() {
        let wp = tab.path.clone();
        self.save_runtime_state(&wp, worktree_path, branch_name);
    }
}
```

**Step 2: Refactor start_local_session**

Replace the body of `start_local_session` with:

```rust
fn start_local_session(&mut self, worktree_path: &Path, branch_name: &str, cx: &mut Context<Self>) {
    let runtime = match create_runtime_from_env(worktree_path, 80, 24) {
        Ok(rt) => rt,
        Err(e) => {
            self.state.error_message = Some(format!("Runtime error: {}", e));
            return;
        }
    };
    let pane_target = runtime.primary_pane_id()
        .unwrap_or_else(|| format!("local:{}", worktree_path.display()));
    self.attach_runtime(runtime, pane_target, worktree_path, branch_name, cx);
}
```

**Step 3: Run tests**

```bash
RUSTUP_TOOLCHAIN=stable cargo test --lib
```

Expected: All tests pass

**Step 4: Commit**

```bash
git add src/ui/app_root.rs
git commit -m "refactor(ui): add attach_runtime, refactor start_local_session to use it"
```

---

## Task 2: Refactor switch_to_worktree to Use attach_runtime

**Files:** `src/ui/app_root.rs`

**Step 1: Replace body of switch_to_worktree**

```rust
fn switch_to_worktree(&mut self, worktree_path: &Path, branch_name: &str, cx: &mut Context<Self>) {
    self.stop_current_session();

    let runtime = match create_runtime_from_env(worktree_path, 80, 24) {
        Ok(rt) => rt,
        Err(e) => {
            self.state.error_message = Some(format!(
                "Runtime error for worktree {}: {}",
                worktree_path.display(),
                e
            ));
            return;
        }
    };
    let pane_target = runtime.primary_pane_id()
        .unwrap_or_else(|| format!("local:{}", worktree_path.display()));
    self.attach_runtime(runtime, pane_target, worktree_path, branch_name, cx);
}
```

**Step 2: Run tests**

```bash
RUSTUP_TOOLCHAIN=stable cargo test --lib
```

Expected: All tests pass

**Step 3: Commit**

```bash
git add src/ui/app_root.rs
git commit -m "refactor(ui): use attach_runtime in switch_to_worktree"
```

---

## Task 3: Implement try_recover_then_switch

**Files:** `src/ui/app_root.rs`

**Step 1: Add recover_runtime import**

Ensure `recover_runtime` is imported:

```rust
use crate::runtime::backends::{create_runtime_from_env, main_window_target, recover_runtime};
```

**Step 2: Add effective_backend helper**

Add a private associated function or method to get backend. For now use env var (Config.backend can be added later):

```rust
fn effective_backend(&self) -> String {
    std::env::var(crate::runtime::backends::PMUX_BACKEND_ENV)
        .unwrap_or_else(|_| crate::runtime::backends::DEFAULT_BACKEND.to_string())
}
```

**Step 3: Implement try_recover_then_switch**

```rust
fn try_recover_then_switch(
    &mut self,
    workspace_path: &Path,
    worktree_path: &Path,
    branch_name: &str,
    cx: &mut Context<Self>,
) -> bool {
    if self.effective_backend() != "tmux" {
        return false;
    }
    let state = match RuntimeState::load() {
        Ok(s) => s,
        Err(_) => return false,
    };
    let workspace_path_buf = workspace_path.to_path_buf();
    let workspace = match state.find_workspace(&workspace_path_buf) {
        Some(w) => w,
        None => return false,
    };
    let worktree = workspace
        .worktrees
        .iter()
        .find(|w| w.path.as_path() == worktree_path)
    else {
        return false;
    };

    let runtime = match recover_runtime(
        &worktree.backend,
        worktree,
        Some(Arc::clone(&self.event_bus)),
    ) {
        Ok(rt) => rt,
        Err(_) => return false,
    };

    let pane_target = worktree
        .pane_ids
        .first()
        .cloned()
        .or_else(|| runtime.primary_pane_id())
        .unwrap_or_else(|| format!("local:{}", worktree_path.display()));

    self.attach_runtime(runtime, pane_target, worktree_path, branch_name, cx);
    true
}
```

**Step 4: Run tests**

```bash
RUSTUP_TOOLCHAIN=stable cargo test --lib
```

**Step 5: Commit**

```bash
git add src/ui/app_root.rs
git commit -m "feat(ui): implement try_recover_then_switch, call recover_runtime"
```

---

## Task 4: Implement try_recover_then_start

**Files:** `src/ui/app_root.rs`

**Context:** `try_recover_then_start` is called when the repo has no worktrees (bare or single-repo). State may have a worktree with `path == workspace_path` (main worktree).

**Step 1: Implement try_recover_then_start**

```rust
fn try_recover_then_start(
    &mut self,
    workspace_path: &Path,
    _repo_name: &str,
    cx: &mut Context<Self>,
) -> bool {
    if self.effective_backend() != "tmux" {
        return false;
    }
    let state = match RuntimeState::load() {
        Ok(s) => s,
        Err(_) => return false,
    };
    let workspace_path_buf = workspace_path.to_path_buf();
    let workspace = match state.find_workspace(&workspace_path_buf) {
        Some(w) => w,
        None => return false,
    };
    // Repo-only: use first worktree in state (main worktree path often equals workspace_path)
    let worktree = workspace.worktrees.first()?;

    let runtime = match recover_runtime(
        &worktree.backend,
        worktree,
        Some(Arc::clone(&self.event_bus)),
    ) {
        Ok(rt) => rt,
        Err(_) => return false,
    };

    let pane_target = worktree
        .pane_ids
        .first()
        .cloned()
        .or_else(|| runtime.primary_pane_id())
        .unwrap_or_else(|| format!("local:{}", worktree.path.display()));

    self.attach_runtime(
        runtime,
        pane_target,
        &worktree.path,
        &worktree.branch,
        cx,
    );
    true
}
```

**Step 2: Run tests**

```bash
RUSTUP_TOOLCHAIN=stable cargo test --lib
```

**Step 3: Commit**

```bash
git add src/ui/app_root.rs
git commit -m "feat(ui): implement try_recover_then_start for repo-only recovery"
```

---

## Task 5: Fix effective_backend and Import

**Files:** `src/ui/app_root.rs`

**Step 1:** `effective_backend` should be a method on `AppRoot` (it uses `&self` only for consistency with other helpers). If we prefer a free function, ensure it's in scope.

**Step 2:** Remove `main_window_target` from imports if not used elsewhere in app_root. Check with:

```bash
grep -n "main_window_target" src/ui/app_root.rs
```

If only used in delete_worktree or similar, keep the import. Otherwise remove.

**Step 3:** Run clippy

```bash
RUSTUP_TOOLCHAIN=stable cargo clippy --lib 2>&1 | head -60
```

Fix any warnings.

---

## Task 6: Manual Verification

**Step 1: Tmux recovery flow**

1. `PMUX_BACKEND=tmux cargo run`
2. Add a workspace, ensure tmux session is created
3. Close pmux (Cmd+Q or quit)
4. Reopen pmux, switch to the same workspace
5. Expected: Terminal shows existing tmux session content (recovered), not a new shell

**Step 2: Local PTY unchanged**

1. `cargo run` (default local backend)
2. Add workspace, close, reopen
3. Expected: New session each time (no recovery; `try_recover_*` returns false)

---

## Verification Checklist

- [ ] `attach_runtime` exists and is used by `start_local_session`, `switch_to_worktree`, `try_recover_then_switch`, `try_recover_then_start`
- [ ] `try_recover_then_switch` and `try_recover_then_start` call `recover_runtime` when backend is tmux
- [ ] `cargo test --lib` passes
- [ ] `cargo clippy --lib` clean (or only acceptable warnings)
- [ ] Manual test: tmux recovery works on restart

---

## Dependencies

- `recover_runtime` in `src/runtime/backends/mod.rs` — **already implemented**
- `RuntimeState::load()`, `WorktreeState` — **already implemented**
- `save_runtime_state` — **already implemented**

---

## Next Steps (Optional)

- Add `backend` to `Config` and use `Config::load().backend` in `effective_backend` (P1, see design-gap-analysis.md)
- Add unit test for `try_recover_then_switch` with mocked state (integration test would require tmux)
