# Webhook Task Progress Display

**Date:** 2026-03-27
**Status:** Approved

## Problem

When Claude Code executes tasks (TaskCreate/TaskUpdate), the dashboard card's "last message" area shows generic tool messages like "Using TaskUpdate" instead of meaningful task progress. The user wants to see the full task list with completion states, matching the Claude Code terminal UI.

## Design

### Data Model

New types in `AgentInfo.swift`:

```swift
enum TaskItemStatus {
    case pending, inProgress, completed
}

struct TaskItem {
    let id: String
    var subject: String
    var status: TaskItemStatus
}
```

Add `var tasks: [TaskItem] = []` to `WebhookStatusProvider.SessionState`.

### Event Interception

In `WebhookStatusProvider.handleEvent()`, intercept task-related tool calls:

- **PostToolUse + tool_name "TaskCreate"**: Parse `tool_input.subject` and task ID from tool response. Append `TaskItem(status: .pending)`.
- **PostToolUse + tool_name "TaskUpdate"**: Parse `tool_input.taskId` and `tool_input.status`. Map `"in_progress"` to `.inProgress`, `"completed"` to `.completed`, `"pending"` to `.pending`.
- **agentStop**: Clear the session's task list (tasks are per-conversation).

Task ID assignment: TaskCreate webhook events may include a task ID in the response/output. If not available, assign incrementing IDs (`"1"`, `"2"`, ...) per session based on creation order.

### Data Flow

```
Claude Code PostToolUse(TaskCreate/TaskUpdate)
    -> WebhookStatusProvider.handleEvent() builds [TaskItem] per session
    -> WebhookStatusProvider.tasks(for: worktreePath) -> [TaskItem]
    -> StatusPublisher reads tasks alongside lastMessage
    -> AgentHead.updateStatus() receives [TaskItem]
    -> AgentInfo stores tasks: [TaskItem]
    -> AgentDisplayInfo exposes tasks: [TaskItem]
    -> AgentCardView renders attributed task list
```

### Query Method

New method on `WebhookStatusProvider`:

```swift
func tasks(for worktreePath: String) -> [TaskItem] {
    // Return tasks from most recent session for this worktree
}
```

### StatusPublisher Integration

In `StatusPublisher.pollAll()`, after reading `webhookMessage`:

```swift
let webhookTasks = webhookProvider.tasks(for: worktreePath)
AgentHead.shared.updateStatus(
    terminalID: terminalID,
    status: detected,
    lastMessage: lastMessage,
    roundDuration: roundDur,
    tasks: webhookTasks
)
```

### AgentHead / AgentInfo

- Add `tasks: [TaskItem]` to `AgentInfo` (default `[]`)
- `updateStatus()` accepts optional `tasks` parameter and stores on AgentInfo

### AgentDisplayInfo

Add `tasks: [TaskItem]` field. Built from `AgentHead.shared.agent(for:)?.tasks ?? []`.

### UI Rendering (AgentCardView)

When `tasks` is non-empty, replace `messageLabel.stringValue` with `messageLabel.attributedStringValue` using an `NSAttributedString`:

| Status | Icon | Text Style | Color |
|--------|------|-----------|-------|
| completed | `✓` | strikethrough | SemanticColors.muted (dimmer) |
| inProgress | `■` | **bold** | SemanticColors.text |
| pending | `□` | regular | SemanticColors.muted |

Format: one line per task, monospaced 11pt font. Example:

```
 ✓ Task 1: Add i18n translation keys
 ■ Task 2: Create FeedbackDialog component
 □ Task 3: Add feedback button
 □ Task 4: Manual verification
```

When no tasks exist, fall back to existing `lastMessage` string display.

### Scope

- Only `AgentCardView` — `MiniCardView` continues showing plain lastMessage (too small for task list).
- No changes to terminal text parsing (`TaskProgressParser` remains unused for now).
- Task list clears on `agentStop` so stale tasks don't persist after conversation ends.
