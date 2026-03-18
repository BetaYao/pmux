<!-- OPENSPEC:START -->
# OpenSpec Instructions

These instructions are for AI assistants working in this project.

Always open `@/openspec/AGENTS.md` when the request:
- Mentions planning or proposals (words like proposal, spec, change, plan)
- Introduces new capabilities, breaking changes, architecture shifts, or big performance/security work
- Sounds ambiguous and you need the authoritative spec before coding

Use `@/openspec/AGENTS.md` to learn:
- How to create and apply change proposals
- Spec format and conventions
- Project structure and guidelines

Keep this managed block so 'openspec update' can refresh the instructions.

<!-- OPENSPEC:END -->

# pmux AGENTS.md

Instructions for AI coding agents working in this repository.

## Project Overview

pmux is an AI Agent multi-branch development workbench - a native desktop GUI application for managing multiple AI agents working in parallel (one per git worktree), with real-time agent status monitoring, notifications, and quick diff review.

**Tech Stack**: Rust, GPUI (Zed's UI framework), tmux control mode, alacritty_terminal (VTE)

## Build Commands

```bash
# Build and run
cargo run

# Build release
cargo build --release

# Check code without building
cargo check

# Run clippy linter
cargo clippy -- -D warnings
```

**Note**: If `RUSTUP_TOOLCHAIN=esp` is set, use `RUSTUP_TOOLCHAIN=stable cargo run` to avoid proc-macro SIGBUS.

## Test Commands

```bash
# Run all unit/integration tests
cargo test

# Run a specific test by name
cargo test test_workspace_tab_creation

# Run tests in a specific module
cargo test workspace_manager::

# Run tests matching a pattern
cargo test status_counts

# Run regression tests (requires GUI, ~5 min)
cd tests/regression && ./run_all.sh

# Run functional tests (~15 min)
cd tests/functional && ./run_all.sh
```

## Code Style

### Imports

```rust
// Standard library first
use std::collections::HashMap;
use std::path::Path;

// External crates second
use gpui::prelude::*;
use serde::{Deserialize, Serialize};
use thiserror::Error;

// Local crates last
use crate::runtime::agent_runtime::PaneId;
use crate::config::Config;
```

### Formatting

- Use 4-space indentation (Rust standard)
- Max line length: 100 characters
- No trailing whitespace
- Use `cargo fmt` before committing

### Naming Conventions

| Type | Convention | Example |
|------|------------|---------|
| Types/Structs/Enums | PascalCase | `AgentStatus`, `StatusCounts` |
| Functions/Methods | snake_case | `is_git_repository`, `highest_priority_for_prefix` |
| Constants | SCREAMING_SNAKE_CASE | `HEX_SEND_KEYS_CHUNK` |
| Module names | snake_case | `agent_status`, `git_utils` |
| File names | snake_case | `agent_status.rs`, `git_utils.rs` |

### Types

- Prefer `Result<T, E>` with custom error types using `thiserror`
- Use `Option<T>` for nullable values
- Derive `Debug, Clone, Copy, PartialEq, Eq, Hash` for enums used as keys
- Use `#[derive(Default)]` when sensible

### Error Handling

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum GitError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Not a git repository")]
    NotARepository,
}
```

- Use `thiserror` for custom error types
- Provide user-friendly error messages
- Use `anyhow` for application-level error handling in `main.rs`
- Never panic in library code; return `Result`

### Documentation

```rust
/// Brief description of the function.
///
/// More detailed explanation if needed.
///
/// # Arguments
/// * `path` - Description of the path parameter
///
/// # Returns
/// Description of return value
pub fn is_git_repository(path: &Path) -> bool {
```

- Use `///` for doc comments (not `//`)
- Document all public items
- Include examples in doc comments when helpful

### Comments

- Use `//` for inline comments
- Use `//!` for module-level documentation
- Avoid redundant comments that just repeat code
- Comment "why", not "what"

## Architecture

### Key Directories

```
src/
├── main.rs              # Application entry point
├── lib.rs               # Library root, module declarations
├── ui/                  # GPUI components (app_root, sidebar, terminal_view)
├── terminal/            # Terminal pipeline (content_extractor, input handling)
├── runtime/             # Runtime backends (tmux_control_mode, status_publisher)
├── hooks/               # Webhook server for external integrations
└── remotes/             # Remote notification channels (Discord, Feishu, Kook)

tests/
├── regression/          # Core regression tests (fast, CI-friendly)
├── functional/          # Feature tests by module
├── e2e/                 # End-to-end scenario tests
└── *.rs                 # Rust integration tests
```

### GPUI Component Pattern

```rust
impl Render for MyComponent {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .bg(rgb(0x1e1e1e))
            .child(/* ... */)
    }
}
```

### Terminal Pipeline

1. Input: GPUI key event → `key_to_bytes()` → `runtime.send_input(pane_id, bytes)`
2. Output: PTY master → `ControlModeParser.feed()` → `%output` event → per-pane channel → VTE

## Testing

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_feature_name() {
        // Arrange
        let temp = TempDir::new().unwrap();

        // Act
        let result = some_function();

        // Assert
        assert!(result.is_ok());
    }
}
```

- Place unit tests in `#[cfg(test)]` modules within source files
- Use `tempfile::TempDir` for filesystem isolation
- Follow Arrange-Act-Assert pattern

### Integration Tests

- Place in `tests/` directory
- Import with `use pmux::module::Item;`
- Use descriptive test names: `test_<feature>_<scenario>`

## Terminal Implementation Rules

See `.cursor/rules/terminal-implementation.mdc` for detailed tmux lifecycle, input/output pipeline, resize handling, and regression testing conventions.

Key points:
- tmux session naming: `pmux-<repo_basename>`
- Use `send-keys -H` for hex-encoded input (chunk limit: 512 bytes)
- Resize must use actual font metrics, not hardcoded values
- Always check `is_alive()` before operations

## Git Workflow

- Never commit without user request
- Never force push to main/master
- Use conventional commit messages: `feat:`, `fix:`, `refactor:`, `docs:`, `test:`

## Configuration

- Config file: `~/.config/pmux/config.json`
- Shell integration scripts: `~/.config/pmux/shell/`
- Window state persistence: automatic on close