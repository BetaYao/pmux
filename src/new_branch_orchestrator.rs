// new_branch_orchestrator.rs - Orchestrator for the new branch creation workflow
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex};
use crate::new_branch_dialog::{ValidationError, validate_branch_name, generate_worktree_path};
use crate::worktree_manager::WorktreeManager;
use crate::notification::{Notification, NotificationType};

/// Result of the branch creation workflow
#[derive(Debug, Clone)]
pub enum CreationResult {
    Success {
        worktree_path: PathBuf,
        branch_name: String,
    },
    ValidationFailed {
        error: String,
    },
    BranchExists {
        branch_name: String,
    },
    GitFailed {
        error: String,
    },
    TmuxFailed {
        worktree_path: PathBuf,
        branch_name: String,
        error: String,
    },
}

/// Orchestrator for managing the complete new branch creation workflow
pub struct NewBranchOrchestrator {
    repo_path: PathBuf,
    worktree_manager: WorktreeManager,
    notification_sender: Option<Arc<StdMutex<dyn NotificationSender>>>,
}

impl NewBranchOrchestrator {
    /// Create a new orchestrator
    pub fn new(repo_path: PathBuf) -> Self {
        let worktree_manager = WorktreeManager::new(repo_path.clone());
        Self {
            repo_path,
            worktree_manager,
            notification_sender: None,
        }
    }

    /// Set the notification sender for user feedback
    pub fn with_notification_sender(mut self, sender: Arc<StdMutex<dyn NotificationSender>>) -> Self {
        self.notification_sender = Some(sender);
        self
    }

    /// Validate a branch name
    pub fn validate_branch_name(&self, name: &str) -> Result<(), ValidationError> {
        validate_branch_name(name)
    }

    /// Check if a branch already exists
    pub fn branch_exists(&self, name: &str) -> bool {
        self.worktree_manager.branch_exists(name)
    }

    /// Create a new branch and worktree synchronously
    pub fn create_branch_sync(&self, branch_name: &str) -> CreationResult {
        // Validate branch name
        if let Err(e) = self.validate_branch_name(branch_name) {
            return CreationResult::ValidationFailed { error: e.message };
        }

        // Check if branch exists
        if self.branch_exists(branch_name) {
            return CreationResult::BranchExists {
                branch_name: branch_name.to_string(),
            };
        }

        // Generate worktree path
        let worktree_path = generate_worktree_path(&self.repo_path, branch_name);

        // Create worktree
        let result = self.worktree_manager.create_worktree(branch_name, &worktree_path);
        
        if result.success {
            self.send_notification(NotificationType::Info, &format!("Created branch '{}' and worktree", branch_name));
            CreationResult::Success {
                worktree_path,
                branch_name: branch_name.to_string(),
            }
        } else {
            CreationResult::GitFailed {
                error: result.error.unwrap_or_else(|| "Unknown error".to_string()),
            }
        }
    }

    /// Create a new branch and worktree asynchronously
    pub async fn create_branch_async(&self, branch_name: &str) -> CreationResult {
        // Validate branch name
        if let Err(e) = self.validate_branch_name(branch_name) {
            return CreationResult::ValidationFailed { error: e.message };
        }

        // Check if branch exists
        if self.branch_exists(branch_name) {
            return CreationResult::BranchExists {
                branch_name: branch_name.to_string(),
            };
        }

        // Generate worktree path
        let worktree_path = generate_worktree_path(&self.repo_path, branch_name);

        // Create worktree asynchronously
        let result = self.worktree_manager.create_worktree_async(branch_name, &worktree_path).await;
        
        if result.success {
            self.send_notification(NotificationType::Info, &format!("Created branch '{}' and worktree", branch_name));
            CreationResult::Success {
                worktree_path,
                branch_name: branch_name.to_string(),
            }
        } else {
            CreationResult::GitFailed {
                error: result.error.unwrap_or_else(|| "Unknown error".to_string()),
            }
        }
    }

    /// Send a notification to the user
    fn send_notification(&self, notif_type: NotificationType, message: &str) {
        if let Some(sender) = &self.notification_sender {
            let _ = sender.lock().map(|sender| {
                sender.send(Notification::new("new-branch", notif_type, message));
            });
        }
    }

    /// Get the list of existing branches
    pub fn get_existing_branches(&self) -> Result<Vec<String>, String> {
        self.worktree_manager.get_existing_branches()
    }

    /// Get the list of existing worktrees
    pub fn get_worktrees(&self) -> Result<Vec<crate::worktree_manager::WorktreeInfo>, String> {
        self.worktree_manager.list_worktrees()
    }
}

/// Trait for sending notifications
pub trait NotificationSender: Send {
    fn send(&self, notification: Notification);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct MockNotificationSender {
        notifications: Arc<StdMutex<Vec<Notification>>>,
    }

    impl MockNotificationSender {
        fn new() -> Self {
            Self {
                notifications: Arc::new(StdMutex::new(Vec::new())),
            }
        }

        fn get_notifications(&self) -> Vec<Notification> {
            let notifications = self.notifications.lock().unwrap();
            notifications.iter().cloned().collect()
        }
    }

    impl NotificationSender for MockNotificationSender {
        fn send(&mut self, notification: Notification) {
            let mut notifications = self.notifications.lock().unwrap();
            notifications.push(notification);
        }
    }

    /// Test: Orchestrator creation
    #[test]
    fn test_orchestrator_creation() {
        let repo_path = PathBuf::from("/tmp/test");
        let orchestrator = NewBranchOrchestrator::new(repo_path.clone());
        assert_eq!(orchestrator.repo_path, repo_path);
    }

    /// Test: Validate valid branch name
    #[test]
    fn test_validate_valid_branch_name() {
        let orchestrator = NewBranchOrchestrator::new(PathBuf::from("/tmp/test"));
        assert!(orchestrator.validate_branch_name("feature/test").is_ok());
    }

    /// Test: Validate invalid branch name with spaces
    #[test]
    fn test_validate_invalid_branch_name() {
        let orchestrator = NewBranchOrchestrator::new(PathBuf::from("/tmp/test"));
        assert!(orchestrator.validate_branch_name("feature test").is_err());
    }

    /// Test: Get existing branches
    #[test]
    fn test_get_existing_branches() {
        // This test would require a real git repository
        // For now, we just verify the method exists
        let orchestrator = NewBranchOrchestrator::new(PathBuf::from("/tmp/test"));
        let _ = orchestrator.get_existing_branches();
    }

    /// Test: Notification sending
    #[test]
    fn test_notification_sending() {
        let mock_sender = Arc::new(Mutex::new(MockNotificationSender::new()));
        let orchestrator = NewBranchOrchestrator::new(PathBuf::from("/tmp/test"))
            .with_notification_sender(mock_sender.clone());

        orchestrator.send_notification(NotificationType::Info, "Test message");

        let notifications = mock_sender.lock().unwrap();
        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].message(), "Test message");
    }

    /// Test: Worktree path generation
    #[test]
    fn test_worktree_path_generation() {
        let repo_path = PathBuf::from("/tmp/project");
        let branch_name = "feature/test";
        let worktree_path = generate_worktree_path(&repo_path, branch_name);
        assert_eq!(worktree_path, PathBuf::from("/tmp/project/feature-test"));
    }

}