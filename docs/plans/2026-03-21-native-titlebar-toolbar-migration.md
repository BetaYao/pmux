# Native Title Bar Migration Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace custom title bar traffic lights with native macOS window controls using `NSToolbar` + title bar accessory while preserving current pmux top-bar style and fixing first-click project-tab switching reliability.

**Architecture:** Keep `TitleBarView` as the visual control surface for dashboard/project tabs and right-side actions, but remove custom traffic-light handling and host the bar in native titlebar infrastructure. Move to `NSToolbar`/titlebar accessory composition, use system window buttons for close/minimize/zoom, and simplify project-tab click handling through standard controls to avoid first-click event loss.

**Tech Stack:** Swift 5.10, AppKit (`NSWindow`, `NSToolbar`, `NSTitlebarAccessoryViewController`, `NSButton`, `NSStackView`), XCTest, XCUITest.

---

### Task 1: Add failing UI regression for first project-tab click from cold launch

**Files:**
- Modify: `UITests/Tests/RegressionTests.swift`
- Modify: `UITests/Pages/TitleBarPage.swift` (only if helper API needed)

**Step 1: Write the failing test**

Add a regression test that:
1. Launches app fresh.
2. Finds first project tab in title bar.
3. Clicks it without clicking dashboard first.
4. Asserts repo sidebar/worktree list appears on the first click.

```swift
func testFirstProjectTabClickWorksFromColdLaunch() {
    guard page.dashboard.dashboardView.waitForExistence(timeout: 10) else { return }

    let projectTabs = page.app.groups.matching(NSPredicate(format: "identifier BEGINSWITH 'titlebar.projectTab.'"))
    XCTAssertGreaterThan(projectTabs.count, 0)

    let firstTab = projectTabs.element(boundBy: 0)
    firstTab.waitAndClick()

    XCTAssertTrue(page.sidebar.worktreeList.waitForExistence(timeout: 10),
                  "First click on project tab should open project view from cold launch")
}
```

**Step 2: Run test to verify it fails**

Run:
`xcodebuild -project pmux.xcodeproj -scheme pmuxUITests -configuration Debug test -only-testing:pmuxUITests/RegressionTests/testFirstProjectTabClickWorksFromColdLaunch`

Expected: FAIL (intermittent or deterministic) showing project view not opened on first click.

**Step 3: Commit test-only change**

```bash
git add UITests/Tests/RegressionTests.swift UITests/Pages/TitleBarPage.swift
git commit -m "test: add regression for first project tab click from launch"
```

---

### Task 2: Replace custom traffic lights with native system window buttons

**Files:**
- Modify: `Sources/App/MainWindowController.swift`
- Modify: `Sources/UI/TitleBar/TitleBarView.swift`
- Test: `Tests/GridLayoutTests.swift` (title bar behavior coverage)

**Step 1: Write the failing unit test**

Add title-bar test that verifies custom traffic dots are no longer required and native button hosting path is active (for example via public/internal test hook in `TitleBarView` or controller-level behavior).

```swift
func testTitleBarUsesSystemWindowButtonsConfiguration() {
    let windowController = MainWindowController()
    let window = windowController.window

    XCTAssertNotNil(window?.standardWindowButton(.closeButton))
    XCTAssertFalse(window?.standardWindowButton(.closeButton)?.isHidden ?? true)
}
```

**Step 2: Run test to verify it fails**

Run:
`xcodebuild -project pmux.xcodeproj -scheme pmuxTests -configuration Debug test -only-testing:pmuxTests/GridLayoutTests/testTitleBarUsesSystemWindowButtonsConfiguration`

Expected: FAIL because current code hides standard buttons and uses custom dots.

**Step 3: Write minimal implementation**

1. In `MainWindowController`:
   - Stop hiding standard window buttons.
   - Add helper to place system buttons with requested offset (`x=12, y=10`) in titlebar coordinate space.
   - Ensure placement updates on window resize and titlebar layout changes.
2. In `TitleBarView`:
   - Remove `TrafficDot` controls and related delegate action wiring for close/minimize/zoom.
   - Keep left controls beginning with dashboard tab/project tabs, with left inset adjusted to avoid overlap with system traffic lights.
3. Keep right control behavior unchanged.

**Step 4: Run tests to verify pass**

Run:
- `xcodebuild -project pmux.xcodeproj -scheme pmuxTests -configuration Debug test -only-testing:pmuxTests/GridLayoutTests`

Expected: PASS with no regressions in existing title-bar tests.

**Step 5: Commit migration slice**

```bash
git add Sources/App/MainWindowController.swift Sources/UI/TitleBar/TitleBarView.swift Tests/GridLayoutTests.swift
git commit -m "refactor: use native macOS traffic lights in title bar"
```

---

### Task 3: Migrate top bar hosting to native titlebar ecosystem (`NSToolbar` + accessory)

**Files:**
- Modify: `Sources/App/MainWindowController.swift`
- Modify: `Sources/UI/TitleBar/TitleBarView.swift`
- Modify: `UITests/Pages/TitleBarPage.swift` (if identifiers or hierarchy shift)

**Step 1: Write failing test for layout/accessibility continuity**

Add/adjust UI test asserting title bar controls are still present and operable after native hosting migration:

```swift
func testTitleBarControlsRemainAccessibleAfterNativeHosting() {
    XCTAssertTrue(page.titleBar.dashboardTab.waitForExistence(timeout: 10))
    XCTAssertTrue(page.titleBar.viewMenuButton.waitForExistence(timeout: 10))
    XCTAssertTrue(page.titleBar.themeToggle.waitForExistence(timeout: 10))
}
```

**Step 2: Run test to verify it fails**

Run:
`xcodebuild -project pmux.xcodeproj -scheme pmuxUITests -configuration Debug test -only-testing:pmuxUITests/RegressionTests/testTitleBarControlsRemainAccessibleAfterNativeHosting`

Expected: FAIL until identifiers and hosting are correctly wired.

**Step 3: Write minimal implementation**

1. Introduce native titlebar hosting in `MainWindowController`:
   - Configure `NSToolbar` (minimal chrome, keeps native titlebar behavior).
   - Attach `TitleBarView` via titlebar accessory or toolbar item host view.
2. Preserve existing top-bar appearance tokens and spacing in `TitleBarView`.
3. Ensure identifiers (`titlebar.*`) stay stable for tests.

**Step 4: Run focused UI tests**

Run:
- `xcodebuild -project pmux.xcodeproj -scheme pmuxUITests -configuration Debug test -only-testing:pmuxUITests/RegressionTests/testSwitchToTopSmallLayout`
- `xcodebuild -project pmux.xcodeproj -scheme pmuxUITests -configuration Debug test -only-testing:pmuxUITests/RegressionTests/testSwitchToTopLargeLayout`
- `xcodebuild -project pmux.xcodeproj -scheme pmuxUITests -configuration Debug test -only-testing:pmuxUITests/RegressionTests/testTitleBarControlsRemainAccessibleAfterNativeHosting`

Expected: PASS.

**Step 5: Commit hosting migration**

```bash
git add Sources/App/MainWindowController.swift Sources/UI/TitleBar/TitleBarView.swift UITests/Pages/TitleBarPage.swift UITests/Tests/RegressionTests.swift
git commit -m "refactor: host pmux top bar with native macOS titlebar components"
```

---

### Task 4: Fix root-cause project-tab click reliability using standard control event path

**Files:**
- Modify: `Sources/UI/TitleBar/TitleBarView.swift`
- Test: `Tests/GridLayoutTests.swift` (unit-level interaction behavior if feasible)
- Test: `UITests/Tests/RegressionTests.swift`

**Step 1: Confirm failing regression from Task 1 remains representative**

Run:
`xcodebuild -project pmux.xcodeproj -scheme pmuxUITests -configuration Debug test -only-testing:pmuxUITests/RegressionTests/testFirstProjectTabClickWorksFromColdLaunch -test-iterations 5`

Expected: at least one failing iteration before fix in affected environments.

**Step 2: Implement minimal event-path fix**

1. Replace custom `ProjectTabView` mouse interception (`hitTest` + `mouseDown`) with standard clickable control behavior (`NSButton` or `NSControl` target/action) for primary tab click.
2. Keep close button interaction independent and non-blocking.
3. Preserve all existing visual states (selected/hover/idle).

**Step 3: Run regression tests**

Run:
- `xcodebuild -project pmux.xcodeproj -scheme pmuxUITests -configuration Debug test -only-testing:pmuxUITests/RegressionTests/testFirstProjectTabClickWorksFromColdLaunch -test-iterations 8`
- `xcodebuild -project pmux.xcodeproj -scheme pmuxUITests -configuration Debug test -only-testing:pmuxUITests/RegressionTests/testProjectTabRemainsAfterRepeatedDashboardSwitches -test-iterations 8`

Expected: PASS all iterations.

**Step 4: Commit click-reliability fix**

```bash
git add Sources/UI/TitleBar/TitleBarView.swift UITests/Tests/RegressionTests.swift Tests/GridLayoutTests.swift
git commit -m "fix: make project tabs reliably switch on first click"
```

---

### Task 5: Full verification pass

**Files:**
- No code changes expected

**Step 1: Run unit tests for impacted modules**

Run:
`xcodebuild -project pmux.xcodeproj -scheme pmuxTests -configuration Debug test -only-testing:pmuxTests/GridLayoutTests`

Expected: PASS.

**Step 2: Run selected UI regression suite**

Run:
`xcodebuild -project pmux.xcodeproj -scheme pmuxUITests -configuration Debug test -only-testing:pmuxUITests/RegressionTests`

Expected: PASS.

**Step 3: Optional manual QA checklist**

1. Cold launch -> click first project tab directly -> opens project view.
2. Click dashboard -> click project tab repeatedly -> always switches.
3. Native traffic lights appear and work in offset position (`x=12,y=10`).
4. Layout popover, notification panel, AI panel still behave correctly.

**Step 4: Final commit (if verification-only changes exist)**

```bash
git add -A
git commit -m "test: finalize native titlebar migration regression coverage"
```
