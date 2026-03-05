# pmux

A native desktop workbench for running multiple AI agents in parallel — one per git worktree — with real-time status monitoring, notifications, and quick diff review.

Built with [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui) (Zed's GPU-accelerated UI framework) and tmux control mode for persistent terminal sessions.

## Features

- **Multi-worktree management** — switch between branches/worktrees; each gets its own persistent terminal session
- **Real-time agent status** — detects when an AI agent (Claude Code, opencode, etc.) is Running / Waiting / Idle / Error
- **OSC 133 shell integration** — accurate status detection via shell prompt markers (zsh/bash/fish)
- **Split panes** — divide the terminal area into multiple panes per worktree
- **Embedded terminal** — GPU-rendered terminal with full VTE support, CJK wide-character handling, search, clickable links
- **Notifications** — desktop and in-app notifications when an agent finishes or needs attention
- **Diff review** — open `git diff` or `nvim -c DiffviewOpen` directly from the sidebar
- **Multi-workspace tabs** — manage several repositories in one window

## Screenshot

> _Screenshots coming soon_

## Requirements

- **macOS** (Apple Silicon or Intel) — Linux support is possible but untested
- **tmux ≥ 3.2** — used in control mode (`-CC`) for persistent sessions
- **Rust toolchain** — stable channel (`rustup install stable`)
- **Xcode** — full installation required for Metal GPU rendering on macOS
  ```bash
  xcode-select --install
  xcodebuild -downloadComponent MetalToolchain   # if 'metal' tool is missing
  ```

## Build

```bash
# Clone
git clone https://github.com/<your-username>/pmux
cd pmux

# Run (debug)
RUSTUP_TOOLCHAIN=stable cargo run

# Build release binary
cargo build --release
# Binary: ./target/release/pmux
```

### Bundle as a macOS .app

```bash
./scripts/bundle.sh          # standard build
./scripts/bundle.sh --dev    # adds a DEV badge to the icon
```

## Configuration

Config is stored at `~/.config/pmux/config.json` and is created automatically on first run.

| Key | Default | Description |
|-----|---------|-------------|
| `workspace_path` | — | Last opened repository path |
| `backend` | `"tmux"` | Terminal backend: `"tmux"` or `"local"` |
| `last_terminal_cols` | — | Saved terminal width (restored on next launch) |
| `last_terminal_rows` | — | Saved terminal height |

Override backend at runtime:
```bash
PMUX_BACKEND=local cargo run   # use local PTY instead of tmux
```

## Shell Integration (OSC 133)

For accurate agent status detection, add OSC 133 markers to your shell prompt. This lets pmux know when a command starts/finishes and its exit code.

See [`docs/shell-integration.md`](docs/shell-integration.md) for setup instructions (zsh / bash / fish).

## Architecture

```
src/
├── main.rs                     # Entry point
├── ui/
│   ├── app_root.rs             # Root component, state management, runtime lifecycle
│   ├── sidebar.rs              # Worktree list with status icons
│   ├── tabbar.rs               # Multi-workspace tab bar
│   ├── terminal_area_entity.rs # Split-pane terminal container
│   └── terminal_view.rs        # Single terminal pane renderer
├── terminal/
│   ├── terminal_core.rs        # alacritty_terminal wrapper
│   ├── terminal_element.rs     # GPUI paint element for terminal grid
│   └── content_extractor.rs    # OSC 133 parser → ShellPhaseInfo
├── runtime/backends/
│   ├── tmux_control_mode.rs    # tmux -CC control mode (default backend)
│   └── local_pty.rs            # Direct PTY backend (fallback)
├── agent_status.rs             # AgentStatus enum (Running/Waiting/Idle/Error)
├── status_detector.rs          # Text + OSC 133 based status detection
├── worktree.rs                 # Git worktree discovery
└── config.rs                   # Config persistence
```

## Tech Stack

| Crate | Purpose |
|-------|---------|
| [gpui](https://github.com/zed-industries/zed/tree/main/crates/gpui) | GPU-accelerated native UI (Zed's framework) |
| [alacritty_terminal](https://github.com/alacritty/alacritty) | VTE parser + terminal grid |
| [flume](https://github.com/zesterer/flume) | Lock-free channel for terminal output |
| [serde](https://serde.rs) | JSON config serialization |
| [rfd](https://github.com/PolyMeilex/rfd) | Native file dialog |
| [thiserror](https://github.com/dtolnay/thiserror) | Structured error types |

## Contributing

Issues and pull requests are welcome. Please:

1. Check existing issues before filing a new one
2. For larger changes, open an issue first to discuss the approach
3. Run `cargo check` and `cargo test` before submitting

## License

MIT — see [LICENSE](LICENSE)
