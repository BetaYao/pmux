# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

amux (AMUX — Agent Multiplexer) is a native macOS terminal multiplexer built with Swift + AppKit. It integrates the Ghostty terminal engine (via C bindings through GhosttyKit.xcframework) to render terminals, uses tmux for session persistence, and provides a dashboard UI for browsing git worktrees with agent status detection.

## Build Commands

```bash
# Generate Xcode project from project.yml (requires xcodegen)
xcodegen generate

# Build
xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build

# Run tests
xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test

# Run a single test class
xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/ConfigTests

# Run a single test method
xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/ConfigTests/testDefaultConfig

# Run UI tests
xcodebuild -project amux.xcodeproj -scheme amuxUITests -configuration Debug test

# Clean build
xcodebuild -project amux.xcodeproj -scheme amux clean
```

The project uses XcodeGen (`project.yml`) to generate the Xcode project file. After modifying `project.yml`, regenerate with `xcodegen generate`.

## Architecture

**Three-layer design:**

1. **UI Layer** (`Sources/UI/`) — AppKit views and view controllers
   - `Dashboard/` — Grid mode (responsive card grid) and focus layouts (left-right, top-bottom) that embed SplitContainerView directly in the dashboard's focus panel
   - `TabBar/` — Tab switching; dashboard is always tab 0
   - `Dialog/` — Quick switcher (Cmd+P) and new branch dialog (Cmd+N)
   - `Shared/` — Theme constants and StatusBadge component

2. **Core Services** (`Sources/Core/`, `Sources/Status/`)
   - `WorkspaceManager` — Tracks tabs and workspaces; disambiguates repo names when parent dirs differ
   - `StatusPublisher` — Timer-based polling (2s interval) of terminal surfaces for agent status
   - `StatusDetector` — Three-layer priority: process exit > OSC 133 shell phase > text pattern matching > Unknown
   - `Config` — Loads/saves JSON config from `~/.config/amux/config.json`; auto-saves on UI changes (zoom, reorder, repo add)
   - `AgentStatus` — Enum: Running, Idle, Waiting, Error, Exited, Unknown

3. **Terminal & System** (`Sources/Terminal/`, `Sources/Git/`)
   - `GhosttyBridge` — Singleton wrapping the Ghostty C API (`ghostty.h` via bridging header)
   - `TerminalSurface` — NSView + Metal renderer + PTY; surfaces are lazily created and reparented between views
   - `WorktreeDiscovery` — Runs `git worktree list --porcelain` to discover worktrees

## Key Patterns

**Surface lifecycle:** TerminalSurface instances are long-lived — created once per worktree, reparented between views (grid cards, focus panel), and destroyed only on explicit deletion or app quit.

**Key orchestrator:** `MainWindowController` owns all TerminalSurface instances (keyed by worktree path), manages tab bar state, and coordinates view transitions.

**Grid vs Spotlight layout:** Grid mode uses frame-based layout (`translatesAutoresizingMaskIntoConstraints = true`). Spotlight mode uses Auto Layout. Sidebar terminals in spotlight are output-only (`setFocus(false)`).

**Terminal persistence:** tmux sessions named `amux-<parent>-<name>` (dots/colons sanitized to underscores) are created per worktree and reattached on relaunch. Sessions are killed via `tmux kill-session` on repo tab close.

**Status detection:** `StatusPublisher` polls every 2s, reads viewport text, finds matching `AgentDef` (case-insensitive CLI name match), runs through `StatusDetector` priority rules, and fires delegate callback. `DebouncedStatusTracker` preserves current state when detection returns Unknown (debouncing).

**Window key handling:** Custom `AmuxWindow` subclass intercepts Escape (exit spotlight) and Ctrl+Tab (cycle spotlight focus) via `sendEvent()` override.

## Key Technical Details

- **Swift 5.10**, macOS 14.0+ (Sonoma), AppKit (not SwiftUI)
- **Ghostty C interop** via `amux-Bridging-Header.h` → `ghostty.h`
- Links against: Metal, QuartzCore, IOSurface, Carbon, libghostty, libc++
- No external SPM dependencies — pure system frameworks + Ghostty
- Delegate pattern used throughout: `DashboardDelegate`, `TabBarDelegate`, `TerminalCardDelegate`, `SplitContainerDelegate`
- `GhosttyBridge.shared` is the singleton entry point for all terminal operations
- Tests use XCTest with `@testable import amux`; no external test dependencies
- Config uses `decodeIfPresent()` throughout for backward compatibility with older config files
