# TODO & Ideas Persistence Design

## Goal

Replace mock TODO and Ideas data in AIPanelView with real, JSON-persisted data associated with projects. TODOs are created by the project's main agent and their status will be updated by AgentHead (via webhook events, logic deferred). Ideas are manually input by the user (external channel sources deferred).

## Data Models

### TodoItem

```swift
struct TodoItem: Codable, Identifiable {
    let id: String           // UUID
    var task: String          // Task description
    var status: String        // pending_approval | approved | running | completed | failed | skipped
    var project: String       // Repo display name (e.g., "amux")
    var branch: String?       // Associated branch/worktree name
    var issue: String?        // GitHub issue ref (e.g., "#42")
    var progress: String?     // Progress description
    let createdAt: Date
    var updatedAt: Date
}
```

### IdeaItem

```swift
struct IdeaItem: Codable, Identifiable {
    let id: String           // UUID
    var text: String          // Idea content
    var project: String       // Repo display name
    var source: String        // "manual" (future: "wechat", "mqtt", etc.)
    var tags: [String]        // Tags (e.g., ["ui", "login"])
    let createdAt: Date
}
```

## Storage

- `~/.config/amux/todos.json` — Array of TodoItem, pretty-printed JSON
- `~/.config/amux/ideas.json` — Array of IdeaItem, pretty-printed JSON
- Both loaded at app launch in `AppDelegate.applicationDidFinishLaunching`
- Debounced async save (same pattern as existing `Config.save()`)

## Store Layer

### TodoStore (`Sources/Core/TodoStore.swift`)

Singleton `TodoStore.shared`. Manages TodoItem CRUD and persistence.

**Methods:**
- `load()` / `save()` — Read/write `~/.config/amux/todos.json`
- `add(task:project:branch:issue:) -> TodoItem` — Create with status "pending_approval"
- `update(id:status:progress:)` — Update status and/or progress, sets `updatedAt`
- `remove(id:)`
- `allItems() -> [TodoItem]`
- `updateStatusFromWebhook(worktreePath:event:)` — Empty stub. Future: match event.cwd → worktree → branch → TodoItem.branch, update status based on event type

### IdeaStore (`Sources/Core/IdeaStore.swift`)

Singleton `IdeaStore.shared`. Manages IdeaItem CRUD and persistence.

**Methods:**
- `load()` / `save()` — Read/write `~/.config/amux/ideas.json`
- `add(text:project:source:tags:) -> IdeaItem`
- `remove(id:)`
- `allItems() -> [IdeaItem]`

## UI Binding

### PanelCoordinator Changes

- On panel open: read `TodoStore.shared.allItems()` and `IdeaStore.shared.allItems()`
- Convert to existing `TodoDisplayItem` / `IdeaDisplayItem` and feed to `AIPanelView`
- Shows all items (no project filtering)
- Refresh after add/remove operations

### AIPanelView Changes

- Remove `loadSampleData()` and all hardcoded sample data
- Ideas input: `sendCurrentInput()` calls delegate instead of local handling

### AIPanelDelegate Extension

- Add `panelDidSubmitIdea(text:)` callback
- PanelCoordinator implements it: calls `IdeaStore.add()`, refreshes panel

### Refresh Triggers

- Panel opened
- After add/remove operation
- After tab (project) switch

## AgentHead Integration (Stub)

- Add `func updateTodoFromWebhook(_ event: WebhookEvent)` to AgentHead — empty body
- Comment block describing future logic: event.cwd → worktree path → branch → match TodoItem.branch → update status (SessionStart → running, Stop → completed, etc.)
- Pre-wire call site in TabCoordinator webhook handler (commented out)

## Files

| Action | File | Responsibility |
|--------|------|----------------|
| Create | `Sources/Core/TodoStore.swift` | TodoItem model + CRUD + JSON persistence |
| Create | `Sources/Core/IdeaStore.swift` | IdeaItem model + CRUD + JSON persistence |
| Modify | `Sources/App/PanelCoordinator.swift` | Wire stores to AIPanelView, handle delegate callbacks |
| Modify | `Sources/UI/Panel/AIPanelView.swift` | Remove mock data, delegate idea input |
| Modify | `Sources/App/AppDelegate.swift` | Load stores at launch |
| Modify | `Sources/Core/AgentHead.swift` | Add stub `updateTodoFromWebhook` |
| Modify | `Sources/App/TabCoordinator.swift` | Commented-out call site for future webhook→todo updates |
| Create | `tests/TodoStoreTests.swift` | Unit tests for TodoStore |
| Create | `tests/IdeaStoreTests.swift` | Unit tests for IdeaStore |

## Out of Scope

- External idea sources (wechat, MQTT) — deferred to channel binding work
- AgentHead webhook → TODO status matching logic — stub only
- Project filtering in panel UI — show all items
