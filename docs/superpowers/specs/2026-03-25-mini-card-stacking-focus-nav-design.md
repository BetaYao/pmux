# Mini Card Stacking + Focus Panel Navigation

## Overview

Two enhancements to the dashboard's focus layouts (left-right, top-small, top-large):

1. Mini cards in the sidebar/row gain stacking ghost effect (matching grid cards)
2. Focus panel header gets prev/next navigation with slide transition animation

## 1. Mini Card Stacking

### New: `StackedMiniCardContainerView`

Same pattern as `StackedCardContainerView` but scaled for compact mini cards:

- **Ghost offset**: 3px diagonal (vs 6px for grid cards)
- **Max ghosts**: 2 (capped at `min(paneCount - 1, 2)`)
- Reuses `GhostCardView` styling from `StackedCardContainerView` (same semantic colors, 1px border, 4px corner radius)
- API: `configure(paneCount:)` — creates/removes ghost views as needed
- Contains a `MiniCardView` as the front card (same relationship as `StackedCardContainerView` has with `AgentCardView`)
- **Click handling**: The container owns click handling via its own `NSClickGestureRecognizer` (same as `StackedCardContainerView`). The inner `MiniCardView.delegate` must be set to `nil` to prevent double-firing. The container exposes its own `delegate: AgentCardDelegate?` and forwards clicks.
- **Layout**: Uses frame-based layout internally (`layoutChildren()`). The `MiniCardView`'s Auto Layout aspect ratio constraint (16:9) should be deactivated when embedded — the container manages sizing via frames, and the parent stack/scroll provides the external size.
- Ghosts only appear when `paneCount > 1`; single-pane agents show the mini card at full size with no ghosts.

### Integration

In `DashboardViewController`, replace bare `MiniCardView` instances with `StackedMiniCardContainerView` in all three focus layouts:

- `leftRightMiniCards` → `[StackedMiniCardContainerView]`
- `topSmallMiniCards` → `[StackedMiniCardContainerView]`
- `topLargeMiniCards` → `[StackedMiniCardContainerView]`

Each container's `configure(paneCount:)` is called during `updateFocusLayoutInPlace()` and `rebuildFocusLayout()`.

## 2. Focus Panel Navigation

### Header Bar Changes

Current layout: `[status dot] [name] [project · thread] [duration] ... [enter button]`

New layout: `[status dot] [name] [project · thread] [duration] ... [◀] [1/4] [▶] [enter button]`

### New UI Elements in `FocusPanelView`

- **prevButton**: `NSButton` with SF Symbol `chevron.left`, 26x24pt
- **nextButton**: `NSButton` with SF Symbol `chevron.right`, 26x24pt
- **counterLabel**: `NSTextField(labelWithString: "")`, shows "1/4" format, muted color, 11pt font
- Buttons disabled (alpha 0.3) at boundaries (first item disables prev, last disables next)
- All three elements hidden when total count is 1 (single agent, no navigation needed)

### New API

```swift
enum NavigationDirection {
    case next, previous
}

// FocusPanelView
func configureNavigation(currentIndex: Int, total: Int)

// FocusPanelDelegate (new method)
func focusPanelDidRequestNavigate(_ panel: FocusPanelView, direction: NavigationDirection)
```

Navigation controls have lower compression resistance than the enter button. When the header is too narrow, duration label truncates first, then meta label.

### DashboardViewController Handling

On `focusPanelDidRequestNavigate`:
1. Compute new index from current `selectedAgentId` position in `sortedAgents()`
2. Clamp to valid range
3. Update `selectedAgentId`
4. Trigger slide animation on the focus panel's `terminalContainer`
5. Re-embed the new terminal surface

## 3. Slide Transition Animation

### Direction Matching

| Layout | Slide Direction |
|--------|----------------|
| left-right | Horizontal: next = `.fromRight`, previous = `.fromLeft` |
| top-small | Vertical: next = `.fromBottom`, previous = `.fromTop` (sidebar is above, so "next" comes from below) |
| top-large | Vertical: next = `.fromTop`, previous = `.fromBottom` (sidebar is below, so "next" comes from above) |

### Implementation

Use `CATransition` on `focusPanel.terminalContainer.layer`:

```swift
let transition = CATransition()
transition.type = .push
transition.subtype = subtype        // .fromRight/.fromLeft or .fromTop/.fromBottom
transition.duration = 0.25
transition.timingFunction = CAMediaTimingFunction(name: .easeInOut)
focusPanel.terminalContainer.layer?.add(transition, forKey: "slideTransition")
```

The animation triggers when the old terminal surface is removed and the new one is added to `terminalContainer`.

Rapid clicks: `CATransition` with the same `forKey` replaces the previous transition automatically. The `selectedAgentId` update is synchronous, so rapid clicks simply jump to the final target — no queuing needed.

## 4. File Changes

| File | Change |
|------|--------|
| New: `Sources/UI/Dashboard/StackedMiniCardContainerView.swift` | Mini card stacking container with 3px ghost offset |
| `Sources/UI/Dashboard/FocusPanelView.swift` | Add prev/next buttons, counter label, navigation delegate |
| `Sources/UI/Dashboard/DashboardViewController.swift` | Wrap mini cards in stacked containers; handle navigation delegate + slide animation |

## 5. Refactoring Note

`GhostCardView` is currently `private` inside `StackedCardContainerView.swift`. To reuse it in `StackedMiniCardContainerView`, either:
- Extract it to its own file, or
- Make it `internal` and keep it in the same file, or
- Duplicate the small class (it's ~20 lines)

Recommended: duplicate in `StackedMiniCardContainerView.swift` as `MiniGhostCardView` to keep files self-contained, matching the existing pattern of each container owning its ghost view.
