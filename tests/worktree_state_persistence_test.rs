//! Feature tests for worktree/session matching by name (no persist).
//!
//! Verifies that window name matches worktree (branch) and session name matches repo;
//! restore uses runtime.session_info() / tmux current window, not config.
//! Also tests find_worktree_index_by_window_name and config save_workspaces (paths + active index only).

use std::process::Command;

use pmux::config::Config;
use pmux::runtime::backends::window_name_for_worktree;
use pmux::worktree::{discover_worktrees, WorktreeInfo};
use tempfile::TempDir;

/// Find worktree index by saved window name (same logic as AppRoot restore).
fn find_worktree_index_by_window_name(
    worktrees: &[WorktreeInfo],
    window_name: &str,
) -> Option<usize> {
    worktrees.iter().position(|wt| {
        window_name_for_worktree(&wt.path, wt.short_branch_name()) == window_name
    })
}

#[test]
fn test_config_save_workspaces_paths_and_active_index_only() {
    let temp = TempDir::new().unwrap();
    let repo_path = temp.path().join("repo");
    std::fs::create_dir_all(&repo_path).unwrap();

    Command::new("git").args(["init"]).current_dir(&repo_path).status().unwrap();
    Command::new("git")
        .args(["config", "user.email", "test@test.local"])
        .current_dir(&repo_path)
        .status()
        .unwrap();
    Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(&repo_path)
        .status()
        .unwrap();
    std::fs::write(repo_path.join("f"), "x").unwrap();
    Command::new("git").args(["add", "f"]).current_dir(&repo_path).status().unwrap();
    Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(&repo_path)
        .status()
        .unwrap();
    let wt2_path = temp.path().join("wt-feature");
    Command::new("git")
        .args(["worktree", "add", wt2_path.to_str().unwrap(), "-b", "feature-x"])
        .current_dir(&repo_path)
        .status()
        .unwrap();

    let worktrees = discover_worktrees(&repo_path).expect("discover_worktrees");
    assert!(worktrees.len() >= 2);
    let main_wn = window_name_for_worktree(
        &worktrees[0].path,
        worktrees[0].short_branch_name(),
    );
    assert_eq!(main_wn, "main");

    // Config only persists paths and active_workspace_index (worktree selection follows tmux window name)
    let config_path = temp.path().join("config.json");
    let mut config = Config::default();
    config.save_workspaces(&[repo_path.clone()], 0);
    config.save_to_path(&config_path).expect("save config");

    let loaded = Config::load_from_path(&config_path).unwrap();
    assert_eq!(loaded.workspace_paths.len(), 1);
    assert_eq!(loaded.active_workspace_index, 0);
}

#[test]
fn test_restore_resolves_worktree_index_by_window_name() {
    let temp = TempDir::new().unwrap();
    let repo_path = temp.path().join("repo");
    std::fs::create_dir_all(&repo_path).unwrap();

    Command::new("git").args(["init"]).current_dir(&repo_path).status().unwrap();
    Command::new("git")
        .args(["config", "user.email", "test@test.local"])
        .current_dir(&repo_path)
        .status()
        .unwrap();
    Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(&repo_path)
        .status()
        .unwrap();
    std::fs::write(repo_path.join("f"), "x").unwrap();
    Command::new("git").args(["add", "f"]).current_dir(&repo_path).status().unwrap();
    Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(&repo_path)
        .status()
        .unwrap();

    let wt2 = temp.path().join("wt-b");
    Command::new("git")
        .args(["worktree", "add", wt2.to_str().unwrap(), "-b", "branch-b"])
        .current_dir(&repo_path)
        .status()
        .unwrap();

    let worktrees = discover_worktrees(&repo_path).unwrap();
    assert!(worktrees.len() >= 2);

    // Restore by "main" -> index 0
    let idx_main = find_worktree_index_by_window_name(&worktrees, "main");
    assert_eq!(idx_main, Some(0));

    // Restore by "branch-b" -> index of that worktree
    let idx_b = find_worktree_index_by_window_name(&worktrees, "branch-b");
    assert!(idx_b.is_some());
    assert_eq!(worktrees[idx_b.unwrap()].short_branch_name(), "branch-b");

    // Unknown window name -> None
    let idx_unknown = find_worktree_index_by_window_name(&worktrees, "no-such-window");
    assert_eq!(idx_unknown, None);
}

#[test]
fn test_multiple_repos_each_restore_own_worktree_window() {
    let temp = TempDir::new().unwrap();
    let repo1 = temp.path().join("repo1");
    let repo2 = temp.path().join("repo2");
    std::fs::create_dir_all(&repo1).unwrap();
    std::fs::create_dir_all(&repo2).unwrap();

    for (repo_path, branch_second) in [(&repo1, "feature-a"), (&repo2, "feature-b")] {
        Command::new("git").args(["init"]).current_dir(repo_path).status().unwrap();
        Command::new("git")
            .args(["config", "user.email", "test@test.local"])
            .current_dir(repo_path)
            .status()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(repo_path)
            .status()
            .unwrap();
        std::fs::write(repo_path.join("f"), "x").unwrap();
        Command::new("git").args(["add", "f"]).current_dir(repo_path).status().unwrap();
        Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(repo_path)
            .status()
            .unwrap();
        let wt2_path = repo_path.join(format!("wt-{}", branch_second));
        Command::new("git")
            .args([
                "worktree",
                "add",
                wt2_path.to_str().unwrap(),
                "-b",
                branch_second,
            ])
            .current_dir(repo_path)
            .status()
            .unwrap();
    }

    let mut config = Config::default();
    config.save_workspaces(&[repo1.clone(), repo2.clone()], 1);

    let config_path = temp.path().join("config.json");
    config.save_to_path(&config_path).unwrap();
    let loaded = Config::load_from_path(&config_path).unwrap();
    assert_eq!(loaded.workspace_paths.len(), 2);
    assert_eq!(loaded.active_workspace_index, 1);

    // Resolve worktree index by window name (used at runtime from session_info(), not from config)
    let wt1 = discover_worktrees(&repo1).unwrap();
    let wt2 = discover_worktrees(&repo2).unwrap();
    let idx1 = find_worktree_index_by_window_name(&wt1, "feature-a");
    let idx2 = find_worktree_index_by_window_name(&wt2, "main");
    assert_eq!(idx1, Some(1));
    assert_eq!(idx2, Some(0));
}
