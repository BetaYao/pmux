//! Backend adapters implementing AgentRuntime.
//!
//! Local PTY only - no tmux.

mod local_pty;

pub use local_pty::LocalPtyRuntime;

use std::path::Path;
use std::sync::Arc;

use crate::runtime::agent_runtime::{AgentRuntime, RuntimeError};

/// Create a LocalPtyRuntime for the given worktree path.
pub fn create_runtime(worktree_path: &Path, cols: u16, rows: u16) -> Result<Arc<dyn AgentRuntime>, RuntimeError> {
    let rt = LocalPtyRuntime::new(worktree_path, cols, rows)?;
    Ok(Arc::new(rt))
}
