# AppRoot Refactoring Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Decompose the 6809-line AppRoot God Object into 5 focused GPUI Entities, merge StatusPublisher duplication, unify status display, and apply performance optimizations.

**Architecture:** Bottom-up extraction starting with the most independent components (DialogManager, NotificationCenter) before the more coupled ones (RuntimeManager, TerminalManager, SplitPaneManager). AppRoot retains render() as a slim layout compositor. All communication uses GPUI Entity + cx.observe() pattern.

**Tech Stack:** Rust, GPUI framework, parking_lot::RwLock, Arc/Mutex patterns

**Design Spec:** `docs/superpowers/specs/2026-03-16-approot-refactoring-design.md`

---

## Chunk 1: Pre-Refactoring (#6 StatusPublisher Merge + #2 Status Display Unification)

### Task 1: Merge StatusPublisher Duplicated Methods

**Files:**
- Modify: `src/runtime/status_publisher.rs`

- [ ] **Step 1: Write test verifying force_status behavior is preserved**

Add test in `src/runtime/status_publisher.rs` under `#[cfg(test)] mod tests`:

```rust
#[test]
fn test_force_status_publishes_same_events_as_before() {
    let bus = Arc::new(EventBus::new(32));
    let rx = bus.subscribe();
    let pub_ = StatusPublisher::new(Arc::clone(&bus));
    pub_.register_pane("pane-1");

    // Force to Running
    let changed = pub_.force_status("pane-1", AgentStatus::Running, "some output", &[]);
    assert!(changed);
    assert_eq!(pub_.current_status("pane-1"), AgentStatus::Running);

    // Drain events
    let mut state_change_count = 0;
    while let Ok(ev) = rx.try_recv() {
        if let RuntimeEvent::AgentStateChange(sc) = ev {
            assert_eq!(sc.state, AgentStatus::Running);
            state_change_count += 1;
        }
    }
    assert_eq!(state_change_count, 1);
}
```

- [ ] **Step 2: Run test to verify it passes (baseline)**

Run: `RUSTUP_TOOLCHAIN=stable cargo test test_force_status_publishes_same_events_as_before -- --nocapture`
Expected: PASS

- [ ] **Step 3: Extract shared logic into `publish_status_change()`**

Replace the bodies of `check_status()` and `force_status()` in `src/runtime/status_publisher.rs` (lines 64-220):

```rust
/// Internal: publish a status change event and optional notification.
/// Returns `true` if status changed.
fn publish_status_change(
    &self,
    pane_id: &str,
    new_status: AgentStatus,
    content: &str,
    skip_patterns: &[String],
) -> bool {
    let mut tracker_guard = match self.tracker.lock() {
        Ok(g) => g,
        Err(_) => return false,
    };

    let tracker = match tracker_guard.get_mut(pane_id) {
        Some(t) => t,
        None => return false,
    };

    let prev_status = tracker.current_status();
    let changed = tracker.update_with_status(new_status);

    if changed {
        let current_status = tracker.current_status();
        let agent_id = pane_id.split(':').next().unwrap_or(pane_id).to_string();
        let last_line = extract_last_line_filtered(content, 80, skip_patterns);
        let last_line_opt = if last_line.is_empty() {
            None
        } else {
            Some(last_line.clone())
        };

        self.event_bus
            .publish(RuntimeEvent::AgentStateChange(AgentStateChange {
                agent_id: agent_id.clone(),
                pane_id: Some(pane_id.to_string()),
                state: current_status,
                prev_state: Some(prev_status),
                last_line: last_line_opt,
            }));

        let should_notify = matches!(
            (prev_status, current_status),
            (AgentStatus::Running, AgentStatus::Idle)
                | (_, AgentStatus::Waiting)
                | (_, AgentStatus::Error)
                | (_, AgentStatus::Exited)
        );

        if should_notify {
            let notif_type = match current_status {
                AgentStatus::Error => NotificationType::Error,
                AgentStatus::Waiting => NotificationType::WaitingInput,
                AgentStatus::Idle | AgentStatus::Exited => NotificationType::Info,
                _ => return true,
            };
            let message = if last_line.is_empty() {
                current_status.display_text().to_string()
            } else {
                last_line
            };
            self.event_bus
                .publish(RuntimeEvent::Notification(Notification {
                    agent_id,
                    pane_id: Some(pane_id.to_string()),
                    message,
                    notif_type,
                }));
        }
    }

    changed
}

pub fn check_status(
    &self,
    pane_id: &str,
    process_status: ProcessStatus,
    shell_info: Option<ShellPhaseInfo>,
    content: &str,
    skip_patterns: &[String],
) -> bool {
    let new_status = self.detector.detect(process_status, shell_info, content);
    self.publish_status_change(pane_id, new_status, content, skip_patterns)
}

pub fn force_status(
    &self,
    pane_id: &str,
    status: AgentStatus,
    content: &str,
    skip_patterns: &[String],
) -> bool {
    self.publish_status_change(pane_id, status, content, skip_patterns)
}
```

- [ ] **Step 4: Run all StatusPublisher tests**

Run: `RUSTUP_TOOLCHAIN=stable cargo test status_publisher -- --nocapture`
Expected: All 8 tests PASS

- [ ] **Step 5: Run cargo check to verify no compilation errors**

Run: `RUSTUP_TOOLCHAIN=stable cargo check`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add src/runtime/status_publisher.rs
git commit -m "refactor: extract shared logic in StatusPublisher (check_status + force_status)"
```

---

### Task 2: Unify Status Display — Add gpui_color() to AgentStatus

**Files:**
- Modify: `src/agent_status.rs`

- [ ] **Step 1: Write test for gpui_color()**

Add test in `src/agent_status.rs` under `#[cfg(test)] mod tests`:

```rust
#[test]
fn test_gpui_color_matches_rgb_color() {
    // Verify gpui_color() produces correct RGBA from rgb_color()
    let status = AgentStatus::Running;
    let (r, g, b) = status.rgb_color(); // (76, 175, 80)
    let rgba = status.gpui_color();
    // gpui::rgba takes 0xRRGGBBAA as u32
    let expected = gpui::rgba(
        ((r as u32) << 24) | ((g as u32) << 16) | ((b as u32) << 8) | 0xff
    );
    assert_eq!(rgba, expected);
}
```

- [ ] **Step 2: Implement gpui_color()**

Add method to `impl AgentStatus` in `src/agent_status.rs` (after `rgb_color()`, around line 55):

```rust
/// Get the GPUI Rgba color for UI rendering.
/// Single source of truth — all UI components should use this instead of
/// manually converting rgb_color() or duplicating color hex values.
pub fn gpui_color(&self) -> gpui::Rgba {
    let (r, g, b) = self.rgb_color();
    gpui::rgba(((r as u32) << 24) | ((g as u32) << 16) | ((b as u32) << 8) | 0xff)
}
```

Add `use gpui;` to the imports if not present. The crate already depends on gpui.

- [ ] **Step 3: Run agent_status tests**

Run: `RUSTUP_TOOLCHAIN=stable cargo test agent_status -- --nocapture`
Expected: All 16 tests PASS (15 existing + 1 new)

- [ ] **Step 4: Commit**

```bash
git add src/agent_status.rs
git commit -m "feat: add AgentStatus::gpui_color() as single source of truth for UI colors"
```

---

### Task 3: Replace Sidebar's Duplicated status_color()

**Files:**
- Modify: `src/ui/sidebar.rs`

- [ ] **Step 1: Find and replace status_color(), status_icon(), status_text() in sidebar.rs**

In `src/ui/sidebar.rs`, the `WorktreeItem` struct (around line 39) has:

```rust
pub fn status_color(&self) -> Rgba {
    match self.status {
        AgentStatus::Running => rgb(0x4caf50),
        AgentStatus::Waiting => rgb(0xffc107),
        AgentStatus::Idle => rgb(0x9e9e9e),
        AgentStatus::Error => rgb(0xf44336),
        AgentStatus::Exited => rgb(0x2196f3),
        AgentStatus::Unknown => rgb(0x9c27b0),
    }
}
```

Replace with:

```rust
pub fn status_color(&self) -> Rgba {
    self.status.gpui_color()
}
```

Also check for `status_icon()` and `status_text()` methods on `WorktreeItem` that duplicate `AgentStatus::icon()` and `AgentStatus::display_text()`. If found, replace them with calls to the AgentStatus methods.

- [ ] **Step 2: Run cargo check**

Run: `RUSTUP_TOOLCHAIN=stable cargo check`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add src/ui/sidebar.rs
git commit -m "refactor: sidebar uses AgentStatus::gpui_color() instead of duplicated match"
```

---

## Chunk 2: Phase 1 — DialogManager Extraction

### Task 4: Create DialogManager struct and module

**Files:**
- Create: `src/ui/dialog_manager.rs`
- Modify: `src/ui/mod.rs` (add module declaration)

- [ ] **Step 1: Create dialog_manager.rs with struct definition**

Create `src/ui/dialog_manager.rs`:

```rust
//! DialogManager - manages all modal dialogs (new branch, delete worktree, close tab, settings)
//!
//! Extracted from AppRoot Phase 1 to reduce God Object complexity.
//! Communication: receives workspace info via setter methods, callbacks to AppRoot via closures.

use crate::config::Config;
use crate::remotes::secrets::Secrets;
use crate::ui::models::NewBranchDialogModel;
use crate::ui::new_branch_dialog_entity::NewBranchDialogEntity;
use crate::ui::close_tab_dialog_ui::CloseTabDialogUi;
use crate::ui::delete_worktree_dialog_ui::DeleteWorktreeDialogUi;
use gpui::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

pub struct DialogManager {
    // New Branch Dialog
    new_branch_dialog_model: Option<Entity<NewBranchDialogModel>>,
    new_branch_dialog_entity: Option<Entity<NewBranchDialogEntity>>,
    dialog_input_focus: Option<FocusHandle>,

    // Delete Worktree Dialog
    delete_worktree_dialog: DeleteWorktreeDialogUi,

    // Close Tab Dialog
    close_tab_dialog: CloseTabDialogUi,

    // Settings Modal
    show_settings: bool,
    settings_draft: Option<Config>,
    settings_secrets_draft: Option<Secrets>,
    settings_configuring_channel: Option<String>,
    settings_editing_agent: Option<usize>,
    settings_tab: String,
    settings_focus: Option<FocusHandle>,
    settings_focused_field: Option<String>,

    // Shared flag: when any modal is open, terminal output loop skips notifying
    modal_overlay_open: Arc<AtomicBool>,
}

impl DialogManager {
    pub fn new(modal_overlay_open: Arc<AtomicBool>) -> Self {
        Self {
            new_branch_dialog_model: None,
            new_branch_dialog_entity: None,
            dialog_input_focus: None,
            delete_worktree_dialog: DeleteWorktreeDialogUi::new(),
            close_tab_dialog: CloseTabDialogUi::new(),
            show_settings: false,
            settings_draft: None,
            settings_secrets_draft: None,
            settings_configuring_channel: None,
            settings_editing_agent: None,
            settings_tab: "channels".to_string(),
            settings_focus: None,
            settings_focused_field: None,
            modal_overlay_open,
        }
    }

    /// Returns true if any modal dialog is currently open.
    pub fn is_any_open(&self, cx: &App) -> bool {
        let new_branch_open = self.new_branch_dialog_model
            .as_ref()
            .map_or(false, |m| m.read(cx).is_open);
        self.show_settings || new_branch_open
            || self.delete_worktree_dialog.is_open()
            || self.close_tab_dialog.is_open()
    }

    /// Check if settings modal is open
    pub fn is_settings_open(&self) -> bool {
        self.show_settings
    }

    /// Check if new branch dialog is open
    pub fn is_new_branch_open(&self, cx: &App) -> bool {
        self.new_branch_dialog_model
            .as_ref()
            .map_or(false, |m| m.read(cx).is_open)
    }

    // --- Focus handles ---

    pub fn dialog_input_focus(&self) -> Option<&FocusHandle> {
        self.dialog_input_focus.as_ref()
    }

    pub fn settings_focus(&self) -> Option<&FocusHandle> {
        self.settings_focus.as_ref()
    }

    // --- Settings ---

    pub fn open_settings(&mut self, config: Config, secrets: Secrets, cx: &mut Context<Self>) {
        self.show_settings = true;
        self.settings_draft = Some(config);
        self.settings_secrets_draft = Some(secrets);
        self.modal_overlay_open.store(true, Ordering::Relaxed);
        cx.notify();
    }

    pub fn close_settings(&mut self, cx: &mut Context<Self>) {
        self.show_settings = false;
        self.settings_draft = None;
        self.settings_secrets_draft = None;
        self.settings_configuring_channel = None;
        self.settings_editing_agent = None;
        self.settings_focused_field = None;
        self.modal_overlay_open.store(false, Ordering::Relaxed);
        cx.notify();
    }

    pub fn settings_draft(&self) -> Option<&Config> {
        self.settings_draft.as_ref()
    }

    pub fn settings_draft_mut(&mut self) -> Option<&mut Config> {
        self.settings_draft.as_mut()
    }

    pub fn settings_secrets_draft(&self) -> Option<&Secrets> {
        self.settings_secrets_draft.as_ref()
    }

    pub fn settings_secrets_draft_mut(&mut self) -> Option<&mut Secrets> {
        self.settings_secrets_draft.as_mut()
    }

    // --- New Branch Dialog ---

    pub fn new_branch_dialog_model(&self) -> Option<&Entity<NewBranchDialogModel>> {
        self.new_branch_dialog_model.as_ref()
    }

    pub fn new_branch_dialog_entity(&self) -> Option<&Entity<NewBranchDialogEntity>> {
        self.new_branch_dialog_entity.as_ref()
    }

    pub fn open_new_branch_dialog(&mut self, cx: &mut Context<Self>) {
        if let Some(ref model) = self.new_branch_dialog_model {
            let _ = cx.update_entity(model, |m, cx| {
                m.open();
                cx.notify();
            });
        }
        self.modal_overlay_open.store(true, Ordering::Relaxed);
        cx.notify();
    }

    pub fn close_new_branch_dialog(&mut self, cx: &mut Context<Self>) {
        if let Some(ref model) = self.new_branch_dialog_model {
            let _ = cx.update_entity(model, |m, cx| {
                m.close();
                cx.notify();
            });
        }
        self.modal_overlay_open.store(false, Ordering::Relaxed);
        cx.notify();
    }

    // --- Delete Worktree Dialog ---

    pub fn delete_worktree_dialog(&self) -> &DeleteWorktreeDialogUi {
        &self.delete_worktree_dialog
    }

    pub fn delete_worktree_dialog_mut(&mut self) -> &mut DeleteWorktreeDialogUi {
        &mut self.delete_worktree_dialog
    }

    pub fn close_delete_dialog(&mut self, cx: &mut Context<Self>) {
        self.delete_worktree_dialog.close();
        cx.notify();
    }

    // --- Close Tab Dialog ---

    pub fn close_tab_dialog(&self) -> &CloseTabDialogUi {
        &self.close_tab_dialog
    }

    pub fn close_tab_dialog_mut(&mut self) -> &mut CloseTabDialogUi {
        &mut self.close_tab_dialog
    }

    pub fn close_close_tab_dialog(&mut self, cx: &mut Context<Self>) {
        self.close_tab_dialog.close();
        cx.notify();
    }

    pub fn toggle_close_tab_kill_tmux(&mut self, cx: &mut Context<Self>) {
        self.close_tab_dialog.toggle_kill_tmux();
        cx.notify();
    }

    // --- Ensure focus handles are created ---

    pub fn ensure_focus_handles(&mut self, cx: &mut Context<Self>) {
        if self.dialog_input_focus.is_none() {
            self.dialog_input_focus = Some(cx.focus_handle());
        }
        if self.settings_focus.is_none() {
            self.settings_focus = Some(cx.focus_handle());
        }
    }
}
```

- [ ] **Step 2: Add module declaration to ui/mod.rs**

In `src/ui/mod.rs`, add:

```rust
pub mod dialog_manager;
```

- [ ] **Step 3: Run cargo check**

Run: `RUSTUP_TOOLCHAIN=stable cargo check`
Expected: PASS (struct compiles, no consumers yet)

- [ ] **Step 4: Commit**

```bash
git add src/ui/dialog_manager.rs src/ui/mod.rs
git commit -m "feat: add DialogManager struct (Phase 1 extraction skeleton)"
```

---

### Task 5: Wire DialogManager into AppRoot

**Files:**
- Modify: `src/ui/app_root.rs`

This is the largest task — move dialog/settings fields from AppRoot to DialogManager Entity, update all references.

- [ ] **Step 1: Add DialogManager Entity field to AppRoot**

In `src/ui/app_root.rs`, add to AppRoot struct (around line 290):

```rust
dialog_mgr: Option<Entity<crate::ui::dialog_manager::DialogManager>>,
```

Initialize in `new()` (around line 580):

```rust
dialog_mgr: None,
```

- [ ] **Step 2: Create DialogManager Entity in ensure_entities()**

In `ensure_entities()` (around line 635), add DialogManager creation before other dialog entities:

```rust
if self.dialog_mgr.is_none() {
    let modal_flag = self.modal_overlay_open.clone();
    let dialog_mgr = cx.new(|cx| {
        let mut mgr = crate::ui::dialog_manager::DialogManager::new(modal_flag);
        mgr.ensure_focus_handles(cx);
        mgr
    });
    self.dialog_mgr = Some(dialog_mgr);
}
```

- [ ] **Step 3: Migrate dialog fields from AppRoot to DialogManager**

Remove from AppRoot struct: `new_branch_dialog_model`, `new_branch_dialog_entity`, `dialog_input_focus`, `delete_worktree_dialog`, `close_tab_dialog`, `show_settings`, `settings_draft`, `settings_secrets_draft`, `settings_configuring_channel`, `settings_editing_agent`, `settings_tab`, `settings_focus`, `settings_focused_field`.

Update ALL references throughout `app_root.rs` to use `self.dialog_mgr.as_ref().unwrap()` pattern:

- `self.show_settings` → `self.dialog_mgr.as_ref().map_or(false, |d| d.read(cx).is_settings_open())`
- `self.settings_draft` → `cx.update_entity(dialog_mgr, |d, cx| d.settings_draft_mut()...)`
- Dialog open/close methods → delegate to `cx.update_entity(&self.dialog_mgr.unwrap(), |d, cx| d.open_settings(...))`

This step requires updating many call sites. The key patterns:

**Reading dialog state (in render):**
```rust
// Before:
let settings_open = self.show_settings;
// After:
let settings_open = self.dialog_mgr.as_ref()
    .map_or(false, |d| d.read(cx).is_settings_open());
```

**Mutating dialog state (in event handlers):**
```rust
// Before:
self.show_settings = true;
self.settings_draft = Some(Config::load());
// After:
if let Some(ref dialog_mgr) = self.dialog_mgr {
    cx.update_entity(dialog_mgr, |d, cx| {
        d.open_settings(Config::load(), Secrets::load(), cx);
    });
}
```

**Rendering dialog entities:**
```rust
// Before:
.when(self.new_branch_dialog_entity.is_some(), |el| {
    el.child(self.new_branch_dialog_entity.as_ref().unwrap().clone())
})
// After:
.when_some(self.dialog_mgr.as_ref().and_then(|d| d.read(cx).new_branch_dialog_entity().cloned()), |el, entity| {
    el.child(entity)
})
```

- [ ] **Step 4: Move settings render methods to DialogManager**

Move settings render methods from AppRoot to DialogManager. Find these by searching for `render_settings` and `settings_channel_card` in app_root.rs:
- `render_settings_modal()` — the main settings overlay
- `render_settings_channels_tab()` — channel configuration cards
- `render_settings_agent_detect_tab()` — agent detection rules
- `settings_channel_card_el()` — individual channel card
- Settings agent card render methods (summary + edit views)
- `render_settings_config_guide()` — configuration guide panel

**Important:** These methods currently use `cx: &mut Context<AppRoot>`. When moved to DialogManager, update signatures to `cx: &mut Context<DialogManager>`.

**Cross-cutting dependencies:** These render methods access `self.settings_draft`, `self.settings_secrets_draft`, etc. — all moving to DialogManager, so no external dependency. If any method references `self.workspace_manager` or `self.state`, pass that data via a setter method (e.g., `set_workspace_info()`) before render, or pass it as a parameter.

DialogManager needs a `Render` impl. It renders the settings modal when open; other dialogs (delete worktree, close tab) remain simple components rendered by AppRoot because they are stateless UI builders.

```rust
impl Render for DialogManager {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.show_settings {
            div().child(self.render_settings_modal(cx))
        } else {
            div()
        }
    }
}
```

**Note:** `diff_view` files also have a local variable named `status_color` but those map `FileChangeStatus` (Added/Modified/Deleted) — a different domain, not agent status. They are out of scope for this refactoring.

- [ ] **Step 5: Run cargo check**

Run: `RUSTUP_TOOLCHAIN=stable cargo check`
Expected: PASS

- [ ] **Step 6: Check AppRoot line count reduction**

Run: `wc -l src/ui/app_root.rs`
Expected: Reduction of ~1000-1500 lines from the settings render methods alone.

- [ ] **Step 7: Also check for app_root_test.rs**

If `src/ui/app_root_test.rs` exists, update any tests that reference migrated fields. Run: `RUSTUP_TOOLCHAIN=stable cargo check --tests`

- [ ] **Step 8: Commit**

```bash
git add src/ui/app_root.rs src/ui/dialog_manager.rs
git commit -m "refactor: extract DialogManager from AppRoot (Phase 1)"
```

---

## Chunk 3: Phase 2 — NotificationCenter Extraction

### Task 6: Create NotificationCenter struct

**Files:**
- Create: `src/ui/notification_center.rs`
- Modify: `src/ui/mod.rs`

- [ ] **Step 1: Create notification_center.rs with struct definition**

```rust
//! NotificationCenter - manages notifications, panel state, and notification jump
//!
//! Extracted from AppRoot Phase 2.
//! Observes RuntimeManager for notification events.
//! Callbacks to AppRoot for pane jump navigation.

use crate::notification::NotificationType;
use crate::notification_manager::NotificationManager;
use crate::ui::models::NotificationPanelModel;
use crate::ui::notification_panel_entity::NotificationPanelEntity;
use gpui::*;
use std::sync::{Arc, Mutex};

pub struct NotificationCenter {
    manager: Arc<Mutex<NotificationManager>>,
    panel_model: Option<Entity<NotificationPanelModel>>,
    panel_entity: Option<Entity<NotificationPanelEntity>>,
    /// Pending notification jump: (pane_id, timestamp).
    /// Set when system notification sent; consumed on window focus transition.
    pending_jump: Arc<Mutex<Option<(String, std::time::Instant)>>>,
}

impl NotificationCenter {
    pub fn new() -> Self {
        Self {
            manager: Arc::new(Mutex::new(NotificationManager::new())),
            panel_model: None,
            panel_entity: None,
            pending_jump: Arc::new(Mutex::new(None)),
        }
    }

    pub fn manager(&self) -> Arc<Mutex<NotificationManager>> {
        self.manager.clone()
    }

    pub fn pending_jump(&self) -> Arc<Mutex<Option<(String, std::time::Instant)>>> {
        self.pending_jump.clone()
    }

    pub fn panel_model(&self) -> Option<&Entity<NotificationPanelModel>> {
        self.panel_model.as_ref()
    }

    pub fn panel_entity(&self) -> Option<&Entity<NotificationPanelEntity>> {
        self.panel_entity.as_ref()
    }

    pub fn set_panel_model(&mut self, model: Entity<NotificationPanelModel>) {
        self.panel_model = Some(model);
    }

    pub fn set_panel_entity(&mut self, entity: Entity<NotificationPanelEntity>) {
        self.panel_entity = Some(entity);
    }

    /// Add a notification to the manager.
    pub fn add(&self, pane_id: &str, notif_type: NotificationType, message: &str) {
        if let Ok(mut mgr) = self.manager.lock() {
            mgr.add(pane_id, notif_type, message);
        }
    }

    /// Add a labeled notification. Returns true if notification was actually added (not duplicate).
    pub fn add_labeled(&self, pane_id: &str, notif_type: NotificationType, message: &str, label: String) -> bool {
        if let Ok(mut mgr) = self.manager.lock() {
            mgr.add_labeled(pane_id, notif_type, message, label)
        } else {
            false
        }
    }

    /// Toggle notification panel visibility.
    pub fn toggle_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(ref model) = self.panel_model {
            let _ = cx.update_entity(model, |m, cx| {
                m.toggle_panel();
                cx.notify();
            });
        }
        cx.notify();
    }

    /// Update unread count in panel model.
    pub fn update_unread_count(&self, cx: &mut App) {
        let unread = self.manager.lock().map(|m| m.unread_count()).unwrap_or(0);
        if let Some(ref model) = self.panel_model {
            let _ = cx.update_entity(model, |m, cx| {
                m.set_unread_count(unread);
                cx.notify();
            });
        }
    }

    /// Set pending jump target (called when system notification sent).
    pub fn set_pending_jump(&self, pane_id: String) {
        if let Ok(mut pending) = self.pending_jump.lock() {
            *pending = Some((pane_id, std::time::Instant::now()));
        }
    }

    /// Check and consume pending jump target. Returns pane_id if valid (within 30s).
    pub fn take_pending_jump(&self) -> Option<String> {
        if let Ok(mut pending) = self.pending_jump.lock() {
            if let Some((ref pane_id, ref ts)) = *pending {
                if ts.elapsed() < std::time::Duration::from_secs(30) {
                    let target = pane_id.clone();
                    *pending = None;
                    return Some(target);
                }
                *pending = None;
            }
        }
        None
    }

    /// Get unread count for display.
    pub fn unread_count(&self) -> usize {
        self.manager.lock().map(|m| m.unread_count()).unwrap_or(0)
    }

    /// Check if notification panel is open.
    pub fn is_panel_open(&self, cx: &App) -> bool {
        self.panel_model
            .as_ref()
            .map_or(false, |m| m.read(cx).show_panel)
    }
}
```

- [ ] **Step 2: Add module declaration**

In `src/ui/mod.rs`:

```rust
pub mod notification_center;
```

- [ ] **Step 3: Run cargo check**

Run: `RUSTUP_TOOLCHAIN=stable cargo check`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src/ui/notification_center.rs src/ui/mod.rs
git commit -m "feat: add NotificationCenter struct (Phase 2 extraction skeleton)"
```

---

### Task 7: Wire NotificationCenter into AppRoot

**Files:**
- Modify: `src/ui/app_root.rs`

- [ ] **Step 1: Add NotificationCenter Entity field to AppRoot**

Add to AppRoot struct:

```rust
notification_center: Option<Entity<crate::ui::notification_center::NotificationCenter>>,
```

Initialize in `new()`:

```rust
notification_center: None,
```

- [ ] **Step 2: Create NotificationCenter Entity in ensure_entities()**

Add before notification panel entity creation:

```rust
if self.notification_center.is_none() {
    let nc = cx.new(|_cx| crate::ui::notification_center::NotificationCenter::new());
    self.notification_center = Some(nc);
}
```

- [ ] **Step 3: Migrate notification fields from AppRoot**

Remove from AppRoot struct: `notification_manager`, `notification_panel_model`, `notification_panel_entity`, `pending_notification_jump`.

Update references:

**Event bus subscription (lines 2998-3040):**
```rust
// Before:
if let Ok(mut mgr) = notification_manager.lock() {
    if mgr.add_labeled(pane_id, notif_type, &message, source_label) { ... }
}
// After:
let nc = notification_center.clone();
cx.update_entity(&nc, |nc, cx| {
    if nc.add_labeled(pane_id, notif_type, &message, source_label) { ... }
});
```

**Notification jump (lines 6640-6682):**
```rust
// Before:
let jump_target = self.pending_notification_jump.lock()...
// After:
let jump_target = self.notification_center.as_ref()
    .and_then(|nc| nc.read(cx).take_pending_jump());
```

**Toggle panel (keyboard "i"):**
```rust
// Before:
if let Some(ref model) = self.notification_panel_model { ... }
// After:
if let Some(ref nc) = self.notification_center {
    cx.update_entity(nc, |nc, cx| nc.toggle_panel(cx));
}
```

**Render panel:**
```rust
// Before:
.when(self.notification_panel_entity.is_some(), |el| { ... })
// After:
.when_some(
    self.notification_center.as_ref().and_then(|nc| nc.read(cx).panel_entity().cloned()),
    |el, entity| el.child(entity)
)
```

- [ ] **Step 4: Run cargo check**

Run: `RUSTUP_TOOLCHAIN=stable cargo check`
Expected: PASS

- [ ] **Step 5: Check AppRoot line count**

Run: `wc -l src/ui/app_root.rs`

- [ ] **Step 6: Write unit test for NotificationCenter**

Add to `src/ui/notification_center.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pending_jump_set_and_take() {
        let nc = NotificationCenter::new();
        assert!(nc.take_pending_jump().is_none());

        nc.set_pending_jump("local:/path/feat".to_string());
        assert_eq!(nc.take_pending_jump(), Some("local:/path/feat".to_string()));
        // Second take returns None (consumed)
        assert!(nc.take_pending_jump().is_none());
    }

    #[test]
    fn test_add_notification() {
        let nc = NotificationCenter::new();
        nc.add("pane-1", NotificationType::Info, "test message");
        assert_eq!(nc.unread_count(), 1);
    }
}
```

Run: `RUSTUP_TOOLCHAIN=stable cargo test notification_center -- --nocapture`
Expected: 2 tests PASS

- [ ] **Step 7: Commit**

```bash
git add src/ui/app_root.rs src/ui/notification_center.rs
git commit -m "refactor: extract NotificationCenter from AppRoot (Phase 2)"
```

---

## Chunk 4: Phase 3 — RuntimeManager Extraction

### Task 8: Create RuntimeManager struct

**Files:**
- Create: `src/ui/runtime_manager.rs`
- Modify: `src/ui/mod.rs`

- [ ] **Step 1: Create runtime_manager.rs with struct and key methods**

```rust
//! RuntimeManager - manages runtime lifecycle, EventBus, StatusPublisher, and agent status.
//!
//! Extracted from AppRoot Phase 3.
//! Publishes status changes to StatusCountsModel (TopBar) and PaneSummaryModel (Sidebar).

use crate::agent_status::{AgentStatus, StatusCounts};
use crate::runtime::{AgentRuntime, EventBus, StatusPublisher};
use crate::ui::models::{StatusCountsModel, PaneSummaryModel};
use crate::ui::topbar_entity::TopBarEntity;
use gpui::*;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};

pub struct RuntimeManager {
    runtime: Option<Arc<dyn AgentRuntime>>,
    event_bus: Arc<EventBus>,
    status_publisher: Option<StatusPublisher>,
    session_scanner: Option<crate::session_scanner::SessionScanner>,
    pane_statuses: Arc<Mutex<HashMap<String, AgentStatus>>>,
    status_counts: StatusCounts,
    status_key_base: Option<String>,
    event_bus_subscription_started: bool,
    pane_index: Option<Arc<std::sync::RwLock<crate::hooks::handler::PaneIndex>>>,
    hook_handler: Option<Arc<crate::hooks::handler::HookEventHandler>>,
    modal_overlay_open: Arc<AtomicBool>,
    // Models
    status_counts_model: Option<Entity<StatusCountsModel>>,
    topbar_entity: Option<Entity<TopBarEntity>>,
    pane_summary_model: Option<Entity<PaneSummaryModel>>,
    // Animation
    running_animation_frame: usize,
    running_animation_task: Option<gpui::Task<()>>,
}

impl RuntimeManager {
    pub fn new(event_bus: Arc<EventBus>, modal_overlay_open: Arc<AtomicBool>) -> Self {
        Self {
            runtime: None,
            event_bus,
            status_publisher: None,
            session_scanner: None,
            pane_statuses: Arc::new(Mutex::new(HashMap::new())),
            status_counts: StatusCounts::new(),
            status_key_base: None,
            event_bus_subscription_started: false,
            pane_index: None,
            hook_handler: None,
            modal_overlay_open,
            status_counts_model: None,
            topbar_entity: None,
            pane_summary_model: None,
            running_animation_frame: 0,
            running_animation_task: None,
        }
    }

    // Accessors
    pub fn runtime(&self) -> Option<Arc<dyn AgentRuntime>> { self.runtime.clone() }
    pub fn event_bus(&self) -> Arc<EventBus> { self.event_bus.clone() }
    pub fn pane_statuses(&self) -> Arc<Mutex<HashMap<String, AgentStatus>>> { self.pane_statuses.clone() }
    pub fn status_counts(&self) -> &StatusCounts { &self.status_counts }
    pub fn status_publisher(&self) -> Option<&StatusPublisher> { self.status_publisher.as_ref() }
    pub fn status_key_base(&self) -> Option<&str> { self.status_key_base.as_deref() }
    pub fn running_animation_frame(&self) -> usize { self.running_animation_frame }
    pub fn pane_summary_model(&self) -> Option<&Entity<PaneSummaryModel>> { self.pane_summary_model.as_ref() }
    pub fn status_counts_model(&self) -> Option<&Entity<StatusCountsModel>> { self.status_counts_model.as_ref() }
    pub fn topbar_entity(&self) -> Option<&Entity<TopBarEntity>> { self.topbar_entity.as_ref() }
    pub fn pane_index(&self) -> Option<Arc<std::sync::RwLock<crate::hooks::handler::PaneIndex>>> { self.pane_index.clone() }
    pub fn hook_handler(&self) -> Option<Arc<crate::hooks::handler::HookEventHandler>> { self.hook_handler.clone() }

    // Setters
    pub fn set_runtime(&mut self, rt: Option<Arc<dyn AgentRuntime>>) { self.runtime = rt; }
    pub fn set_status_publisher(&mut self, sp: Option<StatusPublisher>) { self.status_publisher = sp; }
    pub fn set_session_scanner(&mut self, sc: Option<crate::session_scanner::SessionScanner>) { self.session_scanner = sc; }
    pub fn set_status_key_base(&mut self, base: Option<String>) { self.status_key_base = base; }
    pub fn set_pane_index(&mut self, pi: Option<Arc<std::sync::RwLock<crate::hooks::handler::PaneIndex>>>) { self.pane_index = pi; }
    pub fn set_hook_handler(&mut self, hh: Option<Arc<crate::hooks::handler::HookEventHandler>>) { self.hook_handler = hh; }
    pub fn set_status_counts_model(&mut self, m: Option<Entity<StatusCountsModel>>) { self.status_counts_model = m; }
    pub fn set_topbar_entity(&mut self, e: Option<Entity<TopBarEntity>>) { self.topbar_entity = e; }
    pub fn set_pane_summary_model(&mut self, m: Option<Entity<PaneSummaryModel>>) { self.pane_summary_model = m; }
    pub fn set_modal_overlay_open(&self, open: bool) { self.modal_overlay_open.store(open, Ordering::Relaxed); }

    /// Clear pane statuses for a given key base prefix (on workspace switch).
    pub fn clear_statuses_for_prefix(&mut self, prefix: &str) {
        if let Ok(mut statuses) = self.pane_statuses.lock() {
            let colon_prefix = format!("{}:", prefix);
            statuses.retain(|k, _| k != prefix && !k.starts_with(&colon_prefix));
        }
    }

    /// Update status counts from current pane_statuses.
    pub fn refresh_status_counts(&mut self) {
        if let Ok(statuses) = self.pane_statuses.lock() {
            self.status_counts = StatusCounts::from_pane_statuses_per_worktree(&statuses);
        }
    }
}
```

- [ ] **Step 2: Add module declaration**

In `src/ui/mod.rs`:

```rust
pub mod runtime_manager;
```

- [ ] **Step 3: Run cargo check**

Run: `RUSTUP_TOOLCHAIN=stable cargo check`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src/ui/runtime_manager.rs src/ui/mod.rs
git commit -m "feat: add RuntimeManager struct (Phase 3 extraction skeleton)"
```

---

### Task 9: Wire RuntimeManager into AppRoot

**Files:**
- Modify: `src/ui/app_root.rs`

- [ ] **Step 1: Add RuntimeManager Entity to AppRoot and migrate fields**

Add to AppRoot struct:

```rust
runtime_mgr: Option<Entity<crate::ui::runtime_manager::RuntimeManager>>,
```

Remove from AppRoot struct: `runtime`, `event_bus`, `status_publisher`, `session_scanner`, `pane_statuses`, `status_counts`, `status_key_base`, `event_bus_subscription_started`, `pane_index`, `hook_handler`, `status_counts_model`, `topbar_entity`, `pane_summary_model`, `running_animation_frame`, `running_animation_task`.

Keep `modal_overlay_open` in AppRoot (shared between DialogManager and RuntimeManager).

- [ ] **Step 2: Update all runtime field references**

Key patterns:

```rust
// Before:
self.runtime.as_ref()
// After:
self.runtime_mgr.as_ref().and_then(|rm| rm.read(cx).runtime())

// Before:
self.pane_statuses.clone()
// After:
self.runtime_mgr.as_ref().map(|rm| rm.read(cx).pane_statuses()).unwrap_or_default()

// Before:
self.status_publisher.as_ref().unwrap().check_status(...)
// After:
if let Some(ref rm) = self.runtime_mgr {
    let rm_read = rm.read(cx);
    if let Some(sp) = rm_read.status_publisher() { sp.check_status(...); }
}
```

- [ ] **Step 3: Move EventBus subscription setup to RuntimeManager**

Move `ensure_event_bus_subscription()` (lines 2901-3052) to RuntimeManager, adapting it to use `cx: &mut Context<RuntimeManager>` instead of `Context<AppRoot>`.

The EventBus subscription handles three event types:
- `AgentStateChange` → update pane_statuses, StatusCountsModel, PaneSummaryModel
- `Notification` → delegate to NotificationCenter
- `HookEvent` → delegate to hook_handler

**Cross-entity access:** The subscription handler needs:
- NotificationCenter entity handle → pass at construction or via setter
- `window_focused_shared: Arc<AtomicBool>` → pass at construction (for notification suppression when window focused)
- `last_input_time: Arc<Mutex<Instant>>` → pass at construction (for recent-input suppression)

Add these to RuntimeManager::new():
```rust
pub fn new(
    event_bus: Arc<EventBus>,
    modal_overlay_open: Arc<AtomicBool>,
    window_focused_shared: Arc<AtomicBool>,
    last_input_time: Arc<Mutex<std::time::Instant>>,
) -> Self { ... }
```

And add a setter for NotificationCenter entity:
```rust
pub fn set_notification_center(&mut self, nc: Entity<NotificationCenter>) { ... }
```

- [ ] **Step 4: Move runtime lifecycle methods**

Move these methods to RuntimeManager:
- `manage_running_animation()` (lines 2862-2900)
- `save_runtime_state()` (lines 3054-3087)
- `detach_ui_from_runtime()` (lines 3605-3622)
- `stop_current_session()` (lines 3627-3630)

Keep in AppRoot (they orchestrate multiple managers):
- `attach_runtime()` (calls RuntimeManager + TerminalManager + SplitPaneManager)
- `start_local_session()` (orchestrator)
- `switch_to_worktree()` (orchestrator)

- [ ] **Step 5: Apply #4 Mutex → parking_lot::RwLock for pane_statuses**

In RuntimeManager, change:
```rust
// Before:
pane_statuses: Arc<Mutex<HashMap<String, AgentStatus>>>,
// After:
pane_statuses: Arc<parking_lot::RwLock<HashMap<String, AgentStatus>>>,
```

Update all `.lock()` calls to `.read()` or `.write()` as appropriate.

- [ ] **Step 6: Apply #5 collection eviction**

In RuntimeManager's `clear_statuses_for_prefix()` — already implemented in struct definition above.

Wire it up in `stop_current_session()` / `detach_ui_from_runtime()`.

- [ ] **Step 7: Run cargo check**

Run: `RUSTUP_TOOLCHAIN=stable cargo check`
Expected: PASS

- [ ] **Step 8: Check AppRoot line count**

Run: `wc -l src/ui/app_root.rs`
Expected: Significant reduction (RuntimeManager is the largest extraction).

- [ ] **Step 9: Write unit test for RuntimeManager**

Add to `src/ui/runtime_manager.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::EventBus;

    #[test]
    fn test_clear_statuses_for_prefix() {
        let bus = Arc::new(EventBus::new(8));
        let modal = Arc::new(AtomicBool::new(false));
        let focused = Arc::new(AtomicBool::new(true));
        let last_input = Arc::new(Mutex::new(std::time::Instant::now()));
        let mut rm = RuntimeManager::new(bus, modal, focused, last_input);

        // Insert test data
        if let Ok(mut statuses) = rm.pane_statuses.lock() {
            statuses.insert("local:/path/feat".to_string(), AgentStatus::Running);
            statuses.insert("local:/path/feat:split-0".to_string(), AgentStatus::Idle);
            statuses.insert("local:/path/main".to_string(), AgentStatus::Waiting);
        }

        rm.clear_statuses_for_prefix("local:/path/feat");

        if let Ok(statuses) = rm.pane_statuses.lock() {
            assert_eq!(statuses.len(), 1);
            assert!(statuses.contains_key("local:/path/main"));
        }
    }
}
```

Run: `RUSTUP_TOOLCHAIN=stable cargo test runtime_manager -- --nocapture`
Expected: PASS

- [ ] **Step 10: Commit**

```bash
git add src/ui/app_root.rs src/ui/runtime_manager.rs
git commit -m "refactor: extract RuntimeManager from AppRoot (Phase 3)"
```

---

## Chunk 5: Phase 4 — TerminalManager Extraction

### Task 10: Create TerminalManager struct

**Files:**
- Create: `src/ui/terminal_manager.rs`
- Modify: `src/ui/mod.rs`

- [ ] **Step 1: Create terminal_manager.rs**

```rust
//! TerminalManager - manages terminal buffers, resize, focus, IME, and search.
//!
//! Extracted from AppRoot Phase 4.
//! Observes RuntimeManager for runtime reference.

use crate::ui::terminal_controller::ResizeController;
use crate::ui::terminal_view::TerminalBuffer;
use crate::ui::terminal_area_entity::TerminalAreaEntity;
use gpui::*;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::sync::atomic::AtomicBool;

pub struct TerminalManager {
    buffers: Arc<Mutex<HashMap<String, TerminalBuffer>>>,
    focus: Option<FocusHandle>,
    needs_focus: bool,
    resize_controller: ResizeController,
    preferred_dims: Option<(u16, u16)>,
    shared_dims: Arc<Mutex<Option<(u16, u16)>>>,
    ime_pending_enter: Arc<AtomicBool>,
    area_entity: Option<Entity<TerminalAreaEntity>>,
    // Search
    search_active: bool,
    search_query: String,
    search_current_match: usize,
}

impl TerminalManager {
    pub fn new() -> Self {
        Self {
            buffers: Arc::new(Mutex::new(HashMap::new())),
            focus: None,
            needs_focus: false,
            resize_controller: ResizeController::new(),
            preferred_dims: None,
            shared_dims: Arc::new(Mutex::new(None)),
            ime_pending_enter: Arc::new(AtomicBool::new(false)),
            area_entity: None,
            search_active: false,
            search_query: String::new(),
            search_current_match: 0,
        }
    }

    // Accessors
    pub fn buffers(&self) -> Arc<Mutex<HashMap<String, TerminalBuffer>>> { self.buffers.clone() }
    pub fn focus(&self) -> Option<&FocusHandle> { self.focus.as_ref() }
    pub fn needs_focus(&self) -> bool { self.needs_focus }
    pub fn set_needs_focus(&mut self, v: bool) { self.needs_focus = v; }
    pub fn resize_controller(&self) -> &ResizeController { &self.resize_controller }
    pub fn resize_controller_mut(&mut self) -> &mut ResizeController { &mut self.resize_controller }
    pub fn preferred_dims(&self) -> Option<(u16, u16)> { self.preferred_dims }
    pub fn set_preferred_dims(&mut self, dims: Option<(u16, u16)>) { self.preferred_dims = dims; }
    pub fn shared_dims(&self) -> Arc<Mutex<Option<(u16, u16)>>> { self.shared_dims.clone() }
    pub fn ime_pending_enter(&self) -> Arc<AtomicBool> { self.ime_pending_enter.clone() }
    pub fn area_entity(&self) -> Option<&Entity<TerminalAreaEntity>> { self.area_entity.as_ref() }
    pub fn set_area_entity(&mut self, e: Option<Entity<TerminalAreaEntity>>) { self.area_entity = e; }
    pub fn is_search_active(&self) -> bool { self.search_active }
    pub fn search_query(&self) -> &str { &self.search_query }
    pub fn search_current_match(&self) -> usize { self.search_current_match }

    pub fn set_search_active(&mut self, v: bool) { self.search_active = v; }
    pub fn set_search_query(&mut self, q: String) { self.search_query = q; }
    pub fn set_search_current_match(&mut self, m: usize) { self.search_current_match = m; }

    pub fn ensure_focus(&mut self, cx: &mut Context<Self>) {
        if self.focus.is_none() {
            self.focus = Some(cx.focus_handle());
        }
    }

    /// Clean up terminal buffers for a workspace (on switch/close).
    pub fn cleanup_buffers_for_prefix(&mut self, prefix: &str) {
        if let Ok(mut buffers) = self.buffers.lock() {
            let colon_prefix = format!("{}:", prefix);
            buffers.retain(|k, _| k != prefix && !k.starts_with(&colon_prefix));
        }
    }
}
```

- [ ] **Step 2: Add module declaration, run cargo check, commit**

```bash
# Add to src/ui/mod.rs: pub mod terminal_manager;
RUSTUP_TOOLCHAIN=stable cargo check
git add src/ui/terminal_manager.rs src/ui/mod.rs
git commit -m "feat: add TerminalManager struct (Phase 4 extraction skeleton)"
```

---

### Task 11: Wire TerminalManager into AppRoot

**Files:**
- Modify: `src/ui/app_root.rs`

- [ ] **Step 1: Add TerminalManager Entity, migrate fields**

Same pattern as Phases 1-3: add `terminal_mgr: Option<Entity<TerminalManager>>` to AppRoot, remove migrated fields, update all references.

- [ ] **Step 2: Move terminal setup methods to TerminalManager**

Move to TerminalManager (adapting `cx` type):
- `setup_local_terminal()` (lines 1060-1367) — needs access to RuntimeManager for runtime/status_publisher
- `setup_pane_terminal_output()` (lines 1755-1880)

These methods will take RuntimeManager entity handle as parameter.

- [ ] **Step 3: Apply #4 Mutex → parking_lot::RwLock for terminal_buffers**

```rust
// Before:
buffers: Arc<Mutex<HashMap<String, TerminalBuffer>>>,
// After:
buffers: Arc<parking_lot::RwLock<HashMap<String, TerminalBuffer>>>,
```

- [ ] **Step 4: Apply #5 collection eviction for terminal_buffers**

Already implemented in `cleanup_buffers_for_prefix()`. Wire into workspace switch flow.

- [ ] **Step 5: Apply #3 reduce clones — terminal_buffers Arc cloned once at setup**

In `setup_local_terminal()`, clone `self.buffers` once and pass to the output processing closure, instead of cloning in the render path.

- [ ] **Step 6: Run cargo check, commit**

```bash
RUSTUP_TOOLCHAIN=stable cargo check
git add src/ui/app_root.rs src/ui/terminal_manager.rs
git commit -m "refactor: extract TerminalManager from AppRoot (Phase 4)"
```

---

## Chunk 6: Phase 5 — SplitPaneManager Extraction

### Task 12: Create SplitPaneManager struct

**Files:**
- Create: `src/ui/split_pane_manager.rs`
- Modify: `src/ui/mod.rs`

- [ ] **Step 1: Create split_pane_manager.rs**

```rust
//! SplitPaneManager - manages split layout tree, pane focus, and divider drag.
//!
//! Extracted from AppRoot Phase 5.

use crate::split_tree::SplitNode;
use gpui::*;
use std::sync::{Arc, Mutex};
use std::sync::atomic::AtomicBool;

pub struct SplitPaneManager {
    split_tree: SplitNode,
    focused_pane_index: usize,
    divider_drag: Option<(Vec<bool>, f32, f32, bool)>,
    active_target: Option<String>,
    active_target_shared: Arc<Mutex<String>>,
    targets_shared: Arc<Mutex<Vec<String>>>,
    dragging: Arc<AtomicBool>,
}

impl SplitPaneManager {
    pub fn new() -> Self {
        Self {
            split_tree: SplitNode::pane(""),
            focused_pane_index: 0,
            divider_drag: None,
            active_target: None,
            active_target_shared: Arc::new(Mutex::new(String::new())),
            targets_shared: Arc::new(Mutex::new(Vec::new())),
            dragging: Arc::new(AtomicBool::new(false)),
        }
    }

    // Accessors
    pub fn split_tree(&self) -> &SplitNode { &self.split_tree }
    pub fn split_tree_mut(&mut self) -> &mut SplitNode { &mut self.split_tree }
    pub fn set_split_tree(&mut self, tree: SplitNode) { self.split_tree = tree; }
    pub fn focused_pane_index(&self) -> usize { self.focused_pane_index }
    pub fn set_focused_pane_index(&mut self, idx: usize) { self.focused_pane_index = idx; }
    pub fn active_target(&self) -> Option<&str> { self.active_target.as_deref() }
    pub fn set_active_target(&mut self, t: Option<String>) { self.active_target = t; }
    pub fn active_target_shared(&self) -> Arc<Mutex<String>> { self.active_target_shared.clone() }
    pub fn targets_shared(&self) -> Arc<Mutex<Vec<String>>> { self.targets_shared.clone() }
    pub fn dragging(&self) -> Arc<AtomicBool> { self.dragging.clone() }
    pub fn divider_drag(&self) -> &Option<(Vec<bool>, f32, f32, bool)> { &self.divider_drag }
    pub fn set_divider_drag(&mut self, drag: Option<(Vec<bool>, f32, f32, bool)>) { self.divider_drag = drag; }

    /// Get pane count from split tree.
    pub fn pane_count(&self) -> usize {
        self.split_tree.flatten().len()
    }

    /// Focus a specific pane by index.
    pub fn focus_pane(&mut self, index: usize) {
        let panes = self.split_tree.flatten();
        if index < panes.len() {
            self.focused_pane_index = index;
            let (target, _) = &panes[index];
            self.active_target = Some(target.clone());
            if let Ok(mut guard) = self.active_target_shared.lock() {
                *guard = target.clone();
            }
        }
    }

    /// Update shared targets list from current split tree.
    pub fn sync_targets(&self) {
        let targets: Vec<String> = self.split_tree.flatten().into_iter().map(|(t, _)| t).collect();
        if let Ok(mut guard) = self.targets_shared.lock() {
            *guard = targets;
        }
    }
}
```

- [ ] **Step 2: Add module declaration, run cargo check, commit**

```bash
# Add to src/ui/mod.rs: pub mod split_pane_manager;
RUSTUP_TOOLCHAIN=stable cargo check
git add src/ui/split_pane_manager.rs src/ui/mod.rs
git commit -m "feat: add SplitPaneManager struct (Phase 5 extraction skeleton)"
```

---

### Task 13: Wire SplitPaneManager into AppRoot

**Files:**
- Modify: `src/ui/app_root.rs`

- [ ] **Step 1: Add SplitPaneManager Entity, migrate fields**

Add `split_pane_mgr: Option<Entity<SplitPaneManager>>` to AppRoot, remove migrated fields, update references.

- [ ] **Step 2: Move split/close pane methods**

Move to SplitPaneManager:
- `handle_split_pane()` (lines 4070-4111)
- `handle_close_pane()` (lines 4114-4153)

These need access to RuntimeManager (for `runtime.split_pane()`, `runtime.kill_pane()`) and TerminalManager (for `setup_pane_terminal_output()`), so they take Entity handles as parameters.

- [ ] **Step 3: Apply #4 Mutex → parking_lot::RwLock for shared targets**

```rust
active_target_shared: Arc<parking_lot::RwLock<String>>,
targets_shared: Arc<parking_lot::RwLock<Vec<String>>>,
```

- [ ] **Step 4: Apply #3 SharedString for active_target**

```rust
// Before:
active_target: Option<String>,
// After:
active_target: Option<SharedString>,
```

- [ ] **Step 5: Run cargo check, commit**

```bash
RUSTUP_TOOLCHAIN=stable cargo check
git add src/ui/app_root.rs src/ui/split_pane_manager.rs
git commit -m "refactor: extract SplitPaneManager from AppRoot (Phase 5)"
```

---

## Chunk 7: Phase 6 — AppRoot render() Cleanup + Final Verification

### Task 14: Slim AppRoot render() to compositor

**Files:**
- Modify: `src/ui/app_root.rs`

- [ ] **Step 1: Refactor handle_key_down to routing dispatcher**

Replace the 371-line `handle_key_down()` with a routing dispatcher:

```rust
fn handle_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
    // Track last input time (notification suppression)
    if let Ok(mut t) = self.last_input_time.lock() {
        *t = std::time::Instant::now();
    }

    // 1. Dialog intercept
    if let Some(ref dm) = self.dialog_mgr {
        if dm.read(cx).is_settings_open() {
            // Only Escape closes settings
            if event.keystroke.key == "escape" {
                cx.update_entity(dm, |d, cx| d.close_settings(cx));
                self.terminal_needs_focus_via_mgr(cx);
            }
            return;
        }
        if dm.read(cx).is_new_branch_open(cx) {
            if event.keystroke.key == "escape" {
                cx.update_entity(dm, |d, cx| d.close_new_branch_dialog(cx));
                self.terminal_needs_focus_via_mgr(cx);
            }
            return;
        }
    }

    // 2. Search mode → delegate to TerminalManager
    if let Some(ref tm) = self.terminal_mgr {
        if tm.read(cx).is_search_active() {
            cx.update_entity(tm, |t, cx| {
                // handle search keys: Escape, Enter, Cmd+G, Backspace, printable
            });
            return;
        }
    }

    // 3. App shortcuts (Cmd+key)
    if event.keystroke.modifiers.platform {
        if self.handle_shortcut(event, window, cx) { return; }
    }

    // 4. Terminal input passthrough
    // ... forward to runtime via TerminalManager
}
```

The detailed implementation adapts from the existing 371-line method, splitting logic to the appropriate managers.

- [ ] **Step 2: Verify render() is under 100 lines**

After all phases, `render()` should only compose child entities:
- Check dependency state → render_dependency_check_page()
- Has workspaces → render_workspace_view() (which itself delegates to child entities)
- No workspaces → render_startup_page()
- Settings modal overlay (from DialogManager)

- [ ] **Step 3: Count AppRoot lines**

Run: `wc -l src/ui/app_root.rs`
Expected: Under 1500 lines (target: under 500 with further cleanup)

- [ ] **Step 4: Run cargo check**

Run: `RUSTUP_TOOLCHAIN=stable cargo check`
Expected: PASS

- [ ] **Step 5: Attempt cargo test**

Run: `RUSTUP_TOOLCHAIN=stable cargo test`
Expected: If SIGBUS resolved (app_root.rs now small enough), all tests PASS. If SIGBUS persists, run individual test modules that don't touch AppRoot.

- [ ] **Step 6: Commit**

```bash
git add src/ui/app_root.rs
git commit -m "refactor: slim AppRoot render() to compositor, routing dispatcher (Phase 6)"
```

---

### Task 15: Final Verification and Integration Tests

**Files:**
- Modify: `tests/` (integration tests if applicable)

- [ ] **Step 1: Run full test suite**

```bash
RUSTUP_TOOLCHAIN=stable cargo test
```

If SIGBUS persists, diagnostic:
```bash
# Check file sizes
wc -l src/ui/app_root.rs src/ui/dialog_manager.rs src/ui/notification_center.rs src/ui/runtime_manager.rs src/ui/terminal_manager.rs src/ui/split_pane_manager.rs
```

- [ ] **Step 2: Run cargo build --release**

```bash
RUSTUP_TOOLCHAIN=stable cargo build --release
```

Expected: PASS, record binary size.

- [ ] **Step 3: Verify app runs**

```bash
RUSTUP_TOOLCHAIN=stable cargo run
```

Test manually:
- Open workspace, switch worktrees
- Agent status updates appear in TopBar/Sidebar
- Split pane (Cmd+D), close pane (Cmd+W)
- Settings dialog opens/closes
- New branch dialog works
- Notification panel shows/hides
- Terminal input/output works
- IME Chinese input works
- Search (Cmd+F) works
- Clipboard paste (Cmd+V) works

- [ ] **Step 4: Commit final state**

```bash
git add -A
git commit -m "refactor: AppRoot refactoring complete — 5 Manager Entities extracted"
```

---

## Summary: File Map

| File | Action | Responsibility |
|------|--------|----------------|
| `src/runtime/status_publisher.rs` | Modify | #6 merge check_status/force_status |
| `src/agent_status.rs` | Modify | #2 add gpui_color() |
| `src/ui/sidebar.rs` | Modify | #2 use gpui_color() |
| `src/ui/dialog_manager.rs` | Create | Phase 1: all modal dialogs + settings |
| `src/ui/notification_center.rs` | Create | Phase 2: notifications + panel + jump |
| `src/ui/runtime_manager.rs` | Create | Phase 3: runtime lifecycle + status + events |
| `src/ui/terminal_manager.rs` | Create | Phase 4: terminal buffers + resize + search |
| `src/ui/split_pane_manager.rs` | Create | Phase 5: split tree + pane focus + drag |
| `src/ui/app_root.rs` | Modify | Slim compositor (~500 lines target) |
| `src/ui/mod.rs` | Modify | Add 5 new module declarations |
