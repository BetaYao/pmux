# Activity Feed on Grid Cards

**Date:** 2026-03-30
**Status:** Draft

## Problem

Grid cards in dashboard mode display a large empty black terminal container with at most a single line of "last message" text. The space is underutilized and doesn't convey what each agent is doing.

## Solution

Replace the empty terminal container area with a **live activity feed** — a monospaced, terminal-log-style list of agent actions (tool calls, commands, errors) that scrolls newest-first with a fade-out effect on older entries.

## Design

### Feed Layout

The activity feed occupies the existing `terminalContainer` area in `AgentCardView`. It replaces the current `messageLabel` overlay.

- **Newest entry at top**, older entries pushed down
- **Progressive opacity fade**: entries lose opacity as they get further from the top (e.g., 1.0 → 0.6 → 0.35 → 0.15)
- **Clips to container bounds** — no scrolling needed, entries that overflow simply aren't visible
- Content padding: 10pt on all sides, matching current `messageLabel` insets

### Entry Format

Each entry is a single line with three parts:

```
▸ Read   src/auth/login.swift
▸ Edit   src/auth/login.swift:42
✗ Bash   swift test — 2 failures
▸ Grep   "validateToken"
```

| Part | Font | Color |
|------|------|-------|
| Marker (`▸` normal, `✗` error) | Monospaced 11pt | Normal: `SemanticColors.accent` (cyan), Error: `SemanticColors.danger` (red) |
| Tool name | Monospaced 11pt medium | Normal: `SemanticColors.text`, Error: `SemanticColors.danger` |
| Detail (file path, command, pattern) | Monospaced 11pt regular | Normal: `SemanticColors.muted`, Error: dimmed red |

Line height: ~1.7x font size for readability.

### Error Highlighting

Entries with `isError: true` render entirely in red tones:
- Red marker (`✗` instead of `▸`)
- Red tool name
- Dimmed red detail text

This makes errors immediately visible when glancing at a card.

### Event Data Model

```swift
struct ActivityEvent {
    let tool: String        // "Read", "Edit", "Bash", "Grep", "Write", etc.
    let detail: String      // file path, command, search pattern, etc.
    let isError: Bool       // true for failures (test failures, non-zero exit, etc.)
    let timestamp: Date
}
```

The card stores a fixed-size ring buffer of recent events (e.g., last 20). On each configure() call, the feed view rebuilds its displayed entries from this buffer.

### Data Source (Agent-Agnostic)

The design is agnostic to how events are produced. The app receives `ActivityEvent` items through whichever mechanism the agent head supports:

- **Webhook events** — structured tool call notifications via the existing `WebhookStatusProvider`
- **JSON file polling** — reading a JSON file that the agent head writes to (e.g., `~/.cache/amux/events/<worktree-id>.json`)
- **Terminal text parsing** — extracting tool calls from viewport text patterns (existing `StatusDetector` approach, extended)

The `AgentDisplayInfo` view model gains a new `activityEvents: [ActivityEvent]` property, populated by whichever provider is active. The card view consumes this array without knowing the source.

### Integration with Existing UI

- **Replaces `messageLabel`** in grid mode — the activity feed is the new primary content for the terminal container area
- **Task list rendering** (`TaskListRenderer`) still takes priority when tasks are available — if `tasks` is non-empty, show the task list instead of the activity feed
- **Bottom bar unchanged** — status dots, project name, status text remain as-is
- **MiniCardView unchanged** — mini cards in sidebar continue showing `lastMessage` (not enough space for a feed)
- **FocusPanelView unchanged** — focus mode shows the live terminal, not the feed

### Priority Order for Card Content

1. **Live terminal** — when a terminal surface is embedded (repo tab view), show the terminal
2. **Task list** — when `tasks` is non-empty, show the task list (existing behavior)
3. **Activity feed** — when `activityEvents` is non-empty, show the feed
4. **Last message** — fallback, show `lastMessage` text (existing behavior)

### Implementation in AgentCardView

The feed is rendered as a vertical stack of `NSTextField` labels inside `terminalContainer`, replacing the single `messageLabel`. Each label represents one event line. Opacity is set per-label based on its index (0 = newest = full opacity).

No `NSScrollView` needed — the feed simply shows as many entries as fit in the container, clipped at the bottom. This keeps the implementation simple and avoids scroll-related complexity in grid mode.

### No Grouping

Every action gets its own line. No collapsing of repeated tool types (e.g., 5 consecutive Read calls show as 5 lines).

## Out of Scope

- Data source implementation details (webhook parsing, JSON file format) — those are separate concerns per agent head
- Feed in mini cards or focus panel
- Interactive elements (clicking a feed entry to navigate)
- Filtering or searching the feed
- Persisting feed history across app restarts
