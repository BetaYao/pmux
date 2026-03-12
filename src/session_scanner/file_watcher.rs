//! file_watcher.rs — Monitor Claude Code project directories for JSONL file changes.
//!
//! Uses fsnotify (via `notify` crate) to watch for file modifications,
//! and incrementally reads new lines from JSONL files.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::fs::File;
use std::path::{Path, PathBuf};

/// Tracks read positions for JSONL files and reads incremental content.
pub struct JsonlFileWatcher {
    /// Track read position per file
    file_positions: HashMap<PathBuf, u64>,
    /// The project directory we're watching
    _project_dir: PathBuf,
}

impl JsonlFileWatcher {
    pub fn new(project_dir: PathBuf) -> Self {
        Self {
            file_positions: HashMap::new(),
            _project_dir: project_dir,
        }
    }

    /// Read new lines from a JSONL file since last read position.
    pub fn read_new_lines(&mut self, file_path: &Path) -> Vec<String> {
        let pos = self.file_positions.get(file_path).copied().unwrap_or(0);
        let mut file = match File::open(file_path) {
            Ok(f) => f,
            Err(_) => return vec![],
        };

        if file.seek(SeekFrom::Start(pos)).is_err() {
            return vec![];
        }

        let reader = BufReader::new(&file);
        let mut lines = Vec::new();
        let mut new_pos = pos;

        for line in reader.lines() {
            match line {
                Ok(l) => {
                    new_pos += l.len() as u64 + 1; // +1 for newline
                    if !l.is_empty() {
                        lines.push(l);
                    }
                }
                Err(_) => break,
            }
        }

        self.file_positions.insert(file_path.to_path_buf(), new_pos);
        lines
    }

    /// Extract session ID from JSONL filename (UUID format).
    pub fn session_id_from_path(path: &Path) -> Option<String> {
        path.file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
    }

    /// Initialize: skip all existing content (only process new writes).
    pub fn skip_existing(&mut self, file_path: &Path) {
        if let Ok(metadata) = std::fs::metadata(file_path) {
            self.file_positions
                .insert(file_path.to_path_buf(), metadata.len());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_session_id_from_path() {
        let path = Path::new("/some/dir/70984a90-6fdd-4602-85d9-7f935ef890b1.jsonl");
        assert_eq!(
            JsonlFileWatcher::session_id_from_path(path),
            Some("70984a90-6fdd-4602-85d9-7f935ef890b1".to_string())
        );
    }

    #[test]
    fn test_session_id_from_path_no_extension() {
        let path = Path::new("/some/dir/session-file");
        assert_eq!(
            JsonlFileWatcher::session_id_from_path(path),
            Some("session-file".to_string())
        );
    }

    #[test]
    fn test_read_new_lines_incremental() {
        let dir = tempfile::TempDir::new().unwrap();
        let file_path = dir.path().join("test.jsonl");

        // Write initial content
        {
            let mut f = File::create(&file_path).unwrap();
            writeln!(f, r#"{{"type":"user","sessionId":"s1"}}"#).unwrap();
            writeln!(f, r#"{{"type":"assistant","sessionId":"s1"}}"#).unwrap();
        }

        let mut watcher = JsonlFileWatcher::new(dir.path().to_path_buf());

        // First read gets all lines
        let lines = watcher.read_new_lines(&file_path);
        assert_eq!(lines.len(), 2);

        // Second read gets nothing (no new content)
        let lines = watcher.read_new_lines(&file_path);
        assert_eq!(lines.len(), 0);

        // Append new content
        {
            let mut f = std::fs::OpenOptions::new()
                .append(true)
                .open(&file_path)
                .unwrap();
            writeln!(f, r#"{{"type":"system","sessionId":"s1"}}"#).unwrap();
        }

        // Third read gets only the new line
        let lines = watcher.read_new_lines(&file_path);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("system"));
    }

    #[test]
    fn test_skip_existing() {
        let dir = tempfile::TempDir::new().unwrap();
        let file_path = dir.path().join("test.jsonl");

        // Write initial content
        {
            let mut f = File::create(&file_path).unwrap();
            writeln!(f, r#"{{"type":"user","sessionId":"s1"}}"#).unwrap();
            writeln!(f, r#"{{"type":"assistant","sessionId":"s1"}}"#).unwrap();
        }

        let mut watcher = JsonlFileWatcher::new(dir.path().to_path_buf());
        watcher.skip_existing(&file_path);

        // Read should return nothing (skipped existing)
        let lines = watcher.read_new_lines(&file_path);
        assert_eq!(lines.len(), 0);

        // Append new content
        {
            let mut f = std::fs::OpenOptions::new()
                .append(true)
                .open(&file_path)
                .unwrap();
            writeln!(f, r#"{{"type":"system","sessionId":"s1"}}"#).unwrap();
        }

        // Now should get only the new line
        let lines = watcher.read_new_lines(&file_path);
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn test_read_nonexistent_file() {
        let mut watcher = JsonlFileWatcher::new(PathBuf::from("/tmp"));
        let lines = watcher.read_new_lines(Path::new("/nonexistent/file.jsonl"));
        assert!(lines.is_empty());
    }
}
