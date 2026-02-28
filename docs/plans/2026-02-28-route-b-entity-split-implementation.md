# Route B: Zed-Style Entity Split — Implementation Plan

> **For Claude:** Use TDD when implementing. Consider `subagent-driven-development` for parallel tasks.
>
> **Context:** See [2026-02-28-route-b-zed-style-entity-split-brainstorm.md](2026-02-28-route-b-zed-style-entity-split-brainstorm.md).

**Goal:** Split the single AppRoot tree into multiple independently updateable Model/Entity components so that hovering TopBar, switching worktree, or opening dialogs triggers only the corresponding subtree re-render.

**Architecture:** Extract shared state into GPUI Models (`cx.new()`), elevate TopBar/Sidebar/NotificationPanel/TerminalView to Entities that `observe` these Models. AppRoot becomes a layout orchestrator; each child Entity notifies only itself when its observed state changes.

**Tech Stack:** Rust, GPUI (observe/notify), existing EventBus, StatusCounts, WorktreeInfo.

---

## Phase 0: Spike — Verify Entity-Scoped Re-render

Before full implementation, confirm GPUI re-renders only the notifying Entity's subtree.

### Task 0.1: Create minimal StatusCountsModel and TopBar Entity

**Files:**
- Create: `src/ui/models/status_counts_model.rs`
- Create: `src/ui/models/mod.rs`
- Modify: `src/ui/mod.rs` (add `models` module)
- Modify: `src/ui/app_root.rs` (create TopBar Entity in init, render it)

**Step 1: Create StatusCountsModel**

```rust
// src/ui/models/mod.rs
mod status_counts_model;
pub use status_counts_model::StatusCountsModel;
```

```rust
// src/ui/models/status_counts_model.rs
use crate::agent_status::StatusCounts;

/// Shared model for agent status counts. TopBar/StatusBar observe this.
/// Does NOT implement Render.
pub struct StatusCountsModel {
    pub counts: StatusCounts,
}

impl StatusCountsModel {
    pub fn new() -> Self {
        Self {
            counts: StatusCounts::new(),
        }
    }

    pub fn set_counts(&mut self, counts: StatusCounts) {
        self.counts = counts;
    }
}
```

**Step 2: Add models to ui/mod.rs**

```rust
// In src/ui/mod.rs, add:
pub mod models;
```

**Step 3: Create TopBarEntity with observe**

In `app_root.rs`, add field `topbar_entity: Option<Entity<TopBarEntity>>` and `status_counts_model: Option<Entity<StatusCountsModel>>`. In `init_workspace_restoration` or a new `ensure_entities` called from render when has_workspaces:

- `cx.new(|_cx| StatusCountsModel::new())` → store in status_counts_model
- Create a minimal `TopBarEntity` struct that holds `Entity<StatusCountsModel>` and implements `Render`. In its constructor (called from `cx.new`), use `cx.observe(&status_counts_model, |this, observed, cx| { /* copy counts */ cx.notify(); }).detach();`
- Render: `div().child("Spike TopBar: count=").child(format!("{}", self.counts.running))`

**Step 4: Wire EventBus to StatusCountsModel**

In the EventBus subscription spawn, when `AgentStateChange` arrives: update `status_counts_model` via `cx.update_entity(&status_counts_model, |m, cx| { m.set_counts(...); cx.notify(); })` instead of `app_root_entity.update(..., cx.notify())`.

**Step 5: Add render counter (optional verification)**

Add `static RENDER_COUNT: AtomicU64` in TopBarEntity and increment in render. Log when AppRoot renders vs TopBarEntity renders. Run app, trigger status change, confirm only TopBarEntity render count increases.

**Verification:**
- Run: `cargo run`
- Trigger an agent status change (or inject a test button that updates StatusCountsModel)
- Observe: Only TopBar re-renders (via logs or UI flicker scope)

---

## Phase 1: StatusCountsModel Full Integration

### Task 1.1: Move StatusCounts aggregation into Model

**Files:**
- Modify: `src/ui/models/status_counts_model.rs`
- Modify: `src/ui/app_root.rs`

**Steps:**
1. Add `pane_statuses: Arc<Mutex<HashMap<String, AgentStatus>>>` to StatusCountsModel (or keep in AppRoot and pass to Model when updating).
2. StatusCountsModel computes StatusCounts from pane_statuses; expose `update_pane_status(&mut self, pane_id, status)` and `recompute_counts()`.
3. EventBus handler calls `status_counts_model.update(...)` with new pane status, recomputes, `cx.notify()`.
4. Remove AppRoot's direct `status_counts` aggregation from pane_statuses for display; TopBar and StatusBar read from status_counts_model.

**Commit:** `feat(ui): integrate StatusCountsModel with EventBus`

---

### Task 1.2: TopBar reads from StatusCountsModel

**Files:**
- Modify: `src/ui/topbar.rs`
- Modify: `src/ui/app_root.rs`

**Steps:**
1. Change TopBar to accept `Entity<StatusCountsModel>` instead of `StatusCounts` by value. In render, `model.read(cx).counts` (or observe pattern).
2. Since TopBar is still a Component, it receives the entity as prop. When used as Entity (Phase 2), it observes the model in constructor.
3. For this task: keep TopBar as Component but pass `status_counts_model.read(cx).counts.clone()` when building TopBar in render_workspace_view.

**Commit:** `refactor(ui): TopBar reads StatusCounts from model`

---

## Phase 2: TopBar as Independent Entity

### Task 2.1: TopBar implements Render and becomes Entity

**Files:**
- Modify: `src/ui/topbar.rs`
- Modify: `src/ui/app_root.rs`

**Steps:**
1. TopBar must be constructible with `cx.new()`. It needs `Entity<StatusCountsModel>` and callbacks. Use `cx.observe(&status_counts_model, |topbar, model, cx| { topbar.recompute(); cx.notify(); }).detach()` in `new` (or in a `build` that receives cx).
2. TopBar: `impl Render for TopBar` (replace RenderOnce if present). Store `Entity<StatusCountsModel>` and clone of callbacks.
3. AppRoot: in `ensure_entities` (when has_workspaces), `cx.new(|cx| TopBar::new_entity(status_counts_model.clone(), callbacks...))` → `topbar_entity`.
4. render_workspace_view: `div().child(topbar_entity.clone())` instead of `TopBar::new(...).into_element()`.
5. Ensure TopBar Entity is created once per workspace/session, not every render.

**Commit:** `feat(ui): TopBar as Entity with observe on StatusCountsModel`

---

### Task 2.2: Sidebar as Entity (optional, same pattern)

**Files:**
- Modify: `src/ui/models/worktree_list_model.rs` (create)
- Modify: `src/ui/sidebar.rs`
- Modify: `src/ui/app_root.rs`

**Steps:**
1. Create WorktreeListModel: worktrees, selected_index, pane_statuses. Expose update methods, no Render.
2. Sidebar as Entity: observe WorktreeListModel, implement Render.
3. AppRoot creates `sidebar_entity`, renders `div().child(sidebar_entity)`.
4. Worktree select/switch updates WorktreeListModel and notifies; Sidebar observer runs and notifies self.

**Commit:** `feat(ui): Sidebar as Entity with WorktreeListModel`

---

## Phase 3: NotificationPanel and Dialogs

### Task 3.1: NotificationPanel as Entity

**Files:**
- Create: `src/ui/models/notification_panel_model.rs` (show: bool, notifications: Vec)
- Modify: `src/ui/notification_panel.rs`
- Modify: `src/ui/app_root.rs`

**Steps:**
1. NotificationPanelModel: `show_panel: bool`, `unread_count: usize`, methods to toggle and update.
2. NotificationPanel Entity observes it; render only when `show_panel` or list changed.
3. TopBar's bell icon observes same model for unread count; toggle callback updates model, not AppRoot.

**Commit:** `feat(ui): NotificationPanel as Entity`

---

### Task 3.2: NewBranchDialog as Entity

**Files:**
- Modify: `src/ui/new_branch_dialog_ui.rs`
- Modify: `src/ui/app_root.rs`

**Steps:**
1. NewBranchDialogModel: `is_open: bool`, `worktrees: Vec<WorktreeInfo>`. Open/close/update methods.
2. NewBranchDialog Entity observes it; renders modal when `is_open`.
3. AppRoot no longer holds `new_branch_dialog` state; delegates to model.

**Commit:** `feat(ui): NewBranchDialog as Entity`

---

## Phase 4: TerminalView Fine-Grained Notify

### Task 4.1: TerminalView Entity and scoped notify

**Files:**
- Modify: `src/ui/app_root.rs` (setup_local_terminal, setup_pane_terminal_output)

**Steps:**
1. Store `Entity<TerminalView>` (or per-pane entity) when creating terminal output loop.
2. In the output spawn, when `content_changed`: call `terminal_view_entity.update(cx, |_, cx| cx.notify())` instead of `app_root_entity.update`.
3. Requires passing `terminal_view_entity` (or a channel to request notify) into the spawn. May need `Arc<Mutex<Option<Entity<TerminalView>>>>` or similar for late binding.
4. Verify: terminal typing causes only TerminalView re-render, not full AppRoot.

**Commit:** `feat(ui): TerminalView scoped notify on content change`

---

## Execution Order

| Phase | Tasks | Depends On | Est. |
|-------|-------|------------|------|
| 0 | 0.1 | — | 0.5 day |
| 1 | 1.1, 1.2 | 0 | 0.5 day |
| 2 | 2.1, 2.2 | 1 | 1 day |
| 3 | 3.1, 3.2 | 2 | 0.5 day |
| 4 | 4.1 | 2 | 0.5 day |

**Total:** ~3 days.

---

## Implementation Status (2026-02-28)

| Phase | Status | Notes |
|-------|--------|-------|
| 0 | ✅ Done | StatusCountsModel, TopBarEntity spike |
| 1 | ✅ Done | Model aggregation, TopBar reads from model |
| 2 | ✅ Done | TopBar as Entity; Sidebar as Entity skipped |
| 3.1 | ✅ Done | NotificationPanelModel + NotificationPanelEntity |
| 3.2 | ✅ Done | NewBranchDialogModel + NewBranchDialogEntity |
| 4 | ✅ Done | TerminalAreaEntity; spawn notifies entity instead of AppRoot |

**Cleanup done:** Removed unused `status_change_tx` from AppRoot.

---

## Rollback / Fallback

If Phase 0 spike shows GPUI does **not** support entity-scoped re-render:
- Pivot to `docs/plans/2026-02-28-ui-performance-ultimate.md` Phase 3 **方案 B** (条件渲染 + 轻量 render 路径).
- Keep StatusCountsModel as a cleaner data layer even if full entity split is not feasible.

---

## Reference

- [2026-02-28-route-b-zed-style-entity-split-brainstorm.md](2026-02-28-route-b-zed-style-entity-split-brainstorm.md)
- [GPUI Ownership — Zed Blog](https://zed.dev/blog/gpui-ownership)
- [2026-02-28-ui-performance-ultimate.md](2026-02-28-ui-performance-ultimate.md) Phase 3
