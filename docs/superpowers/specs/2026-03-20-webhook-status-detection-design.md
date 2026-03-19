# Webhook-Based Agent Status Detection

**Date:** 2026-03-20
**Status:** Draft

## Problem

pmux currently detects agent status (Running, Idle, Waiting, Error) by polling terminal viewport text every 2 seconds and matching substrings like `"to interrupt"` or `"ERROR"`. This approach is fragile — it depends on agent UI text that can change between versions, produces false positives on matching content in code output, and has inherent 2-second latency.

Claude Code (and potentially other agents) provide hook/webhook mechanisms that emit structured events in real time. Integrating these hooks as a primary status signal would be more accurate, lower latency, and less brittle.

## Goals

1. **Hook as primary signal** — webhook events are the authoritative status source when available
2. **Text matching as parallel fallback** — continues running alongside hooks; final status is the higher-priority of the two signals
3. **Generic webhook protocol** — not tied to Claude Code; any agent can emit events in the same format
4. **Claude Code first** — implement Claude Code adapter as the first integration
5. **cwd-based worktree matching** — use the working directory from hook payloads to associate events with worktrees

## Non-Goals

- Replacing process exit detection (ProcessStatus remains highest priority)
- Implementing OSC 133 shell phase detection (separate effort)
- Configuring Claude Code hooks from pmux (user configures `~/.claude/settings.json` manually)
- Multi-instance pmux support (single pmux instance per machine assumed)

## Design

### 1. Webhook Protocol

pmux runs an HTTP server on `localhost:<port>` (default 7070, configurable via `WebhookConfig.port`). It accepts `POST /webhook` with a JSON body:

```json
{
  "source": "claude-code",
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

**Note:** Claude Code hooks support `"type": "http"` which sends HTTP POST requests directly. The hook payload includes session context and the event type. pmux needs to handle the Claude Code native format and normalize it.

### 3. Component Architecture

```
┌──────────────────────────────────────────────────┐
│                  StatusPublisher                  │
│                                                  │
│  pollAll() {                                     │
│    for each worktree:                            │
│      processStatus = surface.processStatus       │
│      textStatus = detector.detect(text matching) │
│      hookStatus = webhookProvider.status(for:)    │
│                                                  │
│      if processStatus == .exited/.error:         │
│        final = processStatus mapping             │
│      else:                                       │
│        final = highestPriority(textStatus,        │
│                                hookStatus)        │
│      update tracker with final                   │
│  }                                               │
└──────────────────┬───────────────────────────────┘
                   │ queries
    ┌──────────────┴──────────────┐
    │    WebhookStatusProvider     │
    │                             │
    │  - statuses: [path: Status] │
    │  - status(for:) → Status    │
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

**Why Network.framework over a full HTTP framework:** No external dependencies needed. The endpoint is simple (single route, JSON body). NWListener provides sufficient HTTP parsing for this use case.

### 5. WebhookStatusProvider

Maintains per-worktree hook status.

```swift
class WebhookStatusProvider {
    private var statuses: [String: AgentStatus] = []  // keyed by worktree path
    private var knownWorktrees: [String] = []

    func updateWorktrees(_ paths: [String])
    func handleEvent(_ event: WebhookEvent)
    func status(for worktreePath: String) -> AgentStatus
}
```

**cwd → worktree matching:**
1. Exact match: `event.cwd == worktreePath`
2. Prefix match: `event.cwd.hasPrefix(worktreePath)` (agent running in a subdirectory)
3. No match: log warning, discard event

**Status retention:** Hook status is retained indefinitely until a new event arrives. No timeout. This avoids false status changes during long agent thinking periods.

**Reset:** When a worktree is removed from pmux, its hook status entry is cleaned up.

### 6. Status Merge in StatusPublisher

The detection priority becomes:

```
Priority 1: ProcessStatus (.exited / .error override everything)
Priority 2: Merge of hook + text detection (take highest priority value)
Priority 3: Unknown (no signals)
```

In `pollAll()`:

```swift
// Priority 1: Process lifecycle
switch processStatus {
case .exited: finalStatus = .exited
case .error:  finalStatus = .error
default:
    // Priority 2: Merge hook + text
    let textStatus = detector.detect(processStatus:, shellInfo: nil, content:, agentDef:)
    let hookStatus = webhookProvider.status(for: path)
    finalStatus = AgentStatus.highestPriority([textStatus, hookStatus])
}
```

This means:
- If hook says Running and text says Idle → Running (hook wins, higher priority)
- If hook says Idle and text says Error → Error (text wins, higher priority)
- If hook says Running and text says Unknown → Running (hook wins)
- If no hook events received → `.unknown` from hook, text matching alone determines status

### 7. Data Flow Example

```
1. User runs Claude Code in worktree /Users/matt/project/feature-branch
2. Claude starts using the Bash tool
3. Claude Code fires PreToolUse hook → POST to localhost:7070/webhook:
   {"source": "claude-code", "event": "tool_use_start", "cwd": "/Users/matt/project/feature-branch", ...}
4. WebhookServer receives, parses → WebhookEvent
5. WebhookStatusProvider matches cwd to worktree path → sets hookStatus = .running
6. Next pollAll() cycle (within 2s):
   - textStatus from viewport = .idle (Claude UI shows prompt)
   - hookStatus = .running (from webhook)
   - merged = .running (priority 3 > priority 2)
   - Status updates to Running
7. Claude finishes, fires Stop hook → agent_stop → hookStatus = .idle
8. Next pollAll(): merged = .idle
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
