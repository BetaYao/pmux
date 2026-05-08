# Worktree Inspector Design

## Goal

Add a lightweight project file browser and fast git diff review surface for each worktree. The entry point is the mini card right-click menu, so the user can inspect a worktree without changing the current dashboard selection or terminal focus.

## User Experience

Right-clicking a worktree card opens a context menu with two new actions:

- `Browse Files...`
- `Show Changes...`

Both actions open the same `Worktree Inspector` for the card's `worktreePath`; they differ only in the initially selected tab. `Browse Files...` starts on `Files`. `Show Changes...` starts on `Changes`.

The target worktree is always the card that received the right-click event. It does not use the currently selected agent, because a context menu action should be local to the item under the pointer.

The inspector is read-only in the first version. It does not edit files, stage changes, discard changes, or move files. Those actions can be added later after the browsing and review flows are stable.

## Interface

`Worktree Inspector` is presented as a sheet from the main window, matching the current diff overlay presentation style. It has a compact header with the worktree branch/name and two tabs:

- `Files`
- `Changes`

### Files Tab

The `Files` tab embeds a dedicated Ghostty terminal surface that runs `yazi <worktreePath>`.

This uses Yazi for full project navigation, preview, fuzzy traversal, and familiar terminal file-manager behavior. The surface is not part of the agent split-pane session model and should not be registered with `AgentHead`, `StatusPublisher`, or persisted as a worktree pane.

If `yazi` is not available on `PATH`, the tab shows a small install/help state instead of failing the inspector. The `Changes` tab remains usable.

### Changes Tab

The `Changes` tab is the native amux review surface. It extends the current diff overlay pattern:

- left side: changed-files tree grouped by directory
- right side: syntax-light diff rendering from structured `DiffFile` data
- selecting a file shows only that file's diff
- selecting a directory shows diffs for changed files under that directory
- no selection shows all diffs

The current `DiffOverlayViewController` already implements most of this visual model. It should either be evolved into the new inspector or used as the starting point for a `WorktreeInspectorViewController`.

## Context Menu Integration

Existing context menus live on the card container views:

- `StackedMiniCardContainerView.menu(for:)`
- `StackedCardContainerView.menu(for:)`

The new actions should be added to these container menus. This covers focus-layout mini cards and grid cards with the same behavior.

The delegate path should extend `AgentCardDelegate` with optional methods:

- `agentCardDidRequestBrowseFiles(agentId:)`
- `agentCardDidRequestShowChanges(agentId:)`

`DashboardViewController` resolves `agentId` to `AgentDisplayInfo.worktreePath` and forwards the request through its dashboard delegate. `MainWindowController` owns final presentation, matching the existing `showDiffOverlay` responsibility.

## Git Data Model

The current `GitDiff` implementation should be strengthened before it backs the inspector:

- represent staged, unstaged, and untracked changes separately
- preserve actual file status in `DiffFile.status`
- handle renamed files explicitly
- render untracked small text files as synthetic all-addition diffs
- detect binary or oversized files and show a concise non-renderable-file state instead of trying to render content
- keep git calls off the main thread

The first version can keep the existing inline diff renderer. A side-by-side diff is out of scope.

## Yazi Integration

Yazi is treated as an optional runtime dependency, not a vendored library.

The app checks availability through the existing process-runner pattern. When available, the inspector creates a temporary, non-persistent `TerminalSurface` with:

- working directory: the target worktree path
- command: `yazi <quoted worktreePath>`
- no tmux/zmx session name

The surface should be destroyed when the inspector closes.

The first implementation does not need live synchronization between Yazi hover selection and the native diff view. A later version can use Yazi's chooser/cwd files, DDS event stream, or a small Yazi plugin to publish selected paths back to amux.

References:

- https://github.com/sxyazi/yazi
- https://yazi-rs.github.io/docs/quick-start/
- https://yazi-rs.github.io/docs/dds/
- https://github.com/yazi-rs/plugins/tree/main/git.yazi
- https://github.com/yazi-rs/plugins/tree/main/vcs-files.yazi

## Error Handling

- Missing `yazi`: show install/help state in `Files`; keep `Changes` available.
- Non-git worktree or git command failure: show an empty/error state in `Changes` with the target path.
- Large text diff: show a concise size-limit state and avoid blocking UI.
- Binary file: show a binary-file state.
- Inspector close: destroy the temporary Yazi surface and cancel outstanding diff loads.

## Testing

Unit tests should cover:

- mini card and grid card context menus include `Browse Files...` and `Show Changes...`
- delegate methods receive the right `agentId`
- dashboard resolves the right-clicked card to the correct `worktreePath`
- `GitDiff` parses staged, unstaged, untracked, renamed, and binary cases
- missing `yazi` state is selectable without preventing `Changes`

Focused UI or integration tests should cover:

- right-click mini card opens inspector on `Files`
- right-click mini card `Show Changes...` opens inspector on `Changes`
- the action target is the right-clicked card, not the selected card

## Out of Scope

- editing files in the inspector
- staging, unstaging, or discarding changes
- side-by-side diff
- live Yazi hover-to-native-preview synchronization
- replacing the existing terminal split-pane model with Yazi

## Implementation Notes

The work should stay close to existing app boundaries:

- `Sources/UI/Dashboard/*` owns card menus and local card actions.
- `DashboardViewController` translates card IDs into worktree paths.
- `MainWindowController` owns inspector presentation.
- `Sources/Git/GitDiff.swift` owns git parsing and diff snapshots.
- `Sources/UI/Diff/*` or a new `Sources/UI/Inspector/*` owns the inspector UI.

The first version should favor predictable, read-only browsing over broad file-manager operations.
