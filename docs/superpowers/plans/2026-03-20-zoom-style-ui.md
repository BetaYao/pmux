# Zoom-Style UI Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Redesign pmux's UI to match Zoom's visual language — near-black background, arc-shaped titlebar groupings, 16:9 terminal tiles with independent bottom bars, and slide-out panels.

**Architecture:** Replace the current SemanticColors palette with Zoom design tokens. Rebuild TitleBarView with two rounded-rect "arc blocks". Convert dashboard tiles from info-card style to terminal-preview style with independent bottom bars. Remove StatusBar entirely. Restyle panels and dialogs to match.

**Tech Stack:** Swift 5.10, AppKit, NSView layer-backed views. No SwiftUI. No external dependencies.

**Spec:** `docs/superpowers/specs/2026-03-20-zoom-style-ui-design.md`

---

### Task 1: Update Design Tokens

Update the color system to Zoom's near-black palette. This is the foundation — all subsequent tasks depend on it.

**Files:**
- Modify: `Sources/UI/Shared/SemanticColors.swift`
- Modify: `Sources/UI/Shared/Theme.swift`

- [ ] **Step 1: Update SemanticColors base colors**

Replace the existing base color values in `SemanticColors` with the Zoom palette:

```swift
// Background: #0f1115 → #0b0b0b
// Panel: #15171c → #1a1a1a
// Panel2: #1b1e25 → #111111
// Line: #262a33 → #222222
// Tile surface stays #1a1a1a for cards/bars
```

Key mappings from spec:
| Token | Old Hex (dark) | New Hex (dark) |
|-------|---------------|---------------|
| bg | `0x0f1115` | `0x0b0b0b` |
| panel | `0x15171c` | `0x1a1a1a` |
| panel2 | `0x1b1e25` | `0x111111` |
| line | `0x262a33` | `0x222222` |

Light mode values should be adjusted proportionally (leave light mode for a follow-up if needed — dark-first).

- [ ] **Step 2: Add new Zoom-specific tokens**

Add these new tokens to SemanticColors:

```swift
static let arcBlockHover: NSColor = NSColor(name: nil) { a in
    a.isDark ? NSColor(hex: 0x232323) : NSColor(hex: 0xf0f0f0)
}
static let arcBlockInactive: NSColor = NSColor(name: nil) { a in
    a.isDark ? NSColor(hex: 0x1a1a1a) : NSColor(hex: 0xf5f5f5)
}
static let tileBg: NSColor = NSColor(name: nil) { a in
    a.isDark ? NSColor(hex: 0x111111) : NSColor(hex: 0xfafafa)
}
static let tileBarBg: NSColor = NSColor(name: nil) { a in
    a.isDark ? NSColor(hex: 0x1a1a1a) : NSColor(hex: 0xf5f5f5)
}
```

- [ ] **Step 3: Update Theme facade**

Update `Theme.swift` to set `cardCornerRadius = 4`, `cardPadding = 3` (was 12), `tabBarHeight = 48`.

- [ ] **Step 4: Build and verify**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmux -configuration Debug build`
Expected: BUILD SUCCEEDED

- [ ] **Step 5: Commit**

```bash
git add Sources/UI/Shared/SemanticColors.swift Sources/UI/Shared/Theme.swift
git commit -m "style: update design tokens to Zoom near-black palette"
```

---

### Task 2: Rebuild TitleBarView with Arc Blocks

Complete rewrite of the title bar with two rounded-rect "arc blocks": left (traffic lights + Dashboard + project tabs) and right (4 tool buttons).

**Files:**
- Rewrite: `Sources/UI/TitleBar/TitleBarView.swift`
- Modify: `Sources/App/MainWindowController.swift` (remove StatusBar refs, update titlebar integration)

- [ ] **Step 1: Define the new TitleBarView structure**

Rewrite `TitleBarView` with:
- `leftArcBlock: NSView` (bg `arcBlockHover`/`arcBlockInactive`, cornerRadius 10)
- `rightArcBlock: NSView` (same styling, fixed width)
- Traffic lights inside left block (real system buttons, hidden — use custom dots)
- Dashboard tab: pill shape (radius 14), blue tint when selected
- Project tabs after separator: status dot + name + close button
- Right block: 4 icon buttons (30×30, radius 7) — view switcher, bell, sparkles, theme

Key behaviors:
- Track `isWindowHovered` via NSTrackingArea on the window
- When hovered: arc blocks `#232323`, traffic lights colored, text bright
- When not hovered: arc blocks `#1a1a1a`, traffic lights `#555`, text dim
- Tab states: selected (green border `#33c17b`, bg `#1a2a1a`), hover (bg `#222`, border `rgba(255,255,255,0.08)`), default (transparent)
- Dashboard tab: selected (bg `#4f8cff22`, blue icon/text), not selected (grey)

- [ ] **Step 2: Implement project tab sub-view**

Create a private `ProjectTabView` class inside TitleBarView:
- Properties: `name: String`, `status: AgentStatus`, `isSelected: Bool`, `isHovered: Bool`
- NSTrackingArea for hover
- `updateAppearance()` called from `isSelected`/`isHovered` didSet and `viewDidChangeEffectiveAppearance()`
- Close button ("×") visible always, tinted

- [ ] **Step 3: Implement toolbar icon buttons**

Create 4 SVG-like icon buttons using `NSImage(systemSymbolName:)`:
1. `square.grid.2x2` — view switcher
2. `bell` — notifications
3. `sparkles` — AI
4. `circle.lefthalf.filled` — theme toggle

Each: 30×30, radius 7, hover bg `rgba(255,255,255,0.07)`, stroke color `#888` → `#fff` on hover.
Notification bell: red dot badge (8px) when `notificationCount > 0`.

- [ ] **Step 4: Remove StatusBar from MainWindowController**

In `MainWindowController.swift`:
- Remove `statusBarView` property and all references
- Remove `updateStatusBar()` method
- Remove StatusBar from layout constraints — content container now extends to window bottom edge
- Keep `StatusBarView.swift` file for now (can delete in cleanup task)

- [ ] **Step 5: Update MainWindowController titlebar integration**

- Set titlebar height to 48px
- Wire up window hover tracking: `NSTrackingArea` on `window.contentView` to detect mouse enter/exit
- Forward hover state to `titleBar.setWindowHovered(_:)`
- Wire up right-side button actions: view switcher → `layoutPopover.toggle()`, bell → `toggleNotificationPanel()`, sparkles → `toggleAIPanel()`, theme → `titleBarDidToggleTheme()`

- [ ] **Step 6: Build and verify**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmux -configuration Debug build`

- [ ] **Step 7: Commit**

```bash
git add Sources/UI/TitleBar/TitleBarView.swift Sources/App/MainWindowController.swift
git commit -m "feat: rebuild TitleBar with Zoom arc-block style, remove StatusBar"
```

---

### Task 3: Redesign Terminal Tiles (Grid View)

Convert dashboard grid tiles from info-card (AgentCardView) to Zoom-style terminal preview tiles with independent bottom bars.

**Files:**
- Rewrite: `Sources/UI/Dashboard/AgentCardView.swift`
- Modify: `Sources/UI/Dashboard/DashboardViewController.swift` (grid rebuild logic)
- Modify: `Sources/UI/Dashboard/GridLayout.swift` (aspect ratio, spacing)

- [ ] **Step 1: Update GridLayout constants**

In `GridLayout.swift`, change:
- `spacing` parameter default to `3` (was typically 12)
- Aspect ratio to 16:9 (`0.5625`) for tile calculation

In `DashboardViewController.swift`:
- Change `gridSpacing` from `12` to `3`
- Change `aspectRatio` from `0.6` to `0.5625` (16:9)

- [ ] **Step 2: Rewrite AgentCardView as terminal tile**

New structure:
```
┌────────────────────────────┐ 4px radius
│ Terminal content (flex:1)   │ bg: #111, monospace text
│ (placeholder - real Ghostty │
│  surface will be embedded)  │
├────────────────────────────┤ border-top: 1px #222
│ ● branch-name     Running  │ bg: #1a1a1a, height ~24px
└────────────────────────────┘
```

Keep the existing structure: `wantsLayer = true`, `layer.cornerRadius = 4`.
- Remove old text-heavy layout (titleLabel, messageLabel, timeLabel)
- Add `terminalContainer: NSView` (flex:1, fills top)
- Add `bottomBar: NSView` (fixed height 24px, bg tileBarBg)
- Bottom bar content: status dot (6px) + branch label (9px, white, weight 500) + status label (right-aligned, 8px, dim)
- Hover: `layer.borderColor = accent`, `layer.borderWidth = 1.5`
- Default: `layer.borderColor = transparent`, `layer.borderWidth = 0`

- [ ] **Step 3: Update click behavior**

In `DashboardViewController.agentCardClicked(agentId:)`:
- Grid mode click → switch to left-right layout (Speaker View) with clicked agent as selected
- Remove the old behavior of entering a project tab

```swift
func agentCardClicked(agentId: String) {
    detachTerminals()
    selectedAgentId = agentId
    setLayout(.leftRight)  // Enter Speaker View
}
```

- [ ] **Step 4: Build and verify**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmux -configuration Debug build`

- [ ] **Step 5: Commit**

```bash
git add Sources/UI/Dashboard/AgentCardView.swift Sources/UI/Dashboard/DashboardViewController.swift Sources/UI/Dashboard/GridLayout.swift
git commit -m "feat: redesign grid tiles as Zoom-style terminal tiles with bottom bars"
```

---

### Task 4: Redesign Mini Cards (Speaker View Sidebar)

Replace the current MiniCardView with a 16:9 info card showing project, branch, duration, last update, message, and status.

**Files:**
- Rewrite: `Sources/UI/Dashboard/MiniCardView.swift`
- Modify: `Sources/UI/Dashboard/DashboardViewController.swift` (rebuild methods)

- [ ] **Step 1: Rewrite MiniCardView**

New layout (16:9, e.g. 240×135):
```
┌────────────────────────────┐
│ ● project / branch         │  ← line 1: identity
│ ⏱ 01:23:45 · 2m ago  Running │  ← line 2: meta + status
│ Editing src/Auth.tsx       │  ← line 3+: last message
│ Adding error boundary...   │     monospace, 3-line clamp
│                            │
└────────────────────────────┘
```

Properties: same `configure(id:project:thread:status:lastMessage:totalDuration:roundDuration:)` signature.

States:
- Selected: border `1.5px #33c17b`, bg `#1a1a1a`
- Hover: border `1.5px rgba(255,255,255,0.08)`, bg `#222`, text brightened
- Default: no border, bg `#1a1a1a`

Add `NSTrackingArea` for hover. `viewDidChangeEffectiveAppearance` for theme.

- [ ] **Step 2: Enforce 16:9 aspect ratio**

Add constraint in setup:
```swift
let aspect = widthAnchor.constraint(equalTo: heightAnchor, multiplier: 16.0 / 9.0)
aspect.priority = .defaultHigh
aspect.isActive = true
```

- [ ] **Step 3: Update DashboardViewController sidebar width**

In `rebuildLeftRight()`, change sidebar width reference to `240` to fit 16:9 cards properly.
In `rebuildTopSmall()` and `rebuildTopLarge()`, card width constraint stays `240` with min `180` max `260`.

- [ ] **Step 4: Build and verify**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmux -configuration Debug build`

- [ ] **Step 5: Commit**

```bash
git add Sources/UI/Dashboard/MiniCardView.swift Sources/UI/Dashboard/DashboardViewController.swift
git commit -m "feat: redesign mini cards as 16:9 info cards with project/branch/duration/message"
```

---

### Task 5: Add Arrow Icon to Main Tile (Speaker View)

Add the `>` arrow icon to the main tile's bottom bar for jumping to Project Detail.

**Files:**
- Modify: `Sources/UI/Dashboard/FocusPanelView.swift`
- Modify: `Sources/UI/Dashboard/DashboardViewController.swift` (wire up action)

- [ ] **Step 1: Add enter-project button to FocusPanelView bottom bar**

In the existing bottom bar area (or the header bar), add a 22×22 button on the right side:
- NSButton with custom drawing or NSImage `chevron.right`
- Default: bg `#ffffff0a`, tint `#999`
- Hover: bg `#ffffff18`, tint `#fff`
- Action calls `delegate?.focusPanelDidRequestEnterProject(projectName)`

- [ ] **Step 2: Build and verify**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmux -configuration Debug build`

- [ ] **Step 3: Commit**

```bash
git add Sources/UI/Dashboard/FocusPanelView.swift Sources/UI/Dashboard/DashboardViewController.swift
git commit -m "feat: add arrow icon to Speaker View main tile for Project Detail navigation"
```

---

### Task 6: Rebuild View Switcher Dropdown

Replace the existing `LayoutPopoverView` with a Zoom-style dropdown menu.

**Files:**
- Rewrite: `Sources/UI/TitleBar/LayoutPopoverView.swift`

- [ ] **Step 1: Rewrite LayoutPopoverView**

New design:
- Width 180px, bg `#1a1a1a`, radius 8px, border `1px #333`, shadow
- 4 items: Grid, Left-Right, Top-Small, Top-Large
- Each item: layout thumbnail icon (16×16 SVG via NSImage) + label (12px)
- Selected: blue icon + blue text + checkmark `✓`
- Hover: bg `rgba(255,255,255,0.03)`, text white
- Items separated by 1px line `#222`

Positioning: anchored below the view-switcher button in the right arc block.

- [ ] **Step 2: Build and verify**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmux -configuration Debug build`

- [ ] **Step 3: Commit**

```bash
git add Sources/UI/TitleBar/LayoutPopoverView.swift
git commit -m "feat: rebuild view switcher as Zoom-style dropdown menu"
```

---

### Task 7: Restyle Notification Panel

Update the notification slide panel to match Zoom's dark panel aesthetic.

**Files:**
- Modify: `Sources/UI/Panel/NotificationPanelView.swift`
- Modify: `Sources/UI/Panel/PanelBackdropView.swift`

- [ ] **Step 1: Update NotificationPanelView styling**

- Width stays 320px
- Background: `#1a1a1a`, border-left `1px #222`
- Shadow: `-8px 0 24px rgba(0,0,0,0.3)`, add `shadowPath` in `layout()`
- Header: bell SVG icon + "Notifications" + count + close button (24×24, bg `#ffffff08`)
- Items: status dot + branch name + timestamp (right-aligned) + description
- Error items: tinted `#1f1515` bg with `#ff453a30` border
- Standard items: bg `#111`, border `1px #222`

- [ ] **Step 2: Update PanelBackdropView**

Backdrop color already uses `SemanticColors.backdropBlack` (rgba 0,0,0,0.2). Update to `0.4` to match spec:

```swift
static let backdropBlack: NSColor = NSColor.black.withAlphaComponent(0.4)
```

- [ ] **Step 3: Build and verify**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmux -configuration Debug build`

- [ ] **Step 4: Commit**

```bash
git add Sources/UI/Panel/NotificationPanelView.swift Sources/UI/Panel/PanelBackdropView.swift Sources/UI/Shared/SemanticColors.swift
git commit -m "style: restyle notification panel to Zoom dark aesthetic"
```

---

### Task 8: Restyle AI Panel

Update the AI slide panel with Zoom-style chat bubbles and input.

**Files:**
- Modify: `Sources/UI/Panel/AIPanelView.swift`

- [ ] **Step 1: Update AIPanelView styling**

- Width stays 340px, same panel base styling as notifications
- Header: sparkles icon + "AI Assistant" + close button
- Chat bubbles: assistant `bg #222` radius `8px 8px 8px 2px`, user `bg #263554` radius `8px 8px 2px 8px`
- Input: bg `#111`, border `1px #333`, radius 6px
- Send button: bg `#4f8cff`, 28×28, radius 6px, white arrow icon
- Add `shadowPath` in `layout()` override

- [ ] **Step 2: Build and verify**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmux -configuration Debug build`

- [ ] **Step 3: Commit**

```bash
git add Sources/UI/Panel/AIPanelView.swift
git commit -m "style: restyle AI panel with Zoom-style chat bubbles"
```

---

### Task 9: Restyle Project Detail View

Update RepoViewController and SidebarViewController to match Zoom aesthetic. Add "+" New Thread button to sidebar header.

**Files:**
- Modify: `Sources/UI/Repo/RepoViewController.swift`
- Modify: `Sources/UI/Repo/SidebarViewController.swift`

- [ ] **Step 1: Update RepoViewController colors**

- Background: `SemanticColors.bg` (now `#0b0b0b`)
- Terminal container: bg `#111`, border `1px #222`, radius 4px
- Remove old `SemanticColors.panel2`/`lineAlpha38` refs, use new tokens

- [ ] **Step 2: Update SidebarViewController**

- Background: `#111` (tileBg)
- Add header bar: "Threads" label + count + "+" button (24×24, radius 6px, bg `#ffffff0a`)
- Thread rows: status dot (7px) + branch name (11px) + last message (9px mono, 2-line clamp)
- Row states: selected (`#1a2a1a`, border `rgba(51,193,123,0.25)`), hover (`rgba(255,255,255,0.03)`), default (transparent)
- "+" button action: call `delegate` method which triggers NewBranchDialog

- [ ] **Step 3: Wire "+" button to NewBranchDialog**

In `SidebarViewController`, add a `didRequestNewThread` delegate method.
In `RepoViewController`, forward to `MainWindowController` which presents `NewBranchDialog`.

- [ ] **Step 4: Build and verify**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmux -configuration Debug build`

- [ ] **Step 5: Commit**

```bash
git add Sources/UI/Repo/RepoViewController.swift Sources/UI/Repo/SidebarViewController.swift
git commit -m "style: restyle Project Detail with Zoom aesthetic, add New Thread button"
```

---

### Task 10: Restyle Modal Dialogs

Update UnifiedModalView and NewBranchDialog to match the Zoom dark dialog style.

**Files:**
- Modify: `Sources/UI/Dialog/UnifiedModalView.swift`
- Modify: `Sources/UI/Dialog/NewBranchDialog.swift`
- Modify: `Sources/UI/Dialog/QuickSwitcherViewController.swift`

- [ ] **Step 1: Update UnifiedModalView**

- Backdrop: `rgba(0,0,0,0.6)`
- Dialog: width 400px, bg `#1a1a1a`, radius 10px, border `1px #333`
- Header border: `1px #222`
- Input fields: bg `#111`, border `1px #333`, radius 6px
- Cancel button: bg `#ffffff08`, text `#aaa`
- Confirm button: bg `#4f8cff`, text white
- Destructive confirm: bg `#ff453a`

- [ ] **Step 2: Sync NewBranchDialog**

NewBranchDialog already has a `ZoomColors` enum. Update it to reference `SemanticColors` instead for consistency, or sync the hex values.

- [ ] **Step 3: Update QuickSwitcherViewController**

Replace `Theme.` references with matching `SemanticColors` tokens for the new palette.

- [ ] **Step 4: Build and verify**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmux -configuration Debug build`

- [ ] **Step 5: Commit**

```bash
git add Sources/UI/Dialog/UnifiedModalView.swift Sources/UI/Dialog/NewBranchDialog.swift Sources/UI/Dialog/QuickSwitcherViewController.swift
git commit -m "style: restyle modal dialogs to Zoom dark aesthetic"
```

---

### Task 11: Final Integration and Cleanup

Wire everything together, fix any remaining color references, and clean up unused code.

**Files:**
- Modify: `Sources/App/MainWindowController.swift`
- Modify: `Sources/UI/Dashboard/DashboardViewController.swift`
- Delete or gut: `Sources/UI/StatusBar/StatusBarView.swift` (if not removed in Task 2)
- Modify: `Sources/UI/Shared/Theme.swift` (remove stale constants)

- [ ] **Step 1: Set window background**

In `MainWindowController`, ensure window background color is `SemanticColors.bg` (`#0b0b0b`).

- [ ] **Step 2: Verify all view transitions**

Test each flow:
1. App launch → Dashboard Grid view
2. Click tile → Speaker View (left-right)
3. Click `>` arrow → Project Detail tab
4. Click Dashboard tab → back to Dashboard
5. View switcher → change layout modes
6. Bell → notification panel slides
7. Sparkles → AI panel slides
8. Theme toggle → appearance switch
9. New Thread "+" → dialog appears

- [ ] **Step 3: Remove unused StatusBarView references**

Grep for `statusBar`, `StatusBarView`, `updateStatusBar` across the codebase and remove any remaining references.

- [ ] **Step 4: Build and run full test suite**

```bash
xcodebuild -project pmux.xcodeproj -scheme pmux -configuration Debug build
xcodebuild -project pmux.xcodeproj -scheme pmuxTests -configuration Debug test
```

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat: complete Zoom-style UI redesign integration"
```
