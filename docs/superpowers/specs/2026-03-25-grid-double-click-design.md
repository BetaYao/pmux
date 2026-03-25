# Grid Dashboard Double-Click Navigation

**Date:** 2026-03-25
**Status:** Approved

## Overview

Change the click behaviour in the dashboard grid layout:
- **Single click** — select the card (visual highlight only, stay in grid)
- **Double click** — navigate to the worktree's project detail tab and focus the active pane terminal

Previously, single click in grid mode switched to the leftRight (speaker) layout. That behaviour is removed; single click now only updates visual selection.

## Scope

- **In scope:** Grid layout click handling (`StackedCardContainerView`, `DashboardViewController`)
- **Out of scope:** Focus layouts (leftRight / topSmall / topLarge) — their single-click behaviour is unchanged; `MiniCardView` is not modified

## Behaviour

### Single click (grid)
1. Update `selectedAgentId` to the clicked agent's ID
2. Update `isSelected` on all grid containers
3. Stay in grid layout — no layout switch

### Double click (grid)
1. Navigate to the project detail tab for the clicked worktree (`dashboardDidSelectProject(project:thread:)`)
2. That method: switches to the correct repo tab, selects the worktree in the sidebar, and calls `makeFirstResponder` on the active pane terminal (`tree.focusedId` or first leaf)

The existing `dashboardDidSelectProject(project:thread:)` delegate method already handles all three steps — no new delegate methods are needed.

## Architecture

### `AgentCardDelegate` changes

Add a new optional method with a default no-op implementation so existing conformers (`DashboardViewController`, `MiniCardView`) are unaffected unless they opt in:

```swift
protocol AgentCardDelegate: AnyObject {
    func agentCardClicked(agentId: String)
    func agentCardDoubleClicked(agentId: String)  // default: no-op
}

extension AgentCardDelegate {
    func agentCardDoubleClicked(agentId: String) {}
}
```

### `StackedCardContainerView` changes

Replace the single `NSClickGestureRecognizer` with two recognizers:

```swift
// Double-click recognizer
let doubleClick = NSClickGestureRecognizer(target: self, action: #selector(handleDoubleClick))
doubleClick.numberOfClicksRequired = 2

// Single-click recognizer — must fail before double-click fires
let singleClick = NSClickGestureRecognizer(target: self, action: #selector(handleClick))
singleClick.numberOfClicksRequired = 1
singleClick.require(toFail: doubleClick)

addGestureRecognizer(doubleClick)
addGestureRecognizer(singleClick)
```

`require(toFail:)` ensures single-click fires only when a double-click is ruled out. On a double-click, only `handleDoubleClick` fires.

```swift
@objc private func handleDoubleClick() {
    delegate?.agentCardDoubleClicked(agentId: cardView.agentId)
}
```

### `DashboardViewController` changes

**`agentCardClicked` — grid case:**

```swift
case .grid:
    // Single click → select in place (no layout switch)
    let previousId = selectedAgentId
    selectedAgentId = agentId
    for container in gridCards {
        container.isSelected = (container.agentId == agentId)
    }
```

**New `agentCardDoubleClicked` implementation:**

```swift
func agentCardDoubleClicked(agentId: String) {
    guard let agent = agents.first(where: { $0.id == agentId }) else { return }
    dashboardDelegate?.dashboardDidSelectProject(agent.project, thread: agent.thread)
}
```

This works for all layouts (grid and focus), so the method is not gated on `currentLayout`.

### No changes to

- `DashboardDelegate` protocol — `dashboardDidSelectProject` already exists and covers the full navigation flow
- `MainWindowController.dashboardDidSelectProject` — already switches tab, selects worktree, and focuses terminal
- `MiniCardView` — no double-click support added (out of scope)
- `AgentCardView` — no changes; its own `handleClick` already fires into a nil delegate when hosted inside `StackedCardContainerView`

## Testing

- **Unit:** Single click on grid card updates `selectedAgentId` and card `isSelected`; layout stays `.grid`
- **Unit:** Double click fires `agentCardDoubleClicked(agentId:)`; single click does NOT also fire
- **Manual:** Double-click a multi-pane worktree card → correct project tab opens, correct worktree selected, active pane receives keyboard focus
- **Regression:** Focus layouts (leftRight etc.) single-click behaviour unchanged
