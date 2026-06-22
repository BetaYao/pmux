# 3-Column Layout — Design

**Date:** 2026-06-22
**Status:** Approved (design), pending implementation plan

## Summary

Replace amux's current 4-mode dashboard (`grid` / `leftRight` / `topSmall` / `topLarge`)
with a single fixed **3-column layout**:

```
┌──────────┬─────────────────────┬──────────────┐
│ Col 1    │ Col 2               │ Col 3        │
│ Worktrees│ Terminal (Focus     │ File Tree /  │
│ cards +  │ Panel + Split       │ Git Changes  │
│ new task │ Container)          │ (toggle)     │
│ [collap] │ flexible, fills     │ [collapse]   │
└──────────┴─────────────────────┴──────────────┘
```

- **Col 1** (~260pt, collapsible): existing worktree mini-card stack + `InlineWorktreeCreateView`,
  moved from the current right side to the left.
- **Col 2** (flexible): existing `FocusPanelView` → `SplitContainerView`, internally unchanged.
- **Col 3** (~300pt, collapsible): **new** panel toggling between a File Tree and a Git Changes list.

Stay AppKit (no SwiftUI, no third-party packages) — consistent with the project's existing
architecture and the "No external SPM dependencies" constraint.

## Decisions

| Topic | Decision |
|-------|----------|
| Layout modes | 3-column **replaces** all 4 existing modes; enum + unused setup methods removed |
| Col 3 content | **Toggle** between File Tree and Git Changes (segmented control), not stacked |
| Collapsing | **Both** col 1 and col 3 collapsible, each with its own title-bar toggle; col 2 fills remainder |
| Scope source | Both col-3 views are **worktree-scoped** — follow the focused worktree |
| File tree click | Runs `$EDITOR <path>` in the **center column's active terminal pane** (fallback `vi`) |
| Git changes | Read-only **list** of changed files with status badges (M/A/D/?/R); no inline diff in v1 |
| File tree lib | Native `NSOutlineView`; no third-party component |

## Architecture

### Layout ownership

`DashboardViewController` owns three sibling containers, AutoLayout:

- Col 1 & Col 3: fixed width with collapse constraints (width → 0 / hidden), reusing the
  collapse-constraint pattern already present in the old `leftRight` code.
- Col 2: takes the remaining width, pinned top/bottom.

**Cleanup (in scope):** remove the `DashboardLayout` enum and the now-unused layout-setup methods
for `grid` / `topSmall` / `topLarge`; remove the layout-switcher UI from the title bar. Two
collapse toggles remain.

### New components (`Sources/UI/SidePanel/`)

- **`WorktreeSidePanelViewController`** — col 3 container. Top segmented toggle (Files / Git);
  swaps between the two child VCs below. Holds the current focused-worktree path. Toggle state
  persists per session.
- **`FileTreeViewController`** — `NSOutlineView` + custom data source. Rooted at the focused
  worktree directory. Lazy-loads children on expand via `FileManager`. Hides dotfiles by default
  (no `.gitignore` filtering in v1). Clicking a file → delegate callback up to the dashboard.
- **`GitChangesViewController`** — read-only `NSTableView` / stack listing `git status --porcelain`
  output for the worktree: status badge (M/A/D/?/R, including renamed `->` form) + relative path.
  Empty → "No changes"; non-git → "Not a Git repository" placeholder.

### Wiring / data flow

- Focused-worktree change (from `TabCoordinator` / `AgentHead`) →
  `DashboardViewController.sidePanel.setWorktree(path)` → both child VCs re-root / reload.
- File click: `FileTreeViewController` → delegate → `DashboardViewController` → existing
  `splitContainerDelegate` / surface-input API → sends `$EDITOR <path>\n` to the center column's
  active pane.
- Collapse buttons: title bar exposes two toggles (col 1 / col 3); reuse existing collapse logic.
- **Git execution reuse:** reuse the existing process/`git` execution path used by
  `WorktreeDiscovery` (which already runs `git worktree list --porcelain`) rather than introducing
  a second git-runner.

### Refresh

- **File tree:** refreshes on worktree focus change + a manual refresh action.
- **Git changes:** reloads on (a) worktree focus change, (b) manual refresh, and (c) a lightweight
  poll (its own timer or piggybacking the existing `StatusPublisher` 2s tick). Only the focused
  worktree is polled; `git status --porcelain` only.

## Testing

- **`git status --porcelain` parser** — pure function; unit-tested across status codes
  (M / A / D / `??` / R with `old -> new` rename form).
- **File-tree data source** — directory read + dotfile filtering logic (pure parts) unit-tested.
- **`$EDITOR` command construction** — including `vi` fallback when `$EDITOR` unset — unit-tested.
- NSOutlineView / NSTableView rendering itself is not unit-tested (consistent with current project
  practice).

## Out of scope (v1 / YAGNI)

- Inline git diff view.
- `.gitignore`-aware file-tree filtering.
- File-tree context menus, rename/delete, drag-drop.
- Pinning col 3 to an arbitrary (non-worktree) directory.
