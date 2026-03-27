# Dashboard Refinement Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Ensure all dashboard layouts begin below the titlebar with a consistent gap, normalize card typography to macOS-style readability (SF 12/13pt baseline), and simplify card UI to reduce non-native complexity.

**Architecture:** Centralize dashboard top spacing in `DashboardViewController` so all four layouts share one top baseline. Introduce explicit typography tokens per card component (`AgentCardView`, `MiniCardView`, `FocusPanelView`) to remove undersized text and enforce consistent hierarchy. Simplify focus-panel controls and replace hardcoded grayscale text colors with semantic tokens to reduce visual noise and improve native consistency.

**Tech Stack:** Swift 5.10, AppKit, XCTest, XCUITest, existing amux semantic color system.

---

### Task 1: Add regression coverage for top overlap and typography baseline hooks

**Files:**
- Modify: `UITests/Tests/RegressionTests.swift`
- Modify: `UITests/Pages/DashboardPage.swift`
- Modify: `Tests/GridLayoutTests.swift`
- Modify: `Sources/UI/Dashboard/AgentCardView.swift` (test hook only)
- Modify: `Sources/UI/Dashboard/MiniCardView.swift` (test hook only)
- Modify: `Sources/UI/Dashboard/FocusPanelView.swift` (test hook only)

**Step 1: Write failing UI regression for layout top gap**

Add a regression test that switches among the four layouts and verifies the active layout container starts below titlebar with a visible gap.

```swift
func testDashboardLayoutsStartBelowTitlebarWithGap() {
    guard page.titleBar.titleBar.waitForExistence(timeout: 10) else { return }
    let titleMaxY = page.titleBar.titleBar.frame.maxY

    XCTAssertGreaterThan(page.dashboard.gridLayout.frame.minY, titleMaxY)

    page.titleBar.clickViewMenu()
    page.layoutPopover.selectLeftRight()
    XCTAssertGreaterThan(page.dashboard.leftRightLayout.frame.minY, titleMaxY)

    page.titleBar.clickViewMenu()
    page.layoutPopover.selectTopSmall()
    XCTAssertGreaterThan(page.dashboard.topSmallLayout.frame.minY, titleMaxY)

    page.titleBar.clickViewMenu()
    page.layoutPopover.selectTopLarge()
    XCTAssertGreaterThan(page.dashboard.topLargeLayout.frame.minY, titleMaxY)
}
```

**Step 2: Write failing unit tests for typography baselines**

Add unit tests asserting the components expose readable font-size baselines:

```swift
func testDashboardTypographyBaselines_AreMacReadable() {
    XCTAssertEqual(AgentCardView.Typography.primaryPointSize, 13)
    XCTAssertEqual(AgentCardView.Typography.bodyPointSize, 12)
    XCTAssertEqual(AgentCardView.Typography.secondaryPointSize, 11)

    XCTAssertEqual(MiniCardView.Typography.primaryPointSize, 13)
    XCTAssertEqual(MiniCardView.Typography.bodyPointSize, 12)
    XCTAssertEqual(MiniCardView.Typography.secondaryPointSize, 11)

    XCTAssertEqual(FocusPanelView.Typography.primaryPointSize, 13)
    XCTAssertEqual(FocusPanelView.Typography.bodyPointSize, 12)
    XCTAssertEqual(FocusPanelView.Typography.secondaryPointSize, 11)
}
```

**Step 3: Run tests to verify failures**

Run:
- `xcodebuild -project amux.xcodeproj -scheme amuxUITests -configuration Debug test -only-testing:amuxUITests/RegressionTests/testDashboardLayoutsStartBelowTitlebarWithGap`
- `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/GridLayoutTests/testDashboardTypographyBaselines_AreMacReadable`

Expected: FAIL because gap/typography contracts are not yet implemented.

**Step 4: Commit test-only baseline**

```bash
git add UITests/Tests/RegressionTests.swift UITests/Pages/DashboardPage.swift Tests/GridLayoutTests.swift Sources/UI/Dashboard/AgentCardView.swift Sources/UI/Dashboard/MiniCardView.swift Sources/UI/Dashboard/FocusPanelView.swift
git commit -m "test: add dashboard top-gap and typography baseline regressions"
```

---

### Task 2: Implement shared dashboard top inset for all four layouts

**Files:**
- Modify: `Sources/UI/Dashboard/DashboardViewController.swift`
- Test: `UITests/Tests/RegressionTests.swift`

**Step 1: Add centralized top inset token**

Add a shared constant in `DashboardViewController`:

```swift
private let layoutTopInset: CGFloat = 8
```

**Step 2: Apply token uniformly to layout containers**

Update constraints so all layout roots (`gridScrollView`, `leftRightContainer`, `topSmallContainer`, `topLargeContainer`) derive top spacing from the same token and do not apply double top offsets.

**Step 3: Keep existing inter-card spacing intact**

Ensure only titlebar-to-layout gap changes; internal layout spacing between focus/sidebar/cards remains as before.

**Step 4: Run focused UI test**

Run:
- `xcodebuild -project amux.xcodeproj -scheme amuxUITests -configuration Debug test -only-testing:amuxUITests/RegressionTests/testDashboardLayoutsStartBelowTitlebarWithGap`

Expected: PASS.

**Step 5: Commit top inset change**

```bash
git add Sources/UI/Dashboard/DashboardViewController.swift UITests/Tests/RegressionTests.swift
git commit -m "fix: align all dashboard layouts below titlebar with consistent gap"
```

---

### Task 3: Normalize card typography to SF 12/13/11 baseline

**Files:**
- Modify: `Sources/UI/Dashboard/AgentCardView.swift`
- Modify: `Sources/UI/Dashboard/MiniCardView.swift`
- Modify: `Sources/UI/Dashboard/FocusPanelView.swift`
- Modify: `Sources/UI/Shared/SemanticColors.swift` (only if additional semantic text tokens needed)
- Test: `Tests/GridLayoutTests.swift`

**Step 1: Add per-component typography tokens**

Define nested `Typography` constants in each component:

```swift
enum Typography {
    static let primaryPointSize: CGFloat = 13
    static let bodyPointSize: CGFloat = 12
    static let secondaryPointSize: CGFloat = 11
}
```

**Step 2: Replace undersized fonts**

- Remove 8/9pt text usage in `AgentCardView` and `MiniCardView`.
- Update all labels to use the tokenized 13/12/11 hierarchy with SF system fonts.

**Step 3: Preserve truncation behavior**

Rebalance content compression/hugging priorities only where required so larger text does not break card layout.

**Step 4: Run typography unit test**

Run:
- `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/GridLayoutTests/testDashboardTypographyBaselines_AreMacReadable`

Expected: PASS.

**Step 5: Commit typography normalization**

```bash
git add Sources/UI/Dashboard/AgentCardView.swift Sources/UI/Dashboard/MiniCardView.swift Sources/UI/Dashboard/FocusPanelView.swift Sources/UI/Shared/SemanticColors.swift Tests/GridLayoutTests.swift
git commit -m "refactor: standardize dashboard card typography to macOS-readable baseline"
```

---

### Task 4: Strong simplify card UI to reduce non-native complexity

**Files:**
- Modify: `Sources/UI/Dashboard/FocusPanelView.swift`
- Modify: `Sources/UI/Dashboard/MiniCardView.swift`
- Modify: `Sources/UI/Dashboard/AgentCardView.swift`
- Modify: `Sources/UI/Shared/SemanticColors.swift`
- Test: `UITests/Tests/CoreTests.swift`

**Step 1: Remove duplicated focus-header action affordance**

Keep one project-entry control in `FocusPanelView` and remove redundant alternate control path.

**Step 2: Replace hardcoded grayscale text colors**

Move direct `NSColor(hex: ...)` text grays in dashboard cards to semantic tokens for consistency across appearances.

**Step 3: Reduce decorative hover complexity**

Simplify excessive hover-specific overrides that conflict with macOS restraint while preserving clear selected/hover states.

**Step 4: Add/adjust core UI test coverage**

Ensure core test validates focus panel still has a working project-entry action and no missing interactions.

**Step 5: Run targeted regressions**

Run:
- `xcodebuild -project amux.xcodeproj -scheme amuxUITests -configuration Debug test -only-testing:amuxUITests/CoreTests`
- `xcodebuild -project amux.xcodeproj -scheme amuxUITests -configuration Debug test -only-testing:amuxUITests/RegressionTests/testSwitchToTopSmallLayout -only-testing:amuxUITests/RegressionTests/testSwitchToTopLargeLayout -only-testing:amuxUITests/RegressionTests/testProjectTabRemainsAfterRepeatedDashboardSwitches`

Expected: PASS.

**Step 6: Commit simplification slice**

```bash
git add Sources/UI/Dashboard/FocusPanelView.swift Sources/UI/Dashboard/MiniCardView.swift Sources/UI/Dashboard/AgentCardView.swift Sources/UI/Shared/SemanticColors.swift UITests/Tests/CoreTests.swift UITests/Tests/RegressionTests.swift
git commit -m "refactor: simplify dashboard cards for native macOS feel"
```

---

### Task 5: Full verification pass

**Files:**
- No code changes expected

**Step 1: Run impacted unit tests**

Run:
- `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/GridLayoutTests`

Expected: PASS.

**Step 2: Run targeted UI regression suite**

Run:
- `xcodebuild -project amux.xcodeproj -scheme amuxUITests -configuration Debug test -only-testing:amuxUITests/CoreTests/testViewMenuOpensAndHasAllOptions -only-testing:amuxUITests/CoreTests/testSwitchToGridLayout -only-testing:amuxUITests/CoreTests/testSwitchToLeftRightLayout`
- `xcodebuild -project amux.xcodeproj -scheme amuxUITests -configuration Debug test -only-testing:amuxUITests/RegressionTests`

Expected: PASS.

**Step 3: Manual QA checklist**

1. In each layout mode, card region starts below titlebar with visible gap.
2. No card text appears tiny; smallest text remains readable at standard zoom.
3. Focus header uses one clear enter-project affordance.
4. Overall card visuals feel simpler and less custom-heavy in dark and light mode.

**Step 4: Final commit if needed**

```bash
git add -A
git commit -m "test: finalize dashboard spacing and macOS-style card refinements"
```
