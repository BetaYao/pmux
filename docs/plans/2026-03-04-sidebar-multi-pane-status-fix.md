# Sidebar Multi-Pane Status Fix

> **For Claude:** Use TDD when implementing. Tasks are independent and can be done sequentially by hand or via `subagent-driven-development`.

**Goal:** Fix sidebar status display to show the highest-priority status across all panes in a worktree, instead of only the primary pane.

**Architecture:** The fix is purely in `sidebar.rs` render logic. `pane_statuses` already contains all panes (primary + splits); we just need to aggregate them by worktree path prefix using `AgentStatus::priority()`. No changes to the event pipeline or StatusPublisher needed.

**Tech Stack:** Rust, GPUI

---

## Background

### Current Bug

`Sidebar::render()` at `src/ui/sidebar.rs:580–581` calls `worktree_path_to_pane_id()` which generates `"local:{path}"` — only matching the primary pane. Split panes use IDs like `"local:{path}:split-0"`, `"local:{path}:split-1"`, etc. These are silently ignored.

```rust
// CURRENT (broken for multi-pane):
let pane_id = worktree_path_to_pane_id(&item.info.path);
let status = pane_statuses.get(&pane_id).copied().unwrap_or(AgentStatus::Unknown);
```

### Secondary Inconsistency

`StatusCounts` (shown in TopBar) aggregates **all** panes; Sidebar only shows the primary pane. A user can see "1 Error" in the TopBar but no red icon in any sidebar row.

### Fix Strategy

Replace the single-key lookup with a prefix scan: for a worktree at `/path/feat`, collect all entries whose key starts with `"local:/path/feat"` (i.e. primary + splits), then pick the one with the highest `priority()`.

---

## Task 1: Add `highest_priority_status_for_worktree` helper to `AgentStatus`

**Files:**
- Modify: `src/agent_status.rs`

### Step 1: Write the failing test

Add to `src/agent_status.rs` inside `#[cfg(test)] mod tests`:

```rust
#[test]
fn test_highest_priority_for_worktree_prefix() {
    use std::collections::HashMap;

    let mut statuses: HashMap<String, AgentStatus> = HashMap::new();
    statuses.insert("local:/path/feat".to_string(), AgentStatus::Idle);
    statuses.insert("local:/path/feat:split-0".to_string(), AgentStatus::Error);
    statuses.insert("local:/path/feat:split-1".to_string(), AgentStatus::Running);
    statuses.insert("local:/path/other".to_string(), AgentStatus::Waiting); // different worktree

    let result = AgentStatus::highest_priority_for_prefix(&statuses, "local:/path/feat");
    assert_eq!(result, AgentStatus::Error); // Error has priority 6 > Running 3 > Idle 2
}

#[test]
fn test_highest_priority_falls_back_to_unknown() {
    use std::collections::HashMap;
    let statuses: HashMap<String, AgentStatus> = HashMap::new();
    let result = AgentStatus::highest_priority_for_prefix(&statuses, "local:/path/feat");
    assert_eq!(result, AgentStatus::Unknown);
}

#[test]
fn test_highest_priority_prefix_does_not_cross_worktrees() {
    use std::collections::HashMap;
    let mut statuses: HashMap<String, AgentStatus> = HashMap::new();
    // "local:/path/feature-long" must NOT match prefix "local:/path/feat"
    statuses.insert("local:/path/feature-long".to_string(), AgentStatus::Error);
    statuses.insert("local:/path/feat".to_string(), AgentStatus::Idle);

    let result = AgentStatus::highest_priority_for_prefix(&statuses, "local:/path/feat");
    assert_eq!(result, AgentStatus::Idle);
}
```

### Step 2: Run tests to verify failure

```bash
RUSTUP_TOOLCHAIN=stable cargo test agent_status::tests::test_highest_priority
```

Expected: FAIL — `highest_priority_for_prefix` not defined.

### Step 3: Implement the helper in `src/agent_status.rs`

Add after the `from_pane_statuses` method (around line 203):

```rust
/// Find the highest-priority AgentStatus across all panes whose key matches the given prefix.
///
/// Matching rule: key equals `prefix` OR key starts with `"{prefix}:"`.
/// This correctly matches "local:/path/feat" and "local:/path/feat:split-0"
/// but NOT "local:/path/feature-long".
///
/// Returns `AgentStatus::Unknown` if no matching keys exist.
pub fn highest_priority_for_prefix(
    statuses: &std::collections::HashMap<String, AgentStatus>,
    prefix: &str,
) -> AgentStatus {
    let colon_prefix = format!("{}:", prefix);
    statuses
        .iter()
        .filter(|(k, _)| *k == prefix || k.starts_with(&colon_prefix))
        .map(|(_, v)| *v)
        .max_by_key(|s| s.priority())
        .unwrap_or(AgentStatus::Unknown)
}
```

### Step 4: Run tests to verify pass

```bash
RUSTUP_TOOLCHAIN=stable cargo test agent_status::tests::test_highest_priority
RUSTUP_TOOLCHAIN=stable cargo test agent_status::tests::test_highest_priority_falls_back_to_unknown
RUSTUP_TOOLCHAIN=stable cargo test agent_status::tests::test_highest_priority_prefix_does_not_cross_worktrees
```

Expected: PASS (all 3)

### Step 5: Commit

```bash
git add src/agent_status.rs
git commit -m "feat: add AgentStatus::highest_priority_for_prefix for multi-pane status aggregation"
```

---

## Task 2: Update `Sidebar::render()` to use prefix-based status lookup

**Files:**
- Modify: `src/ui/sidebar.rs:578–583`

### Step 1: Write the failing test

Add to `src/ui/sidebar.rs` inside `#[cfg(test)] mod tests`:

```rust
#[test]
fn test_sidebar_status_aggregates_split_panes() {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    let mut pane_statuses: HashMap<String, AgentStatus> = HashMap::new();
    // Primary pane is Idle, but split-0 has an Error
    pane_statuses.insert("local:/tmp/feat".to_string(), AgentStatus::Idle);
    pane_statuses.insert("local:/tmp/feat:split-0".to_string(), AgentStatus::Error);

    // Verify the helper picks Error over Idle
    let prefix = worktree_path_to_pane_id(std::path::Path::new("/tmp/feat"));
    let status = AgentStatus::highest_priority_for_prefix(&pane_statuses, &prefix);
    assert_eq!(status, AgentStatus::Error);
}

#[test]
fn test_sidebar_status_primary_only_when_no_splits() {
    use std::collections::HashMap;

    let mut pane_statuses: HashMap<String, AgentStatus> = HashMap::new();
    pane_statuses.insert("local:/tmp/feat".to_string(), AgentStatus::Running);

    let prefix = worktree_path_to_pane_id(std::path::Path::new("/tmp/feat"));
    let status = AgentStatus::highest_priority_for_prefix(&pane_statuses, &prefix);
    assert_eq!(status, AgentStatus::Running);
}
```

### Step 2: Run tests to verify they compile and pass

```bash
RUSTUP_TOOLCHAIN=stable cargo test sidebar::tests::test_sidebar_status
```

Expected: PASS (these tests use the pure helper directly, not GPUI render).

### Step 3: Update `Sidebar::render()` at `src/ui/sidebar.rs:580–581`

Replace:
```rust
let pane_id = worktree_path_to_pane_id(&item.info.path);
let status = pane_statuses.get(&pane_id).copied().unwrap_or(AgentStatus::Unknown);
```

With:
```rust
let pane_prefix = worktree_path_to_pane_id(&item.info.path);
let status = AgentStatus::highest_priority_for_prefix(&pane_statuses, &pane_prefix);
```

Also update the notification lookup two lines below (line ~591) to use `pane_prefix` instead of `pane_id`:
```rust
// Before:
m.by_pane(&pane_id).first()
// After:
m.by_pane(&pane_prefix).first()
```

> Note: the notification lookup uses the primary pane ID for `NotificationManager::by_pane`, which is correct — notifications are stored per-pane, not aggregated. The `pane_prefix` equals the primary pane ID so this is a safe rename only.

### Step 4: Verify build

```bash
RUSTUP_TOOLCHAIN=stable cargo check
RUSTUP_TOOLCHAIN=stable cargo test sidebar::
```

Expected: compiles cleanly, all sidebar tests pass.

### Step 5: Commit

```bash
git add src/ui/sidebar.rs
git commit -m "fix: sidebar now shows highest-priority status across all split panes in a worktree"
```

---

## Task 3: Add `StatusCounts::from_pane_statuses_per_worktree` (align TopBar with Sidebar semantics)

**Context:** `StatusCounts` used by TopBar counts every pane ID separately. After the fix, the Sidebar now shows one status per worktree (highest priority). The TopBar should match: count one status per worktree, not one per pane.

**Files:**
- Modify: `src/agent_status.rs`

### Step 1: Write the failing test

Add to `src/agent_status.rs` `#[cfg(test)] mod tests`:

```rust
#[test]
fn test_status_counts_per_worktree_not_per_pane() {
    use std::collections::HashMap;

    let mut statuses: HashMap<String, AgentStatus> = HashMap::new();
    // worktree "feat": primary=Idle, split=Error → net status = Error
    statuses.insert("local:/path/feat".to_string(), AgentStatus::Idle);
    statuses.insert("local:/path/feat:split-0".to_string(), AgentStatus::Error);
    // worktree "main": primary=Running, no splits → net status = Running
    statuses.insert("local:/path/main".to_string(), AgentStatus::Running);

    let counts = StatusCounts::from_pane_statuses_per_worktree(&statuses);
    // 1 Error (feat) + 1 Running (main); split-0 should NOT be counted separately
    assert_eq!(counts.error, 1);
    assert_eq!(counts.running, 1);
    assert_eq!(counts.idle, 0);   // Idle from primary is eclipsed by Error
    assert_eq!(counts.total(), 2);
}
```

### Step 2: Run test to verify failure

```bash
RUSTUP_TOOLCHAIN=stable cargo test agent_status::tests::test_status_counts_per_worktree
```

Expected: FAIL — `from_pane_statuses_per_worktree` not defined.

### Step 3: Implement in `src/agent_status.rs`

Add after `from_pane_statuses` (around line 203):

```rust
/// Compute StatusCounts treating each worktree as one entry (highest-priority pane wins).
///
/// Groups pane_ids by worktree prefix (the part before the first `:` suffix separator),
/// then picks the highest-priority status per group. This matches what Sidebar displays.
///
/// Pane ID format: `"local:{path}"` (primary) or `"local:{path}:{suffix}"` (splits).
/// Worktree prefix = `"local:{path}"`.
pub fn from_pane_statuses_per_worktree(
    statuses: &std::collections::HashMap<String, AgentStatus>,
) -> Self {
    use std::collections::HashSet;

    // Collect unique worktree prefixes (primary pane IDs).
    // A primary pane ID has exactly one ':' separator (between "local" and the path).
    // Split panes have a trailing ":suffix". So the worktree prefix is everything up to
    // the second ':' when the key has the pattern "local:{path}:{suffix}".
    let prefixes: HashSet<String> = statuses.keys().map(|k| {
        // k = "local:/some/path" or "local:/some/path:split-0"
        // Split on ':' at most 3 parts: ["local", "/some/path", "split-0"]
        // prefix = "local:" + path part = first two parts joined
        let parts: Vec<&str> = k.splitn(3, ':').collect();
        if parts.len() >= 2 {
            format!("{}:{}", parts[0], parts[1])
        } else {
            k.clone()
        }
    }).collect();

    let mut counts = Self::new();
    for prefix in &prefixes {
        let status = AgentStatus::highest_priority_for_prefix(statuses, prefix);
        counts.increment(&status);
    }
    counts
}
```

### Step 4: Run test to verify pass

```bash
RUSTUP_TOOLCHAIN=stable cargo test agent_status::tests::test_status_counts_per_worktree
RUSTUP_TOOLCHAIN=stable cargo test agent_status::
```

Expected: all pass.

### Step 5: Wire into `StatusCountsModel`

Find where `StatusCounts::from_pane_statuses` is called in `src/ui/models/status_counts_model.rs` and replace with the new method:

```rust
// Before:
StatusCounts::from_pane_statuses(&statuses)
// After:
StatusCounts::from_pane_statuses_per_worktree(&statuses)
```

Also check `src/ui/app_root.rs` for any direct calls to `update_status_counts` that use `from_pane_statuses`:

```bash
grep -n "from_pane_statuses" src/
```

Replace any remaining calls with `from_pane_statuses_per_worktree`.

### Step 6: Final build + full test run

```bash
RUSTUP_TOOLCHAIN=stable cargo check
RUSTUP_TOOLCHAIN=stable cargo test
```

Expected: all tests pass, no compile errors.

### Step 7: Commit

```bash
git add src/agent_status.rs src/ui/models/status_counts_model.rs src/ui/app_root.rs
git commit -m "fix: TopBar StatusCounts now counts per-worktree (highest-priority pane), matching Sidebar semantics"
```

---

## Summary of Changes

| File | Change |
|------|--------|
| `src/agent_status.rs` | Add `AgentStatus::highest_priority_for_prefix()` + `StatusCounts::from_pane_statuses_per_worktree()` |
| `src/ui/sidebar.rs:580` | Replace single-key lookup with `highest_priority_for_prefix()` |
| `src/ui/models/status_counts_model.rs` | Use `from_pane_statuses_per_worktree` for TopBar counts |

No changes to the event pipeline, StatusPublisher, EventBus, or TerminalAreaEntity — the fix is entirely in read/display logic.

---

## Verification

After all tasks, manually test by:
1. Opening a worktree, splitting a pane (Cmd+D or equivalent)
2. Running a command that triggers Error status in the split pane while the primary is Idle
3. Confirm sidebar shows Error (red ✕) for that worktree row
4. Confirm TopBar error count = 1 (not 2)
