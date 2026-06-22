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
| Col 3 implementation | **Reuse existing inspector components** (`DiffReviewView` + yazi terminal surface) docked as a column, instead of building new views — see "Column 3 — reuse decision" below |

## Architecture

### Layout ownership

`DashboardViewController` owns three sibling containers, AutoLayout:

- Col 1 & Col 3: fixed width with collapse constraints (width → 0 / hidden), reusing the
  collapse-constraint pattern already present in the old `leftRight` code.
- Col 2: takes the remaining width, pinned top/bottom.

**Cleanup (in scope):** remove the `DashboardLayout` enum and the now-unused layout-setup methods
for `grid` / `topSmall` / `topLarge`; remove the layout-switcher UI from the title bar. Two
collapse toggles remain.

### Column 3 — reuse decision

The codebase **already has** the file-browse + git-changes UI, today presented as a modal sheet
(`WorktreeInspectorViewController`):

- **Files** = a `yazi` TUI file browser in an ephemeral `TerminalSurface`
  (`TerminalSurface.createEphemeral(in:workingDirectory:command:)`; command from
  `WorktreeInspectorViewController.yaziCommand(yaziPath:configDirectory:)`). Yazi handles browsing
  and opening files itself; if yazi is not installed, a "Yazi is not installed" placeholder shows.
- **Changes** = `DiffReviewView(worktreePath:)` — changed-files list with status badges **plus an
  inline diff**. Git status parsing already done by `GitDiff.changedFileEntries(worktreePath:)`.

We dock this content as column 3 instead of building NSOutlineView + a custom list/parser. This
supersedes the earlier NSOutlineView / `$EDITOR`-on-click / `git status` parser plan. (Inline diff
in the Changes tab is now in scope — it comes free with `DiffReviewView`. The `$EDITOR`-on-click
interaction is dropped; yazi owns file opening.)

### New component

- **`WorktreeSidePanelViewController`** (`Sources/UI/SidePanel/`) — col 3 container. Top
  `NSSegmentedControl` ("Files" / "Changes") + a content view. `setWorktree(_ path: String?)`
  rebuilds the active tab's content for the given worktree (nil → empty placeholder). Tab switch
  rebuilds content. Owns/destroys its ephemeral yazi `TerminalSurface` (mirrors the inspector's
  `showSelectedTab()` / `deinit` lifecycle). Reuses `DiffReviewView` and the static
  `WorktreeInspectorViewController.yaziCommand(...)` directly — no duplication of git logic.

### Wiring / data flow

- Focused-worktree change (selection in col 1, or `updateAgents`) →
  `DashboardViewController` calls `sidePanel.setWorktree(selectedWorktreePath)` → the panel rebuilds
  the active tab. Both center terminal and right panel stay in sync on the focused worktree.
- Collapse buttons: title bar exposes two toggles (col 1 / col 3); reuse the existing
  constraint-swap + animation pattern from `toggleSidebarCollapse()`.

### Refresh

- The panel rebuilds on worktree focus change and on tab switch. `DiffReviewView` loads a fresh
  git snapshot when constructed; yazi reflects the live filesystem. No extra polling timer in v1
  (rebuild-on-focus-change is sufficient; a manual refresh can be added later).

## Testing

Reusing existing components removes most new testable logic (git parsing lives in `GitDiff`, file
browsing in yazi). New tests:

- **`WorktreeSidePanelViewController`** — `setWorktree` updates the held path and rebuilds; `nil`
  shows the empty placeholder; tab switch swaps content; switching away from Files destroys the
  yazi surface (no leak). Follows the style of the existing `WorktreeInspector` tests (inject
  `makeDiffReviewView` / yazi-surface closures to avoid real processes).
- **Collapse state** — col 1 and col 3 collapse flags toggle independently and activate the right
  constraints (logic-level assertions, mirroring existing dashboard tests).
- **Regression** — update `ConfigTests.testDefaultDashboardLayout` and
  `DashboardViewControllerClickTests` for the removed layout modes.
- View rendering itself is not unit-tested (consistent with current project practice).

## Out of scope (v1 / YAGNI)

- A custom NSOutlineView file tree (superseded — yazi is reused).
- `$EDITOR`-on-click file opening (superseded — yazi owns file opening).
- A dedicated git-status polling timer (rebuild-on-focus-change is enough).
- Pinning col 3 to an arbitrary (non-worktree) directory.
- Persisting which tab (Files/Changes) was last selected across launches.
