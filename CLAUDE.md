# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

pmux is an AI Agent multi-branch development workbench - a native desktop GUI application for managing multiple AI agents working in parallel (one per git worktree), with real-time agent status monitoring, notifications, and quick diff review.

## Build and Test Commands

```bash
# Build and run the application (use stable toolchain if RUSTUP_TOOLCHAIN=esp is set)
RUSTUP_TOOLCHAIN=stable cargo run
# or: unset RUSTUP_TOOLCHAIN && cargo run

# Run all tests (SIGBUS in gpui_macros can occur on some macOS setups; use stable)
RUSTUP_TOOLCHAIN=stable cargo test

# Run a specific test by name
cargo test test_workspace_tab_creation

# Run tests in a specific module
cargo test workspace_manager::

# Build release version
cargo build --release

# Check code without building
cargo check
```

**Note:** If `RUSTUP_TOOLCHAIN=esp` is set (e.g. for ESP-IDF development), pmux must use `stable` to avoid proc-macro SIGBUS. The project's `rust-toolchain.toml` specifies `stable`; override the env when needed.

## Architecture

### Tech Stack
- **GPUI** - Zed editor's UI framework (native GPU-accelerated UI)
- **serde** - JSON serialization for config
- **rfd** - Cross-platform file dialogs
- **alacritty_terminal** - Terminal rendering
- **thiserror** - Error handling

### Key Modules

**UI Layer (`src/ui/`)**
- `app_root.rs` - Root GPUI component, manages application state and view switching
- `sidebar.rs` - Left sidebar showing worktree list
- `tabbar.rs` - Tab bar for multi-workspace navigation
- `terminal_view.rs` - Terminal rendering with content polling
- `new_branch_dialog_ui.rs` - Modal dialog for branch creation

**State Management**
- `workspace_manager.rs` - Multi-workspace tab management (add/switch/close tabs)
- `app_state.rs` - Complete application state for persistence
- `config.rs` - Configuration persistence (~/.config/pmux/config.json)

**Tmux Integration (`src/tmux/`)**
- `session.rs` - Tmux session lifecycle management
- `pane.rs` - Pane operations (capture, send-keys)
- `window.rs` - Window management

**Git/Worktree**
- `worktree.rs` - Git worktree discovery and parsing
- `git_utils.rs` - Repository validation
- `worktree_manager.rs` - Worktree creation and management

**Agent Status Detection (`src/`)**
- `agent_status.rs` - Agent status enumeration (Running, Waiting, Idle, Error, Unknown) with display properties
- `status_detector.rs` - Analyzes pane content to determine agent status
- `StatusPublisher` - Publishes agent state changes via Event Bus (reads from stream/Term buffers; no capture-pane)

**Input Handling**
- `input_handler.rs` - Forwards keyboard events to tmux sessions
- Keyboard shortcuts intercepted at AppRoot (Cmd+B toggles sidebar)

## Development Patterns

### Test Organization
- Unit tests are inline in source files under `#[cfg(test)]` modules
- Integration tests are in `tests/` directory
- Tests follow TDD pattern with Arrange-Act-Assert structure
- Use `tempfile::TempDir` for filesystem test isolation
- See `test-driven-development` skill for full TDD workflow

### Spec-Driven Development
The project uses openspec for feature specifications:
- `openspec/changes/` - Active feature specifications
- `openspec/archive/` - Completed specs
- Each spec contains: proposal.md, design.md, specs/, tasks.md

### Subagent-Driven Development
When implementing plans from `docs/plans/` (Runtime Phase 1–4), use the `subagent-driven-development` skill to delegate tasks to subagents (explore/shell/generalPurpose) for parallel execution. See `.cursor/skills/subagent-driven-development/SKILL.md`.

### Code Review
When the user requests a code review, use the `requesting-code-review` skill. Review against design.md, success criteria, and pmux conventions. See `.cursor/skills/requesting-code-review/SKILL.md`.

### UI Component Pattern
GPUI components implement the `Render` trait:
```rust
impl Render for AppRoot {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Return div() chain with styling and event handlers
    }
}
```

### Error Handling
- Custom error types using `thiserror` derive macro
- Git operations return `GitError` enum
- Tmux operations return `SessionError`, `PaneError`, `WindowError`
- Error messages should be user-friendly (some in Chinese for the target audience)

### State Flow
1. AppRoot loads Config on startup
2. Valid workspace triggers tmux session creation via `start_tmux_session()`
3. Background polling loops are started:
   - Terminal content polling (200ms) updates TerminalContent via Arc<Mutex<>>
   - Status polling (500ms) checks StatusPoller for agent state changes
   - StatusPoller runs in its own background thread, polling tmux panes
4. When agent status changes:
   - Status polling loop updates AppRoot's pane_statuses HashMap
   - AppRoot recomputes StatusCounts
   - cx.notify() triggers UI redraw
   - Sidebar displays updated status icons
   - TopBar displays updated aggregate counts
5. Keyboard events forwarded to tmux via InputHandler
6. Workspace switching stops current StatusPoller and starts new one
7. All state changes trigger cx.notify() for UI redraw

## Important Notes

- The application requires tmux to be installed for terminal functionality
- Configuration is stored at ~/.config/pmux/config.json
- GPUI is pinned to a specific git commit (rev = "269b03f4") in Cargo.toml
- The UI uses a dark theme with rgb(0x1e1e1e) as the background color
- Git repository validation supports normal repos, bare repos, and worktrees
- Agent status detection uses background polling (500ms) with debouncing to avoid UI flicker
- Status updates propagate from StatusPoller → AppRoot HashMap → Sidebar/TopBar via cx.notify()
