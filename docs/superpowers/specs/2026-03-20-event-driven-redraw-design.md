# Event-Driven Redraw Performance Optimization

## Problem

All views use `wantsUpdateLayer = true` and perform expensive work in `updateLayer()` every frame:
- Creating new NSColor objects via `withAlphaComponent()` / `blended()` (6+ allocations per view per frame)
- Iterating subview arrays (TitleBarView iterates all tabs, AIPanelView iterates all bubbles, StatusBarView has triple-nested loops)
- Setting `needsDisplay = true` on child views from parent `updateLayer()`, creating cascade chains

This violates AppKit's event-driven redraw model and causes high CPU even when the window is idle.

## Solution

Convert from continuous frame-driven rendering to AppKit's standard event-driven model:

1. **Pre-compute derived colors** as `static let` in `SemanticColors` — allocated once, auto-resolve per appearance
2. **Remove `wantsUpdateLayer`** from views that don't need per-frame updates
3. **Trigger `needsDisplay` only on data change** — `configure()`, `mouseEntered/Exited`, `isSelected` didSet
4. **Handle theme changes** via `viewDidChangeEffectiveAppearance()` — one-shot refresh, not per-frame
5. **Add `shadowPath`** to panel views for GPU-cached shadow rendering

## Files Changed

- `SemanticColors.swift` — Add ~15 pre-computed derived colors
- `AgentCardView.swift` — Remove wantsUpdateLayer, event-driven updateAppearance
- `MiniCardView.swift` — Same pattern
- `FocusPanelView.swift` — Colors set once in setup(), appearance callback for theme
- `TitleBarView.swift` — Remove tab iteration loop from updateLayer
- `StatusBarView.swift` — Remove triple-nested updateHintColors loop from updateLayer
- `AIPanelView.swift` — Remove bubble iteration, add shadowPath
- `NotificationPanelView.swift` — Remove item iteration, add shadowPath
- `PanelBackdropView.swift` — Static color, no updateLayer
- `LayoutPopoverView.swift` — Add shadowPath, event-driven
- `RepoViewController.swift` — Move colors from viewDidLayout to appearance callback
