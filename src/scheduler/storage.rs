use crate::scheduler::ScheduledTasksStore;
use std::io;
use std::path::PathBuf;
use thiserror::Error;

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
    use crate::scheduler::{ScheduledTask, TaskTarget, TaskType};
    use tempfile::TempDir;

    #[test]
    fn test_save_and_load_tasks() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("tasks.json");

        let mut store = ScheduledTasksStore::default();
        store.tasks.push(ScheduledTask::new(
            "Test Task".to_string(),
            "0 2 * * *".to_string(),
            TaskType::Shell {
                command: "echo hello".to_string(),
            },
            TaskTarget::ExistingWorktree {
                workspace_index: 0,
                worktree_name: "main".to_string(),
            },
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
