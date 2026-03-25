# Stacked Card Effect for Multi-Pane Worktrees

**Date:** 2026-03-25
**Status:** Approved

## Overview

When a worktree has multiple split panes, the dashboard grid card for that worktree should visually convey "there are multiple panes here" using a physical card-stack metaphor: ghost cards layered behind the main card, offset to the bottom-right on screen.

## Scope

- **In scope:** Grid layout (`AgentCardView` in `DashboardViewController.rebuildGrid`)
- **Out of scope:** Focus layouts (`MiniCardView` in leftRight/topSmall/topLarge sidebars) — not worth the complexity for small thumbnail cards

## Visual Design

### Stacking rules

| paneCount | Ghost cards | Screen offsets (right, down) |
|-----------|-------------|------------------------------|
| 1         | 0 (no change) | — |
| 2         | 1 ghost | (6, 6)px |
| 3+        | 2 ghosts | (6, 6)px and (12, 12)px |

### Coordinate system note

`DraggableGridView` does **not** override `isFlipped`, so AppKit default coordinates apply: Y increases upward, origin is bottom-left. To make ghost cards appear **below and to the right** of the main card on screen:
- X offset = +Δx (rightward)
- Y offset = -Δy (downward on screen = negative Y in AppKit)

### Ghost card appearance

- **Size:** Same width and height as the main card
- **Background (dark mode):** ghost-1 = `#1a1a2e`, ghost-2 = `#161625` — intentionally purple-tinted to match the existing dark theme palette
- **Background (light mode):** ghost-1 = `#e8e8f0`, ghost-2 = `#dcdce8` — slightly blue-tinted to match the light theme
- **Border:** Same corner radius as main card (4px); border color = `SemanticColors.tileGhostBorder`
- **Content:** None — purely decorative
- **No opacity changes** — color difference alone creates the depth cue

## Architecture

### New component: `StackedCardContainerView`

A lightweight `NSView` wrapper with no background, no border, `wantsLayer = true`, `layer.masksToBounds = false` (so ghost views, which are children of the container, are not clipped at the container boundary).

The container frame is **exactly the same size as the logical card frame** — no expansion. Ghost cards overflow the container boundary visually, which is allowed because `masksToBounds = false`. The grid scroll view's clip view will naturally clip any overflow at the edge of the visible area, which is acceptable.

```
StackedCardContainerView  (frame = logical card frame, masksToBounds = false)
  ├── ghostView2  (NSView, frame = {12, -12, cardWidth, cardHeight}, z-order: bottom)
  ├── ghostView1  (NSView, frame = {6, -6, cardWidth, cardHeight}, z-order: middle)
  └── AgentCardView  (frame = {0, 0, cardWidth, cardHeight}, z-order: top)
```

In AppKit coordinates (Y-up), `y = -6` positions a view 6pt below the container's bottom edge on screen — i.e., lower on the display.

**Exposed API:**
```swift
final class StackedCardContainerView: NSView {
    let cardView: AgentCardView            // main card, always present
    private(set) var ghostViews: [NSView] = []  // 0, 1, or 2 ghost views
    var agentId: String { cardView.agentId }    // forwarded for gridCards lookups
    var isSelected: Bool {                      // forwarded to cardView
        get { cardView.isSelected }
        set { cardView.isSelected = newValue }
    }

    /// Updates ghost view count to match paneCount.
    /// Surplus ghost views are removed from the view hierarchy via removeFromSuperview().
    /// Newly needed ghost views are created and inserted below cardView.
    func configure(paneCount: Int)
}
```

### Hit-testing

Ghost views overflow the container bounds and should be **non-interactive** — clicks on ghost areas fall through to the grid background or are ignored. `StackedCardContainerView` overrides `hitTest(_:)` to confine responses to the `cardView` frame only:

```swift
override func hitTest(_ point: NSPoint) -> NSView? {
    guard cardView.frame.contains(point) else { return nil }
    return super.hitTest(point)
}
```

This means only the main card area is clickable. Clicks anywhere on the main card (including at its edges) work normally.

### Click handling and delegate ownership

`AgentCardView.delegate` is **not set** when `AgentCardView` is hosted inside `StackedCardContainerView`. The container owns click forwarding:

- `StackedCardContainerView` installs an `NSClickGestureRecognizer`
- On click, it reads `cardView.agentId` and calls `delegate?.agentCardClicked(agentId:)`
- `AgentCardView`'s own click recognizer is still present but its `delegate` is nil, so it fires harmlessly
- `StackedCardContainerView` exposes `weak var delegate: AgentCardDelegate?`

This avoids double-fire: only the container's recognizer fires the delegate callback.

### `configure` call convention

`StackedCardContainerView.configure(paneCount:)` handles ghost view creation/removal. The `AgentCardView` data is set separately via `container.cardView.configure(...)`. Callers always call both:

```swift
// In rebuildGrid and updateGridInPlace:
container.configure(paneCount: agent.paneCount)
container.cardView.configure(
    id: agent.id, project: agent.project, thread: agent.thread,
    status: agent.status, lastMessage: agent.lastMessage,
    totalDuration: agent.totalDuration, roundDuration: agent.roundDuration,
    paneCount: agent.paneCount
)
container.isSelected = (agent.id == selectedAgentId)  // if needed
```

### Grid layout changes

`DashboardViewController` changes `gridCards` from `[AgentCardView]` to `[StackedCardContainerView]`.

**Frame math:** Container frames are identical to the logical card frames from `GridLayout.cardFrame(at:)` — no expansion. Ghost views overflow the container visually via `masksToBounds = false`.

**`rebuildGrid` loop:**
```swift
let container = StackedCardContainerView()
container.delegate = self
container.configure(paneCount: agent.paneCount)
container.cardView.configure(id: agent.id, ...)
container.translatesAutoresizingMaskIntoConstraints = true
gridCards.append(container)
gridContainer.addSubview(container)
```

**`updateGridInPlace` loop:**
```swift
guard sorted.count == gridCards.count else { rebuildGrid(); return }
for (index, agent) in sorted.enumerated() {
    gridCards[index].configure(paneCount: agent.paneCount)
    gridCards[index].cardView.configure(id: agent.id, ...)
}
```

**Recovery path:** `terminalSurfaceDidRecover` accesses `container.cardView.terminalContainer` instead of the former `card.terminalContainer`.

### `SemanticColors` additions

```swift
static var tileGhost1Bg: NSColor    // dark: #1a1a2e, light: #e8e8f0
static var tileGhost2Bg: NSColor    // dark: #161625, light: #dcdce8
static var tileGhostBorder: NSColor // NSColor.separatorColor at reduced alpha, or line color at 60%
```

### Appearance update propagation

Ghost views override `updateLayer()` to resolve colors. `StackedCardContainerView` overrides `viewDidChangeEffectiveAppearance()` and calls `needsDisplay = true` on each ghost view so colors update when the system appearance changes.

### No changes to

- `AgentCardView` internals
- `AgentDisplayInfo` (`paneCount` already exists)
- `TerminalSurface` lifecycle
- `GridLayout` frame math
- `gridSpacing` value
- Focus layouts

## Testing

- **Unit test:** `StackedCardContainerView` with `paneCount` 1/2/3/5 produces `ghostViews.count` of 0/1/2/2
- **Hit-test:** Click on ghost card area (outside `cardView.frame`) does not trigger selection
- **Click:** Click on main card area triggers `agentCardClicked` exactly once
- **Visual:** Build and verify ghost cards appear below-right in grid layout with 2-pane and 3-pane worktrees
- **Light mode:** Switch system appearance; ghost cards update to light-mode colors
- **Drag-reorder:** Confirm drag initiation still works; grid spacing unchanged
- **Regression:** Single-pane cards look and behave identically to before; `terminalSurfaceDidRecover` re-embeds surface correctly
