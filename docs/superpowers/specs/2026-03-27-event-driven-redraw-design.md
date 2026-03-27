# Event-Driven Redraw Optimization — Updated Design

> Supersedes `2026-03-20-event-driven-redraw-design.md` with current codebase state.

## Problem

7 views use `wantsUpdateLayer = true` and resolve CGColors every frame via `updateLayer()`, even when the window is idle. This causes unnecessary CPU work (color resolution, layer property writes) on every display refresh.

## Affected Views

| View | File | updateLayer work | Trigger frequency needed |
|------|------|-----------------|------------------------|
| AgentCardView | `Sources/UI/Dashboard/AgentCardView.swift` | 6+ resolvedCGColor calls, border update | configure(), hover, select, theme |
| MiniCardView | `Sources/UI/Dashboard/MiniCardView.swift` | 5+ resolvedCGColor calls, shadow setup | configure(), hover, select, theme |
| FocusPanelView | `Sources/UI/Dashboard/FocusPanelView.swift` | 2 resolvedCGColor calls | configure(), theme |
| GhostCardView | `Sources/UI/Dashboard/StackedCardContainerView.swift` | 2 resolvedCGColor calls | theme only |
| MiniGhostCardView | `Sources/UI/Dashboard/StackedMiniCardContainerView.swift` | 2 resolvedCGColor calls | theme only |
| DashboardRootView | `Sources/UI/Dashboard/DashboardViewController.swift` | 1 resolvedCGColor call | theme only |
| RepoRootView | `Sources/UI/Repo/RepoViewController.swift` | 1 resolvedCGColor call | theme only |

## Solution

Unified pattern for all 7 views:

1. **Remove `wantsUpdateLayer` override** and **`updateLayer()` override**
2. **Call `applyColors()` from `viewDidChangeEffectiveAppearance()`** instead of `needsDisplay = true`
3. **Call `applyColors()` from state mutation points**: `configure()`, `mouseEntered/Exited`, `isSelected` didSet
4. **Add `shadowPath` to MiniCardView** for GPU-cached shadow rendering (AIPanelView and NotificationPanelView already have this)

### Pattern (per view)

**Before:**
```swift
override var wantsUpdateLayer: Bool { true }
override func updateLayer() { applyColors() }
override func viewDidChangeEffectiveAppearance() {
    super.viewDidChangeEffectiveAppearance()
    needsDisplay = true
}
```

**After:**
```swift
// No wantsUpdateLayer, no updateLayer
override func viewDidChangeEffectiveAppearance() {
    super.viewDidChangeEffectiveAppearance()
    applyColors()
}
```

Plus ensure `applyColors()` is called at every state change point (already the case for most views — configure/hover/select already call it).

### MiniCardView shadowPath

Add in `layout()` or after shadow setup:
```swift
layer.shadowPath = CGPath(roundedRect: bounds, cornerWidth: layer.cornerRadius, cornerHeight: layer.cornerRadius, transform: nil)
```

## Out of Scope

- SemanticColors refactoring — already using `static let` + `NSColor(name:)` pattern correctly
- AIPanelView / NotificationPanelView shadows — already have `shadowPath`
- TitleBarView / StatusBarView — no `wantsUpdateLayer` found in current code (already fixed or removed)

## Files Changed

| File | Change |
|------|--------|
| `Sources/UI/Dashboard/AgentCardView.swift` | Remove wantsUpdateLayer/updateLayer, applyColors from appearance callback |
| `Sources/UI/Dashboard/MiniCardView.swift` | Same + add shadowPath |
| `Sources/UI/Dashboard/FocusPanelView.swift` | Same pattern |
| `Sources/UI/Dashboard/StackedCardContainerView.swift` | GhostCardView: same pattern |
| `Sources/UI/Dashboard/StackedMiniCardContainerView.swift` | MiniGhostCardView: same pattern |
| `Sources/UI/Dashboard/DashboardViewController.swift` | DashboardRootView: same pattern |
| `Sources/UI/Repo/RepoViewController.swift` | RepoRootView: same pattern |

## Testing

- Build and run — verify no visual regressions in grid mode, spotlight mode, repo tabs
- Switch dark/light mode — verify colors update immediately
- Hover/select cards — verify border/background transitions work
- Verify MiniCardView shadow renders correctly with shadowPath
