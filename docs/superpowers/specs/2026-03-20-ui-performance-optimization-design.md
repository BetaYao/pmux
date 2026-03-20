# UI Performance Optimization — Design Spec

**Date:** 2026-03-20
**Approach:** B — Incremental Updates + Layout System Simplification

## Problem

The entire UI feels sluggish. Performance tests reveal two hot spots:
- **StatusDetector text matching:** 88ms per poll cycle (10 surfaces × 1000 lines)
- **SemanticColors allocation:** 84ms per 10K accesses (computed property, no cache)

These compound with a **full view rebuild every 2 seconds** (polling-driven), creating sustained micro-stutter across all interactions.

## Root Causes

1. **Polling triggers full rebuild:** `StatusPublisher` fires delegate even when status unchanged → `DashboardViewController.updateAgents()` destroys and recreates all cards every 2s
2. **No incremental updates:** Cards, sidebar rows, notification items are always destroyed+recreated, never updated in-place
3. **SemanticColors are computed properties:** Every `updateLayer()` call across all views creates new NSColor instances
4. **4 layout hierarchies instantiated at loadView:** Grid, LeftRight, TopSmall, TopLarge all built eagerly, even when hidden
5. **Sidebar has no cell reuse:** `tableView(_:viewFor:)` creates new NSView + labels + constraints per row per reload
6. **StatusBadge uses draw() with NSBezierPath:** CPU rasterization instead of GPU-cached CALayer
7. **Shadow without shadowPath:** Panels force Core Animation to rasterize from shape each frame

## Design

### 1. StatusPublisher — Only Notify on Change

**File:** `Sources/Status/StatusPublisher.swift`

Current: `pollAll()` always calls `delegate?.statusDidChange()` for every surface.

Change: Only call delegate when status or lastMessage actually differs from previous value. Store previous `(AgentStatus, String)` per surface path, compare before firing.

Move `detector.detect()` + `extractLastMessage()` off main thread — poll on a background DispatchQueue, dispatch results to main only when changed.

### 2. SemanticColors — Cache Instances

**File:** `Sources/UI/Shared/SemanticColors.swift`

Current: All colors are `static var` computed properties returning `NSColor(name:) { ... }`.

Change: Convert to `static let` stored properties. `NSColor(name:)` already supports dynamic appearance resolution — the closure is called by AppKit when appearance changes, so caching the NSColor instance is safe and correct. No need for manual invalidation.

```swift
// Before
static var bg: NSColor {
    NSColor(name: nil) { $0.isDark ? NSColor(hex: 0x1a1a1a) : NSColor(hex: 0xf3f4f7) }
}

// After
static let bg = NSColor(name: nil) { $0.isDark ? NSColor(hex: 0x1a1a1a) : NSColor(hex: 0xf3f4f7) }
```

### 3. Dashboard — In-Place Card Updates

**File:** `Sources/UI/Dashboard/DashboardViewController.swift`

Current: `updateAgents()` calls `rebuildCurrentLayout()` which destroys all cards and recreates them.

Change:
- `updateAgents()` compares new agent list with existing cards by ID
- If card exists: call `card.configure()` to update labels/status in-place
- If card added/removed: add/remove only the diff
- `rebuildCurrentLayout()` only called on layout mode change or initial load

Add a `cardsByID: [String: AgentCardView]` and `miniCardsByID: [String: MiniCardView]` dictionary to DashboardViewController for O(1) lookup.

### 4. AgentCardView / MiniCardView — Efficient configure()

**Files:** `Sources/UI/Dashboard/AgentCardView.swift`, `MiniCardView.swift`

Current: `configure()` sets all label values and calls `updateAppearance()`. This is already efficient for in-place updates.

Change: Add early-return if status hasn't changed (avoid `updateAppearance()` when only message text changed).

### 5. Dashboard — Lazy Layout Instantiation

**File:** `Sources/UI/Dashboard/DashboardViewController.swift`

Current: `loadView()` calls `setupGridLayout()`, `setupLeftRightLayout()`, `setupTopSmallLayout()`, `setupTopLargeLayout()` — all 4 hierarchies created upfront.

Change:
- Only instantiate the active layout mode on `loadView()`
- Other layouts created on-demand when `setLayout()` is called
- Track which layouts have been instantiated with a `Set<LayoutMode>`
- When switching away from a layout, leave it instantiated but hidden (don't destroy — user may switch back)

### 6. Grid Mode — Replace Manual Frames with NSCollectionView

**File:** `Sources/UI/Dashboard/DashboardViewController.swift`

Current: Grid uses frame-based layout with manual `layoutGridFrames()` calculation in `viewDidLayout()`.

Change:
- Replace `gridContainer` + manual frame math with `NSCollectionView` + `NSCollectionViewFlowLayout`
- Use `NSCollectionViewItem` subclass wrapping `AgentCardView`
- Collection view handles: cell reuse, layout calculation, scroll, resize
- Remove `GridLayout` struct (no longer needed for UI; keep if tests reference it)
- `updateAgents()` calls `collectionView.reloadData()` or `performBatchUpdates` for diffs

### 7. Sidebar — Cell Reuse

**File:** `Sources/UI/Repo/SidebarViewController.swift`

Current: `tableView(_:viewFor:)` creates new NSView + 3 labels + constraints every time.

Change:
- Create a `SidebarCellView: NSTableCellView` subclass with pre-built labels and constraints
- Register with `tableView.register()`  or use `makeView(withIdentifier:)` for reuse
- `tableView(_:viewFor:)` dequeues or creates once, then configures labels
- Replace `tableView.reloadData()` with `tableView.reloadData(forRowIndexes:columnIndexes:)` for single-row status updates

### 8. StatusBadge — Layer-Backed Rendering

**File:** `Sources/UI/Shared/StatusBadge.swift`

Current: Overrides `draw(_:)` with `NSBezierPath(ovalIn:)`.

Change:
- Set `wantsLayer = true`, `wantsUpdateLayer = true`
- Override `updateLayer()` instead of `draw()`
- Use `layer.cornerRadius = bounds.width / 2` + `layer.backgroundColor`
- Remove `draw(_:)` override entirely

### 9. ThreadRowView — Layer-Backed Selection

**File:** `Sources/UI/Repo/SidebarViewController.swift` (ThreadRowView)

Current: Overrides `draw(_:)` for selection background with `NSBezierPath(roundedRect:)`.

Change:
- Use `wantsLayer = true`, override `updateLayer()`
- Set `layer.backgroundColor` and `layer.cornerRadius` for selection state
- Remove `draw(_:)` override

### 10. Shadow Paths

**Files:** `AIPanelView.swift`, `NotificationPanelView.swift`

Current: Shadows set without `shadowPath`.

Change: Set `layer.shadowPath = CGPath(roundedRect:cornerWidth:cornerHeight:transform:)` matching the panel bounds. Update in `layout()` override when bounds change.

### 11. Theme Refresh — Targeted Instead of Recursive

**File:** `Sources/UI/Shared/Theme.swift`

Current: `refreshSubviews()` recursively walks entire view tree, setting `needsDisplay`/`needsLayout` on every view.

Change: With `SemanticColors` using `NSColor(name:)` (dynamic colors), views with `wantsUpdateLayer = true` automatically get `updateLayer()` called when appearance changes. The recursive refresh is unnecessary for layer-backed views.

Simplify to: just set `window.appearance`, post notification. Let AppKit handle the rest via its built-in appearance propagation.

## Files Changed

| File | Change Type |
|------|-------------|
| `Sources/Status/StatusPublisher.swift` | Diff-based notification + background thread |
| `Sources/UI/Shared/SemanticColors.swift` | `var` → `let` |
| `Sources/UI/Shared/Theme.swift` | Remove recursive refresh |
| `Sources/UI/Shared/StatusBadge.swift` | draw() → updateLayer() |
| `Sources/UI/Dashboard/DashboardViewController.swift` | In-place updates, lazy layouts, NSCollectionView for grid |
| `Sources/UI/Dashboard/AgentCardView.swift` | Early-return optimization |
| `Sources/UI/Dashboard/MiniCardView.swift` | Early-return optimization |
| `Sources/UI/Repo/SidebarViewController.swift` | Cell reuse, targeted reload, ThreadRowView layer-backed |
| `Sources/UI/Panel/AIPanelView.swift` | Shadow path |
| `Sources/UI/Panel/NotificationPanelView.swift` | Shadow path |

## Testing Strategy

- Existing `PerformanceTests` provide baselines — run before/after to measure improvement
- Key targets: `testSemanticColorsAllocationPerformance` should drop ~10x, `testFullRebuildCyclePerformance` should be near-zero (no rebuild on status poll)
- Manual testing: verify no visual regressions across all 4 layout modes + theme switching
- Verify status updates still appear correctly (no missed updates from diff-based notification)

## Out of Scope

- Replacing custom buttons with NSButton (Approach C territory)
- NSOutlineView for sidebar (Approach C)
- NSPopover/NSPanel for panels (Approach C)
- NSAppearance-based theming (Approach C)
