# Activity Event Data Pipeline

**Date:** 2026-03-30
**Status:** Draft

## Problem

The activity feed UI on grid cards is built (Task 1-7 of activity-feed-grid-cards), but `AgentInfo.activityEvents` is always empty. We need to populate it from real agent data — both webhook events (Claude Code) and terminal text parsing (other agents).

## Solution

Extend `AgentHead.handleWebhookEvent()` to create `ActivityEvent` items from `toolUseEnd`/`toolUseFailed` events, with tool-specific detail extraction. For non-webhook agents, extend `StatusPublisher` to detect tool-call-like patterns from terminal viewport text.

## Design

### Source 1: Webhook Events (AgentHead)

When `AgentHead.handleWebhookEvent()` receives a `toolUseEnd` or `toolUseFailed` event, it:

1. Extracts `tool_name` and `tool_input` from `event.data`
2. Calls a detail extractor to produce a human-readable detail string
3. Creates an `ActivityEvent(tool:detail:isError:timestamp:)`
4. Appends to `AgentInfo.activityEvents` ring buffer (max 20, newest first)

#### Detail Extraction

A new `ActivityEventExtractor` enum provides a static method `extractDetail(toolName:toolInput:)` that returns a detail string based on the tool type:

| Tool Name | `tool_input` Key | Detail Format | Example |
|-----------|------------------|---------------|---------|
| `Read` | `file_path` | file path, basename only | `main.swift` |
| `Edit` | `file_path` | path + line if present | `main.swift:42` |
| `Write` | `file_path` | file path, basename only | `config.json` |
| `Bash` | `command` | command, truncated to 60 chars | `swift test --filter Auth...` |
| `Grep` | `pattern` | quoted pattern | `"validateToken"` |
| `Glob` | `pattern` | pattern | `**/*.swift` |
| `Agent` | `prompt` | prompt, truncated to 40 chars | `Explore grid card UI code...` |
| `WebSearch` | `query` | quoted query | `"swift NSTextField"` |
| `WebFetch` | `url` | URL, truncated to 60 chars | `https://docs.swift.org/...` |
| Other | — | tool name as detail | `TaskCreate` |

For file paths, show only the basename + parent directory to keep it compact (e.g., `auth/login.swift` instead of `/Volumes/project/src/auth/login.swift`). Use `URL(fileURLWithPath:)` to extract the last 2 path components.

#### Error Detection

- `toolUseFailed` events → `isError: true`
- `toolUseEnd` with `tool_name == "Bash"`: check `tool_result` for patterns indicating failure — if `tool_result` contains `"Exit code:"` followed by a non-zero number, or contains `"error:"` (case-insensitive), mark `isError: true`
- All other `toolUseEnd` → `isError: false`

#### Ring Buffer

`AgentInfo.activityEvents` is maintained as an array with max capacity 20. When a new event is added:
- Insert at index 0 (newest first)
- If count exceeds 20, drop the last element

On `agentStop` event, clear the entire `activityEvents` array (fresh start for next session).

#### Integration Point

In `AgentHead.handleWebhookEvent()`, after the existing routing to `HooksChannel`, add activity event extraction:

```
func handleWebhookEvent(_ event: WebhookEvent) {
    // ... existing channel routing ...

    // Extract activity event from tool use events
    if event.event == .toolUseEnd || event.event == .toolUseFailed {
        let activityEvent = ActivityEventExtractor.extract(from: event)
        appendActivityEvent(activityEvent, forTerminalID: tid)
    }
}
```

### Source 2: Terminal Text Parsing (StatusPublisher)

For agents that don't send webhook events, `StatusPublisher` already reads viewport text every 2 seconds. Extend the polling to detect tool-call-like patterns and create `ActivityEvent` items.

#### Pattern Detection

Add a new method `extractActivityEvents(from text: String) -> [ActivityEvent]` to `StatusDetector` that scans the viewport text for lines matching common tool output patterns:

- Lines starting with `▸` or `⏺` followed by a tool name (Claude Code terminal output format)
- Lines matching `Reading file:`, `Editing file:`, `Running:` etc.

This is best-effort — extract what we can, skip what doesn't match. The patterns should be defined in `AgentDef` alongside existing status detection patterns, so they're agent-type specific.

#### Deduplication

Terminal text doesn't change on every poll. Use the existing viewport hash mechanism in `StatusPublisher` — only re-extract activity events when the hash changes. When re-extracting, replace the entire `activityEvents` array (since we're reading a snapshot of the viewport, not appending incrementally).

#### Limitation

Terminal text parsing can only show what's currently visible in the viewport. When the terminal scrolls, older events are lost. This is acceptable — webhook-based agents get full history (up to 20), text-parsed agents get a best-effort snapshot.

### Data Flow

```
Webhook path:
  WebhookServer → WebhookEvent
    → AgentHead.handleWebhookEvent()
    → ActivityEventExtractor.extract(from: event)
    → AgentInfo.activityEvents (ring buffer, max 20)

Terminal text path:
  StatusPublisher.pollAll()
    → Read viewport text
    → StatusDetector.extractActivityEvents(from: text)
    → AgentHead.updateActivityEvents(terminalID, events)
    → AgentInfo.activityEvents (replaced on each poll)

Both paths:
  → TabCoordinator.buildAgentDisplayInfos()
    → AgentDisplayInfo.activityEvents
    → DashboardViewController → AgentCardView
    → ActivityFeedRenderer → UI
```

### New Files

- `Sources/Core/ActivityEventExtractor.swift` — detail extraction from webhook events + terminal text patterns

### Modified Files

- `Sources/Core/AgentHead.swift` — add activity event creation in `handleWebhookEvent()`, add `appendActivityEvent()` and `updateActivityEvents()` methods
- `Sources/Status/StatusPublisher.swift` — call `StatusDetector.extractActivityEvents()` during poll, pass to AgentHead
- `Sources/Status/StatusDetector.swift` — add `extractActivityEvents(from:agentDef:)` method
- `Sources/App/TabCoordinator.swift` — already passes `agent.activityEvents` (done in previous feature)

### Thread Safety

- `AgentHead.appendActivityEvent()` uses the existing `NSLock`
- `StatusDetector.extractActivityEvents()` is a pure function (no state)
- `ActivityEventExtractor.extract()` is a pure function (no state)
- No new threading concerns

## Out of Scope

- Custom event sources beyond webhook and terminal text
- Persisting activity events across app restarts
- Configurable ring buffer size
- Agent-specific detail extraction rules beyond the table above
