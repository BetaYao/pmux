# Scheduled Tasks Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a scheduled task system that automatically triggers Agent or Shell commands at specified times using cron expressions.

**Architecture:** New `SchedulerManager` entity holds `tokio-cron-scheduler` instance, manages task CRUD, and triggers task execution via RuntimeManager. Tasks stored in `~/.config/pmux/scheduled_tasks.json`. UI is a collapsible section at bottom of Sidebar.

**Tech Stack:** `tokio-cron-scheduler`, `chrono`, existing `uuid`, `serde`

---

## Task 1: Add Dependencies

**Files:**
- Modify: `Cargo.toml`

**Step 1: Add new dependencies**

Add to `[dependencies]` section after line 38:

```toml
tokio-cron-scheduler = "0.13"
chrono = { version = "0.4", features = ["serde"] }
```

**Step 2: Verify build**

Run: `cargo check`
Expected: Compiles successfully

**Step 3: Commit**

```bash
git add Cargo.toml
git commit -m "chore: add tokio-cron-scheduler and chrono dependencies"
```

---

## Task 2: Create Data Models

**Files:**
- Create: `src/scheduler/mod.rs`
- Create: `src/scheduler/task.rs`
- Modify: `src/lib.rs`

**Step 1: Create scheduler module directory**

Run: `mkdir -p src/scheduler`

**Step 2: Create `src/scheduler/task.rs` with data models**

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskRunStatus {
    Never,
    Triggered,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FailureAction {
    Retry { max_retries: u32 },
    NotifyOnly,
    Ignore,
}

impl Default for FailureAction {
    fn default() -> Self {
        FailureAction::NotifyOnly
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskCompletionAction {
    #[serde(default = "default_true")]
    pub notify: bool,
    #[serde(default)]
    pub auto_close: bool,
    #[serde(default)]
    pub on_failure: FailureAction,
}

fn default_true() -> bool {
    true
}

impl Default for TaskCompletionAction {
    fn default() -> Self {
        Self {
            notify: true,
            auto_close: false,
            on_failure: FailureAction::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskType {
    Agent { command: String, prompt: String },
    Shell { command: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskTarget {
    ExistingWorktree {
        workspace_index: usize,
        worktree_name: String,
    },
    AutoCreate {
        branch_prefix: String,
        #[serde(default)]
        cleanup: bool,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledTask {
    pub id: Uuid,
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub cron: String,
    pub task_type: TaskType,
    pub target: TaskTarget,
    #[serde(default)]
    pub on_completion: TaskCompletionAction,
    pub last_run: Option<DateTime<Utc>>,
    pub last_status: Option<TaskRunStatus>,
}

impl ScheduledTask {
    pub fn new(name: String, cron: String, task_type: TaskType, target: TaskTarget) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            enabled: true,
            cron,
            task_type,
            target,
            on_completion: TaskCompletionAction::default(),
            last_run: None,
            last_status: Some(TaskRunStatus::Never),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScheduledTasksStore {
    pub tasks: Vec<ScheduledTask>,
}
```

**Step 3: Create `src/scheduler/mod.rs`**

```rust
pub mod task;

pub use task::*;
```

**Step 4: Register module in `src/lib.rs`**

Add after line 39 (`pub mod workspace_state;`):

```rust
pub mod scheduler;
```

**Step 5: Verify build**

Run: `cargo check`
Expected: Compiles successfully

**Step 6: Commit**

```bash
git add src/scheduler/mod.rs src/scheduler/task.rs src/lib.rs
git commit -m "feat: add ScheduledTask data models"
```

---

## Task 3: Create Storage Layer

**Files:**
- Create: `src/scheduler/storage.rs`
- Modify: `src/scheduler/mod.rs`

**Step 1: Create `src/scheduler/storage.rs`**

```rust
use std::path::PathBuf;
use std::io;
use thiserror::Error;
use crate::scheduler::ScheduledTasksStore;

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

pub fn default_tasks_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("pmux")
        .join("scheduled_tasks.json")
}

pub fn load_tasks(path: &PathBuf) -> Result<ScheduledTasksStore, StorageError> {
    if !path.exists() {
        return Ok(ScheduledTasksStore::default());
    }
    let content = std::fs::read_to_string(path)?;
    let store: ScheduledTasksStore = serde_json::from_str(&content)?;
    Ok(store)
}

pub fn save_tasks(path: &PathBuf, store: &ScheduledTasksStore) -> Result<(), StorageError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(store)?;
    std::fs::write(path, content)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use crate::scheduler::{ScheduledTask, TaskType, TaskTarget};

    #[test]
    fn test_save_and_load_tasks() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("tasks.json");
        
        let mut store = ScheduledTasksStore::default();
        store.tasks.push(ScheduledTask::new(
            "Test Task".to_string(),
            "0 2 * * *".to_string(),
            TaskType::Shell { command: "echo hello".to_string() },
            TaskTarget::ExistingWorktree { workspace_index: 0, worktree_name: "main".to_string() },
        ));
        
        save_tasks(&path, &store).unwrap();
        let loaded = load_tasks(&path).unwrap();
        
        assert_eq!(loaded.tasks.len(), 1);
        assert_eq!(loaded.tasks[0].name, "Test Task");
    }

    #[test]
    fn test_load_nonexistent_file() {
        let path = PathBuf::from("/nonexistent/path/tasks.json");
        let loaded = load_tasks(&path).unwrap();
        assert!(loaded.tasks.is_empty());
    }
}
```

**Step 2: Update `src/scheduler/mod.rs`**

```rust
pub mod storage;
pub mod task;

pub use storage::*;
pub use task::*;
```

**Step 3: Verify build and tests**

Run: `cargo test scheduler::storage`
Expected: 2 tests pass

**Step 4: Commit**

```bash
git add src/scheduler/storage.rs src/scheduler/mod.rs
git commit -m "feat: add task storage layer with JSON persistence"
```

---

## Task 4: Create SchedulerManager Entity

**Files:**
- Create: `src/scheduler/manager.rs`
- Modify: `src/scheduler/mod.rs`

**Step 1: Create `src/scheduler/manager.rs`**

```rust
use std::path::PathBuf;
use std::sync::Arc;
use chrono::{DateTime, Utc};
use gpui::{App, Context, Entity};
use tokio_cron_scheduler::{Job, JobScheduler};
use uuid::Uuid;
use crate::scheduler::{ScheduledTask, ScheduledTasksStore, TaskRunStatus, default_tasks_path, load_tasks, save_tasks, StorageError};

pub struct SchedulerManager {
    scheduler: Option<JobScheduler>,
    tasks: Vec<ScheduledTask>,
    store_path: PathBuf,
}

impl SchedulerManager {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let store_path = default_tasks_path();
        let tasks = load_tasks(&store_path).unwrap_or_default().tasks;
        
        let mut manager = Self {
            scheduler: None,
            tasks,
            store_path,
        };
        
        manager.start_scheduler(cx);
        manager
    }
    
    fn start_scheduler(&mut self, _cx: &mut Context<Self>) {
        let scheduler = JobScheduler::new().expect("Failed to create job scheduler");
        
        for task in &self.tasks {
            if task.enabled {
                if let Err(e) = self.schedule_task(task, &scheduler) {
                    eprintln!("Failed to schedule task {}: {}", task.name, e);
                }
            }
        }
        
        if let Err(e) = scheduler.start() {
            eprintln!("Failed to start scheduler: {}", e);
        }
        
        self.scheduler = Some(scheduler);
    }
    
    fn schedule_task(&self, task: &ScheduledTask, scheduler: &JobScheduler) -> Result<(), Box<dyn std::error::Error>> {
        let task_id = task.id;
        let cron = task.cron.clone();
        
        let job = Job::new(&cron, move |_uuid, _l| {
            println!("Task {} triggered at {:?}", task_id, Utc::now());
        })?;
        
        scheduler.add(job)?;
        Ok(())
    }
    
    pub fn tasks(&self) -> &[ScheduledTask] {
        &self.tasks
    }
    
    pub fn add_task(&mut self, mut task: ScheduledTask, cx: &mut Context<Self>) -> Result<Uuid, StorageError> {
        let id = task.id;
        
        if task.enabled {
            if let Some(ref scheduler) = self.scheduler {
                if let Err(e) = self.schedule_task(&task, scheduler) {
                    eprintln!("Failed to schedule task {}: {}", task.name, e);
                }
            }
        }
        
        self.tasks.push(task);
        self.save()?;
        cx.notify();
        
        Ok(id)
    }
    
    pub fn update_task(&mut self, task: ScheduledTask, cx: &mut Context<Self>) -> Result<(), StorageError> {
        if let Some(idx) = self.tasks.iter().position(|t| t.id == task.id) {
            self.tasks[idx] = task;
            self.save()?;
            cx.notify();
        }
        Ok(())
    }
    
    pub fn remove_task(&mut self, id: Uuid, cx: &mut Context<Self>) -> Result<(), StorageError> {
        self.tasks.retain(|t| t.id != id);
        self.save()?;
        cx.notify();
        Ok(())
    }
    
    pub fn toggle_task(&mut self, id: Uuid, cx: &mut Context<Self>) -> Result<(), StorageError> {
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == id) {
            task.enabled = !task.enabled;
            self.save()?;
            cx.notify();
        }
        Ok(())
    }
    
    pub fn mark_triggered(&mut self, id: Uuid, cx: &mut Context<Self>) -> Result<(), StorageError> {
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == id) {
            task.last_run = Some(Utc::now());
            task.last_status = Some(TaskRunStatus::Triggered);
            self.save()?;
            cx.notify();
        }
        Ok(())
    }
    
    pub fn mark_failed(&mut self, id: Uuid, cx: &mut Context<Self>) -> Result<(), StorageError> {
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == id) {
            task.last_status = Some(TaskRunStatus::Failed);
            self.save()?;
            cx.notify();
        }
        Ok(())
    }
    
    fn save(&self) -> Result<(), StorageError> {
        let store = ScheduledTasksStore {
            tasks: self.tasks.clone(),
        };
        save_tasks(&self.store_path, &store)
    }
}
```

**Step 2: Update `src/scheduler/mod.rs`**

```rust
pub mod manager;
pub mod storage;
pub mod task;

pub use manager::*;
pub use storage::*;
pub use task::*;
```

**Step 3: Verify build**

Run: `cargo check`
Expected: Compiles successfully

**Step 4: Commit**

```bash
git add src/scheduler/manager.rs src/scheduler/mod.rs
git commit -m "feat: add SchedulerManager entity with CRUD operations"
```

---

## Task 5: Wire SchedulerManager into AppRoot

**Files:**
- Modify: `src/ui/app_root.rs`

**Step 1: Add import at top of `src/ui/app_root.rs`**

Add after line 8 (after `use crate::runtime...`):

```rust
use crate::scheduler::SchedulerManager;
```

**Step 2: Add field to AppRoot struct**

Find the `AppRoot` struct (around line 300) and add after other entity fields:

```rust
pub(crate) scheduler_manager: Entity<SchedulerManager>,
```

**Step 3: Initialize in AppRoot::new()**

Find the `AppRoot::new()` function and add initialization after other entity creations:

```rust
let scheduler_manager = cx.new(|cx| SchedulerManager::new(cx));
```

Then add to the struct initialization:

```rust
scheduler_manager,
```

**Step 4: Verify build**

Run: `cargo check`
Expected: Compiles successfully

**Step 5: Commit**

```bash
git add src/ui/app_root.rs
git commit -m "feat: wire SchedulerManager into AppRoot"
```

---

## Task 6: Add Tasks Collapsible Section to Sidebar

**Files:**
- Modify: `src/ui/sidebar.rs`

**Step 1: Add imports**

Add at top of file after existing imports:

```rust
use uuid::Uuid;
use crate::scheduler::{ScheduledTask, TaskRunStatus};
```

**Step 2: Add fields to Sidebar struct**

Add after `on_settings` field (around line 90):

```rust
scheduled_tasks: Vec<ScheduledTask>,
tasks_expanded: bool,
on_toggle_task: Option<Arc<dyn Fn(Uuid, &mut Window, &mut App) + Send + Sync>>,
on_run_task: Option<Arc<dyn Fn(Uuid, &mut Window, &mut App) + Send + Sync>>,
on_add_task: Option<Arc<dyn Fn(&mut Window, &mut App) + Send + Sync>>,
```

**Step 3: Initialize fields in Sidebar::new()**

Add to struct initialization:

```rust
scheduled_tasks: Vec::new(),
tasks_expanded: true,
on_toggle_task: None,
on_run_task: None,
on_add_task: None,
```

**Step 4: Add setter methods**

Add after existing setter methods (around line 190):

```rust
pub fn set_scheduled_tasks(&mut self, tasks: Vec<ScheduledTask>) {
    self.scheduled_tasks = tasks;
}

pub fn with_tasks_expanded(mut self, expanded: bool) -> Self {
    self.tasks_expanded = expanded;
    self
}

pub fn on_toggle_task<F: Fn(Uuid, &mut Window, &mut App) + Send + Sync + 'static>(mut self, f: F) -> Self {
    self.on_toggle_task = Some(Arc::new(f));
    self
}

pub fn on_run_task<F: Fn(Uuid, &mut Window, &mut App) + Send + Sync + 'static>(mut self, f: F) -> Self {
    self.on_run_task = Some(Arc::new(f));
    self
}

pub fn on_add_task<F: Fn(&mut Window, &mut App) + Send + Sync + 'static>(mut self, f: F) -> Self {
    self.on_add_task = Some(Arc::new(f));
    self
}
```

**Step 5: Verify build**

Run: `cargo check`
Expected: Compiles successfully

**Step 6: Commit**

```bash
git add src/ui/sidebar.rs
git commit -m "feat: add scheduled tasks fields and setters to Sidebar"
```

---

## Task 7: Render Tasks Section in Sidebar

**Files:**
- Modify: `src/ui/sidebar.rs`

**Step 1: Add helper method to render task item**

Add before `impl RenderOnce for Sidebar`:

```rust
fn render_task_item(
    task: &ScheduledTask,
    on_toggle: Option<Arc<dyn Fn(Uuid, &mut Window, &mut App) + Send + Sync>>,
    on_run: Option<Arc<dyn Fn(Uuid, &mut Window, &mut App) + Send + Sync>>,
) -> Div {
    let status_text = match &task.last_status {
        Some(TaskRunStatus::Never) => "Never run",
        Some(TaskRunStatus::Triggered) => "Triggered",
        Some(TaskRunStatus::Failed) => "Failed",
        None => "Never run",
    };
    
    let status_color = match &task.last_status {
        Some(TaskRunStatus::Never) => rgb(0x888888),
        Some(TaskRunStatus::Triggered) => rgb(0x4ade80),
        Some(TaskRunStatus::Failed) => rgb(0xf87171),
        None => rgb(0x888888),
    };
    
    let icon = if task.enabled { "▶" } else { "⏸" };
    let icon_color = if task.enabled { rgb(0x4ade80) } else { rgb(0x888888) };
    
    let task_id = task.id;
    let task_id_for_run = task.id;
    
    div()
        .flex()
        .items_center()
        .justify_between()
        .w_full()
        .px_2()
        .py_1()
        .rounded_md()
        .hover(|style| style.bg(rgb(0x2a2a2a)))
        .child(
            div()
                .flex()
                .flex_col()
                .gap_0()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(div().text_color(icon_color).text_sm().child(icon))
                        .child(div().text_color(rgb(0xe0e0e0)).text_sm().child(&task.name))
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(div().text_color(rgb(0x888888)).text_xs().child(&task.cron))
                        .child(div().text_color(status_color).text_xs().child(status_text))
                )
        )
        .when_some(on_toggle, |el, cb| {
            el.on_click(move |_, window, cx| {
                cb(task_id, window, cx);
            })
        })
        .when_some(on_run.clone(), move |el, cb| {
            el.child(
                div()
                    .text_color(rgb(0x888888))
                    .text_xs()
                    .hover(|style| style.text_color(rgb(0xe0e0e0)))
                    .on_click(move |_, window, cx| {
                        cb(task_id_for_run, window, cx);
                    })
                    .child("Run")
            )
        })
}

fn render_tasks_section(
    tasks: &[ScheduledTask],
    expanded: bool,
    on_toggle_expand: Option<Arc<dyn Fn(&mut Window, &mut App) + Send + Sync>>,
    on_toggle_task: Option<Arc<dyn Fn(Uuid, &mut Window, &mut App) + Send + Sync>>,
    on_run_task: Option<Arc<dyn Fn(Uuid, &mut Window, &mut App) + Send + Sync>>,
    on_add_task: Option<Arc<dyn Fn(&mut Window, &mut App) + Send + Sync>>,
) -> Div {
    let count = tasks.len();
    let expand_icon = if expanded { "▼" } else { "▶" };
    
    div()
        .flex()
        .flex_col()
        .w_full()
        .border_t_1()
        .border_color(rgb(0x333333))
        .mt_2()
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .w_full()
                .px_2()
                .py_2()
                .hover(|style| style.bg(rgb(0x2a2a2a)))
                .when_some(on_toggle_expand, |el, cb| {
                    el.on_click(move |_, window, cx| {
                        cb(window, cx);
                    })
                })
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(div().text_color(rgb(0x888888)).text_xs().child(expand_icon))
                        .child(div().text_color(rgb(0x888888)).text_sm().child(format!("Scheduled Tasks ({})", count)))
                )
                .when_some(on_add_task.clone(), |el, cb| {
                    el.child(
                        div()
                            .text_color(rgb(0x888888))
                            .text_xs()
                            .hover(|style| style.text_color(rgb(0xe0e0e0)))
                            .on_click(move |_, window, cx| {
                                cb(window, cx);
                            })
                            .child("+Add")
                    )
                })
        )
        .when(expanded, |el| {
            el.children(tasks.iter().map(|task| {
                Self::render_task_item(task, on_toggle_task.clone(), on_run_task.clone())
            }))
        })
}
```

**Step 2: Call render_tasks_section in render method**

Find the render method and add after the worktree list rendering:

```rust
.child(Self::render_tasks_section(
    &self.scheduled_tasks,
    self.tasks_expanded,
    None,
    self.on_toggle_task.clone(),
    self.on_run_task.clone(),
    self.on_add_task.clone(),
))
```

**Step 3: Verify build**

Run: `cargo check`
Expected: Compiles successfully

**Step 4: Commit**

```bash
git add src/ui/sidebar.rs
git commit -m "feat: render scheduled tasks section in Sidebar"
```

---

## Task 8: Wire Tasks Section to AppRoot

**Files:**
- Modify: `src/ui/app_root.rs`

**Step 1: Pass tasks to Sidebar in build_sidebar**

Find the `build_sidebar` method and add scheduled tasks from scheduler_manager:

```rust
let scheduled_tasks = self.scheduler_manager.read(cx).tasks().to_vec();
```

Then chain to the Sidebar builder:

```rust
.with_scheduled_tasks(scheduled_tasks)
.with_tasks_expanded(self.tasks_expanded)
```

**Step 2: Add tasks_expanded field to AppRoot**

Add to struct:

```rust
tasks_expanded: bool,
```

Initialize to `true` in `new()`.

**Step 3: Add callback handlers**

Add handlers for:
- `on_toggle_task` - toggle task enabled state
- `on_run_task` - run task immediately
- `on_add_task` - open task dialog

**Step 4: Verify build**

Run: `cargo check`
Expected: Compiles successfully

**Step 5: Commit**

```bash
git add src/ui/app_root.rs
git commit -m "feat: wire scheduled tasks section to AppRoot"
```

---

## Task 9: Create Task Dialog

**Files:**
- Create: `src/ui/task_dialog.rs`
- Modify: `src/ui/mod.rs`

**Step 1: Create `src/ui/task_dialog.rs`**

This file will contain the task creation/edit dialog with fields for:
- Name
- Cron expression
- Task type (Agent/Shell)
- Target (Existing worktree/Auto-create)
- Completion options

(See design doc for full dialog layout)

**Step 2: Export in `src/ui/mod.rs`**

Add:

```rust
pub mod task_dialog;
```

**Step 3: Verify build**

Run: `cargo check`
Expected: Compiles successfully

**Step 4: Commit**

```bash
git add src/ui/task_dialog.rs src/ui/mod.rs
git commit -m "feat: add task creation/edit dialog"
```

---

## Task 10: Implement Task Execution

**Files:**
- Modify: `src/scheduler/manager.rs`

**Step 1: Add task execution method**

Add method to trigger task execution:

```rust
pub fn execute_task(&self, task_id: Uuid, runtime_manager: &Entity<RuntimeManager>) {
    if let Some(task) = self.tasks.iter().find(|t| t.id == task_id) {
        // Execute based on TaskTarget and TaskType
        // See design doc for execution flow
    }
}
```

**Step 2: Wire to RuntimeManager**

Use RuntimeManager to:
- Switch to/create pane
- Send command/prompt

**Step 3: Verify build**

Run: `cargo check`
Expected: Compiles successfully

**Step 4: Commit**

```bash
git add src/scheduler/manager.rs
git commit -m "feat: implement task execution via RuntimeManager"
```

---

## Task 11: Add Notification on Task Trigger

**Files:**
- Modify: `src/scheduler/manager.rs`

**Step 1: Wire to NotificationManager**

When task is triggered, send notification via Discord/Feishu if configured.

**Step 2: Verify build**

Run: `cargo check`
Expected: Compiles successfully

**Step 3: Commit**

```bash
git add src/scheduler/manager.rs
git commit -m "feat: send notification on task trigger"
```

---

## Task 12: Persist Tasks Expanded State

**Files:**
- Modify: `src/window_state.rs`

**Step 1: Add field to PersistentAppState**

```rust
pub tasks_expanded: bool,
```

**Step 2: Save/load in window_state.rs**

**Step 3: Commit**

```bash
git add src/window_state.rs
git commit -m "feat: persist tasks section expanded state"
```

---

## Task 13: Integration Tests

**Files:**
- Create: `tests/scheduler_integration_test.rs`

**Step 1: Write integration tests**

- Test task CRUD operations
- Test cron scheduling
- Test persistence

**Step 2: Run tests**

Run: `cargo test`
Expected: All tests pass

**Step 3: Commit**

```bash
git add tests/scheduler_integration_test.rs
git commit -m "test: add scheduler integration tests"
```

---

## Final Steps

**Step 1: Run full test suite**

Run: `cargo test`
Expected: All tests pass

**Step 2: Run clippy**

Run: `cargo clippy -- -D warnings`
Expected: No warnings

**Step 3: Build release**

Run: `cargo build --release`
Expected: Build succeeds

**Step 4: Manual testing**

- Create a task via UI
- Verify it appears in sidebar
- Verify it triggers at scheduled time
- Verify notification is sent