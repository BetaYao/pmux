# Scheduled Tasks Design

Date: 2026-03-17
Status: Approved

## Overview

Add a scheduled task system to pmux that allows users to automatically trigger Agent or Shell commands at specified times using cron expressions.

## Requirements Summary

| Item | Choice |
|------|--------|
| Use case | Scheduled execution |
| Schedule format | Cron expression |
| Task types | Agent + Prompt, Shell command |
| Storage | Persistent (`~/.config/pmux/scheduled_tasks.json`) |
| Run target | Existing worktree or auto-create |
| Completion behavior | Notification + auto-close |
| UI | Collapsible section at bottom of Sidebar |

## Data Model

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledTask {
    pub id: Uuid,
    pub name: String,
    pub enabled: bool,
    pub cron: String,              // "0 2 * * *" = daily at 2:00
    pub task_type: TaskType,
    pub target: TaskTarget,
    pub on_completion: TaskCompletionAction,
    pub last_run: Option<DateTime<Utc>>,
    pub last_status: Option<TaskRunStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskType {
    Agent { command: String, prompt: String },  // claude/opencode + prompt
    Shell { command: String },                   // shell command
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskTarget {
    ExistingWorktree { workspace_index: usize, worktree_name: String },
    AutoCreate { branch_prefix: String, cleanup: bool },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskCompletionAction {
    pub notify: bool,              // send notification
    pub auto_close: bool,          // auto-close worktree (AutoCreate only)
    pub on_failure: FailureAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FailureAction {
    Retry { max_retries: u32 },
    NotifyOnly,
    Ignore,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskRunStatus {
    Never,
    Triggered,
    Failed,
}
```

## Architecture

```
┌─────────────────────────────────────────────────────┐
│                    AppRoot                          │
│  ┌───────────────────────────────────────────────┐  │
│  │           SchedulerManager (Entity)            │  │
│  │  - tokio-cron-scheduler instance               │  │
│  │  - Task CRUD operations                        │  │
│  │  - Task execution callback → RuntimeManager    │  │
│  └───────────────────────────────────────────────┘  │
│                      ↓                              │
│  ┌───────────────────────────────────────────────┐  │
│  │           RuntimeManager (Entity)              │  │
│  │  - Create/switch pane                         │  │
│  │  - Send command/prompt                        │  │
│  └───────────────────────────────────────────────┘  │
│                      ↓                              │
│  ┌───────────────────────────────────────────────┐  │
│  │        NotificationManager (Entity)            │  │
│  │  - Discord/Feishu notification                │  │
│  └───────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────┘
```

## UI Design

### Sidebar Collapsible Section

```
┌─────────────────────────────────┐
│  [≡] [🔔] [+]              [⚙] │  ← top controls
├─────────────────────────────────┤
│  worktree item 1                │
│  worktree item 2                │
│  worktree item 3                │
│  ...                            │
├─────────────────────────────────┤
│  ▼ Scheduled Tasks (3)    [+Add]│  ← collapsible header
│  ┌─────────────────────────────┐│
│  │ ⏰ Nightly PR Review        ││
│  │    0 2 * * * · next 6h      ││
│  └─────────────────────────────┘│
│  ┌─────────────────────────────┐│
│  │ ⏰ Weekly Cleanup      [▶]  ││
│  │    0 0 * * 0 · paused       ││
│  └─────────────────────────────┘│
└─────────────────────────────────┘
```

### Task Creation/Edit Dialog

```
┌─────────────────────────────────────────────┐
│  New Scheduled Task                    [×]  │
├─────────────────────────────────────────────┤
│  Name: [Nightly PR Review____________]      │
│                                             │
│  Schedule: [0 2 * * *_______________]       │
│            ↓ Daily at 02:00                 │
│                                             │
│  Type:  ○ Agent + Prompt  ○ Shell Command  │
│                                             │
│  ── Agent Task ──                           │
│  Command: [claude___________________]       │
│  Prompt:  [Review open PRs and summarize___]│
│                                             │
│  ── Target ──                               │
│  ○ Existing worktree: [main ▼]              │
│  ○ Auto-create: branch prefix [auto-____]   │
│    ☑ Cleanup after completion               │
│                                             │
│  ── On Completion ──                        │
│  ☑ Send notification                        │
│  ☑ Auto-close (auto-create only)            │
│  On failure: [Notify only ▼]                │
│                                             │
│                    [Cancel]  [Save Task]    │
└─────────────────────────────────────────────┘
```

## Task Execution Flow

```
Cron trigger
    ↓
SchedulerManager::on_task_trigger(task_id)
    - Load task config
    - Check enabled status
    - Update last_run = now
    - Update last_status = Running
    ↓
Based on TaskTarget:
    ExistingWorktree:
        - Find corresponding pane, activate
    AutoCreate:
        - git worktree add -b {prefix}-{timestamp}
        - Create new pane
    ↓
Based on TaskType:
    Agent:
        - Send command: "claude\n"
        - Wait for startup (500ms)
        - Send prompt
    Shell:
        - Send command directly
    ↓
Send trigger notification
    - Discord/Feishu: "Task X triggered"
    
last_status = Triggered
(No subsequent tracking)
```

## File Structure

```
src/
├── scheduler/
│   ├── mod.rs           # Module exports
│   ├── task.rs          # ScheduledTask, TaskType, TaskTarget models
│   ├── manager.rs       # SchedulerManager Entity
│   └── storage.rs       # JSON load/save
├── ui/
│   ├── sidebar.rs       # Add collapsible task section
│   ├── task_panel.rs    # Task list rendering
│   └── task_dialog.rs   # Task create/edit dialog
```

## Dependencies

- `tokio-cron-scheduler` - Cron scheduling
- `uuid` - Task ID generation
- `chrono` - DateTime handling
- `serde`/`serde_json` - Serialization

## Open Questions

- None at this time. All decisions finalized.