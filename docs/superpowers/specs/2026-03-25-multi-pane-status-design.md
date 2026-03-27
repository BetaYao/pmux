# Multi-Pane Status, LastMessage & Notification Design

## Problem

amux now supports multiple panes per worktree via SplitTree/SplitNode. However, the status detection, lastMessage tracking, and notification systems still assume a 1:1 mapping between worktree and terminal. When a worktree has multiple panes running different agents, only the last-polled pane's status is visible, messages from other panes are lost, and notifications don't indicate which pane triggered them.

## Design Decisions

- **Status display:** Multiple status dots shown side-by-side on dashboard cards, one per pane, ordered by SplitTree leaf order.
- **LastMessage:** Automatically shows the message from the most recently changed pane.
- **Notifications:** Each pane triggers independently; multi-pane worktrees include `[Pane N]` in the notification title.
- **Pane identification:** Sequential numbering (Pane 1, 2, 3...) based on SplitTree leaf order. Renumbered on pane add/close to stay contiguous.
- **Single-pane backward compatibility:** All behavior is identical to current when a worktree has one pane.

## Architecture: Aggregation Layer (Option B)

A new `WorktreeStatusAggregator` sits between the existing per-terminal tracking (`AgentHead`) and the UI/notification consumers. This provides clean separation:

```
StatusPublisher (polls each surface)
  → AgentHead (per-terminal AgentInfo storage)
    → WorktreeStatusAggregator (builds per-worktree view)
      → Dashboard (renders multi-dot cards)
      → NotificationManager (per-pane notifications)
```

### Why not extend AgentHead directly?

AgentHead's job is per-terminal agent info storage. Adding worktree-level aggregation, pane ordering, and "most recent" tracking would mix two concerns. The aggregator keeps AgentHead simple and gives the UI a clean, pre-computed view.

### Required AgentHead Changes

`AgentHead.worktreeIndex` must change from `[String: String]` (1:1) to `[String: [String]]` (1:N) to support multiple terminals per worktree. The `agent(forWorktree:)` method is **not changed** — the aggregator replaces its role for UI consumers. `worktreeIndex` becomes an internal detail used only by the aggregator for reverse lookups.

### Reverse Lookup: terminalID → worktreePath

The aggregator maintains its own `terminalToWorktree: [String: String]` mapping, built from `StatusPublisher.worktreePaths[surfaceID]` (which already exists). This avoids adding a reverse lookup to `TerminalSurfaceManager`.

### Ownership and Lifecycle

`WorktreeStatusAggregator` is created and owned by `MainWindowController` (not a singleton). It is initialized with references to `AgentHead.shared` and the surface manager. `MainWindowController` replaces its current `StatusPublisherDelegate` conformance with `WorktreeStatusDelegate` conformance. `NotificationManager` also conforms to `WorktreeStatusDelegate` — the aggregator supports multiple delegates via a delegate list or `MainWindowController` forwards calls to `NotificationManager`.

### Threading

`StatusPublisher` polls on a background queue (`pollQueue`). The `agentDidUpdate(terminalID:)` call from `StatusPublisher` dispatches to main queue before accessing `SplitTree` (which is not thread-safe). All delegate callbacks fire on main queue.

## Data Model

### PaneStatus

```swift
struct PaneStatus {
    let paneIndex: Int           // 1-based, follows SplitTree leaf order
    let terminalID: String       // TerminalSurface.id
    var status: AgentStatus
    var lastMessage: String
    var lastUpdated: Date        // When status or message last changed
}
```

### WorktreeStatus

```swift
struct WorktreeStatus {
    let worktreePath: String
    var panes: [PaneStatus]              // Ordered by SplitTree leaf position
    var mostRecentPaneIndex: Int         // Pane whose lastMessage is displayed
    var mostRecentMessage: String        // That pane's lastMessage

    // Convenience
    var statuses: [AgentStatus]          // panes.map(\.status)
    var hasUrgent: Bool                  // Any pane is error or waiting
    var highestPriority: AgentStatus     // Max by AgentStatus.priority (existing enum ordering: error=6 > exited=5 > waiting=4 > running=3 > idle=2 > unknown=1)
}
```

## WorktreeStatusAggregator

```swift
protocol WorktreeStatusDelegate: AnyObject {
    /// Called when any pane's status or message changes within a worktree.
    func worktreeStatusDidUpdate(_ status: WorktreeStatus)

    /// Called when a specific pane's AgentStatus transitions.
    func paneStatusDidChange(worktreePath: String, paneIndex: Int,
                             oldStatus: AgentStatus, newStatus: AgentStatus,
                             lastMessage: String)
}

class WorktreeStatusAggregator {
    weak var delegate: WorktreeStatusDelegate?
    private(set) var statuses: [String: WorktreeStatus]  // keyed by worktreePath

    private let agentHead: AgentHead
    private let surfaceManager: TerminalSurfaceManager
}
```

### Responsibility Matrix

| Component | Responsibility | Does NOT do |
|-----------|---------------|-------------|
| StatusPublisher | Poll each surface, detect status, update AgentHead | Aggregation, pane ordering |
| AgentHead | Store per-terminal AgentInfo | Know how many panes a worktree has |
| WorktreeStatusAggregator | Listen to AgentHead changes, query SplitTree for leaf order, build WorktreeStatus, diff and fire delegate callbacks | Read terminal content directly |
| NotificationManager | Listen to paneStatusDidChange, apply cooldown, send macOS notifications | Status aggregation |
| Dashboard views | Consume WorktreeStatus, render dots and messages | Access AgentHead directly |

### Data Flow

1. `StatusPublisher` polls a surface and detects a status/message change.
2. `StatusPublisher` updates `AgentHead.agents[terminalID]`.
3. `StatusPublisher` calls `aggregator.agentDidUpdate(terminalID:)`.
4. Aggregator looks up `worktreePath` via `surfaceManager`, finds the terminal's position in `SplitTree.allLeaves`.
5. Aggregator builds/updates `PaneStatus` for that pane, compares with previous state.
6. If status changed: fires `paneStatusDidChange(paneIndex:...)`.
7. If any change detected (status or message): fires `worktreeStatusDidUpdate(status)` with the full snapshot. No-op if nothing changed (avoids unnecessary UI rebuilds).
8. `MainWindowController` (implementing `WorktreeStatusDelegate`) rebuilds `AgentDisplayInfo` and refreshes dashboard.
9. `NotificationManager` (implementing `WorktreeStatusDelegate`) handles per-pane notifications.

### SplitTree Structure Changes

When panes are added or closed, `surfaceManager` notifies the aggregator to rebuild the affected worktree's `WorktreeStatus`. Pane indices are reassigned based on the new `SplitTree.allLeaves` order.

## Notification Changes

### Cooldown Key

```swift
// Current: worktreePath
// New: keyed by terminalID (stable across reindex)
private var lastNotificationTimes: [String: Date]  // key = terminalID
```

Each pane has independent 30-second cooldown. Uses `terminalID` (not `paneIndex`) as the cooldown key so that pane renumbering after add/close does not reset cooldowns.

### Notification Content

Notification titles remain per-status as in current code. Only the `[Pane N]` suffix is added for multi-pane worktrees:

```swift
// Single pane (paneCount == 1): unchanged from current behavior
// → idle:    "Agent finished — \(branch)"
// → waiting: "Agent needs input — \(branch)"
// → error:   "Agent error — \(branch)"

// Multi pane (paneCount > 1): add pane identifier
// → idle:    "Agent finished — \(branch) [Pane \(paneIndex)]"
// → waiting: "Agent needs input — \(branch) [Pane \(paneIndex)]"
// → error:   "Agent error — \(branch) [Pane \(paneIndex)]"
body: lastMessage
```

### NotificationHistory

`NotificationEntry` gains a `paneIndex: Int?` field (nil for single-pane). Clicking a history entry navigates to the worktree and, if multi-pane, focuses the specific pane.

### Trigger Logic

Unchanged: fires when a pane transitions from `running` to `waiting`, `error`, or `idle`. The only difference is the notification is scoped to a specific pane.

### Webhook Status Scoping

Webhook status is keyed by worktreePath and cannot distinguish individual panes. When webhook status is present, it applies to **all panes** in the worktree (overrides per-pane detection for the entire worktree). This matches current behavior where webhook is the highest priority source.

## Dashboard UI Changes

### AgentCardView (Grid Mode) — Bottom Bar

```
Before:  ● project-name          Running
After:   ●●○ project-name        Most recent lastMessage...
```

- Left: array of colored status dots, one per pane, SplitTree leaf order.
- Single pane: one dot, identical to current.
- lastMessage: `WorktreeStatus.mostRecentMessage`.

### MiniCardView (Spotlight Sidebar)

```
Before:  ● agent-name   lastMessage...   1m30s
After:   ●●○ agent-name  lastMessage...   1m30s
```

Same dot array treatment.

### FocusPanelView

- Header shows dot array; the currently viewed pane's dot is highlighted/enlarged.
- Clicking a dot switches to that pane's terminal surface.
- Existing prev/next navigation continues to work, cycling through panes.

### AgentDisplayInfo Changes

All existing fields (name, project, thread, worktreePath, totalDuration, roundDuration, etc.) are retained unchanged. Only status-related fields change:

```swift
struct AgentDisplayInfo {
    // ... all existing fields retained (name, project, thread, worktreePath, etc.)

    // Existing fields retained
    let paneCount: Int
    let paneSurfaces: [TerminalSurface]
    let surface: TerminalSurface              // mostRecent pane's surface

    // New fields
    let paneStatuses: [AgentStatus]           // From WorktreeStatus.panes
    let mostRecentMessage: String             // From WorktreeStatus
    let mostRecentPaneIndex: Int              // From WorktreeStatus

    // Removed
    // status: AgentStatus (replaced by paneStatuses; use paneStatuses[0] for single-pane compat)
}
```

`MainWindowController` builds `AgentDisplayInfo` from `WorktreeStatus` in `worktreeStatusDidUpdate`.

## Migration / Backward Compatibility

No config migration needed. Single-pane worktrees produce a `WorktreeStatus` with one `PaneStatus` entry — all UI and notification behavior is identical to current.

## Testing Strategy

- **Unit tests for WorktreeStatusAggregator:** Verify correct pane ordering, mostRecent selection, diff detection, reindex on pane add/close.
- **Unit tests for PaneStatus/WorktreeStatus:** Convenience properties (hasUrgent, highestPriority, statuses).
- **Unit tests for NotificationManager:** Per-pane cooldown keying, title formatting with/without pane suffix.
- **Integration test:** StatusPublisher → AgentHead → Aggregator → delegate callback chain.
