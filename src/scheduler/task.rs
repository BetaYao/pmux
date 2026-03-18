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
