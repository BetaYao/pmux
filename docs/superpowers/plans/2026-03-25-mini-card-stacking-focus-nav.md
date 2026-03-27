# Mini Card Stacking + Focus Panel Navigation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add ghost card stacking to mini cards in focus layouts and prev/next navigation with slide animation to the focus panel.

**Architecture:** `StackedMiniCardContainerView` wraps `MiniCardView` with 3px-offset ghosts (mirroring `StackedCardContainerView`). `FocusPanelView` gains nav buttons + counter label in its header bar. `DashboardViewController` handles navigation delegate calls, updates selection, and triggers `CATransition` slide animations matched to layout direction.

**Tech Stack:** Swift 5.10, AppKit, CATransition, XCTest

**Spec:** `docs/superpowers/specs/2026-03-25-mini-card-stacking-focus-nav-design.md`

---

## File Structure

| File | Action | Responsibility |
|------|--------|----------------|
| `Sources/UI/Dashboard/StackedMiniCardContainerView.swift` | Create | Wraps MiniCardView with 3px ghost offset, owns click handling |
| `Sources/UI/Dashboard/FocusPanelView.swift` | Modify | Add NavigationDirection enum, prev/next buttons, counter label, delegate method |
| `Sources/UI/Dashboard/DashboardViewController.swift` | Modify | Replace MiniCardView with stacked containers; implement navigation + slide animation |
| `Tests/StackedMiniCardContainerViewTests.swift` | Create | Unit tests for ghost count, hit testing, delegation |
| `Tests/FocusPanelNavigationTests.swift` | Create | Unit tests for configureNavigation visibility/state |

---

### Task 0: Regenerate Xcode Project After Creating New Files

**Important:** Run `xcodegen generate` after creating any new source/test files and before running build/test commands. Each task below that creates files includes a `xcodegen generate` step.

---

### Task 1: StackedMiniCardContainerView — Tests & Implementation

**Files:**
- Create: `Tests/StackedMiniCardContainerViewTests.swift`
- Create: `Sources/UI/Dashboard/StackedMiniCardContainerView.swift`

- [ ] **Step 1: Write failing tests**

```swift
// Tests/StackedMiniCardContainerViewTests.swift
import XCTest
@testable import amux

final class StackedMiniCardContainerViewTests: XCTestCase {

    func testNoPanesProducesNoGhosts() {
        let container = StackedMiniCardContainerView()
        container.configure(paneCount: 1)
        XCTAssertEqual(container.ghostViews.count, 0)
    }

    func testTwoPanesProducesOneGhost() {
        let container = StackedMiniCardContainerView()
        container.configure(paneCount: 2)
        XCTAssertEqual(container.ghostViews.count, 1)
    }

    func testThreePanesProducesTwoGhosts() {
        let container = StackedMiniCardContainerView()
        container.configure(paneCount: 3)
        XCTAssertEqual(container.ghostViews.count, 2)
    }

    func testFivePanesCapsAtTwoGhosts() {
        let container = StackedMiniCardContainerView()
        container.configure(paneCount: 5)
        XCTAssertEqual(container.ghostViews.count, 2)
    }

    func testGhostsRemovedWhenPaneCountDrops() {
        let container = StackedMiniCardContainerView()
        container.configure(paneCount: 3)
        XCTAssertEqual(container.ghostViews.count, 2)
        container.configure(paneCount: 1)
        XCTAssertEqual(container.ghostViews.count, 0)
        // Only miniCardView remains
        XCTAssertEqual(container.subviews.count, 1)
    }

    func testGhostOffset3px() {
        let container = StackedMiniCardContainerView()
        container.frame = NSRect(x: 0, y: 0, width: 220, height: 128)
        container.configure(paneCount: 3)
        container.layoutChildren()
        // With 2 ghosts, maxOffset = 2 * 3 = 6
        // miniCardView frame: (0, 6, 214, 122)
        XCTAssertEqual(container.miniCardView.frame.origin.x, 0)
        XCTAssertEqual(container.miniCardView.frame.origin.y, 6)
        XCTAssertEqual(container.miniCardView.frame.width, 214)
        XCTAssertEqual(container.miniCardView.frame.height, 122)
        // First ghost at offset (3, 3, 214, 122)
        XCTAssertEqual(container.ghostViews[0].frame.origin.x, 3)
        XCTAssertEqual(container.ghostViews[0].frame.origin.y, 3)
        // Second ghost at offset (6, 0, 214, 122)
        XCTAssertEqual(container.ghostViews[1].frame.origin.x, 6)
        XCTAssertEqual(container.ghostViews[1].frame.origin.y, 0)
    }

    func testHitTestOutsideMiniCardReturnsNil() {
        let container = StackedMiniCardContainerView()
        container.frame = NSRect(x: 0, y: 0, width: 220, height: 128)
        container.miniCardView.frame = NSRect(x: 0, y: 6, width: 214, height: 122)
        container.configure(paneCount: 2)
        // Point below miniCardView (in ghost overflow)
        let ghostPoint = NSPoint(x: 10, y: 2)
        XCTAssertNil(container.hitTest(ghostPoint))
    }

    func testAgentIdForwarding() {
        let container = StackedMiniCardContainerView()
        container.miniCardView.configure(
            id: "test-id", project: "proj", thread: "main",
            status: "idle", lastMessage: "", totalDuration: "", roundDuration: ""
        )
        XCTAssertEqual(container.agentId, "test-id")
    }

    func testIsSelectedForwarding() {
        let container = StackedMiniCardContainerView()
        container.isSelected = true
        XCTAssertTrue(container.miniCardView.isSelected)
        container.isSelected = false
        XCTAssertFalse(container.miniCardView.isSelected)
    }
}
```

- [ ] **Step 2: Implement StackedMiniCardContainerView**

Create the source file so it exists before running xcodegen.

```swift
// Sources/UI/Dashboard/StackedMiniCardContainerView.swift
import AppKit

final class StackedMiniCardContainerView: NSView {
    let miniCardView = MiniCardView()
    private(set) var ghostViews: [NSView] = []

    /// The container owns click handling. miniCardView.delegate must remain nil.
    weak var delegate: AgentCardDelegate?

    var agentId: String { miniCardView.agentId }

    var isSelected: Bool {
        get { miniCardView.isSelected }
        set { miniCardView.isSelected = newValue }
    }

    override init(frame: NSRect) {
        super.init(frame: frame)
        setup()
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) not supported") }

    private func setup() {
        wantsLayer = true
        layer?.masksToBounds = false

        // Disable MiniCardView's own click handler to prevent double-firing.
        // Remove its gesture recognizers and use the container's instead.
        miniCardView.gestureRecognizers.forEach { miniCardView.removeGestureRecognizer($0) }
        miniCardView.translatesAutoresizingMaskIntoConstraints = true
        addSubview(miniCardView)

        let click = NSClickGestureRecognizer(target: self, action: #selector(handleClick))
        addGestureRecognizer(click)
    }

    // MARK: - Configure

    func configure(paneCount: Int) {
        let needed = min(max(paneCount - 1, 0), 2)

        while ghostViews.count > needed {
            ghostViews.removeLast().removeFromSuperview()
        }

        while ghostViews.count < needed {
            let ghost = MiniGhostCardView()
            ghost.ghostIndex = ghostViews.count
            ghost.wantsLayer = true
            addSubview(ghost, positioned: .below, relativeTo: miniCardView)
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
        let ghostOffset: CGFloat = 3
        let maxOffset = CGFloat(ghostViews.count) * ghostOffset
        let cardW = w - maxOffset
        let cardH = h - maxOffset

        miniCardView.frame = NSRect(x: 0, y: maxOffset, width: cardW, height: cardH)

        for (i, ghost) in ghostViews.enumerated() {
            let offset = CGFloat(i + 1) * ghostOffset
            ghost.frame = NSRect(x: offset, y: maxOffset - offset, width: cardW, height: cardH)
        }
    }

    // MARK: - Hit testing

    override func hitTest(_ point: NSPoint) -> NSView? {
        guard miniCardView.frame.contains(point) else { return nil }
        return super.hitTest(point)
    }

    // MARK: - Click

    @objc private func handleClick() {
        delegate?.agentCardClicked(agentId: miniCardView.agentId)
    }

    // MARK: - Appearance

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        ghostViews.forEach { $0.needsDisplay = true }
    }
}

// MARK: - MiniGhostCardView

private final class MiniGhostCardView: NSView {
    var ghostIndex: Int = 0

    override var wantsUpdateLayer: Bool { true }

    override func updateLayer() {
        layer?.cornerRadius = 4
        layer?.masksToBounds = true
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

- [ ] **Step 3: Regenerate Xcode project**

Run: `cd /Users/matt.chow/workspace/amux-swift && xcodegen generate`

- [ ] **Step 4: Run tests to verify they pass**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/StackedMiniCardContainerViewTests 2>&1 | tail -20`
Expected: All 9 tests PASS

- [ ] **Step 5: Commit**

```bash
git add Sources/UI/Dashboard/StackedMiniCardContainerView.swift Tests/StackedMiniCardContainerViewTests.swift amux.xcodeproj
git commit -m "feat: add StackedMiniCardContainerView with 3px ghost offset"
```

---

### Task 2: FocusPanelView Navigation — Tests & Implementation

**Files:**
- Create: `Tests/FocusPanelNavigationTests.swift`
- Modify: `Sources/UI/Dashboard/FocusPanelView.swift`

- [ ] **Step 1: Write failing tests**

```swift
// Tests/FocusPanelNavigationTests.swift
import XCTest
@testable import amux

final class FocusPanelNavigationTests: XCTestCase {

    func testNavigationHiddenWhenTotalIsOne() {
        let panel = FocusPanelView()
        panel.configureNavigation(currentIndex: 0, total: 1)
        XCTAssertTrue(panel.prevButton.isHidden)
        XCTAssertTrue(panel.nextButton.isHidden)
        XCTAssertTrue(panel.counterLabel.isHidden)
    }

    func testNavigationVisibleWhenTotalGreaterThanOne() {
        let panel = FocusPanelView()
        panel.configureNavigation(currentIndex: 0, total: 4)
        XCTAssertFalse(panel.prevButton.isHidden)
        XCTAssertFalse(panel.nextButton.isHidden)
        XCTAssertFalse(panel.counterLabel.isHidden)
    }

    func testCounterLabelFormat() {
        let panel = FocusPanelView()
        panel.configureNavigation(currentIndex: 2, total: 5)
        XCTAssertEqual(panel.counterLabel.stringValue, "3/5")
    }

    func testPrevDisabledAtFirstIndex() {
        let panel = FocusPanelView()
        panel.configureNavigation(currentIndex: 0, total: 4)
        XCTAssertFalse(panel.prevButton.isEnabled)
        XCTAssertTrue(panel.nextButton.isEnabled)
    }

    func testNextDisabledAtLastIndex() {
        let panel = FocusPanelView()
        panel.configureNavigation(currentIndex: 3, total: 4)
        XCTAssertTrue(panel.prevButton.isEnabled)
        XCTAssertFalse(panel.nextButton.isEnabled)
    }

    func testBothEnabledInMiddle() {
        let panel = FocusPanelView()
        panel.configureNavigation(currentIndex: 1, total: 4)
        XCTAssertTrue(panel.prevButton.isEnabled)
        XCTAssertTrue(panel.nextButton.isEnabled)
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/FocusPanelNavigationTests 2>&1 | tail -20`
Expected: FAIL — `prevButton`, `nextButton`, `counterLabel`, `configureNavigation` do not exist

- [ ] **Step 3: Add NavigationDirection enum and nav UI to FocusPanelView**

Add `NavigationDirection` enum at the top of `FocusPanelView.swift`:

```swift
enum NavigationDirection {
    case next, previous
}
```

Add new properties to `FocusPanelView` (after existing properties):

```swift
let prevButton = NSButton()
let nextButton = NSButton()
let counterLabel = NSTextField(labelWithString: "")
```

Add to `FocusPanelDelegate` as an optional method with default empty extension (so existing conformances don't break until Task 4 adds the implementation):

```swift
// Add to protocol:
func focusPanelDidRequestNavigate(_ panel: FocusPanelView, direction: NavigationDirection)

// Add default implementation so existing conformances compile:
extension FocusPanelDelegate {
    func focusPanelDidRequestNavigate(_ panel: FocusPanelView, direction: NavigationDirection) {}
}
```

Add new method `setupNavigation()` called from `setup()`, after `setupHeader()` and before `setupTerminalContainer()`:

```swift
private func setupNavigation() {
    // Previous button
    prevButton.bezelStyle = .texturedRounded
    prevButton.isBordered = false
    prevButton.image = NSImage(systemSymbolName: "chevron.left", accessibilityDescription: "Previous")
    prevButton.symbolConfiguration = NSImage.SymbolConfiguration(pointSize: 11, weight: .medium)
    prevButton.target = self
    prevButton.action = #selector(prevClicked)
    prevButton.setAccessibilityIdentifier("dashboard.focusPanel.prev")
    prevButton.translatesAutoresizingMaskIntoConstraints = false
    prevButton.isHidden = true
    headerView.addSubview(prevButton)

    // Counter label
    counterLabel.font = NSFont.monospacedDigitSystemFont(ofSize: Typography.secondaryPointSize, weight: .medium)
    counterLabel.textColor = SemanticColors.muted
    counterLabel.alignment = .center
    counterLabel.translatesAutoresizingMaskIntoConstraints = false
    counterLabel.isHidden = true
    headerView.addSubview(counterLabel)

    // Next button
    nextButton.bezelStyle = .texturedRounded
    nextButton.isBordered = false
    nextButton.image = NSImage(systemSymbolName: "chevron.right", accessibilityDescription: "Next")
    nextButton.symbolConfiguration = NSImage.SymbolConfiguration(pointSize: 11, weight: .medium)
    nextButton.target = self
    nextButton.action = #selector(nextClicked)
    nextButton.setAccessibilityIdentifier("dashboard.focusPanel.next")
    nextButton.translatesAutoresizingMaskIntoConstraints = false
    nextButton.isHidden = true
    headerView.addSubview(nextButton)

    NSLayoutConstraint.activate([
        prevButton.trailingAnchor.constraint(equalTo: counterLabel.leadingAnchor, constant: -2),
        prevButton.centerYAnchor.constraint(equalTo: headerView.centerYAnchor),
        prevButton.widthAnchor.constraint(equalToConstant: 26),
        prevButton.heightAnchor.constraint(equalToConstant: 24),

        counterLabel.trailingAnchor.constraint(equalTo: nextButton.leadingAnchor, constant: -2),
        counterLabel.centerYAnchor.constraint(equalTo: headerView.centerYAnchor),

        nextButton.trailingAnchor.constraint(equalTo: enterButton.leadingAnchor, constant: -8),
        nextButton.centerYAnchor.constraint(equalTo: headerView.centerYAnchor),
        nextButton.widthAnchor.constraint(equalToConstant: 26),
        nextButton.heightAnchor.constraint(equalToConstant: 24),
    ])
}

@objc private func prevClicked() {
    delegate?.focusPanelDidRequestNavigate(self, direction: .previous)
}

@objc private func nextClicked() {
    delegate?.focusPanelDidRequestNavigate(self, direction: .next)
}

func configureNavigation(currentIndex: Int, total: Int) {
    let showNav = total > 1
    prevButton.isHidden = !showNav
    nextButton.isHidden = !showNav
    counterLabel.isHidden = !showNav

    guard showNav else { return }

    counterLabel.stringValue = "\(currentIndex + 1)/\(total)"
    prevButton.isEnabled = currentIndex > 0
    prevButton.alphaValue = currentIndex > 0 ? 1.0 : 0.3
    nextButton.isEnabled = currentIndex < total - 1
    nextButton.alphaValue = currentIndex < total - 1 ? 1.0 : 0.3
}
```

Also update the existing `durationLabel` trailing constraint — change `enterButton.leadingAnchor` to `prevButton.leadingAnchor`:

```swift
// Old:
durationLabel.trailingAnchor.constraint(lessThanOrEqualTo: enterButton.leadingAnchor, constant: -8),
// New:
durationLabel.trailingAnchor.constraint(lessThanOrEqualTo: prevButton.leadingAnchor, constant: -8),
```

- [ ] **Step 4: Regenerate Xcode project**

Run: `cd /Users/matt.chow/workspace/amux-swift && xcodegen generate`

- [ ] **Step 5: Run tests to verify they pass**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/FocusPanelNavigationTests 2>&1 | tail -20`
Expected: All 6 tests PASS

- [ ] **Step 6: Commit**

```bash
git add Sources/UI/Dashboard/FocusPanelView.swift Tests/FocusPanelNavigationTests.swift amux.xcodeproj
git commit -m "feat: add prev/next navigation to FocusPanelView"
```

---

### Task 3: Integrate StackedMiniCardContainerView into DashboardViewController

**Files:**
- Modify: `Sources/UI/Dashboard/DashboardViewController.swift`

- [ ] **Step 1: Change mini card array types**

Replace the three `MiniCardView` arrays with `StackedMiniCardContainerView`:

```swift
// Old:
private var leftRightMiniCards: [MiniCardView] = []
private var topSmallMiniCards: [MiniCardView] = []
private var topLargeMiniCards: [MiniCardView] = []

// New:
private var leftRightMiniCards: [StackedMiniCardContainerView] = []
private var topSmallMiniCards: [StackedMiniCardContainerView] = []
private var topLargeMiniCards: [StackedMiniCardContainerView] = []
```

- [ ] **Step 2: Update rebuildLeftRight()**

Replace the mini card creation loop in `rebuildLeftRight()`:

```swift
// Old (lines ~613-633):
for agent in sorted {
    let card = MiniCardView()
    card.delegate = self
    card.configure(...)
    card.isSelected = (agent.id == selectedAgentId)
    card.translatesAutoresizingMaskIntoConstraints = false
    leftRightMiniCards.append(card)
    leftRightSidebarStack.addArrangedSubview(card)
    NSLayoutConstraint.activate([
        card.widthAnchor.constraint(equalToConstant: sidebarWidth),
    ])
}

// New:
for agent in sorted {
    let container = StackedMiniCardContainerView()
    container.delegate = self
    container.configure(paneCount: agent.paneCount)
    container.miniCardView.configure(
        id: agent.id,
        project: agent.project,
        thread: agent.thread,
        status: agent.status,
        lastMessage: agent.lastMessage,
        totalDuration: agent.totalDuration,
        roundDuration: agent.roundDuration
    )
    container.isSelected = (agent.id == selectedAgentId)
    container.translatesAutoresizingMaskIntoConstraints = false
    leftRightMiniCards.append(container)
    leftRightSidebarStack.addArrangedSubview(container)
    NSLayoutConstraint.activate([
        container.widthAnchor.constraint(equalToConstant: sidebarWidth),
        container.heightAnchor.constraint(equalTo: container.widthAnchor, multiplier: 9.0 / 16.0),
    ])
}
```

- [ ] **Step 3: Update rebuildTopSmall()**

Same pattern — replace `MiniCardView()` with `StackedMiniCardContainerView()`:

```swift
// New loop body:
for agent in sorted {
    let container = StackedMiniCardContainerView()
    container.delegate = self
    container.configure(paneCount: agent.paneCount)
    container.miniCardView.configure(
        id: agent.id,
        project: agent.project,
        thread: agent.thread,
        status: agent.status,
        lastMessage: agent.lastMessage,
        totalDuration: agent.totalDuration,
        roundDuration: agent.roundDuration
    )
    container.isSelected = (agent.id == selectedAgentId)
    container.translatesAutoresizingMaskIntoConstraints = false
    topSmallMiniCards.append(container)
    topSmallTopStack.addArrangedSubview(container)

    let widthConstraint = container.widthAnchor.constraint(equalToConstant: 220)
    widthConstraint.priority = .defaultHigh
    let minWidth = container.widthAnchor.constraint(greaterThanOrEqualToConstant: 180)
    let maxWidth = container.widthAnchor.constraint(lessThanOrEqualToConstant: 260)
    // Container has no intrinsic size — add explicit height via aspect ratio
    let heightConstraint = container.heightAnchor.constraint(equalTo: container.widthAnchor, multiplier: 9.0 / 16.0)
    NSLayoutConstraint.activate([widthConstraint, minWidth, maxWidth, heightConstraint])
}
```

- [ ] **Step 4: Update rebuildTopLarge()**

Same pattern as Step 3 but for `topLargeMiniCards` / `topLargeBottomStack`. Include the same height constraint (`container.heightAnchor.constraint(equalTo: container.widthAnchor, multiplier: 9.0 / 16.0)`).

- [ ] **Step 5: Update updateFocusLayoutInPlace()**

Change the method to work with `StackedMiniCardContainerView`:

```swift
// Old:
private func updateFocusLayoutInPlace(_ sorted: [AgentDisplayInfo], miniCards: [MiniCardView], focusPanel: FocusPanelView) {
    guard sorted.count == miniCards.count else {
        rebuildCurrentLayout()
        return
    }
    if let selected = sorted.first(where: { $0.id == selectedAgentId }) ?? sorted.first {
        configureFocusPanel(focusPanel, with: selected)
    }
    for (index, agent) in sorted.enumerated() {
        miniCards[index].configure(
            id: agent.id,
            project: agent.project,
            thread: agent.thread,
            status: agent.status,
            lastMessage: agent.lastMessage,
            totalDuration: agent.totalDuration,
            roundDuration: agent.roundDuration
        )
        miniCards[index].isSelected = (agent.id == selectedAgentId)
    }
}

// New:
private func updateFocusLayoutInPlace(_ sorted: [AgentDisplayInfo], miniCards: [StackedMiniCardContainerView], focusPanel: FocusPanelView) {
    guard sorted.count == miniCards.count else {
        rebuildCurrentLayout()
        return
    }
    if let selected = sorted.first(where: { $0.id == selectedAgentId }) ?? sorted.first {
        configureFocusPanel(focusPanel, with: selected)
    }
    for (index, agent) in sorted.enumerated() {
        miniCards[index].configure(paneCount: agent.paneCount)
        miniCards[index].layoutChildren()
        miniCards[index].miniCardView.configure(
            id: agent.id,
            project: agent.project,
            thread: agent.thread,
            status: agent.status,
            lastMessage: agent.lastMessage,
            totalDuration: agent.totalDuration,
            roundDuration: agent.roundDuration
        )
        miniCards[index].isSelected = (agent.id == selectedAgentId)
    }
}
```

- [ ] **Step 6: Build to verify compilation**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -10`
Expected: BUILD SUCCEEDED

- [ ] **Step 7: Commit**

```bash
git add Sources/UI/Dashboard/DashboardViewController.swift
git commit -m "feat: integrate StackedMiniCardContainerView in focus layouts"
```

---

### Task 4: Navigation Delegate + Slide Animation in DashboardViewController

**Files:**
- Modify: `Sources/UI/Dashboard/DashboardViewController.swift`

- [ ] **Step 1: Conform to navigation delegate**

Add `focusPanelDidRequestNavigate` to the `FocusPanelDelegate` conformance section:

```swift
func focusPanelDidRequestNavigate(_ panel: FocusPanelView, direction: NavigationDirection) {
    let sorted = sortedAgents()
    guard sorted.count > 1 else { return }

    guard let currentIndex = sorted.firstIndex(where: { $0.id == selectedAgentId }) else { return }

    let newIndex: Int
    switch direction {
    case .next:
        newIndex = min(currentIndex + 1, sorted.count - 1)
    case .previous:
        newIndex = max(currentIndex - 1, 0)
    }
    guard newIndex != currentIndex else { return }

    let newAgent = sorted[newIndex]

    // Determine which focus panel is active
    let focusPanel: FocusPanelView
    switch currentLayout {
    case .leftRight: focusPanel = leftRightFocusPanel
    case .topSmall: focusPanel = topSmallFocusPanel
    case .topLarge: focusPanel = topLargeFocusPanel
    case .grid: return
    }

    // Add slide transition
    let transition = CATransition()
    transition.type = .push
    transition.duration = 0.25
    transition.timingFunction = CAMediaTimingFunction(name: .easeInOut)
    transition.subtype = slideSubtype(for: currentLayout, direction: direction)
    focusPanel.terminalContainer.layer?.add(transition, forKey: "slideTransition")

    // Swap terminal
    detachTerminals()
    selectedAgentId = newAgent.id
    configureFocusPanel(focusPanel, with: newAgent)
    focusPanel.configureNavigation(currentIndex: newIndex, total: sorted.count)
    embedSurface(newAgent, in: focusPanel.terminalContainer)

    // Update mini card selection
    updateMiniCardSelection()
}

private func slideSubtype(for layout: DashboardLayout, direction: NavigationDirection) -> CATransitionSubtype {
    switch layout {
    case .leftRight:
        return direction == .next ? .fromRight : .fromLeft
    case .topSmall:
        return direction == .next ? .fromBottom : .fromTop
    case .topLarge:
        return direction == .next ? .fromTop : .fromBottom
    case .grid:
        return .fromRight
    }
}

private func updateMiniCardSelection() {
    let updateCards: ([StackedMiniCardContainerView]) -> Void = { cards in
        for card in cards {
            card.isSelected = (card.agentId == self.selectedAgentId)
        }
    }
    switch currentLayout {
    case .leftRight: updateCards(leftRightMiniCards)
    case .topSmall: updateCards(topSmallMiniCards)
    case .topLarge: updateCards(topLargeMiniCards)
    case .grid: break
    }
}
```

- [ ] **Step 2: Call configureNavigation in configureFocusPanel**

Update `configureFocusPanel(_:with:)` to also configure navigation:

```swift
private func configureFocusPanel(_ panel: FocusPanelView, with agent: AgentDisplayInfo) {
    panel.configure(
        name: agent.name,
        project: agent.project,
        thread: agent.thread,
        status: agent.status,
        total: agent.totalDuration,
        round: agent.roundDuration
    )
    // Configure navigation
    let sorted = sortedAgents()
    if let index = sorted.firstIndex(where: { $0.id == agent.id }) {
        panel.configureNavigation(currentIndex: index, total: sorted.count)
    }
}
```

- [ ] **Step 3: Build and verify**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -10`
Expected: BUILD SUCCEEDED

- [ ] **Step 4: Run all tests**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test 2>&1 | tail -20`
Expected: All tests PASS

- [ ] **Step 5: Commit**

```bash
git add Sources/UI/Dashboard/DashboardViewController.swift
git commit -m "feat: add focus panel navigation with slide animation"
```



> **Note:** `xcodegen generate` is run within Tasks 1 and 2 after creating new files. The project uses glob-based source discovery (`path: Sources` / `path: Tests`), so no changes to `project.yml` are needed.
