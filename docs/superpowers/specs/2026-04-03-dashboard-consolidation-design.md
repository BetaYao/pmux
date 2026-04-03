# Dashboard Consolidation Design

**Date:** 2026-04-03
**Status:** Approved

## Summary

Consolidate the UI by removing the separate project detail (RepoViewController) interface. All interaction happens within the dashboard. The focus panel in non-grid layouts gains split-pane terminal support (reusing existing SplitContainerView), and the title bar simplifies from a tab list to a single worktree info capsule.

## Goals

1. Remove the project detail interface — no more "Open in Tab" concept
2. Dashboard focus panel supports split-pane terminals (reuse existing split system)
3. Title bar left capsule shows selected worktree info instead of tab list
4. Sidebar collapse toggle for non-grid layouts
5. Grid layout unchanged

## Components to Delete

- **RepoViewController** (`Sources/UI/Repo/RepoViewController.swift`) — functionality absorbed by dashboard focus panel + SplitContainerView
- **SidebarViewController** (`Sources/UI/Repo/SidebarViewController.swift`) — worktree list role taken by dashboard mini cards
- **ProjectTabView** in TitleBarView — no more repo tabs
- Tab-related logic in TabCoordinator: `repoVCs` cache, `switchToTab` repo branch, repo tab creation/closing
- `RepoViewDelegate` protocol usage in MainWindowController

## Components to Modify

### DashboardViewController

**Focus panel upgrade:** In non-grid layouts (leftRight, topSmall, topLarge), the FocusPanelView currently embeds a single terminal surface. After this change:

- When a worktree is selected (via card click), embed that worktree's `SplitContainerView` into the focus panel area
- Obtain the SplitTree from `TerminalSurfaceManager` (same as current RepoViewController.showTerminal flow)
- On worktree switch: detach old SplitContainerView, embed new one
- Split keyboard shortcuts (Cmd+D, Cmd+Shift+D, Cmd+Opt+Arrow) operate on the focus panel's SplitContainerView

**No change to grid layout** — cards display terminal previews as before, no focus panel.

### TitleBarView — Left Capsule

**Non-grid layouts (worktree selected):**

```
┌──────────────────────────────────────────────────────────────────┐
│ ● feature/auth-refactor · amux-main · Running · claude-code [+][≡] │
└──────────────────────────────────────────────────────────────────┘
```

Content (left-aligned):
- Status dot (color matches agent status: green=running, yellow=waiting, etc.)
- Worktree branch name (bold)
- Repo name (dimmed)
- Status text + agent name (dimmed)

Right-side buttons:
- **[+] New worktree** — opens new worktree dialog
- **[≡] Collapse sidebar** — toggles mini card area visibility (non-grid only)

**Grid layout (no selection):**

```
┌─────────────────────┐
│ AMUX Dashboard      │
└─────────────────────┘
```

No buttons displayed. Just the app title.

### TitleBarView — Right Capsule

No changes. Layout menu, Notifications, AI, Theme toggle remain as-is.

### TabCoordinator

Simplify to dashboard-only management:
- Remove `repoVCs` dictionary and repo VC caching
- Remove repo tab creation/switching logic
- Keep workspace discovery, agent data aggregation, worktree lifecycle management
- Keep status polling coordination

### MainWindowController

- Remove repo tab embedding logic from `switchToTab`
- Split pane shortcuts (Cmd+D etc.) target the dashboard's focus panel SplitContainerView instead of a repo tab's container
- Remove RepoViewDelegate conformance
- Simplify `AmuxWindow.performKeyEquivalent` — no need to check which tab type is active

### TerminalCoordinator

- Split operations (`splitFocusedPane`, `closeFocusedPane`, `moveFocus`, `resizeSplit`) now target the SplitContainerView embedded in dashboard's focus panel
- No conceptual change — just the container location moves from RepoViewController to DashboardViewController

## Sidebar Collapse Behavior

Toggle button [≡] in title bar left capsule:

- **leftRight:** Right-side mini card column (22%) collapses to 0, focus panel expands to 100% width
- **topSmall:** Top mini card row hides, focus panel expands to full height
- **topLarge:** Bottom mini card row hides, focus panel expands to full height
- **Toggle:** Click again to restore original proportions
- **Animation:** ~0.2s frame transition
- **Persistence:** Not saved to config — always starts expanded
- **Grid layout:** Button not shown (no sidebar to collapse)

## Components Unchanged

- `SplitContainerView`, `SplitTree`, `SplitNode`, `DividerView` — full reuse
- `DimOverlayView` — unfocused pane dimming
- Grid layout (cards, drag-and-drop, zoom)
- `StatusPublisher`, `StatusDetector` — status detection pipeline
- `GhosttyBridge`, `TerminalSurface` — terminal engine
- `WorktreeDiscovery` — git worktree scanning
- `Config` — persistence (split layouts per worktree path still work)
- Right capsule of title bar

## Data Flow Changes

### Worktree Selection (non-grid layout)

1. User clicks a mini card in sidebar area
2. DashboardViewController receives selection
3. Detach current SplitContainerView from focus panel (if any)
4. Get or create SplitContainerView for selected worktree from TerminalSurfaceManager
5. Embed SplitContainerView into focus panel
6. Update title bar left capsule with worktree info
7. TerminalCoordinator now targets this SplitContainerView for split operations

### Split Pane Operation

1. User presses Cmd+D
2. AmuxWindow.performKeyEquivalent captures it
3. MainWindowController forwards to TerminalCoordinator
4. TerminalCoordinator operates on dashboard's current focus panel SplitContainerView
5. Same split logic as before — just different container parent

### Worktree Deletion

1. User deletes the currently-selected worktree
2. Kill tmux session, remove SplitContainerView
3. Auto-select the next worktree in the card list (or clear focus panel if none remain)
4. Update title bar capsule accordingly

### App Startup

1. MainWindowController creates window, sets up dashboard as sole content
2. TabCoordinator discovers repos, builds agent list
3. Dashboard renders with current layout
4. If non-grid layout: auto-select first worktree, embed its SplitContainerView
5. Status polling starts
6. Restore last selected worktree from config (instead of last active tab)
