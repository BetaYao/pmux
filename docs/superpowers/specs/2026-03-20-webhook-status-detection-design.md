# Webhook-Based Agent Status Detection

**Date:** 2026-03-20
**Status:** Draft

## Problem

amux currently detects agent status (Running, Idle, Waiting, Error) by polling terminal viewport text every 2 seconds and matching substrings like `"to interrupt"` or `"ERROR"`. This approach is fragile — it depends on agent UI text that can change between versions, produces false positives on matching content in code output, and has inherent 2-second latency.

Claude Code (and potentially other agents) provide hook/webhook mechanisms that emit structured events in real time. Integrating these hooks as a primary status signal would be more accurate, lower latency, and less brittle.

## Goals

1. **Hook as primary signal** — webhook events are the authoritative status source when available
2. **Text matching as parallel fallback** — continues running alongside hooks; final status is the higher-priority of the two signals
3. **Generic webhook protocol** — not tied to Claude Code; any agent can emit events in the same format
4. **Claude Code first** — implement Claude Code adapter as the first integration
5. **cwd + session_id worktree matching** — use cwd to associate events with worktrees, session_id to track multiple concurrent sessions per worktree

## Non-Goals

- Replacing process exit detection (ProcessStatus remains highest priority)
- Implementing OSC 133 shell phase detection (separate effort)
- Configuring Claude Code hooks from amux (user configures `~/.claude/settings.json` manually)
- Multi-instance amux support (single amux instance per machine assumed)

## Design

### 1. Webhook Protocol

amux runs an HTTP server on `localhost:<port>` (default 7070, configurable via `WebhookConfig.port`). It accepts `POST /webhook` with a JSON body:

```json
{
  "source": "claude-code",
  "session_id": "abc123",
  "event": "tool_use_start",
  "cwd": "/Users/matt/workspace/myproject",
  "timestamp": "2026-03-20T12:34:56Z",
  "data": {}
}
```

**Fields:**

| Field | Type | Required | Description |
|---|---|---|---|
| `source` | string | yes | Agent identifier (e.g., `"claude-code"`, `"opencode"`) |
| `session_id` | string | yes | Unique session identifier; distinguishes multiple agent instances in the same worktree |
| `event` | string | yes | Standardized event type (see below) |
| `cwd` | string | yes | Working directory of the agent session |
| `timestamp` | string | no | ISO 8601 timestamp |
| `data` | object | no | Event-specific payload |

**Standard Event Types:**

| Event | Meaning | → AgentStatus |
|---|---|---|
| `session_start` | Agent session begins | Running |
| `tool_use_start` | Agent begins executing a tool | Running |
| `tool_use_end` | Agent finishes executing a tool | Running |
| `agent_stop` | Agent stops generating / goes idle | Idle |
| `notification` | Agent emits a notification | Depends on `data.level` |
| `error` | An error occurred | Error |
| `prompt` | Agent is waiting for user input | Waiting |

**Notification level mapping** (when event is `notification`):

| `data.level` | → AgentStatus |
|---|---|
| `"error"` | Error |
| `"warning"` | Waiting |
| other / missing | Idle |

**Response:** The server responds `200 OK` with an empty body for valid requests, `400 Bad Request` for malformed payloads, `404 Not Found` for unknown paths.

### 2. Claude Code Adapter

Claude Code hooks are configured in `~/.claude/settings.json` to POST to `http://localhost:7070/webhook`. The adapter maps Claude Code hook types to the generic protocol:

| Claude Code Hook | → Generic Event | Notes |
|---|---|---|
| `SessionStart` | `session_start` | |
| `PreToolUse` | `tool_use_start` | `data.tool` contains tool name |
| `PostToolUse` | `tool_use_end` | `data.tool` contains tool name |
| `Stop` | `agent_stop` | |
| `Notification` | `notification` | `data.level` and `data.message` extracted from payload |

The adapter is a thin translation layer. Claude Code sends its native hook format; the adapter normalizes it to the generic protocol before processing.

**Claude Code native hook payload examples** (HTTP hook sends JSON body):

`PreToolUse`:
```json
{
  "hook_event_name": "PreToolUse",
  "session_id": "sess_abc123",
  "cwd": "/Users/matt/workspace/myproject",
  "transcript_path": "/Users/matt/.claude/projects/.../transcript.jsonl",
  "permission_mode": "default",
  "tool_name": "Bash",
  "tool_input": { "command": "npm test" }
}
```

`Stop`:
```json
{
  "hook_event_name": "Stop",
  "session_id": "sess_abc123",
  "cwd": "/Users/matt/workspace/myproject",
  "transcript_path": "...",
  "stop_reason": "end_turn"
}
```

`Notification`:
```json
{
  "hook_event_name": "Notification",
  "session_id": "sess_abc123",
  "cwd": "/Users/matt/workspace/myproject",
  "title": "Task complete",
  "message": "All tests pass",
  "level": "info"
}
```

**Adapter normalization:** The adapter maps Claude Code's native fields to the generic protocol:

| Claude Code field | → Generic field |
|---|---|
| `hook_event_name` | `event` (mapped via table in section 2) |
| `session_id` | `session_id` (passed through) |
| `cwd` | `cwd` (passed through) |
| hardcoded `"claude-code"` | `source` |
| remaining fields | `data` (tool_name, tool_input, stop_reason, etc.) |

### 3. Component Architecture

```
┌──────────────────────────────────────────────────┐
│                  StatusPublisher                  │
│                                                  │
│  pollAll() {                                     │
│    for each worktree:                            │
│      textStatus = detector.detect(process+text)  │
│      hookStatus = webhookProvider.status(for:)    │
│      final = highestPriority(textStatus,          │
│                              hookStatus)          │
│      tracker.update(final)                       │
│  }                                               │
└──────────────────┬───────────────────────────────┘
                   │ queries
    ┌──────────────┴──────────────┐
    │    WebhookStatusProvider     │
    │                             │
    │  - sessions: [sid: State]   │
    │  - status(for:) → Status    │
    │    (aggregates per worktree) │
    │  - handleEvent(event)       │
    └──────────────┬──────────────┘
                   │ receives parsed events
    ┌──────────────┴──────────────┐
    │       WebhookServer         │
    │                             │
    │  - NWListener on port 7070  │
    │  - POST /webhook            │
    │  - Parses JSON, validates   │
    │  - Delegates to provider    │
    └─────────────────────────────┘
```

### 4. WebhookServer

Lightweight HTTP server using `Network.framework` (`NWListener`).

- Listens on `localhost` only (security: no external access)
- Accepts `POST /webhook` only
- Parses JSON body into `WebhookEvent` struct
- Delegates parsed event to `WebhookStatusProvider`
- Starts when `WebhookConfig.enabled == true`, stops when disabled
- Lifecycle managed by `MainWindowController` (start on launch, stop on quit)

**Why Network.framework over a full HTTP framework:** No external dependencies needed. The endpoint is simple (single route, JSON body).

**HTTP parsing strategy:** `NWListener` provides TCP connection handling, not HTTP parsing. The server uses `CFHTTPMessage` (from CFNetwork) to parse raw HTTP request bytes — this handles request line parsing, header extraction, and content-length-based body reading. This avoids writing a manual HTTP parser while staying within system frameworks.

### 5. WebhookStatusProvider

Maintains per-worktree hook status.

```swift
class WebhookStatusProvider {
    private let queue = DispatchQueue(label: "amux.webhook-status")
    private var sessions: [String: SessionState] = [:]     // keyed by session_id
    private var knownWorktrees: [String] = []

    struct SessionState {
        let sessionId: String
        let worktreePath: String
        var status: AgentStatus
        var lastEvent: Date
    }

    func updateWorktrees(_ paths: [String])
    func handleEvent(_ event: WebhookEvent)   // called from NWListener queue
    func status(for worktreePath: String) -> AgentStatus  // called from main thread
}
```

**Thread safety:** `handleEvent` is called from the NWListener dispatch queue; `status(for:)` is called from the main thread during `pollAll()`. All access to `sessions` is synchronized through a serial dispatch queue.

**Session tracking:**

Events are tracked per `session_id`. On `session_start`, a new `SessionState` is created mapping `session_id → worktreePath`. Subsequent events from the same `session_id` update that session's status. On `agent_stop`, the session status is set to `.idle` (session entry retained for future events from the same session).

**Multi-session aggregation:**

A single worktree may have multiple concurrent agent sessions (e.g., two Claude Code instances). `status(for:)` aggregates all sessions for that worktree using `highestPriority`:

```swift
func status(for worktreePath: String) -> AgentStatus {
    let sessionStatuses = sessions.values
        .filter { $0.worktreePath == worktreePath }
        .map { $0.status }
    return AgentStatus.highestPriority(sessionStatuses)  // returns .unknown if empty
}
```

This means: if one session is Running and another is Idle, the worktree shows Running.

**cwd → worktree matching:**

All paths are resolved to canonical form (`URL.resolvingSymlinksInPath()`) before comparison to handle symlinks and trailing slashes.

1. Exact match: `canonicalize(event.cwd) == canonicalize(worktreePath)`
2. Prefix match: `canonicalize(event.cwd).hasPrefix(canonicalize(worktreePath))` (agent running in a subdirectory)
3. No match: log warning, discard event

**Worktree list sync:** `StatusPublisher` calls `webhookProvider.updateWorktrees()` from its `updateSurfaces()` method, passing the current surface keys. This keeps the worktree list in sync without requiring separate management from `MainWindowController`.

**Status retention:** Session status is retained indefinitely until a new event arrives for that session. No timeout. This avoids false status changes during long agent thinking periods.

**Cleanup:** When a worktree is removed from amux, all sessions associated with that worktree are cleaned up. Stale sessions (no events for >1 hour) are pruned during `updateWorktrees()` to prevent unbounded memory growth.

### 6. Status Merge in StatusPublisher

The detection priority becomes:

```
Priority 1: ProcessStatus (.exited / .error override everything)
Priority 2: Merge of hook + text detection (take highest priority value)
Priority 3: Unknown (no signals)
```

In `pollAll()`:

```swift
// StatusDetector.detect() already handles processStatus as Priority 1
// (returns .exited/.error immediately, skipping text matching)
let textStatus = detector.detect(processStatus: processStatus, shellInfo: nil, content: content, agentDef: agentDef)
let hookStatus = webhookProvider.status(for: path)
let finalStatus = AgentStatus.highestPriority([textStatus, hookStatus])
```

No separate process status switch needed — `StatusDetector.detect()` already returns `.exited`/`.error` for those cases, and their high priority values (5/6) will naturally win in `highestPriority`.

This means:
- If hook says Running and text says Idle → Running (hook wins, higher priority)
- If hook says Idle and text says Error → Error (text wins, higher priority)
- If hook says Running and text says Unknown → Running (hook wins)
- If no hook events received → `.unknown` from hook, text matching alone determines status
- If both hook and text return `.unknown` → `.unknown`, and `DebouncedStatusTracker` preserves the previous status (existing behavior, prevents flicker)

### 7. Data Flow Example

```
1. User runs Claude Code in worktree /Users/matt/project/feature-branch
2. Claude Code fires SessionStart hook → POST to localhost:7070/webhook:
   {"hook_event_name": "SessionStart", "session_id": "sess_abc", "cwd": "/Users/matt/project/feature-branch", ...}
3. WebhookServer parses → adapter normalizes → WebhookEvent(source: "claude-code", session_id: "sess_abc", event: "session_start", ...)
4. WebhookStatusProvider creates SessionState(sessionId: "sess_abc", worktreePath: matched, status: .running)
5. Claude starts using the Bash tool → PreToolUse hook arrives → session status stays .running
6. Next pollAll() cycle (within 2s):
   - textStatus from viewport = .idle (Claude UI shows prompt)
   - hookStatus = .running (aggregated from session "sess_abc")
   - merged = .running (priority 3 > priority 2)
   - Status updates to Running
7. Claude finishes, fires Stop hook → agent_stop → session status = .idle
8. Next pollAll(): merged = .idle

Multi-session example:
9. User starts a second Claude Code session in the same worktree (session_id: "sess_xyz")
10. WebhookStatusProvider now has two sessions for this worktree:
    - sess_abc: .idle
    - sess_xyz: .running
11. status(for: worktreePath) = highestPriority([.idle, .running]) = .running
```

### 8. Configuration

Existing `WebhookConfig` in `Config.swift`:

```swift
struct WebhookConfig: Codable {
    var enabled: Bool = true
    var port: UInt16 = 7070
}
```

No changes needed. The webhook server starts/stops based on `enabled`. Port is configurable.

Users configure Claude Code hooks in `~/.claude/settings.json`:

```json
{
  "hooks": {
    "PreToolUse": [{"hooks": [{"type": "http", "url": "http://localhost:7070/webhook"}]}],
    "PostToolUse": [{"hooks": [{"type": "http", "url": "http://localhost:7070/webhook"}]}],
    "Stop": [{"hooks": [{"type": "http", "url": "http://localhost:7070/webhook"}]}],
    "Notification": [{"hooks": [{"type": "http", "url": "http://localhost:7070/webhook"}]}],
    "SessionStart": [{"hooks": [{"type": "http", "url": "http://localhost:7070/webhook"}]}]
  }
}
```

### 9. Error Handling

- **Server fails to bind port:** Log error, disable webhook, fall back to text-only detection. No crash.
- **Malformed payload:** Return 400, log warning, ignore event.
- **Unknown cwd:** Log warning, discard event. No error status change.
- **Server not running:** Claude Code hook failures are non-blocking (fire-and-forget HTTP). No impact on Claude Code operation.

### 10. Testing Strategy

- **WebhookStatusProvider:** Unit test event handling, cwd matching (exact, prefix, no match), status retention, worktree cleanup
- **WebhookServer:** Integration test with real HTTP requests to localhost
- **Status merge:** Unit test StatusPublisher with mocked webhook provider and text detector, verify highestPriority logic
- **Claude Code adapter:** Unit test hook format → generic event normalization

## New Files

| File | Purpose |
|---|---|
| `Sources/Status/WebhookServer.swift` | HTTP listener using Network.framework |
| `Sources/Status/WebhookStatusProvider.swift` | Event processing, cwd matching, status storage |
| `Sources/Status/WebhookEvent.swift` | `WebhookEvent` struct and event type enum |
| `Tests/WebhookStatusProviderTests.swift` | Unit tests for provider |
| `Tests/WebhookServerTests.swift` | Integration tests for server |

## Modified Files

| File | Change |
|---|---|
| `Sources/Status/StatusPublisher.swift` | Add `webhookProvider` property, merge hook + text status in `pollAll()` |
| `Sources/App/MainWindowController.swift` | Start/stop webhook server on launch/quit |
| `project.yml` | Add new source files |
