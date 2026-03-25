# Stacked Card Effect Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** When a dashboard grid card represents a worktree with multiple split panes, display 1–2 ghost cards behind it offset to the bottom-right, creating a physical card-stack appearance.

**Architecture:** Introduce `StackedCardContainerView` — a thin `NSView` wrapper that holds the existing `AgentCardView` plus 0–2 decorative ghost `NSView`s. `DashboardViewController.gridCards` changes type from `[AgentCardView]` to `[StackedCardContainerView]`. Ghost views overflow the container boundary (via `masksToBounds = false`) using AppKit Y-up coordinates.

**Tech Stack:** Swift 5.10, AppKit, XCTest — no new dependencies.

---

## File map

| Action | File | What changes |
|--------|------|-------------|
| Modify | `Sources/UI/Shared/SemanticColors.swift` | Add 3 new color tokens: `tileGhost1Bg`, `tileGhost2Bg`, `tileGhostBorder` |
| Create | `Sources/UI/Dashboard/StackedCardContainerView.swift` | New component: container + ghost logic |
| Modify | `Sources/UI/Dashboard/DashboardViewController.swift` | Change `gridCards` type; update `rebuildGrid`, `updateGridInPlace`, `terminalSurfaceDidRecover` |
| Create | `tests/StackedCardContainerViewTests.swift` | Unit tests for ghost count and hit-testing |

---

## Task 1: Add ghost color tokens to SemanticColors

**Files:**
- Modify: `Sources/UI/Shared/SemanticColors.swift` (after the `tileBarBg` token, around line 229)

- [ ] **Step 1: Add the three color tokens**

Open `Sources/UI/Shared/SemanticColors.swift` and add after the `tileBarBg` token:

```swift
static let tileGhost1Bg: NSColor = NSColor(name: nil) { appearance in
    appearance.isDark
        ? NSColor(hex: 0x1a1a2e)
        : NSColor(hex: 0xe8e8f0)
}
static let tileGhost2Bg: NSColor = NSColor(name: nil) { appearance in
    appearance.isDark
        ? NSColor(hex: 0x161625)
        : NSColor(hex: 0xdcdce8)
}
static let tileGhostBorder: NSColor = NSColor(name: nil) { appearance in
    let ln = appearance.isDark ? NSColor(hex: 0x222222) : NSColor(hex: 0xd7dbe3)
    return ln.withAlphaComponent(0.60)
}
```

- [ ] **Step 2: Build to verify no errors**

```bash
xcodebuild -project pmux.xcodeproj -scheme pmux -configuration Debug build 2>&1 | tail -5
```
Expected: `** BUILD SUCCEEDED **`

- [ ] **Step 3: Commit**

```bash
git add Sources/UI/Shared/SemanticColors.swift
git commit -m "feat: add ghost card color tokens to SemanticColors"
```

---

## Task 2: Create StackedCardContainerView

**Files:**
- Create: `Sources/UI/Dashboard/StackedCardContainerView.swift`
- Create: `tests/StackedCardContainerViewTests.swift`

- [ ] **Step 1: Write the failing tests**

Create `tests/StackedCardContainerViewTests.swift`:

```swift
import XCTest
@testable import pmux

final class StackedCardContainerViewTests: XCTestCase {

    func testNoPanesProducesNoGhosts() {
        let container = StackedCardContainerView()
        container.configure(paneCount: 1)
        XCTAssertEqual(container.ghostViews.count, 0)
    }

    func testTwoPanesProducesOneGhost() {
        let container = StackedCardContainerView()
        container.configure(paneCount: 2)
        XCTAssertEqual(container.ghostViews.count, 1)
    }

    func testThreePanesProducesTwoGhosts() {
        let container = StackedCardContainerView()
        container.configure(paneCount: 3)
        XCTAssertEqual(container.ghostViews.count, 2)
    }

    func testFivePanesProducesTwoGhosts() {
        let container = StackedCardContainerView()
        container.configure(paneCount: 5)
        XCTAssertEqual(container.ghostViews.count, 2)
    }

    func testGhostsRemovedWhenPaneCountDrops() {
        let container = StackedCardContainerView()
        container.configure(paneCount: 3)
        XCTAssertEqual(container.ghostViews.count, 2)
        container.configure(paneCount: 1)
        XCTAssertEqual(container.ghostViews.count, 0)
        // Verify they were removed from the view hierarchy too
        XCTAssertEqual(container.subviews.count, 1) // only cardView remains
    }

    func testGhostsAreNotInSubviewsWhenZero() {
        let container = StackedCardContainerView()
        container.configure(paneCount: 1)
        // cardView is the only subview
        XCTAssertEqual(container.subviews.count, 1)
        XCTAssertTrue(container.subviews.first === container.cardView)
    }

    func testHitTestOutsideCardViewReturnsNil() {
        let container = StackedCardContainerView()
        container.frame = NSRect(x: 0, y: 0, width: 200, height: 130)
        container.cardView.frame = NSRect(x: 0, y: 0, width: 200, height: 130)
        container.configure(paneCount: 2)
        // Point in ghost overflow zone (below card in AppKit = negative y)
        let ghostPoint = NSPoint(x: 10, y: -5)
        XCTAssertNil(container.hitTest(ghostPoint))
    }

    func testHitTestInsideCardViewReturnsNonNil() {
        let container = StackedCardContainerView()
        container.frame = NSRect(x: 0, y: 0, width: 200, height: 130)
        container.cardView.frame = NSRect(x: 0, y: 0, width: 200, height: 130)
        let centerPoint = NSPoint(x: 100, y: 65)
        XCTAssertNotNil(container.hitTest(centerPoint))
    }

    func testAgentIdForwarding() {
        let container = StackedCardContainerView()
        container.cardView.configure(
            id: "test-id", project: "proj", thread: "main",
            status: "idle", lastMessage: "", totalDuration: "", roundDuration: ""
        )
        XCTAssertEqual(container.agentId, "test-id")
    }

    func testIsSelectedForwarding() {
        let container = StackedCardContainerView()
        container.isSelected = true
        XCTAssertTrue(container.cardView.isSelected)
        container.isSelected = false
        XCTAssertFalse(container.cardView.isSelected)
    }
}
```

- [ ] **Step 2: Run tests to confirm they fail**

```bash
xcodebuild -project pmux.xcodeproj -scheme pmuxTests -configuration Debug test \
  -only-testing:pmuxTests/StackedCardContainerViewTests 2>&1 | tail -10
```
Expected: build error — `StackedCardContainerView` does not exist yet.

- [ ] **Step 3: Implement StackedCardContainerView**

Create `Sources/UI/Dashboard/StackedCardContainerView.swift`:

```swift
import AppKit

final class StackedCardContainerView: NSView {
    let cardView = AgentCardView()
    private(set) var ghostViews: [NSView] = []

    weak var delegate: AgentCardDelegate? {
        didSet { /* cardView.delegate intentionally left nil */ }
    }

    var agentId: String { cardView.agentId }

    var isSelected: Bool {
        get { cardView.isSelected }
        set { cardView.isSelected = newValue }
    }

    override init(frame: NSRect) {
        super.init(frame: frame)
        setup()
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) not supported")
    }

    private func setup() {
        wantsLayer = true
        layer?.masksToBounds = false

        // cardView on top; ghost views are inserted below it
        addSubview(cardView)

        let click = NSClickGestureRecognizer(target: self, action: #selector(handleClick))
        addGestureRecognizer(click)
    }

    // MARK: - Configure

    /// Updates ghost view count. Surplus ghosts are removed via removeFromSuperview().
    /// Needed ghosts are created and inserted below cardView.
    func configure(paneCount: Int) {
        let needed = min(max(paneCount - 1, 0), 2)

        // Remove surplus ghosts
        while ghostViews.count > needed {
            ghostViews.removeLast().removeFromSuperview()
        }

        // Add missing ghosts
        while ghostViews.count < needed {
            let ghost = makeGhostView(index: ghostViews.count)
            // Insert below cardView (index 0 = bottom of z-order)
            addSubview(ghost, positioned: .below, relativeTo: cardView)
            ghostViews.append(ghost)
        }
    }

    // MARK: - Layout

    override func resizeSubviews(withOldSize oldSize: NSSize) {
        super.resizeSubviews(withOldSize: oldSize)
        layoutChildren()
    }

    func layoutChildren() {
        let w = bounds.width
        let h = bounds.height

        // Main card fills the container's own bounds
        cardView.frame = NSRect(x: 0, y: 0, width: w, height: h)

        // Ghost offsets: in AppKit (Y-up), down on screen = negative Y
        // ghost at index 0 = closest (offset 6,6), index 1 = farthest (offset 12,12)
        let offsets: [(CGFloat, CGFloat)] = [(6, -6), (12, -12)]
        for (i, ghost) in ghostViews.enumerated() {
            let (dx, dy) = offsets[i]
            ghost.frame = NSRect(x: dx, y: dy, width: w, height: h)
        }
    }

    // MARK: - Hit testing

    override func hitTest(_ point: NSPoint) -> NSView? {
        guard cardView.frame.contains(point) else { return nil }
        return super.hitTest(point)
    }

    // MARK: - Click

    @objc private func handleClick() {
        delegate?.agentCardClicked(agentId: cardView.agentId)
    }

    // MARK: - Appearance

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        ghostViews.forEach { $0.needsDisplay = true }
    }

    // MARK: - Private helpers

    private func makeGhostView(index: Int) -> NSView {
        let v = GhostCardView()
        v.ghostIndex = index
        v.wantsLayer = true
        v.layer?.cornerRadius = 4
        v.layer?.masksToBounds = true
        return v
    }
}

// MARK: - GhostCardView

/// A purely decorative view that renders a ghost card background and border.
private final class GhostCardView: NSView {
    var ghostIndex: Int = 0

    override var wantsUpdateLayer: Bool { true }

    override func updateLayer() {
        let bg = ghostIndex == 0 ? SemanticColors.tileGhost1Bg : SemanticColors.tileGhost2Bg
        layer?.backgroundColor = resolvedCGColor(bg)
        layer?.borderColor = resolvedCGColor(SemanticColors.tileGhostBorder)
        layer?.borderWidth = 1
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        needsDisplay = true
    }
}
```

- [ ] **Step 4: Run tests to confirm they pass**

```bash
xcodebuild -project pmux.xcodeproj -scheme pmuxTests -configuration Debug test \
  -only-testing:pmuxTests/StackedCardContainerViewTests 2>&1 | tail -15
```
Expected: `** TEST SUCCEEDED **`

- [ ] **Step 5: Add StackedCardContainerView.swift to project.yml**

Open `project.yml`. Find the `sources` section under the main pmux target (look for the pattern `Sources/UI/Dashboard/`). Add the new file:

```yaml
- Sources/UI/Dashboard/StackedCardContainerView.swift
```

Then regenerate:
```bash
xcodegen generate
```

- [ ] **Step 6: Build to confirm no errors**

```bash
xcodebuild -project pmux.xcodeproj -scheme pmux -configuration Debug build 2>&1 | tail -5
```
Expected: `** BUILD SUCCEEDED **`

- [ ] **Step 7: Commit**

```bash
git add Sources/UI/Dashboard/StackedCardContainerView.swift \
        tests/StackedCardContainerViewTests.swift \
        project.yml pmux.xcodeproj
git commit -m "feat: add StackedCardContainerView for multi-pane card stacking"
```

---

## Task 3: Migrate DashboardViewController to use StackedCardContainerView

**Files:**
- Modify: `Sources/UI/Dashboard/DashboardViewController.swift`

The changes are mechanical. Work through them one section at a time.

- [ ] **Step 1: Change gridCards type declaration**

Find line ~68:
```swift
private var gridCards: [AgentCardView] = []
```
Change to:
```swift
private var gridCards: [StackedCardContainerView] = []
```

- [ ] **Step 2: Update rebuildGrid**

Find the `rebuildGrid` method (~line 535). Replace the card-creation block:

```swift
// OLD:
let card = AgentCardView()
card.delegate = self
card.configure(
    id: agent.id,
    project: agent.project,
    thread: agent.thread,
    status: agent.status,
    lastMessage: agent.lastMessage,
    totalDuration: agent.totalDuration,
    roundDuration: agent.roundDuration,
    paneCount: agent.paneCount
)
card.translatesAutoresizingMaskIntoConstraints = true
gridCards.append(card)
gridContainer.addSubview(card)
```

```swift
// NEW:
let container = StackedCardContainerView()
container.delegate = self
container.configure(paneCount: agent.paneCount)
container.cardView.configure(
    id: agent.id,
    project: agent.project,
    thread: agent.thread,
    status: agent.status,
    lastMessage: agent.lastMessage,
    totalDuration: agent.totalDuration,
    roundDuration: agent.roundDuration,
    paneCount: agent.paneCount
)
container.isSelected = (agent.id == selectedAgentId)
container.translatesAutoresizingMaskIntoConstraints = true
gridCards.append(container)
gridContainer.addSubview(container)
```

- [ ] **Step 3: Update layoutGridFrames**

Find `layoutGridFrames` (~line 576). The loop assigns `card.frame`. Update to also call `layoutChildren()` so ghost offsets are applied after the frame is set:

```swift
// OLD:
for (index, card) in gridCards.enumerated() {
    card.frame = layout.cardFrame(at: index)
}
```

```swift
// NEW:
for (index, container) in gridCards.enumerated() {
    container.frame = layout.cardFrame(at: index)
    container.layoutChildren()
}
```

- [ ] **Step 4: Update updateGridInPlace**

Find `updateGridInPlace` → `updateGridInPlace` calls `updateGridInPlace(_:)` (~line 168). Update the loop body:

```swift
// OLD:
gridCards[index].configure(
    id: agent.id,
    project: agent.project,
    thread: agent.thread,
    status: agent.status,
    lastMessage: agent.lastMessage,
    totalDuration: agent.totalDuration,
    roundDuration: agent.roundDuration,
    paneCount: agent.paneCount
)
```

```swift
// NEW:
gridCards[index].configure(paneCount: agent.paneCount)
gridCards[index].cardView.configure(
    id: agent.id,
    project: agent.project,
    thread: agent.thread,
    status: agent.status,
    lastMessage: agent.lastMessage,
    totalDuration: agent.totalDuration,
    roundDuration: agent.roundDuration,
    paneCount: agent.paneCount
)
gridCards[index].isSelected = (agent.id == selectedAgentId)
```

- [ ] **Step 5: Update terminalSurfaceDidRecover**

Find `terminalSurfaceDidRecover` (~line 839). Update the gridCards lookup:

```swift
// OLD:
if let card = gridCards.first(where: { $0.agentId == agent.id }) {
    embedSurface(agent, in: card.terminalContainer)
    return
}
```

```swift
// NEW:
if let container = gridCards.first(where: { $0.agentId == agent.id }) {
    embedSurface(agent, in: container.cardView.terminalContainer)
    return
}
```

- [ ] **Step 6: Build to confirm no compiler errors**

```bash
xcodebuild -project pmux.xcodeproj -scheme pmux -configuration Debug build 2>&1 | tail -5
```
Expected: `** BUILD SUCCEEDED **`

- [ ] **Step 7: Run all unit tests**

```bash
xcodebuild -project pmux.xcodeproj -scheme pmuxTests -configuration Debug test 2>&1 | tail -10
```
Expected: `** TEST SUCCEEDED **`

- [ ] **Step 8: Commit**

```bash
git add Sources/UI/Dashboard/DashboardViewController.swift
git commit -m "feat: wire StackedCardContainerView into dashboard grid"
```

---

## Verification checklist (manual, after all tasks)

- [ ] Grid layout: single-pane cards look unchanged (no ghost cards)
- [ ] Grid layout: 2-pane worktree shows 1 ghost card offset down-right
- [ ] Grid layout: 3+-pane worktree shows 2 ghost cards stacked
- [ ] Dark mode: ghost cards are purple-tinted darker than main card
- [ ] Light mode (System Preferences → Appearance → Light): ghost cards are blue-tinted lighter than main card
- [ ] Clicking a card in the grid opens spotlight / speaker view as before
- [ ] Drag-reorder: dragging cards in the grid still works
- [ ] `terminalSurfaceDidRecover`: terminal surface re-embeds after reconnect (can simulate by killing tmux session and waiting)
