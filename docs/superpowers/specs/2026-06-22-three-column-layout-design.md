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

## Revision 2 (2026-06-22) — otty-style header + native panel + center overlay

After Tasks 1–6 shipped, the design was revised. These changes SUPERSEDE the yazi/in-panel-diff
approach from Revision 1 for the right panel, and flatten the header.

### Decisions (Revision 2)

| Topic | Decision |
|---|---|
| Header | **Remove the capsule entirely** (leftArcBlock + all capsule labels + usage progress bars). Flatten to otty-style: a plain **centered title** (focused worktree/session title) + the existing **right-side flat icon buttons** (theme, collapse-left, collapse-right, clean-worktrees). No token/usage readout in the header. |
| Right panel "Files" | **Native `NSOutlineView` file tree** rooted at the focused worktree. **Drop yazi entirely.** Lazy-load dirs via `FileManager`, hide dotfiles. |
| Right panel "Changes" | **Plain list** of changed files (status badge + path) from `GitDiff.changedFileEntries(worktreePath:)`. **No inline diff in the panel.** |
| Detail location | **Center overlay.** Clicking a file in the tree → its content opens as a **full-cover overlay over the center terminal**. Clicking a changed file → `DiffReviewView` (reused) opens as a full-cover overlay. The terminal keeps running underneath; dismiss with **Esc or a close button**. |

### Components (Revision 2)

- **Header flatten** — `TitleBarView`: delete the capsule subtree and its state (timer, frames, usage bars,
  `usesFocusedWorktreeMode`, capsule tracking area). Keep a single centered `NSTextField` title driven by the
  existing `updateFocusedWorktree(title:tokenText:)` (token text ignored). Keep the right icon buttons and
  `setWindowHovered`/`updateChromeState` (button-only). Remove `updatePrimaryCapsuleFrames` and its call site;
  neutralize `updateNotificationSummary` (no capsule border to color).
- **Center overlay host** — `DashboardViewController.showCenterOverlay(_ content: NSView, title: String)` /
  `dismissCenterOverlay()`. A container pinned over `leftRightFocusPanel` (full cover) with a small header
  (title + close button) and Esc handling; terminal `SplitContainerView` stays embedded underneath.
- **File tree** — `FileTreeOutlineController` (`Sources/UI/SidePanel/`): `NSOutlineView` + `FileManager` lazy
  data source, dotfiles hidden. Selecting a file → delegate callback with the file path.
- **File content viewer** — `FileContentView` (`Sources/UI/SidePanel/`): read-only mono `NSTextView`. Reads
  text via `String(contentsOf:encoding:)`; shows a placeholder for binary/unreadable/oversized (>1 MB) files.
- **Changes list** — a plain list (NSTableView or stacked rows) of `GitChangedFile` (fields: `path`, `oldPath`,
  `status`, `stage`). Selecting a row → delegate callback with the path.
- **Side panel rewrite** — `WorktreeSidePanelViewController`: Files tab hosts the file tree; Changes tab hosts
  the list. Remove `makeYaziSurface` and the in-panel `DiffReviewView`. Add a delegate
  (`WorktreeSidePanelDelegate`) with `didSelectFile(path:)` and `didSelectChange(path:)`, routed by the
  dashboard to `showCenterOverlay`. `DiffReviewView` is now used ONLY inside the center overlay (for changes).

### Testing (Revision 2)

- `FileTreeOutlineController` data source: directory listing + dotfile filtering (pure logic) — unit-tested
  against a temp directory.
- `FileContentView` read helper: returns text for a UTF-8 file, nil/placeholder for missing/oversized — unit-tested.
- `WorktreeSidePanelViewController`: tab switch, `setWorktree`, and that selecting a file/change fires the
  delegate (injected spy) — update existing tests (yazi cases removed).
- Header + overlay are view wiring — build + manual run.

## Out of scope (v1 / YAGNI)

- A custom NSOutlineView file tree (superseded — yazi is reused).
- `$EDITOR`-on-click file opening (superseded — yazi owns file opening).
- A dedicated git-status polling timer (rebuild-on-focus-change is enough).
- Pinning col 3 to an arbitrary (non-worktree) directory.
- Persisting which tab (Files/Changes) was last selected across launches.
