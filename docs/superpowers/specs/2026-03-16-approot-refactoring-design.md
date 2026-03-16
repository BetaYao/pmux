# AppRoot Refactoring Design

## Overview

Refactor the monolithic `AppRoot` (6809 lines, 143 fields, 84 methods) into 5 focused GPUI Entities (AppRoot retains render()), plus perform 5 additional improvements: merge StatusPublisher duplicated methods, unify status display logic, reduce render-path clones, replace Mutex with RwLock where read-heavy, and add collection eviction on workspace switch.

## Motivation

- **AppRoot is a God Object** — all UI state, runtime management, terminal handling, dialog state, notifications, and split pane logic live in one struct
- **SIGBUS on test compilation** — gpui_macros proc-macro stack overflows parsing the 6809-line file, blocking `cargo test`
- **Performance** — 367 `.clone()` calls in hot paths, 225+ Mutex lock acquisitions, unbounded HashMap growth
- **Maintainability** — impossible to test UI independently of runtime, status display logic duplicated in sidebar.rs

## Approach: Impact-First, Bottom-Up

Strategy B (impact-first) with approach A (bottom-up execution): tackle AppRoot first since it's the root cause, but extract components starting from the most independent.

## Pre-Refactoring: #6 + #2

Before touching AppRoot, two independent low-risk changes:

### #6 Merge StatusPublisher Duplicated Methods

`check_status()` and `force_status()` share 95% identical logic. Extract shared logic into a private `publish_status_change()` method:

```rust
impl StatusPublisher {
    fn publish_status_change(
        &self,
        pane_id: &str,
        new_status: AgentStatus,
        content: &str,
        skip_patterns: &[String],
    ) -> bool {
        // Shared: tracker lock, prev_status, update, publish event, notify
    }

    pub fn check_status(&self, pane_id: &str, process_status: ProcessStatus,
                        shell_info: Option<ShellPhaseInfo>, content: &str,
                        skip_patterns: &[String]) -> bool {
        let new_status = self.detector.detect(process_status, shell_info, content);
        self.publish_status_change(pane_id, new_status, content, skip_patterns)
    }

    pub fn force_status(&self, pane_id: &str, status: AgentStatus,
                        content: &str, skip_patterns: &[String]) -> bool {
        self.publish_status_change(pane_id, status, content, skip_patterns)
    }
}
```

Public API unchanged. Verified by existing 7 tests in `status_publisher::tests`.

### #2 Unify Status Display

Add `gpui_color()` to `AgentStatus`:

```rust
impl AgentStatus {
    pub fn gpui_color(&self) -> gpui::Rgba {
        let (r, g, b) = self.rgb_color();
        gpui::rgba(((r as u32) << 24) | ((g as u32) << 16) | ((b as u32) << 8) | 0xff)
    }
}
```

Delete duplicated `status_color()` implementation in sidebar.rs. All call `status.gpui_color()`.

## AppRoot Decomposition

### Component Breakdown

| Entity | Fields (~) | Responsibility |
|--------|-----------|----------------|
| **DialogManager** | ~13 | All modal dialogs: new branch, delete worktree, close tab, settings |
| **NotificationCenter** | ~6 | Notification management, panel state, notification jump |
| **RuntimeManager** | ~15 | Runtime lifecycle, EventBus, StatusPublisher, pane_statuses, hooks |
| **TerminalManager** | ~12 | Terminal buffers, resize, focus, IME, output processing loop, search |
| **SplitPaneManager** | ~8 | Split layout tree, pane focus, divider drag |

No separate LayoutManager — AppRoot retains `render()` as a slim layout compositor after Phases 1-5 reduce it to ~25 fields. A 25-field AppRoot composing 5 child entities is maintainable; a separate LayoutManager would cause re-render amplification (any child entity change → LayoutManager re-renders all children).

### Slim AppRoot (~25 fields)

```rust
pub struct AppRoot {
    state: AppState,
    workspace_manager: WorkspaceManager,

    // Child Entity references
    runtime_mgr: Entity<RuntimeManager>,
    terminal_mgr: Entity<TerminalManager>,
    split_pane_mgr: Entity<SplitPaneManager>,
    dialog_mgr: Entity<DialogManager>,
    notification_center: Entity<NotificationCenter>,

    // Workspace/worktree state
    cached_worktrees: Vec<WorktreeInfo>,
    cached_worktrees_repo: Option<PathBuf>,
    cached_tmux_windows: Option<(PathBuf, Vec<String>)>,
    worktree_to_repo_map: HashMap<PathBuf, PathBuf>,
    active_worktree_index: Option<usize>,
    worktree_switch_loading: Option<usize>,
    pending_worktree_selection: Option<usize>,
    dependency_check: Option<DependencyCheckResult>,
    update_available: Option<UpdateInfo>,
    update_downloading: bool,
    window_focused_shared: Arc<AtomicBool>,
    was_window_focused: bool,
    last_input_time: Arc<Mutex<Instant>>,

    // Sidebar state (remains in AppRoot, passed to Sidebar component on render)
    sidebar_visible: bool,
    sidebar_width: u32,
    sidebar_context_menu: Option<(usize, f32, f32)>,
    terminal_context_menu: Option<(f32, f32)>,

    // Diff view
    diff_view_entity: Option<Entity<DiffViewOverlay>>,
}
```

### Communication Topology (Entity observe)

All communication uses GPUI Entity + `cx.observe()` pattern (already established in codebase with TopBarEntity, NotificationPanelEntity, etc.):

```
RuntimeManager
    +-- publishes -> StatusCountsModel (TopBarEntity observes)
    +-- publishes -> PaneSummaryModel (Sidebar reads)
    +-- publishes -> NotificationCenter (notification events)

TerminalManager
    +-- observes RuntimeManager (runtime reference)
    +-- notifies -> TerminalAreaEntity (terminal content changes)

SplitPaneManager
    +-- observes TerminalManager (buffers reference)
    +-- holds split_tree, focus state

DialogManager
    +-- receives workspace info via setter methods (no observe on AppRoot)
    +-- callbacks -> AppRoot (create branch, delete worktree)

NotificationCenter
    +-- observes RuntimeManager (notification events)
    +-- callbacks -> AppRoot (click-to-jump pane)
```

### AppRoot Retains render()

AppRoot keeps its `Render` impl as a slim layout compositor. After Phases 1-5, render() only composes child entity views — no business logic:

```rust
impl Render for AppRoot {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Compose: sidebar + tabbar + terminal area + dialog overlays + notifications
        // Each child entity handles its own rendering via Entity<T> Render impl
    }
}
```

### Input Routing

AppRoot retains `handle_key_down()` as a dispatcher. After decomposition it becomes a routing table:

```rust
fn handle_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
    // 1. Dialog intercept (Escape, Enter when dialog open)
    if self.dialog_mgr.read(cx).is_any_open() {
        cx.update_entity(&self.dialog_mgr, |d, cx| d.handle_key(event, cx));
        return;
    }
    // 2. Search mode (when search bar active)
    if self.terminal_mgr.read(cx).is_search_active() {
        cx.update_entity(&self.terminal_mgr, |t, cx| t.handle_search_key(event, cx));
        return;
    }
    // 3. Keyboard shortcuts (Cmd+B sidebar, Cmd+\ split, etc.)
    if self.handle_shortcut(event, window, cx) { return; }
    // 4. Terminal input passthrough
    cx.update_entity(&self.terminal_mgr, |t, cx| t.send_key(event, cx));
}
```

## Extraction Order

### Phase 1: DialogManager

Extract: new_branch_dialog_model/entity, delete_worktree_dialog, close_tab_dialog, settings_*, dialog_input_focus

Rationale: Zero coupling with other responsibilities. Dialogs are independent state machines.

Key interface:
- `open_new_branch()`, `open_delete_worktree()`, `open_close_tab()`, `open_settings()`
- `close_all()`, `is_any_open()`
- Render methods for each dialog overlay

### Phase 2: NotificationCenter

Extract: notification_manager, notification_panel_model/entity, pending_notification_jump

Rationale: Only depends on EventBus (notification events) and AppRoot pane-jump callback.

Key interface:
- `add()`, `toggle_panel()`, `check_pending_jump()`

### Phase 3: RuntimeManager

Extract: runtime, event_bus, status_publisher, session_scanner, pane_statuses, pane_index, hook_handler, status_key_base, status_counts, modal_overlay_open, status_counts_model, topbar_entity, pane_summary_model

Rationale: Most important extraction. Separates runtime lifecycle from UI.

Key interface:
- `start_runtime()`, `stop_runtime()`, `runtime()`, `event_bus()`
- `pane_statuses()`, `status_counts()`, `set_modal_overlay_open()`

### Phase 4: TerminalManager

Extract: terminal_buffers, terminal_focus, resize_controller, preferred_terminal_dims, shared_terminal_dims, ime_pending_enter, terminal_needs_focus, terminal_area_entity, search_active, search_query, search_current_match

Key interface:
- `setup_terminal_output()`, `focus_terminal()`, `send_input()`
- `toggle_search()`, `resize()`

### Phase 5: SplitPaneManager

Extract: split_tree, focused_pane_index, split_divider_drag, active_pane_target, active_pane_target_shared, pane_targets_shared, split_dragging

Key interface:
- `split_vertical()`, `split_horizontal()`, `close_pane()`, `focus_pane()`

### Phase 6: AppRoot render() cleanup

Refactor AppRoot's render() method to be a slim compositor that only composes child entity views. Extract any remaining business logic from render() into the appropriate Manager. Target: render() under 100 lines.

## Performance Optimizations (During Extraction)

### #3 Reduce Render-Path Clones

Applied per-Manager as each is extracted:

| Manager | Optimization |
|---------|-------------|
| DialogManager | settings_draft cloned once on open, dropped on close |
| NotificationCenter | Render reads notification list via lock().map(), no Vec clone |
| RuntimeManager | TopBar/Sidebar get cached copies via Model observe, no per-frame HashMap lock+clone |
| TerminalManager | terminal_buffers Arc cloned once at setup, render uses TerminalAreaEntity reference |
| SplitPaneManager | active_pane_target changed to SharedString |

Target: max 5 `.clone()` calls per Manager's render method.

### #4 Mutex to RwLock

Replace during extraction:

| Field | Mutex -> RwLock | Reason |
|-------|----------------|--------|
| pane_statuses | Yes | Read every frame, written on status change |
| terminal_buffers | Yes | Read every frame, written on output |
| pane_targets_shared | Yes | Read every frame, written on pane switch |
| active_pane_target_shared | Yes | Read every frame, written on focus change |
| notification_manager | No (keep Mutex) | Write-heavy (notifications arrive frequently) |
| last_input_time | No (keep Mutex) | Lightweight, no benefit |

Use `parking_lot::RwLock` (already in Cargo.toml dependencies). `std::sync::RwLock` on macOS can cause writer starvation under high read contention (render reads at ~60Hz, background tasks write on terminal output). `parking_lot::RwLock` is fair and non-poisoning.

### #5 Collection Eviction

Event-driven cleanup, not LRU (active pane count is bounded, typically <20):

- **Workspace switch**: RuntimeManager.stop_runtime() clears pane_statuses for old workspace; TerminalManager.cleanup_buffers_for_workspace() clears terminal_buffers
- **Worktree delete**: Clean up corresponding entries
- **Tab close**: Clean up corresponding workspace's data

## Testing Strategy

### Non-UI Modules (cargo test)

| Module | Tests |
|--------|-------|
| status_publisher.rs | Existing 7 tests verify #6 merge |
| agent_status.rs | Existing 15 tests + new gpui_color() test for #2 |
| RuntimeManager | register/unregister pane, status propagation, cleanup on stop |
| SplitPaneManager | split/close/focus operations, tree structure |
| NotificationCenter | add/toggle/check_pending_jump |

### UI Modules (layered verification)

- **Phase 1-3**: `cargo check` + non-Render unit tests
- **Phase 4-5**: app_root.rs should shrink below ~2000 lines, SIGBUS likely resolves -> switch to `cargo test` full suite
- **Phase 6**: Full regression, performance, and functional tests
- **SIGBUS fallback**: If SIGBUS persists after Phase 5, investigate whether the issue is file size or type complexity in proc-macro expansion. Diagnostic: create a minimal reproduction by commenting out sections of AppRoot to find the threshold.

### Performance Verification

After each Phase:
- `cargo build --release` to confirm no compile-time regression
- Record at which Phase SIGBUS disappears (validates "file too large for proc-macro" hypothesis)

Post Phase 6:
- Compare pre/post `cargo check` compile time
- Compare release binary size

### Integration Tests (Post Phase 6)

```rust
// tests/integration/
test_workspace_switch_cleans_up_buffers()
test_status_change_propagates_to_topbar()
test_dialog_open_close_lifecycle()
test_split_pane_operations()
test_notification_jump_to_pane()
test_keyboard_input_routing_after_decomposition()
test_ime_composition_across_managers()
test_clipboard_paste_runtime_terminal()
test_window_focus_notification_jump()
```

## Success Criteria

1. AppRoot < 500 lines, holding workspace/worktree state + sidebar state + child Entity references + slim render() + input routing dispatcher
2. `cargo test` passes (SIGBUS resolved)
3. Each Manager is independently testable
4. No status display duplication across files
5. Render-path clones reduced to <= 5 per Manager
6. Read-heavy collections use RwLock
7. terminal_buffers and pane_statuses cleaned up on workspace switch
8. All existing UI functionality preserved
