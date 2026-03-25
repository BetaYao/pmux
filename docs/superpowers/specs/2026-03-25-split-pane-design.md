# Split Pane Terminal Support

Recursive split-pane terminals within a single worktree in the repo detail view, using the zmx backend.

## Context

Current architecture: one worktree = one TerminalSurface = one zmx session. Users need multiple terminals per worktree (e.g., AI agent + dev server + log tail). zmx has no native pane support, so pmux manages splits at the AppKit layer with independent zmx sessions per pane.

## Data Model

### SplitNode (recursive enum)

```swift
enum SplitAxis {
    case horizontal  // left | right
    case vertical    // top / bottom
}

enum SplitNode {
    case leaf(id: String, surface: TerminalSurface)
    case split(
        id: String,
        axis: SplitAxis,
        ratio: CGFloat,       // 0.0...1.0, default 0.5
        first: SplitNode,
        second: SplitNode
    )
}
```

### SplitTree (wrapper)

```swift
class SplitTree {
    var root: SplitNode          // starts as single leaf
    var focusedId: String        // which leaf has keyboard focus
    var worktreePath: String
}
```

- Each leaf is an independent TerminalSurface with its own zmx session.
- Single-pane worktree = SplitTree with one leaf node.
- TerminalSurfaceManager changes from `[String: TerminalSurface]` to `[String: SplitTree]`.

### zmx Session Naming

- First pane keeps original name: `pmux-<parent>-<name>` (backward compatible).
- Additional panes append index: `pmux-<parent>-<name>-1`, `pmux-<parent>-<name>-2`, etc.
- Index is monotonically increasing per worktree (never reused within a session).
- No persistent counter needed: on split, derive next index by scanning existing session names in the SplitTree (max index + 1). On restart, indices are restored from persisted layout.

## Layout Engine

### SplitContainerView

An NSView that owns a SplitTree and recursively computes frame-based layout.

```swift
class SplitContainerView: NSView {
    var tree: SplitTree
    private var dividers: [String: DividerView]  // keyed by split node id

    func layoutTree() {
        layoutNode(tree.root, in: bounds)
    }

    private func layoutNode(_ node: SplitNode, in rect: CGRect) {
        switch node {
        case .leaf(_, let surface):
            surface.view.frame = rect
        case .split(let id, let axis, let ratio, let first, let second):
            // Compute firstRect, dividerRect, secondRect based on axis + ratio
            // Position divider view
            // Recurse into first and second
        }
    }
}
```

- Uses frame-based layout (`translatesAutoresizingMaskIntoConstraints = true`), consistent with existing grid mode.
- Embedded in RepoViewController's terminalContainer, replacing direct surface embedding.
- On window resize: `resizeSubviews()` → `layoutTree()` → each leaf surface auto `syncSize()` → zmx handles SIGWINCH.

### DividerView

```swift
class DividerView: NSView {
    let splitNodeId: String
    let axis: SplitAxis
    weak var delegate: DividerDelegate?
}

protocol DividerDelegate: AnyObject {
    func dividerDidMove(_ id: String, newRatio: CGFloat)
}
```

- 4px wide (horizontal split) or tall (vertical split).
- Hover: highlight + cursor change (`col-resize` / `row-resize`).
- Drag: `mouseDragged` → `delegate.dividerDidMove(id, newRatio)` → update tree ratio → `layoutTree()`.
- Double-click: reset ratio to 0.5.
- Minimum pane size: 100px (prevents collapse).

## Split Operations

### Create Split (Cmd+D horizontal, Cmd+Shift+D vertical)

1. Find focused leaf in tree.
2. Create new zmx session with indexed name.
3. Create new TerminalSurface with that session, working directory = worktree path.
4. Replace focused leaf with `split(axis, 0.5, oldLeaf, newLeaf)`.
5. Set `focusedId = newLeaf.id`.
6. Register new surface in StatusPublisher.
7. `layoutTree()` to position both panes.
8. Trigger config auto-save (layout persistence).

### Close Pane (Cmd+Shift+W)

1. Kill zmx session for focused leaf.
2. Destroy TerminalSurface.
3. Unregister from StatusPublisher.
4. Find parent split node → replace with sibling.
5. Set `focusedId = sibling`'s first leaf (or sibling itself if leaf).
6. Remove divider view.
7. `layoutTree()` → sibling expands to fill space.
8. Last remaining pane cannot be closed.
9. Trigger config auto-save.

### Focus Navigation (Cmd+Option+Arrow)

Spatial navigation: from focused leaf's frame center, find the nearest leaf whose frame overlaps in the target direction.

- `Cmd+Opt+←` — nearest leaf to the left.
- `Cmd+Opt+→` — nearest leaf to the right.
- `Cmd+Opt+↑` — nearest leaf above.
- `Cmd+Opt+↓` — nearest leaf below.
- No-op if no pane exists in that direction.

### Keyboard Resize

- `Cmd+Ctrl+←/→` — adjust ratio ±0.05 on the nearest ancestor horizontal split.
- `Cmd+Ctrl+↑/↓` — adjust ratio ±0.05 on the nearest ancestor vertical split.
- `Cmd+Ctrl+=` — reset nearest ancestor split ratio to 0.5.

## Status Detection

StatusPublisher changes:

- **Old**: poll single surface per worktree path.
- **New**: poll all leaf surfaces in the SplitTree for that worktree.
- Aggregate per-worktree status: `AgentStatus.highestPriority(leafStatuses)`.
- Priority order: Error > Waiting > Running > Idle > Exited > Unknown.
- AgentHead external API unchanged — still stores one aggregated status per worktree.
- DebouncedStatusTracker continues to work per-surface (each leaf tracked independently before aggregation).

## Dashboard Integration

- Agent cards show aggregated worktree status (unchanged) plus pane count indicator (e.g., "3 panes").
- Focus panel displays only the focused leaf's surface (reparented temporarily).
- Mini cards do not show split layout details.
- On tab switch back to repo: focused leaf surface reparented back into SplitContainerView → `layoutTree()`.

## Reparenting & Tab Switching

- **Tab switch away from repo**: entire SplitContainerView removed from superview. All leaf surfaces remain as SplitContainerView's subviews (no per-surface detach needed).
- **Tab switch back to repo**: re-add SplitContainerView to terminalContainer → `layoutTree()`.
- **Dashboard focus panel**: temporarily reparent focused leaf's surface view. On return, reparent back and re-layout.

## Layout Persistence

SplitNode implements `Codable`. Config stores split layouts per worktree path:

```json
{
  "splitLayouts": {
    "/path/to/worktree": {
      "type": "split",
      "axis": "horizontal",
      "ratio": 0.5,
      "first": { "type": "leaf", "sessionName": "pmux-repo-branch" },
      "second": { "type": "leaf", "sessionName": "pmux-repo-branch-1" }
    }
  }
}
```

- On launch: read layout → attach zmx sessions by name → rebuild SplitTree.
- Single-pane worktrees have no entry (backward compatible).
- Split/close triggers config auto-save (existing debounce mechanism).
- If a persisted zmx session no longer exists at launch, fall back to creating a fresh session.

## File Changes

### New Files

| File | Purpose |
|------|---------|
| `Sources/Terminal/SplitNode.swift` | SplitNode enum, SplitAxis, Codable conformance |
| `Sources/Terminal/SplitTree.swift` | SplitTree class: tree mutations, focus tracking, leaf enumeration |
| `Sources/UI/Split/SplitContainerView.swift` | Frame-based recursive layout, divider management |
| `Sources/UI/Split/DividerView.swift` | Draggable divider with hover/resize/double-click |

### Modified Files

| File | Change |
|------|--------|
| `TerminalSurfaceManager` | Storage: `[String: TerminalSurface]` → `[String: SplitTree]`. Add tree-level APIs. |
| `RepoViewController` | Embed SplitContainerView instead of direct surface. Forward split/close/focus key events. |
| `StatusPublisher` | Poll all leaves per tree. Aggregate status with `highestPriority()`. |
| `MainWindowController` / `PmuxWindow` | Intercept split keybindings (Cmd+D, Cmd+Shift+D, Cmd+Shift+W, Cmd+Opt+arrows, Cmd+Ctrl+arrows). |
| `SessionManager` | Support indexed session names for additional panes. |
| `Config` | Add `splitLayouts: [String: CodableSplitNode]` field with `decodeIfPresent` for backward compat. |
| `DashboardViewController` | Show pane count on agent cards. |

### Unchanged

- `TerminalSurface` — each leaf is still one surface, no API change.
- `GhosttyBridge` — no change.
- `AgentHead` — stores aggregated status per worktree, API unchanged.
- `WorktreeDiscovery` — no change.
- `WorkspaceManager` — no change.

## Out of Scope

- Split layout in dashboard focus panel (show focused leaf only).
- Drag-to-rearrange panes.
- Named/typed panes (agent vs auxiliary — user decides).
- tmux backend split support (zmx only for now).
