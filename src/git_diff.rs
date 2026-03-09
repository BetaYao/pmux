// git_diff.rs - Git diff data retrieval and parsing
use std::path::Path;
use std::process::Command;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum GitDiffError {
    #[error("git command failed: {0}")]
    CommandFailed(String),
    #[error("parse error: {0}")]
    ParseError(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Status of a file change
#[derive(Debug, Clone, PartialEq)]
pub enum FileChangeStatus {
    Added,
    Modified,
    Deleted,
    Renamed(String), // old name
}

impl FileChangeStatus {
    pub fn label(&self) -> &str {
        match self {
            FileChangeStatus::Added => "A",
            FileChangeStatus::Modified => "M",
            FileChangeStatus::Deleted => "D",
            FileChangeStatus::Renamed(_) => "R",
        }
    }
}

/// A changed file in the diff
#[derive(Debug, Clone)]
pub struct ChangedFile {
    pub path: String,
    pub status: FileChangeStatus,
}

/// A single line in a diff hunk
#[derive(Debug, Clone, PartialEq)]
pub enum DiffLineKind {
    Context,
    Added,
    Removed,
}

#[derive(Debug, Clone)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    pub content: String,
    pub old_line_no: Option<usize>,
    pub new_line_no: Option<usize>,
}

/// A hunk within a file diff
#[derive(Debug, Clone)]
pub struct DiffHunk {
    pub old_start: usize,
    pub old_count: usize,
    pub new_start: usize,
    pub new_count: usize,
    pub header: String,
    pub lines: Vec<DiffLine>,
}

/// Complete diff for a single file
#[derive(Debug, Clone)]
pub struct FileDiff {
    pub path: String,
    pub hunks: Vec<DiffHunk>,
    pub is_binary: bool,
}

/// A commit in the log
#[derive(Debug, Clone)]
pub struct CommitInfo {
    pub hash: String,
    pub short_hash: String,
    pub subject: String,
    pub author: String,
    pub date: String,
}

/// Detect main branch name (main vs master)
pub fn detect_main_branch(worktree: &Path) -> String {
    let output = Command::new("git")
        .args(["branch", "-l", "main", "master", "--format=%(refname:short)"])
        .current_dir(worktree)
        .output();

    if let Ok(output) = output {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                let name = line.trim();
                if name == "main" || name == "master" {
                    return name.to_string();
                }
            }
        }
    }
    "main".to_string()
}

/// List files changed between main branch and HEAD
pub fn changed_files(worktree: &Path) -> Result<Vec<ChangedFile>, GitDiffError> {
    let main_branch = detect_main_branch(worktree);
    let output = Command::new("git")
        .args(["diff", "--name-status", &format!("{}...HEAD", main_branch)])
        .current_dir(worktree)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(GitDiffError::CommandFailed(stderr.to_string()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_name_status(&stdout)
}

/// Get diff for a specific file (main...HEAD)
pub fn file_diff(worktree: &Path, file_path: &str) -> Result<FileDiff, GitDiffError> {
    let main_branch = detect_main_branch(worktree);
    let output = Command::new("git")
        .args([
            "diff",
            &format!("{}...HEAD", main_branch),
            "--",
            file_path,
        ])
        .current_dir(worktree)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(GitDiffError::CommandFailed(stderr.to_string()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_unified_diff(&stdout, file_path)
}

/// List commits between main and HEAD
pub fn commits(worktree: &Path) -> Result<Vec<CommitInfo>, GitDiffError> {
    let main_branch = detect_main_branch(worktree);
    let output = Command::new("git")
        .args([
            "log",
            &format!("{}...HEAD", main_branch),
            "--format=%H%n%h%n%s%n%an%n%ad",
            "--date=relative",
        ])
        .current_dir(worktree)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(GitDiffError::CommandFailed(stderr.to_string()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_commits(&stdout)
}

/// Get files changed in a specific commit
pub fn commit_files(worktree: &Path, hash: &str) -> Result<Vec<ChangedFile>, GitDiffError> {
    let output = Command::new("git")
        .args(["show", "--name-status", "--format=", hash])
        .current_dir(worktree)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(GitDiffError::CommandFailed(stderr.to_string()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_name_status(&stdout)
}

/// Get diff for a specific file in a specific commit
pub fn commit_file_diff(
    worktree: &Path,
    hash: &str,
    file_path: &str,
) -> Result<FileDiff, GitDiffError> {
    let output = Command::new("git")
        .args(["show", hash, "--", file_path])
        .current_dir(worktree)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(GitDiffError::CommandFailed(stderr.to_string()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_unified_diff(&stdout, file_path)
}

/// Parse `git diff --name-status` output
fn parse_name_status(output: &str) -> Result<Vec<ChangedFile>, GitDiffError> {
    let mut files = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.splitn(3, '\t').collect();
        if parts.len() < 2 {
            continue;
        }
        let status = match parts[0] {
            "A" => FileChangeStatus::Added,
            "M" => FileChangeStatus::Modified,
            "D" => FileChangeStatus::Deleted,
            s if s.starts_with('R') => {
                let old_name = if parts.len() >= 3 {
                    parts[1].to_string()
                } else {
                    String::new()
                };
                FileChangeStatus::Renamed(old_name)
            }
            _ => FileChangeStatus::Modified,
        };
        let path = if matches!(status, FileChangeStatus::Renamed(_)) && parts.len() >= 3 {
            parts[2].to_string()
        } else {
            parts[1].to_string()
        };
        files.push(ChangedFile { path, status });
    }
    Ok(files)
}

/// Parse `git log` output (5-line groups: hash, short_hash, subject, author, date)
fn parse_commits(output: &str) -> Result<Vec<CommitInfo>, GitDiffError> {
    let lines: Vec<&str> = output.lines().collect();
    let mut commits = Vec::new();
    let mut i = 0;
    while i + 4 < lines.len() {
        commits.push(CommitInfo {
            hash: lines[i].to_string(),
            short_hash: lines[i + 1].to_string(),
            subject: lines[i + 2].to_string(),
            author: lines[i + 3].to_string(),
            date: lines[i + 4].to_string(),
        });
        i += 5;
    }
    Ok(commits)
}

/// Parse unified diff format into FileDiff
pub fn parse_unified_diff(output: &str, file_path: &str) -> Result<FileDiff, GitDiffError> {
    if output.contains("Binary files") {
        return Ok(FileDiff {
            path: file_path.to_string(),
            hunks: Vec::new(),
            is_binary: true,
        });
    }

    let mut hunks = Vec::new();
    let mut current_hunk: Option<DiffHunk> = None;
    let mut old_line: usize = 0;
    let mut new_line: usize = 0;

    for line in output.lines() {
        // Skip diff header lines
        if line.starts_with("diff --git")
            || line.starts_with("index ")
            || line.starts_with("--- ")
            || line.starts_with("+++ ")
            || line.starts_with("commit ")
            || line.starts_with("Author:")
            || line.starts_with("Date:")
            || line.starts_with("    ") && current_hunk.is_none()
        {
            continue;
        }

        // Parse hunk header: @@ -old_start,old_count +new_start,new_count @@
        if line.starts_with("@@") {
            if let Some(hunk) = current_hunk.take() {
                hunks.push(hunk);
            }
            if let Some(hunk) = parse_hunk_header(line) {
                old_line = hunk.old_start;
                new_line = hunk.new_start;
                current_hunk = Some(hunk);
            }
            continue;
        }

        // Parse diff lines
        if let Some(ref mut hunk) = current_hunk {
            if let Some(content) = line.strip_prefix('+') {
                hunk.lines.push(DiffLine {
                    kind: DiffLineKind::Added,
                    content: content.to_string(),
                    old_line_no: None,
                    new_line_no: Some(new_line),
                });
                new_line += 1;
            } else if let Some(content) = line.strip_prefix('-') {
                hunk.lines.push(DiffLine {
                    kind: DiffLineKind::Removed,
                    content: content.to_string(),
                    old_line_no: Some(old_line),
                    new_line_no: None,
                });
                old_line += 1;
            } else if let Some(content) = line.strip_prefix(' ') {
                hunk.lines.push(DiffLine {
                    kind: DiffLineKind::Context,
                    content: content.to_string(),
                    old_line_no: Some(old_line),
                    new_line_no: Some(new_line),
                });
                old_line += 1;
                new_line += 1;
            } else if line == "\\ No newline at end of file" {
                // Skip this marker
            } else {
                // Treat as context (handles lines without prefix)
                hunk.lines.push(DiffLine {
                    kind: DiffLineKind::Context,
                    content: line.to_string(),
                    old_line_no: Some(old_line),
                    new_line_no: Some(new_line),
                });
                old_line += 1;
                new_line += 1;
            }
        }
    }

    if let Some(hunk) = current_hunk {
        hunks.push(hunk);
    }

    Ok(FileDiff {
        path: file_path.to_string(),
        hunks,
        is_binary: false,
    })
}

/// Parse hunk header like "@@ -1,5 +1,7 @@ fn main() {"
fn parse_hunk_header(line: &str) -> Option<DiffHunk> {
    // Find the range part between @@ markers
    let after_at = line.strip_prefix("@@")?;
    let end_at = after_at.find("@@")?;
    let range_part = after_at[..end_at].trim();

    let parts: Vec<&str> = range_part.split_whitespace().collect();
    if parts.len() < 2 {
        return None;
    }

    let (old_start, old_count) = parse_range(parts[0].strip_prefix('-')?)?;
    let (new_start, new_count) = parse_range(parts[1].strip_prefix('+')?)?;

    Some(DiffHunk {
        old_start,
        old_count,
        new_start,
        new_count,
        header: line.to_string(),
        lines: Vec::new(),
    })
}

/// Parse range like "1,5" or "1" into (start, count)
fn parse_range(s: &str) -> Option<(usize, usize)> {
    if let Some((start, count)) = s.split_once(',') {
        Some((start.parse().ok()?, count.parse().ok()?))
    } else {
        Some((s.parse().ok()?, 1))
    }
}

/// Reject (revert) a single hunk by constructing a reverse patch and applying it.
/// This undoes the changes introduced by that specific hunk in the working tree.
pub fn reject_hunk(
    worktree: &Path,
    file_path: &str,
    hunk: &DiffHunk,
) -> Result<(), GitDiffError> {
    // Construct a minimal unified diff patch for this hunk
    let mut patch = String::new();
    patch.push_str(&format!("--- a/{}\n", file_path));
    patch.push_str(&format!("+++ b/{}\n", file_path));
    patch.push_str(&format!(
        "@@ -{},{} +{},{} @@\n",
        hunk.old_start, hunk.old_count, hunk.new_start, hunk.new_count
    ));

    for line in &hunk.lines {
        match line.kind {
            DiffLineKind::Context => {
                patch.push(' ');
                patch.push_str(&line.content);
                patch.push('\n');
            }
            DiffLineKind::Added => {
                patch.push('+');
                patch.push_str(&line.content);
                patch.push('\n');
            }
            DiffLineKind::Removed => {
                patch.push('-');
                patch.push_str(&line.content);
                patch.push('\n');
            }
        }
    }

    // Apply the patch in reverse (undoes the hunk)
    let mut child = Command::new("git")
        .args(["apply", "--reverse", "--unidiff-zero", "-"])
        .current_dir(worktree)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;

    if let Some(ref mut stdin) = child.stdin {
        use std::io::Write;
        stdin.write_all(patch.as_bytes()).map_err(|e| {
            GitDiffError::CommandFailed(format!("failed to write patch to stdin: {}", e))
        })?;
    }
    // Drop stdin so git sees EOF
    drop(child.stdin.take());

    let output = child.wait_with_output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(GitDiffError::CommandFailed(format!(
            "git apply --reverse failed: {}",
            stderr
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_name_status() {
        let input = "M\tsrc/main.rs\nA\tsrc/new.rs\nD\tsrc/old.rs\n";
        let files = parse_name_status(input).unwrap();
        assert_eq!(files.len(), 3);
        assert_eq!(files[0].path, "src/main.rs");
        assert_eq!(files[0].status, FileChangeStatus::Modified);
        assert_eq!(files[1].path, "src/new.rs");
        assert_eq!(files[1].status, FileChangeStatus::Added);
        assert_eq!(files[2].path, "src/old.rs");
        assert_eq!(files[2].status, FileChangeStatus::Deleted);
    }

    #[test]
    fn test_parse_name_status_rename() {
        let input = "R100\told/path.rs\tnew/path.rs\n";
        let files = parse_name_status(input).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "new/path.rs");
        assert!(matches!(files[0].status, FileChangeStatus::Renamed(_)));
    }

    #[test]
    fn test_parse_hunk_header() {
        let hunk = parse_hunk_header("@@ -1,5 +1,7 @@ fn main() {").unwrap();
        assert_eq!(hunk.old_start, 1);
        assert_eq!(hunk.old_count, 5);
        assert_eq!(hunk.new_start, 1);
        assert_eq!(hunk.new_count, 7);
    }

    #[test]
    fn test_parse_hunk_header_single_line() {
        let hunk = parse_hunk_header("@@ -1 +1 @@").unwrap();
        assert_eq!(hunk.old_start, 1);
        assert_eq!(hunk.old_count, 1);
        assert_eq!(hunk.new_start, 1);
        assert_eq!(hunk.new_count, 1);
    }

    #[test]
    fn test_parse_unified_diff() {
        let diff_output = r#"diff --git a/src/main.rs b/src/main.rs
index abc123..def456 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,5 +1,6 @@
 fn main() {
-    println!("hello");
+    println!("hello world");
+    println!("goodbye");
     let x = 1;
     let y = 2;
 }
"#;
        let diff = parse_unified_diff(diff_output, "src/main.rs").unwrap();
        assert_eq!(diff.path, "src/main.rs");
        assert!(!diff.is_binary);
        assert_eq!(diff.hunks.len(), 1);

        let hunk = &diff.hunks[0];
        assert_eq!(hunk.old_start, 1);
        assert_eq!(hunk.old_count, 5);
        assert_eq!(hunk.new_start, 1);
        assert_eq!(hunk.new_count, 6);

        // Lines: context, removed, added, added, context, context, context
        assert_eq!(hunk.lines.len(), 7);
        assert_eq!(hunk.lines[0].kind, DiffLineKind::Context);
        assert_eq!(hunk.lines[1].kind, DiffLineKind::Removed);
        assert_eq!(hunk.lines[2].kind, DiffLineKind::Added);
        assert_eq!(hunk.lines[3].kind, DiffLineKind::Added);
    }

    #[test]
    fn test_parse_binary_diff() {
        let diff_output = "Binary files a/image.png and b/image.png differ\n";
        let diff = parse_unified_diff(diff_output, "image.png").unwrap();
        assert!(diff.is_binary);
        assert!(diff.hunks.is_empty());
    }

    #[test]
    fn test_parse_commits() {
        let output = "abc123def456\nabc123d\nFix bug in parser\nJohn Doe\n2 hours ago\n";
        let commits = parse_commits(output).unwrap();
        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].hash, "abc123def456");
        assert_eq!(commits[0].short_hash, "abc123d");
        assert_eq!(commits[0].subject, "Fix bug in parser");
        assert_eq!(commits[0].author, "John Doe");
        assert_eq!(commits[0].date, "2 hours ago");
    }

    #[test]
    fn test_line_numbers() {
        let diff_output = "@@ -10,3 +20,4 @@\n context\n-removed\n+added1\n+added2\n context2\n";
        let diff = parse_unified_diff(diff_output, "test.rs").unwrap();
        let hunk = &diff.hunks[0];

        // Context line at old=10, new=20
        assert_eq!(hunk.lines[0].old_line_no, Some(10));
        assert_eq!(hunk.lines[0].new_line_no, Some(20));

        // Removed line at old=11
        assert_eq!(hunk.lines[1].old_line_no, Some(11));
        assert_eq!(hunk.lines[1].new_line_no, None);

        // Added lines at new=21, 22
        assert_eq!(hunk.lines[2].old_line_no, None);
        assert_eq!(hunk.lines[2].new_line_no, Some(21));
        assert_eq!(hunk.lines[3].new_line_no, Some(22));

        // Context at old=12, new=23
        assert_eq!(hunk.lines[4].old_line_no, Some(12));
        assert_eq!(hunk.lines[4].new_line_no, Some(23));
    }

    #[test]
    fn test_file_change_status_label() {
        assert_eq!(FileChangeStatus::Added.label(), "A");
        assert_eq!(FileChangeStatus::Modified.label(), "M");
        assert_eq!(FileChangeStatus::Deleted.label(), "D");
        assert_eq!(FileChangeStatus::Renamed("old".into()).label(), "R");
    }
}
