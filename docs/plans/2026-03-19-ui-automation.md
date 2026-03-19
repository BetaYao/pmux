# Implementation Plan: UI Automation Testing

**Spec:** `docs/superpowers/specs/2026-03-19-ui-automation-design.md`

## Task 1: Add pmuxUITests target to project.yml

**File:** `project.yml`

Add after the `pmuxTests` target block:

```yaml
  pmuxUITests:
    type: bundle.ui-testing
    platform: macOS
    sources: [UITests]
    dependencies:
      - target: pmux
    settings:
      PRODUCT_BUNDLE_IDENTIFIER: com.pmux.uitests
      GENERATE_INFOPLIST_FILE: YES
      TEST_TARGET_NAME: pmux
```

Then run `xcodegen generate` to regenerate the Xcode project.

---

## Task 2: Add UITestConfig launch argument support to Config.swift

**File:** `Sources/Core/Config.swift`

In the `load()` static method (~line 55), add check for `-UITestConfig` launch argument before the default path:

```swift
static func load() -> Config {
    // Check for UI test config override
    if let idx = CommandLine.arguments.firstIndex(of: "-UITestConfig"),
       idx + 1 < CommandLine.arguments.count {
        let testPath = CommandLine.arguments[idx + 1]
        if let data = FileManager.default.contents(atPath: testPath) {
            return (try? JSONDecoder().decode(Config.self, from: data)) ?? Config()
        }
    }
    // ... existing default path loading ...
}
```

---

## Task 3: Add accessibility identifiers to TabBarView

**File:** `Sources/UI/TabBar/TabBarView.swift`

In `rebuildButtons()` (~line 75-83), after creating each tab button:
- Dashboard button: `button.setAccessibilityIdentifier("tabbar.dashboard")`
- Repo buttons: `button.setAccessibilityIdentifier("tabbar.repo.\(tab.displayName)")`

For `statusLabel` (~line 20 init): `statusLabel.setAccessibilityIdentifier("tabbar.statusBadge")`

---

## Task 4: Add accessibility identifiers to DashboardViewController

**File:** `Sources/UI/Dashboard/DashboardViewController.swift`

- `gridContainer` (~line 64-66): `gridContainer.setAccessibilityIdentifier("dashboard.grid")`

**File:** `Sources/UI/Dashboard/TerminalCardView.swift`

In `setup()` (~line 87-100), after creating controls:
- Card view itself: `self.setAccessibilityIdentifier("dashboard.card.\(worktreeName)")`
- `statusLabel`: `statusLabel.setAccessibilityIdentifier("dashboard.cardStatus.\(worktreeName)")`
- `messageLabel`: `messageLabel.setAccessibilityIdentifier("dashboard.cardMessage.\(worktreeName)")`

---

## Task 5: Add accessibility identifiers to SidebarViewController

**File:** `Sources/UI/Repo/SidebarViewController.swift`

- `tableView` (~line 30-40): `tableView.setAccessibilityIdentifier("sidebar.worktreeList")`
- In `tableView(_:viewFor:row:)` (~line 86-108), for each cell view: `cellView.setAccessibilityIdentifier("sidebar.row.\(worktree.displayName)")`

---

## Task 6: Add accessibility identifiers to RepoViewController & TerminalSplitView

**File:** `Sources/UI/Repo/RepoViewController.swift`

- `splitView` (~line 28-32): Not needed (NSSplitView is a container)

**File:** `Sources/UI/Repo/TerminalSplitView.swift`

- In `makeTerminalContainer()` (~line 254-259): `container.setAccessibilityIdentifier("repo.pane.\(paneIndex)")`
  - Need a pane counter or use the surface ID

---

## Task 7: Add accessibility identifiers to Dialogs

**File:** `Sources/UI/Dialog/QuickSwitcherViewController.swift`

- Container view (~line 25-35): `view.setAccessibilityIdentifier("dialog.quickSwitcher")`
- `searchField` (~line 38-45): `searchField.setAccessibilityIdentifier("dialog.quickSwitcher.searchField")`
- `resultsTableView` (~line 48-66): `resultsTableView.setAccessibilityIdentifier("dialog.quickSwitcher.resultsList")`

**File:** `Sources/UI/Dialog/NewBranchDialog.swift`

- Container view: `view.setAccessibilityIdentifier("dialog.newBranch")`
- `branchField` (~line 51-56): `branchField.setAccessibilityIdentifier("dialog.newBranch.nameField")`
- `createButton` (~line 71-81): `createButton.setAccessibilityIdentifier("dialog.newBranch.createButton")`

---

## Task 8: Add accessibility identifiers to Settings & Diff

**File:** `Sources/UI/Settings/SettingsViewController.swift`

- Container view: `view.setAccessibilityIdentifier("settings.sheet")`
- `pathListView` (~line 99-111): `pathListView.setAccessibilityIdentifier("settings.workspacePaths")`
- `addButton` (~line 114-126): `addButton.setAccessibilityIdentifier("settings.addPath")`
- `removeButton` (~line 114-126): `removeButton.setAccessibilityIdentifier("settings.removePath")`

**File:** `Sources/UI/Diff/DiffOverlayViewController.swift`

- Container view: `view.setAccessibilityIdentifier("repo.diffOverlay")`

---

## Task 9: Create UITests directory structure and helpers

Create:
- `UITests/Helpers/XCUIElementExtensions.swift`
- `UITests/Helpers/PmuxUITestCase.swift`

**PmuxUITestCase.swift:**
```swift
import XCTest

class PmuxUITestCase: XCTestCase {
    var page: AppPage!

    override func setUp() {
        super.setUp()
        continueAfterFailure = false
        page = AppPage().launch()
    }

    override func tearDown() {
        if testRun?.failureCount ?? 0 > 0 {
            let screenshot = XCUIScreen.main.screenshot()
            let attachment = XCTAttachment(screenshot: screenshot)
            attachment.lifetime = .keepAlways
            add(attachment)
        }
        page.terminate()
        super.tearDown()
    }
}
```

**XCUIElementExtensions.swift:**
```swift
import XCTest

extension XCUIElement {
    func waitAndClick(timeout: TimeInterval = 5) {
        XCTAssertTrue(waitForExistence(timeout: timeout), "\(identifier) not found")
        click()
    }

    func waitForNonExistence(timeout: TimeInterval = 5) -> Bool {
        let predicate = NSPredicate(format: "exists == false")
        let expectation = XCTNSPredicateExpectation(predicate: predicate, object: self)
        return XCTWaiter().wait(for: [expectation], timeout: timeout) == .completed
    }
}
```

---

## Task 10: Create Page Objects

Create all 7 page objects under `UITests/Pages/`:

- `AppPage.swift` — launch with optional test config, properties for all sub-pages
- `DashboardPage.swift` — grid, cards query, card operations
- `TabBarPage.swift` — dashboard tab, repo tabs, status badge
- `SidebarPage.swift` — worktree list, row operations
- `RepoPage.swift` — split view, diff overlay, split/close shortcuts
- `DialogPage.swift` — quick switcher + new branch dialog elements
- `SettingsPage.swift` — sheet, paths table, add/remove buttons

See spec for exact code.

---

## Task 11: Write Phase A tests — NavigationTests.swift

**File:** `UITests/Tests/NavigationTests.swift`

```swift
import XCTest

class NavigationTests: PmuxUITestCase {
    func testDashboardShowsOnLaunch() {
        XCTAssertTrue(page.dashboard.grid.waitForExistence(timeout: 5))
    }

    func testTabBarShowsDashboard() {
        XCTAssertTrue(page.tabBar.dashboardTab.waitForExistence(timeout: 3))
    }

    func testDashboardCardsExist() {
        XCTAssertTrue(page.dashboard.grid.waitForExistence(timeout: 5))
        XCTAssertGreaterThan(page.dashboard.cards.count, 0)
    }

    func testClickCardOpensRepo() {
        XCTAssertTrue(page.dashboard.grid.waitForExistence(timeout: 5))
        let firstCard = page.dashboard.cards.firstMatch
        XCTAssertTrue(firstCard.waitForExistence(timeout: 3))
        firstCard.doubleClick()
        // Verify repo view appeared (sidebar visible)
        XCTAssertTrue(page.sidebar.worktreeList.waitForExistence(timeout: 5))
    }

    func testSidebarWorktreeList() {
        // Navigate to a repo tab first
        let firstCard = page.dashboard.cards.firstMatch
        XCTAssertTrue(firstCard.waitForExistence(timeout: 5))
        firstCard.doubleClick()
        XCTAssertTrue(page.sidebar.worktreeList.waitForExistence(timeout: 5))
    }

    func testTabSwitchBackToDashboard() {
        let firstCard = page.dashboard.cards.firstMatch
        XCTAssertTrue(firstCard.waitForExistence(timeout: 5))
        firstCard.doubleClick()
        // Switch back
        page.tabBar.clickDashboardTab()
        XCTAssertTrue(page.dashboard.grid.waitForExistence(timeout: 5))
    }
}
```

---

## Task 12: Write Phase B tests — WorktreeTests, SplitPaneTests, SettingsTests

**WorktreeTests.swift:** testNewBranchDialogFlow, testDeleteWorktreeContextMenu
**SplitPaneTests.swift:** testSplitPaneCreation, testDiffOverlayToggle
**SettingsTests.swift:** testSettingsOpenAndClose, testSettingsWorkspacePaths

---

## Task 13: Write Phase C tests — ShortcutTests.swift

Test all keyboard shortcuts: Cmd+,, Cmd+P, Cmd+N, Cmd+D, Cmd+Shift+D, Cmd+Shift+W, Esc.

---

## Task 14: Create run_ui_tests.sh

**File:** `run_ui_tests.sh` (at project root)

Script that runs `xcodegen generate` then `xcodebuild test -only-testing:pmuxUITests` with optional filter argument. See spec for full script.

Make executable: `chmod +x run_ui_tests.sh`

---

## Task 15: Compile and verify

1. Run `xcodegen generate` to regenerate project
2. Build: `xcodebuild build-for-testing -scheme pmux -destination 'platform=macOS'`
3. Run UI tests: `./run_ui_tests.sh`
4. Fix any compilation or runtime issues

---

## Execution Order

Tasks 1-2 (infrastructure) → Tasks 3-8 (accessibility IDs, parallelizable) → Tasks 9-10 (test framework) → Tasks 11-13 (test cases) → Task 14 (script) → Task 15 (verify)
