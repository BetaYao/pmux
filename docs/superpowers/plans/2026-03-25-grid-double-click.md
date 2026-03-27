# Grid Double-Click Navigation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Change grid click behavior so single-click selects the card in-place and double-click navigates to the worktree's project detail tab with terminal focus.

**Architecture:** Three targeted changes across three files: expose `AgentCardView`'s click recognizer so the container can suppress it during double-clicks; add a double-click recognizer to `StackedCardContainerView`; update `DashboardViewController` grid click logic and add `agentCardDoubleClicked` implementation.

**Tech Stack:** Swift 5.10, AppKit, `NSClickGestureRecognizer`, XCTest

---

## File Map

| File | Change |
|---|---|
| `Sources/UI/Dashboard/AgentCardView.swift` | Add `agentCardDoubleClicked` to `AgentCardDelegate`; expose `clickRecognizer` as `private(set) var` |
| `Sources/UI/Dashboard/StackedCardContainerView.swift` | Replace single click recognizer with single + double pair; wire `require(toFail:)` on both |
| `Sources/UI/Dashboard/DashboardViewController.swift` | Update `.grid` case in `agentCardClicked`; add `agentCardDoubleClicked` |
| `tests/StackedCardContainerDoubleClickTests.swift` | New — unit tests for single/double click dispatch behavior |
| `tests/DashboardViewControllerClickTests.swift` | New — unit tests for grid click behavior in `DashboardViewController` |

---

### Task 1: Extend `AgentCardDelegate` and expose `clickRecognizer`

**Files:**
- Modify: `Sources/UI/Dashboard/AgentCardView.swift:3-5` (protocol) and `Sources/UI/Dashboard/AgentCardView.swift:155-157` (setup)

**Context:** `AgentCardDelegate` currently has only `agentCardClicked(agentId:)`. We need to add `agentCardDoubleClicked(agentId:)` with a default no-op so existing conformers (`DashboardViewController`) don't break. `AgentCardView` creates an `NSClickGestureRecognizer` locally in `setup()` — we need to store it as `private(set) var clickRecognizer` so `StackedCardContainerView` can call `clickRecognizer.require(toFail:)` on it.

- [ ] **Step 1: Update the `AgentCardDelegate` protocol and add default extension**

Replace the protocol block at the top of `Sources/UI/Dashboard/AgentCardView.swift`:

```swift
protocol AgentCardDelegate: AnyObject {
    func agentCardClicked(agentId: String)
    func agentCardDoubleClicked(agentId: String)
}

extension AgentCardDelegate {
    func agentCardDoubleClicked(agentId: String) {}
}
```

- [ ] **Step 2: Store the click recognizer as `private(set) var`**

In `AgentCardView`, add a stored property below `private var currentStatus`:

```swift
private(set) var clickRecognizer: NSClickGestureRecognizer!
```

Then in `setup()`, replace:

```swift
// Click handler
let click = NSClickGestureRecognizer(target: self, action: #selector(handleClick))
addGestureRecognizer(click)
```

with:

```swift
// Click handler — stored so container can wire require(toFail:)
clickRecognizer = NSClickGestureRecognizer(target: self, action: #selector(handleClick))
addGestureRecognizer(clickRecognizer)
```

- [ ] **Step 3: Build to verify no compile errors**

```bash
xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -20
```

Expected: `BUILD SUCCEEDED`

- [ ] **Step 4: Commit**

```bash
git add Sources/UI/Dashboard/AgentCardView.swift
git commit -m "feat: extend AgentCardDelegate with double-click and expose clickRecognizer"
```

---

### Task 2: Add double-click recognizer to `StackedCardContainerView`

**Files:**
- Modify: `Sources/UI/Dashboard/StackedCardContainerView.swift:27-36` (setup), `Sources/UI/Dashboard/StackedCardContainerView.swift:97-99` (handleClick)

**Context:** `StackedCardContainerView.setup()` currently adds a single unrestricted `NSClickGestureRecognizer`. We need to:
1. Replace it with a single-click recognizer (1 click required) and a double-click recognizer (2 clicks required)
2. Make single-click `require(toFail:)` the double-click so it waits before firing
3. Also make `cardView.clickRecognizer.require(toFail:)` the double-click to prevent `AgentCardView`'s unrestricted recognizer from consuming the first tap and blocking the double-click

- [ ] **Step 1: Write unit tests first**

Create `tests/StackedCardContainerDoubleClickTests.swift`:

```swift
import XCTest
@testable import amux

final class StackedCardContainerDoubleClickTests: XCTestCase {

    // MARK: - Gesture recognizer configuration

    func testSingleClickRecognizerRequiresDoubleClickToFail() {
        let container = StackedCardContainerView(frame: NSRect(x: 0, y: 0, width: 200, height: 150))
        let recognizers = container.gestureRecognizers.compactMap { $0 as? NSClickGestureRecognizer }
        let single = recognizers.first(where: { $0.numberOfClicksRequired == 1 })
        let double_ = recognizers.first(where: { $0.numberOfClicksRequired == 2 })
        XCTAssertNotNil(single, "Container must have a single-click recognizer")
        XCTAssertNotNil(double_, "Container must have a double-click recognizer")
    }

    func testCardViewClickRecognizerIsExposed() {
        let container = StackedCardContainerView(frame: NSRect(x: 0, y: 0, width: 200, height: 150))
        XCTAssertNotNil(container.cardView.clickRecognizer,
                        "AgentCardView must expose clickRecognizer as private(set)")
    }

    func testContainerHasTwoClickRecognizers() {
        let container = StackedCardContainerView(frame: NSRect(x: 0, y: 0, width: 200, height: 150))
        let clickRecognizers = container.gestureRecognizers.compactMap { $0 as? NSClickGestureRecognizer }
        XCTAssertEqual(clickRecognizers.count, 2,
                       "Container must have exactly two NSClickGestureRecognizers (single + double)")
    }

    // MARK: - Delegate wiring

    func testSingleClickFiresAgentCardClicked() {
        let container = StackedCardContainerView(frame: NSRect(x: 0, y: 0, width: 200, height: 150))
        let spy = DelegateSpy()
        container.delegate = spy

        // Simulate the container's single-click handler directly
        container.simulateSingleClick()

        XCTAssertEqual(spy.clickedIds.count, 1)
        XCTAssertTrue(spy.doubleClickedIds.isEmpty)
    }

    func testDoubleClickFiresAgentCardDoubleClicked() {
        let container = StackedCardContainerView(frame: NSRect(x: 0, y: 0, width: 200, height: 150))
        let spy = DelegateSpy()
        container.delegate = spy

        // Simulate the container's double-click handler directly
        container.simulateDoubleClick()

        XCTAssertEqual(spy.doubleClickedIds.count, 1)
        XCTAssertTrue(spy.clickedIds.isEmpty)
    }
}

// MARK: - Test helpers

private class DelegateSpy: AgentCardDelegate {
    var clickedIds: [String] = []
    var doubleClickedIds: [String] = []

    func agentCardClicked(agentId: String) { clickedIds.append(agentId) }
    func agentCardDoubleClicked(agentId: String) { doubleClickedIds.append(agentId) }
}
```

Note: `simulateSingleClick()` and `simulateDoubleClick()` are `@testable` internal helpers we will add to `StackedCardContainerView` in Step 3.

- [ ] **Step 2: Run tests to confirm they fail**

```bash
xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/StackedCardContainerDoubleClickTests 2>&1 | tail -30
```

Expected: compile error or test failures (methods don't exist yet)

- [ ] **Step 3: Update `StackedCardContainerView.setup()` and add helpers**

Replace the `setup()` method and `handleClick` in `Sources/UI/Dashboard/StackedCardContainerView.swift`:

```swift
private func setup() {
    wantsLayer = true
    layer?.masksToBounds = false

    // cardView on top; ghost views are inserted below it
    addSubview(cardView)

    // Double-click fires navigation
    let doubleClick = NSClickGestureRecognizer(target: self, action: #selector(handleDoubleClick))
    doubleClick.numberOfClicksRequired = 2

    // Single-click fires selection; must fail before double-click fires
    let singleClick = NSClickGestureRecognizer(target: self, action: #selector(handleClick))
    singleClick.numberOfClicksRequired = 1
    singleClick.require(toFail: doubleClick)

    // cardView's own recognizer is functionally a no-op (delegate is nil), but without
    // this dependency it could still consume the first tap and prevent the container's
    // double-click from seeing the second tap.
    cardView.clickRecognizer.require(toFail: doubleClick)

    addGestureRecognizer(doubleClick)
    addGestureRecognizer(singleClick)
}

// MARK: - Click

@objc private func handleClick() {
    delegate?.agentCardClicked(agentId: cardView.agentId)
}

@objc private func handleDoubleClick() {
    delegate?.agentCardDoubleClicked(agentId: cardView.agentId)
}

// MARK: - Test helpers (internal for @testable access)

func simulateSingleClick() { handleClick() }
func simulateDoubleClick() { handleDoubleClick() }
```

- [ ] **Step 4: Run tests to confirm they pass**

```bash
xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/StackedCardContainerDoubleClickTests 2>&1 | tail -30
```

Expected: `TEST SUCCEEDED` — all 5 tests green

- [ ] **Step 5: Build the app to confirm no compile errors**

```bash
xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -20
```

Expected: `BUILD SUCCEEDED`

- [ ] **Step 6: Commit**

```bash
git add Sources/UI/Dashboard/StackedCardContainerView.swift tests/StackedCardContainerDoubleClickTests.swift
git commit -m "feat: add double-click recognizer to StackedCardContainerView"
```

---

### Task 3: Update `DashboardViewController` click handling

**Files:**
- Modify: `Sources/UI/Dashboard/DashboardViewController.swift:759-772` (`agentCardClicked`) and after line 772 (add `agentCardDoubleClicked`)
- Create: `tests/DashboardViewControllerClickTests.swift`

**Context:** `agentCardClicked` currently switches to `.leftRight` layout when in `.grid`. That behavior must be removed — single-click in grid now only updates selection. A new `agentCardDoubleClicked` method navigates via the existing `dashboardDelegate?.dashboardDidSelectProject(project:thread:)`. `currentLayout` and `selectedAgentId` are non-private `var` properties on `DashboardViewController`, so they are testable via `@testable import`.

- [ ] **Step 1: Write failing tests**

Create `tests/DashboardViewControllerClickTests.swift`:

```swift
import XCTest
@testable import amux

final class DashboardViewControllerClickTests: XCTestCase {

    // MARK: - Grid single-click

    func testGridSingleClickUpdatesSelectedAgentId() {
        let vc = DashboardViewController()
        vc.currentLayout = .grid
        vc.agentCardClicked(agentId: "agent-1")
        XCTAssertEqual(vc.selectedAgentId, "agent-1")
    }

    func testGridSingleClickDoesNotChangeLayout() {
        let vc = DashboardViewController()
        vc.currentLayout = .grid
        vc.agentCardClicked(agentId: "agent-1")
        XCTAssertEqual(vc.currentLayout, .grid,
                       "Single click in grid must not switch to another layout")
    }

    func testGridSingleClickDoesNotCallDelegate() {
        let vc = DashboardViewController()
        let spy = DashboardDelegateSpy()
        vc.dashboardDelegate = spy
        vc.currentLayout = .grid
        vc.agentCardClicked(agentId: "agent-1")
        XCTAssertFalse(spy.didSelectProjectCalled,
                       "Single click in grid must not call dashboardDidSelectProject")
    }

    // MARK: - Double-click on unknown agentId (guard path)

    func testDoubleClickWithUnknownAgentIdIsNoop() {
        let vc = DashboardViewController()
        let spy = DashboardDelegateSpy()
        vc.dashboardDelegate = spy
        vc.agentCardDoubleClicked(agentId: "nonexistent")
        XCTAssertFalse(spy.didSelectProjectCalled,
                       "Double click on unknown agentId must not call delegate")
    }
}

// MARK: - Test helpers

private class DashboardDelegateSpy: DashboardDelegate {
    var didSelectProjectCalled = false
    var lastProject: String?
    var lastThread: String?

    func dashboardDidSelectProject(_ project: String, thread: String) {
        didSelectProjectCalled = true
        lastProject = project
        lastThread = thread
    }
    func dashboardDidRequestEnterProject(_ project: String) {}
    func dashboardDidReorderCards(order: [String]) {}
    func dashboardDidRequestDelete(_ terminalID: String) {}
    func dashboardDidRequestAddProject() {}
}
```

- [ ] **Step 2: Run tests to confirm they fail**

```bash
xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/DashboardViewControllerClickTests 2>&1 | tail -30
```

Expected: compile error or failures (methods not yet updated / `agentCardDoubleClicked` missing)

- [ ] **Step 3: Update `agentCardClicked` — remove grid layout switch**

Replace the `agentCardClicked` method (lines 759–772) in `Sources/UI/Dashboard/DashboardViewController.swift`:

```swift
func agentCardClicked(agentId: String) {
    switch currentLayout {
    case .grid:
        // Single click → select in place (no layout switch)
        selectedAgentId = agentId
        for container in gridCards {
            container.isSelected = (container.agentId == agentId)
        }
    default:
        // In other layouts, change selection and refresh focus panel
        detachTerminals()
        selectedAgentId = agentId
        rebuildCurrentLayout()
    }
}
```

- [ ] **Step 4: Add `agentCardDoubleClicked` — navigate to project tab**

Insert immediately after `agentCardClicked`:

```swift
func agentCardDoubleClicked(agentId: String) {
    guard let agent = agents.first(where: { $0.id == agentId }) else { return }
    dashboardDelegate?.dashboardDidSelectProject(agent.project, thread: agent.thread)
}
```

- [ ] **Step 5: Run new tests to confirm they pass**

```bash
xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/DashboardViewControllerClickTests 2>&1 | tail -30
```

Expected: `TEST SUCCEEDED` — all 4 tests green

- [ ] **Step 6: Run the full test suite**

```bash
xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test 2>&1 | tail -30
```

Expected: `TEST SUCCEEDED`

- [ ] **Step 7: Commit**

```bash
git add Sources/UI/Dashboard/DashboardViewController.swift tests/DashboardViewControllerClickTests.swift
git commit -m "feat: single-click selects grid card; double-click navigates to project tab"
```

---

## Manual Verification Checklist

After all tasks pass:

1. **Single-click in grid** — card gets accent border highlight; layout stays grid; no tab switch
2. **Double-click in grid** — correct repo tab opens; correct worktree is selected in sidebar; active pane terminal has keyboard focus
3. **Single-click in focus layouts** (leftRight/topSmall/topLarge) — behaviour unchanged (selection + focus panel update)
4. **Multi-pane worktree** — double-clicking a stacked card focuses the tree's `focusedId` pane (or first pane if none)
5. **No double-firing** — single-click does NOT also trigger `agentCardDoubleClicked`
