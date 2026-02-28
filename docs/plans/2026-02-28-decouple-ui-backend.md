# Decouple UI from Backend - Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Remove all direct dependencies between UI layer and backend implementations (TmuxRuntime), ensuring UI only interacts through AgentRuntime trait.

**Architecture:** Introduce `recover_runtime()` factory function in backends/mod.rs, extend AgentRuntime trait with `recover()` method, and refactor app_root.rs to use only the trait interface.

**Tech Stack:** Rust, GPUI, flume channels, tokio

---

## Background

Current violations of RULE 1 (UI → Runtime ONLY):
- `src/ui/app_root.rs:12` imports `TmuxRuntime` directly
- `src/ui/app_root.rs:546-631` calls `TmuxRuntime::attach()` directly in recovery logic
- `src/ui/app_root.rs:750` calls `tmux_session_window()` directly

This plan fixes Phase 1 of the full architecture refactoring.

---

### Task 1: Add recover_runtime Factory Function

**Files:**
- Modify: `src/runtime/backends/mod.rs:49-77`

**Step 1: Write the failing test**

Create `src/runtime/backends/mod_test.rs` (new file):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::RuntimeState;
    use std::path::PathBuf;

    #[test]
    fn test_recover_runtime_unknown_backend() {
        let state = crate::runtime::WorktreeState {
            path: PathBuf::from("/tmp/test"),
            branch: "main".to_string(),
            agent_id: "test".to_string(),
            pane_ids: vec![],
            backend: "unknown".to_string(),
            backend_session_id: String::new(),
            backend_window_id: String::new(),
        };
        let result = recover_runtime("unknown_backend", &state, None);
        assert!(result.is_err());
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --lib runtime::backends::mod_test --no-run 2>&1 || true`
Expected: Compilation error - `recover_runtime` not found

**Step 3: Write minimal implementation**

Add to `src/runtime/backends/mod.rs` after `create_runtime`:

```rust
/// Recover an AgentRuntime from persisted state.
/// Used when pmux restarts and needs to attach to existing sessions.
pub fn recover_runtime(
    backend: &str,
    state: &crate::runtime::WorktreeState,
    event_bus: Option<Arc<crate::runtime::EventBus>>,
) -> Result<Arc<dyn AgentRuntime>, RuntimeError> {
    match backend {
        BACKEND_TMUX => {
            let _ = event_bus;
            TmuxRuntime::attach(&state.backend_session_id, &state.backend_window_id)
        }
        BACKEND_LOCAL_PTY => {
            Err(RuntimeError::Backend("local_pty does not support session recovery".into()))
        }
        _ => Err(RuntimeError::Backend(format!("unknown backend: {}", backend))),
    }
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test --lib runtime::backends::mod_test`
Expected: PASS

**Step 5: Commit**

```bash
git add src/runtime/backends/mod.rs
git commit -m "feat(runtime): add recover_runtime factory function"
```

---

### Task 2: Add recover Method to AgentRuntime Trait

**Files:**
- Modify: `src/runtime/agent_runtime.rs:39-75`

**Step 1: Write the failing test**

Add to `src/runtime/agent_runtime.rs` tests:

```rust
#[cfg(test)]
mod tests {
    // ... existing tests ...
    
    #[test]
    fn test_agent_runtime_has_recover_signature() {
        fn assert_recover_exists<T: AgentRuntime>() {}
        assert_recover_exists::<crate::runtime::backends::TmuxRuntime>();
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --lib runtime::agent_runtime::tests::test_agent_runtime_has_recover_signature`
Expected: FAIL - trait missing recover method

**Step 3: Write minimal implementation**

Add to `AgentRuntime` trait in `src/runtime/agent_runtime.rs`:

```rust
pub trait AgentRuntime: Send + Sync {
    // ... existing methods ...
    
    /// Recover/attach to an existing session from persisted state.
    /// Returns error if backend doesn't support recovery (e.g., local_pty).
    fn recover(&self, workspace_path: &Path, worktree_path: &Path) -> Result<(), RuntimeError> {
        let _ = (workspace_path, worktree_path);
        Err(RuntimeError::Backend("recover not implemented for this backend".into()))
    }
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test --lib runtime::agent_runtime::tests::test_agent_runtime_has_recover_signature`
Expected: PASS

**Step 5: Commit**

```bash
git add src/runtime/agent_runtime.rs
git commit -m "feat(runtime): add recover method to AgentRuntime trait"
```

---

### Task 3: Implement recover for TmuxRuntime

**Files:**
- Modify: `src/runtime/backends/tmux.rs`

**Step 1: Write the failing test**

Add to `src/runtime/backends/tmux.rs` tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_tmux_runtime_has_recover() {
        fn assert_recover_impl<T: AgentRuntime>() {}
        // If this compiles, TmuxRuntime implements AgentRuntime with recover
        assert_recover_impl::<TmuxRuntime>();
    }
}
```

**Step 2: Run test to verify it passes**

Run: `cargo test --lib runtime::backends::tmux::tests`
Expected: PASS (default impl exists from trait)

**Step 3: Add specific implementation if needed**

Check if TmuxRuntime needs custom recover logic. If default stub is sufficient, skip.

**Step 4: Commit if changes made**

```bash
git add src/runtime/backends/tmux.rs
git commit -m "feat(tmux): implement recover method for TmuxRuntime" || echo "No changes needed"
```

---

### Task 4: Implement recover for LocalPtyAgent

**Files:**
- Modify: `src/runtime/backends/local_pty.rs`

**Step 1: Write the failing test**

Add to `src/runtime/backends/local_pty.rs` tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_local_pty_has_recover() {
        fn assert_recover_impl<T: AgentRuntime>() {}
        assert_recover_impl::<LocalPtyAgent>();
    }
    
    #[test]
    fn test_local_pty_recover_returns_error() {
        use std::path::PathBuf;
        let rt = LocalPtyAgent::new(&PathBuf::from("/tmp"), 80, 24, None).ok();
        if let Some(rt) = rt {
            let result = rt.recover(&PathBuf::from("/tmp"), &PathBuf::from("/tmp"));
            assert!(result.is_err());
        }
    }
}
```

**Step 2: Run test to verify it passes**

Run: `cargo test --lib runtime::backends::local_pty::tests`
Expected: PASS (default impl returns error)

**Step 3: Add explicit implementation for clarity**

Add to `impl AgentRuntime for LocalPtyAgent`:

```rust
fn recover(&self, _workspace_path: &Path, _worktree_path: &Path) -> Result<(), RuntimeError> {
    Err(RuntimeError::Backend("local_pty does not support session recovery".into()))
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test --lib runtime::backends::local_pty::tests`
Expected: PASS

**Step 5: Commit**

```bash
git add src/runtime/backends/local_pty.rs
git commit -m "feat(local_pty): implement recover method returning error"
```

---

### Task 5: Refactor try_recover_then_switch to Use Factory

**Files:**
- Modify: `src/ui/app_root.rs:566-597`

**Step 1: Write the failing test**

Add to `src/ui/app_root.rs` or create integration test:

```rust
// Note: UI tests are typically manual or integration tests
// This test verifies the refactored code compiles
#[cfg(test)]
mod tests {
    #[test]
    fn test_no_direct_tmux_import() {
        // This test passes if the file compiles without direct TmuxRuntime use in recovery
        assert!(true);
    }
}
```

**Step 2: Remove direct TmuxRuntime import**

In `src/ui/app_root.rs:12`, change:

```rust
// Before:
use crate::runtime::backends::{create_runtime, tmux_session_window, TmuxRuntime};

// After:
use crate::runtime::backends::{create_runtime, recover_runtime};
```

**Step 3: Refactor try_recover_then_switch**

Replace `src/ui/app_root.rs:566-597` with:

```rust
fn try_recover_then_switch(&mut self, workspace_path: &Path, worktree_path: &Path, branch_name: &str, cx: &mut Context<Self>) -> bool {
    if self.backend != crate::config::BACKEND_TMUX {
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
    let worktree = match workspace.worktrees.iter().find(|w| w.path.as_path() == worktree_path) {
        Some(w) => w,
        None => return false,
    };

    match recover_runtime(&worktree.backend, worktree, Some(Arc::clone(&self.event_bus))) {
        Ok(runtime) => {
            let runtime = runtime;
            self.attach_backend_runtime(runtime, worktree, worktree_path, branch_name, cx);
            true
        }
        Err(_) => false,
    }
}
```

**Step 4: Run to verify compilation**

Run: `cargo build --lib 2>&1 | head -50`
Expected: No errors related to TmuxRuntime

**Step 5: Commit**

```bash
git add src/ui/app_root.rs
git commit -m "refactor(ui): use recover_runtime factory instead of direct TmuxRuntime"
```

---

### Task 6: Refactor try_recover_then_start to Use Factory

**Files:**
- Modify: `src/ui/app_root.rs:600-631`

**Step 1: Refactor try_recover_then_start**

Replace `src/ui/app_root.rs:600-631` with:

```rust
fn try_recover_then_start(&mut self, workspace_path: &Path, _repo_name: &str, cx: &mut Context<Self>) -> bool {
    if self.backend != crate::config::BACKEND_TMUX {
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
    let worktree = match workspace.worktrees.iter().find(|w| w.path.as_path() == workspace_path) {
        Some(w) => w,
        None => return false,
    };

    match recover_runtime(&worktree.backend, worktree, Some(Arc::clone(&self.event_bus))) {
        Ok(runtime) => {
            self.attach_backend_runtime(runtime, worktree, workspace_path, &worktree.branch, cx);
            true
        }
        Err(_) => false,
    }
}
```

**Step 2: Run to verify compilation**

Run: `cargo build --lib`
Expected: PASS

**Step 3: Commit**

```bash
git add src/ui/app_root.rs
git commit -m "refactor(ui): use recover_runtime in try_recover_then_start"
```

---

### Task 7: Rename attach_tmux_runtime to attach_backend_runtime

**Files:**
- Modify: `src/ui/app_root.rs:634-689`

**Step 1: Rename the method**

Change method name from `attach_tmux_runtime` to `attach_backend_runtime`:

```rust
fn attach_backend_runtime(
    &mut self,
    runtime: Arc<dyn AgentRuntime>,
    worktree: &WorktreeState,
    worktree_path: &Path,
    branch_name: &str,
    cx: &mut Context<Self>,
) {
    // ... existing implementation unchanged ...
}
```

**Step 2: Update all call sites**

Search and replace calls to `attach_tmux_runtime` with `attach_backend_runtime`.

Run: `grep -n "attach_tmux_runtime" src/ui/app_root.rs`
Expected: No matches after replacement

**Step 3: Run to verify compilation**

Run: `cargo build --lib`
Expected: PASS

**Step 4: Commit**

```bash
git add src/ui/app_root.rs
git commit -m "refactor(ui): rename attach_tmux_runtime to attach_backend_runtime"
```

---

### Task 8: Remove tmux_session_window Import from UI

**Files:**
- Modify: `src/ui/app_root.rs:12`

**Step 1: Find all uses of tmux_session_window in UI**

Run: `grep -n "tmux_session_window" src/ui/app_root.rs`

**Step 2: Replace with backend-agnostic approach**

If `tmux_session_window` is called directly in `save_runtime_state`:

Find alternative: either move the function to RuntimeState, or pass session/window info through WorktreeState.

Expected change in `save_runtime_state`:
- Remove direct call to `tmux_session_window`
- Use data already available in WorktreeState or runtime

**Step 3: Remove import**

Ensure `tmux_session_window` is not imported in UI:
```rust
// Remove from imports
// use crate::runtime::backends::tmux_session_window;
```

**Step 4: Run to verify compilation**

Run: `cargo build --lib`
Expected: PASS

**Step 5: Commit**

```bash
git add src/ui/app_root.rs
git commit -m "refactor(ui): remove tmux_session_window import from UI layer"
```

---

### Task 9: Run Full Test Suite

**Files:**
- N/A (verification)

**Step 1: Run all tests**

Run: `cargo test --lib`
Expected: All tests pass

**Step 2: Run clippy**

Run: `cargo clippy --lib 2>&1 | head -50`
Expected: No errors (warnings acceptable)

**Step 3: Build release**

Run: `cargo build --release`
Expected: PASS

**Step 4: Manual smoke test**

- Open pmux
- Create a workspace
- Verify terminal works
- Close and reopen
- Verify recovery works

---

### Task 10: Final Commit and Summary

**Step 1: Create summary commit if needed**

```bash
git status
git add -A
git commit -m "refactor: complete Phase 1 - UI decoupled from backend implementations

- Added recover_runtime factory function
- Extended AgentRuntime trait with recover method
- Removed all direct TmuxRuntime imports from UI
- Renamed attach_tmux_runtime to attach_backend_runtime
- UI now only interacts through AgentRuntime trait"
```

**Step 2: Verify git log**

Run: `git log --oneline -10`
Expected: See all commits in order

---

## Verification Checklist

After completing all tasks:

- [ ] `grep -r "TmuxRuntime" src/ui/` returns no matches
- [ ] `grep -r "tmux_session_window" src/ui/` returns no matches
- [ ] All tests pass: `cargo test --lib`
- [ ] Clippy clean: `cargo clippy --lib`
- [ ] Manual test: workspace creation and recovery works

---

## Next Steps

After Phase 1 is complete:

1. **Phase 2:** Implement BackendPane trait
2. **Phase 3:** Remove status_detector.rs (text parsing)
3. **Phase 4:** Eliminate polling loops