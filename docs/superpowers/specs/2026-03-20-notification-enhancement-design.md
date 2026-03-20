# Notification Enhancement Design

## Overview

Enhance the notification system with three improvements:
1. **Content enhancement** — include agent's last message in notification body
2. **Action buttons** — add "Open Terminal" and "Dismiss" buttons to notifications
3. **In-app notification history** — a timeline view showing past notifications

## A) Content Enhancement

### Current
```
Title: "Agent finished"
Body:  "feature-branch completed its task"
```

### Enhanced
```
Title: "Agent finished — feature-branch"
Body:  "✓ All 15 tests passed, no failures"  ← lastMessage from StatusPublisher
```

**Changes to NotificationManager.notify():**
- Accept `lastMessage: String` parameter
- Use lastMessage as notification body when available
- Fall back to current generic messages when lastMessage is empty

## B) Action Buttons

Add UNNotificationAction buttons to notifications:

| Action | Identifier | Title | Behavior |
|--------|-----------|-------|----------|
| Open Terminal | `open_terminal` | "Open Terminal" | Bring app to front, navigate to worktree, switch to repo tab |
| Dismiss | `dismiss` | "Dismiss" | Close notification (default UNNotificationAction) |

**Implementation:**
- Register a `UNNotificationCategory` with these actions during `requestPermission()`
- Set `content.categoryIdentifier` when creating notifications
- Handle action identifiers in `didReceive response`

## C) In-App Notification History

### NotificationHistory

**File:** `Sources/Status/NotificationHistory.swift`

```swift
struct NotificationEntry {
    let id: UUID
    let timestamp: Date
    let branch: String
    let worktreePath: String
    let status: AgentStatus
    let message: String      // lastMessage or fallback
    var isRead: Bool
}

class NotificationHistory {
    static let shared = NotificationHistory()
    private(set) var entries: [NotificationEntry] = []
    let maxEntries = 100

    func add(entry: NotificationEntry)
    func markRead(id: UUID)
    func markAllRead()
    func clear()
    var unreadCount: Int
}
```

### NotificationHistoryViewController

**File:** `Sources/UI/Notification/NotificationHistoryViewController.swift`

A sheet/panel showing notification timeline:

```
┌─────────────────────────────────────────────┐
│ Notifications                    [Clear All] │
├─────────────────────────────────────────────┤
│ ● 14:32  feature-branch  Agent finished     │
│   "✓ All 15 tests passed"                   │
├─────────────────────────────────────────────┤
│ ○ 14:28  fix-bug  Agent error               │
│   "ERROR: compilation failed"               │
├─────────────────────────────────────────────┤
│ ○ 14:15  main  Agent waiting                │
│   "Do you want to proceed? (y/n)"           │
└─────────────────────────────────────────────┘
```

- NSTableView with custom cells
- Click row → navigate to worktree
- ● = unread, ○ = read
- Status-colored dot (green/red/yellow)
- Presented as sheet via Cmd+Shift+N or bell icon in tab bar

### TabBar Badge

Add unread notification count badge next to the status counts in TabBarView.

## Integration

### MainWindowController
- Pass `lastMessage` to NotificationManager.notify()
- Add notification to NotificationHistory when sending
- Add Cmd+Shift+N menu item for notification history
- Update TabBar badge on unread count change

### Accessibility Identifiers
- `notification.history` (sheet container)
- `notification.clearButton`
- `notification.row.{index}`

## Testing

### Unit Tests
- NotificationHistory: add, markRead, markAllRead, clear, maxEntries cap
- NotificationEntry formatting

### UI Tests
- testCmdShiftNOpensHistory
- testNotificationHistoryCloseOnEsc
