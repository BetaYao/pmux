# Terminal Engine Migration: gpui-ghostty + tmux session-per-window

## Summary

Replace pmux's terminal engine from gpui-terminal (alacritty_terminal + custom renderer) to gpui-ghostty (Ghostty VT + built-in TerminalView). Simultaneously migrate from tmux -CC control mode to standard tmux with one window per terminal pane, eliminating all control mode hacks.

## Motivation

1. **Terminal bugs** — gpui-terminal (alacritty_terminal) has numerous rendering and compatibility issues that are hard to fix upstream
2. **Custom renderer maintenance burden** — pmux maintains ~1500 lines of custom TerminalElement rendering code (paint, batching, cursor, selection, overlays)
3. **tmux -CC hacks** — Control mode doesn't pass through alt-screen, mouse mode, or certain escape sequences, requiring manual sequence injection and periodic capture_pane_resync
4. **Validated path** — gpui-ghostty is actively maintained (68 stars, 81 commits), and ceo (zmanian/ceo) has validated the Rust + GPUI + gpui-ghostty stack for an AI agent terminal multiplexer

## Architecture

### Current

```
tmux -CC (one session per repo)
  ├── window per worktree
  │     └── multiple panes (shared PTY context)
  ↓
ControlModeParser (%output pane_id data)
  ↓
alacritty_terminal::Processor::advance(&bytes)
  ↓
Custom TerminalElement::paint() → GPUI quads/text
```

Hacks required:
- capture_pane_resync (periodic full-screen refresh)
- Manual alt-screen sequence injection (ESC[?1049h/l)
- Manual mouse mode sequence injection (ESC[?1002h etc.)

### New

```
tmux (one session per worktree)
  ├── window-0 → single pane (independent PTY) → terminal-1
  ├── window-1 → single pane (independent PTY) → terminal-2
  └── window-N → single pane (independent PTY) → terminal-N
  ↓
PTY master fd (complete VT byte stream, no filtering)
  ↓
gpui-ghostty TerminalSession.feed(&bytes)
  ↓
gpui-ghostty TerminalView (Ghostty renderer)
```

Zero hacks. Full VT transparency.

### tmux Mapping

```
Before (control mode):
  repo        → tmux session "pmux-<repo>"
  worktree    → tmux window
  terminal    → tmux pane (shared PTY within window)

After (standard mode):
  worktree    → tmux session "pmux-<worktree>"
  terminal    → tmux window (each window = 1 pane = 1 independent PTY)
  split layout → pmux UI layer only (SplitPaneContainer)
```

Benefits of session-per-worktree:
- `tmux kill-session -t pmux-<worktree>` cleans up all terminals for a worktree
- `tmux list-windows -t pmux-<worktree>` lists all terminals in a worktree
- Preserves semantic grouping
- Fewer sessions than session-per-terminal approach

### Data Flow

#### Output (tmux → screen)

```
tmux window PTY master fd
  ↓ read thread (4KB buffer loop)
  ↓
tee: ──┬── gpui-ghostty TerminalSession.feed(&bytes)
       │     ↓
       │   TerminalView renders via Ghostty
       │
       └── ContentExtractor.feed(&bytes)
             ↓
           OSC 133 parsing + visible text extraction
             ↓
           StatusPublisher → EventBus → UI updates
```

#### Input (keyboard → tmux)

```
gpui-ghostty TerminalView
  ↓ TerminalInput callback
  ↓
PTY master fd write (raw bytes, tmux forwards to shell)
```

#### Resize

```
SplitPaneContainer layout change
  ↓ new cols/rows
  ↓
tmux resize-window -t "pmux-<wt>:<win>" -x cols -y rows
  ↓
SIGWINCH propagates to foreground process
```

### Session Lifecycle

#### Create terminal pane

```rust
// 1. Create tmux window in worktree session
tmux new-window -t "pmux-<worktree>" -c "<working_dir>"
// Returns window index

// 2. Get PTY master fd
// Open PTY pair, connect to tmux window via:
tmux respawn-window -t "pmux-<worktree>:<win>" -c "<working_dir>" "<shell>"
// Or: directly open PTY and pipe to tmux

// 3. Create gpui-ghostty session
let config = TerminalConfig { cols, rows, default_fg, default_bg, .. };
let session = TerminalSession::new(config)?;

// 4. Wire input
let input = TerminalInput::new(move |bytes| {
    pty_writer.write_all(bytes);
});

// 5. Create view
let view = TerminalView::new_with_input(session, focus_handle, input);

// 6. Start output reader thread
spawn(move || {
    loop {
        let n = pty_reader.read(&mut buf)?;
        output_tx.send(buf[..n].to_vec());
    }
});

// 7. Start UI update loop (16ms timer, coalesce chunks)
// feed bytes to session + content extractor
```

#### Recover on restart

```rust
// 1. List existing tmux sessions matching "pmux-*"
tmux list-sessions -F "#{session_name}"

// 2. For each session, list windows
tmux list-windows -t "pmux-<wt>" -F "#{window_index}:#{pane_pid}:#{pane_current_path}"

// 3. Re-attach PTY to each window
// Use tmux pipe-pane or direct PTY reconnection

// 4. Create TerminalSession + TerminalView for each
// Feed initial screen content via capture-pane snapshot
```

## Components Affected

### Deleted

| File | Reason |
|------|--------|
| `src/terminal/terminal_element.rs` | Custom rendering → gpui-ghostty TerminalView |
| `src/terminal/terminal_rendering.rs` | Batch rendering logic → gpui-ghostty |
| `src/terminal/terminal_core.rs` | alacritty_terminal wrapper → gpui-ghostty TerminalSession |
| `src/terminal/input.rs` | key_to_bytes → gpui-ghostty TerminalInput handles this |
| `src/terminal/terminal_input_handler.rs` | IME handling → gpui-ghostty built-in |
| `src/runtime/backends/tmux_control_mode.rs` | Control mode parser → no longer needed |
| `src/runtime/backends/tmux.rs` | Legacy tmux backend → replaced |

### Modified

| File | Changes |
|------|---------|
| `src/terminal/mod.rs` | Re-export gpui-ghostty types instead of custom types |
| `src/terminal/stream_adapter.rs` | Simplify: direct PTY read/write, remove tee complexity |
| `src/terminal/content_extractor.rs` | No change (operates on raw bytes, independent of terminal engine) |
| `src/ui/terminal_view.rs` | Wrap gpui-ghostty TerminalView instead of custom TerminalElement |
| `src/ui/terminal_area_entity.rs` | Adapt to new terminal view API |
| `src/ui/terminal_manager.rs` | Create gpui-ghostty sessions instead of Terminal structs |
| `src/runtime/backends/mod.rs` | New tmux standard mode backend |
| `src/ui/app_root.rs` | Remove coalesce_and_process_output, use new output pipeline |
| `Cargo.toml` | Replace gpui-terminal dep with gpui-ghostty |

### New

| File | Purpose |
|------|---------|
| `src/runtime/backends/tmux_standard.rs` | New tmux backend: session/window management, PTY bridging |
| Build scripts / Zig bootstrap | gpui-ghostty requires Zig 0.14.1 for ghostty_vt_sys |

### Unchanged

| Component | Why |
|-----------|-----|
| `src/terminal/content_extractor.rs` | Reads raw bytes, not terminal state |
| `src/shell_integration.rs` | OSC 133 parser, byte-level |
| `src/status_detector.rs` | Text pattern matching, independent |
| `src/agent_status.rs` | Pure data types |
| `src/ui/sidebar.rs` | Consumes status events, no terminal dependency |
| `src/ui/split_pane_container.rs` | UI layout, independent of terminal engine |
| `src/ui/notification_*.rs` | Event-driven, no terminal dependency |
| `src/ui/diff_view/*` | Git diff, no terminal dependency |
| `src/config.rs` | Configuration, needs minor updates for new backend |
| `src/worktree_manager.rs` | Git worktree ops, independent |

## Migration Strategy

### Phase 1: Add gpui-ghostty dependency
- Add gpui-ghostty crate to Cargo.toml
- Set up Zig build toolchain (bootstrap script)
- Verify basic compilation

### Phase 2: New tmux standard backend
- Implement `tmux_standard.rs`: session create/destroy, window create/destroy, PTY bridging
- Session naming convention: `pmux-<worktree-name>`
- Window-per-terminal lifecycle management
- Resize via `tmux resize-window`

### Phase 3: Replace terminal view
- Replace `TerminalElement` with gpui-ghostty `TerminalView`
- Wire `TerminalInput` callback to PTY write
- Wire PTY read thread → `TerminalSession.feed()`
- Adapt `terminal_view.rs` wrapper

### Phase 4: Rewire output pipeline
- PTY read thread tees bytes to both TerminalSession and ContentExtractor
- Remove coalesce_and_process_output from app_root
- Verify agent status detection still works

### Phase 5: Session recovery
- Discover existing `pmux-*` tmux sessions on startup
- Re-attach PTY to each window
- Restore TerminalView state via capture-pane snapshot

### Phase 6: Cleanup
- Delete old files (terminal_element, terminal_rendering, terminal_core, input, tmux_control_mode, tmux legacy)
- Remove alacritty_terminal dependency from Cargo.toml
- Update config schema if needed

## Risks

| Risk | Severity | Mitigation |
|------|----------|------------|
| gpui-ghostty API instability (no tagged release) | Medium | Pin to specific commit, same as current GPUI pinning strategy |
| Zig build toolchain adds complexity | Low | One-time bootstrap script, CI can cache |
| PTY bridging to tmux window may need experimentation | Medium | Prototype in Phase 2 before committing to full migration |
| ContentExtractor byte tee timing | Low | Same pattern as current tee_output, proven approach |
| Session recovery edge cases | Medium | Graceful degradation: if recovery fails, offer to create fresh session |
| gpui-ghostty TerminalView missing features pmux needs | Medium | Evaluate in Phase 1; can contribute upstream or fork if needed |

## Success Criteria

1. All existing terminal functionality works (shell, TUI apps, Claude Code, vim)
2. Alt-screen, mouse mode, 256-color, truecolor all work without hacks
3. Agent status detection (OSC 133 + pattern matching) continues to work
4. Split pane layout works with independent terminal resize
5. Session persistence: kill pmux, relaunch, terminals recover
6. Zero capture_pane_resync or sequence injection hacks
7. Build passes on macOS with documented Zig setup
