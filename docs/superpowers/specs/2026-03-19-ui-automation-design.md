# UI Automation Testing Design

## Overview

Introduce XCUITest-based UI automation testing for pmux-swift, replacing manual click-testing after feature development. Uses Page Object pattern for maintainability.

## Decision

- **Framework:** XCUITest (Apple native, out-of-process)
- **Pattern:** Page Object — each UI area encapsulated as a Swift class
- **Runner:** `run_ui_tests.sh` script wrapping `xcodebuild test`
- **Coverage priority:** A) Core navigation → B) Feature operations → C) Keyboard shortcuts

## Project Structure

```
pmux-swift/
├── UITests/
│   ├── Pages/                           # Page Object layer
│   │   ├── AppPage.swift                # Top-level app entry, launch/terminate
│   │   ├── DashboardPage.swift          # Dashboard grid/card operations
│   │   ├── TabBarPage.swift             # Tab switching, status badges
│   │   ├── SidebarPage.swift            # Sidebar worktree list
│   │   ├── RepoPage.swift              # Repo view, split pane operations
│   │   ├── DialogPage.swift             # Dialogs (new branch, quick switcher)
│   │   └── SettingsPage.swift           # Settings window
│   ├── Tests/                           # Test cases by priority group
│   │   ├── NavigationTests.swift        # A) Dashboard, tabs, sidebar
│   │   ├── WorktreeTests.swift          # B) Create/delete worktree
│   │   ├── SplitPaneTests.swift         # B) Split pane operations
│   │   ├── SettingsTests.swift          # B) Settings UI
│   │   └── ShortcutTests.swift          # C) Keyboard shortcuts
│   └── Helpers/
│       └── XCUIElementExtensions.swift  # waitAndClick, waitForNonExistence
├── run_ui_tests.sh                      # One-click test runner
```

### project.yml Addition

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

## Accessibility Identifier Specification

Naming convention: `area.controlName` with dot-separated hierarchy. Dynamic elements use `area.control.{dynamicName}`.

**Important:** Child elements within a card use a flat prefix to avoid over-matching in queries. Card containers use `dashboard.card.{name}`, while child labels use `dashboard.cardStatus.{name}` and `dashboard.cardMessage.{name}` (not nested under `dashboard.card.{name}.`).

### TabBar

| Control | Identifier |
|---------|-----------|
| Dashboard Tab | `tabbar.dashboard` |
| Repo Tab (dynamic) | `tabbar.repo.{worktreeName}` |
| Status Badge | `tabbar.statusBadge` |

### Dashboard

| Control | Identifier |
|---------|-----------|
| Grid container | `dashboard.grid` |
| Terminal card (dynamic) | `dashboard.card.{worktreeName}` |
| Card status label | `dashboard.cardStatus.{name}` |
| Card message label | `dashboard.cardMessage.{name}` |
| Zoom In/Out buttons | `dashboard.zoomIn` / `dashboard.zoomOut` |

### Sidebar

| Control | Identifier |
|---------|-----------|
| Worktree list | `sidebar.worktreeList` |
| Worktree row (dynamic) | `sidebar.row.{worktreeName}` |

### Dialogs

| Control | Identifier |
|---------|-----------|
| New branch dialog | `dialog.newBranch` |
| Branch name field | `dialog.newBranch.nameField` |
| Create button | `dialog.newBranch.createButton` |
| Quick Switcher | `dialog.quickSwitcher` |
| Search field | `dialog.quickSwitcher.searchField` |
| Results list | `dialog.quickSwitcher.resultsList` |

### Settings

Settings is presented as a sheet (via `presentAsSheet`), not a standalone window. Use `app.sheets` or `app.otherElements` to locate it, not `app.windows`.

| Control | Identifier |
|---------|-----------|
| Settings sheet | `settings.sheet` |
| Workspace paths list | `settings.workspacePaths` |
| Add path button | `settings.addPath` |
| Remove path button | `settings.removePath` |

### Sidebar

**Note:** Sidebar only exists within `RepoViewController`, not on the Dashboard. Sidebar tests only apply when a repo tab is active.

### Repo

| Control | Identifier |
|---------|-----------|
| Split view container | `repo.splitView` |
| Terminal pane (dynamic) | `repo.pane.{index}` |
| Diff overlay | `repo.diffOverlay` |

## Page Object Design

### AppPage — Top-level Entry

```swift
class AppPage {
    let app: XCUIApplication

    init() { app = XCUIApplication() }

    func launch(testConfigPath: String? = nil) -> Self {
        if let path = testConfigPath {
            app.launchArguments += ["-UITestConfig", path]
        }
        app.launch()
        return self
    }

    func terminate() { app.terminate() }

    var dashboard: DashboardPage { DashboardPage(app) }
    var tabBar: TabBarPage { TabBarPage(app) }
    var sidebar: SidebarPage { SidebarPage(app) }
    var settings: SettingsPage { SettingsPage(app) }
    var dialog: DialogPage { DialogPage(app) }
    var repo: RepoPage { RepoPage(app) }
}
```

### DashboardPage

```swift
class DashboardPage {
    private let app: XCUIApplication
    init(_ app: XCUIApplication) { self.app = app }

    var grid: XCUIElement { app.otherElements["dashboard.grid"] }
    var cards: XCUIElementQuery {
        app.otherElements.matching(NSPredicate(format: "identifier MATCHES 'dashboard\\.card\\.[^.]+$'"))
    }

    func card(named name: String) -> XCUIElement { app.otherElements["dashboard.card.\(name)"] }
    func cardStatus(named name: String) -> String { app.staticTexts["dashboard.cardStatus.\(name)"].label }
    func tapCard(named name: String) { card(named: name).click() }
    func doubleClickCard(named name: String) { card(named: name).doubleClick() }
}
```

### TabBarPage

```swift
class TabBarPage {
    private let app: XCUIApplication
    init(_ app: XCUIApplication) { self.app = app }

    var dashboardTab: XCUIElement { app.buttons["tabbar.dashboard"] }
    func repoTab(named name: String) -> XCUIElement { app.buttons["tabbar.repo.\(name)"] }
    func clickDashboardTab() { dashboardTab.click() }
    func clickRepoTab(named name: String) { repoTab(named: name).click() }
}
```

### SidebarPage

```swift
class SidebarPage {
    private let app: XCUIApplication
    init(_ app: XCUIApplication) { self.app = app }

    var worktreeList: XCUIElement { app.tables["sidebar.worktreeList"] }
    func row(named name: String) -> XCUIElement { app.otherElements["sidebar.row.\(name)"] }
    func rightClickRow(named name: String) { row(named: name).rightClick() }
}
```

### DialogPage

```swift
class DialogPage {
    private let app: XCUIApplication
    init(_ app: XCUIApplication) { self.app = app }

    // Quick Switcher
    var quickSwitcher: XCUIElement { app.otherElements["dialog.quickSwitcher"] }
    var searchField: XCUIElement { app.textFields["dialog.quickSwitcher.searchField"] }
    func openQuickSwitcher() { app.typeKey("p", modifierFlags: .command) }
    func search(_ query: String) { searchField.typeText(query) }
    func selectFirstResult() { app.typeKey(.enter, modifierFlags: []) }

    // New Branch
    var newBranchDialog: XCUIElement { app.otherElements["dialog.newBranch"] }
    var branchNameField: XCUIElement { app.textFields["dialog.newBranch.nameField"] }
    var createButton: XCUIElement { app.buttons["dialog.newBranch.createButton"] }
    func openNewBranchDialog() { app.typeKey("n", modifierFlags: .command) }
}
```

### SettingsPage

```swift
class SettingsPage {
    private let app: XCUIApplication
    init(_ app: XCUIApplication) { self.app = app }

    var sheet: XCUIElement { app.sheets["settings.sheet"] }
    var workspacePaths: XCUIElement { app.tables["settings.workspacePaths"] }
    var addPathButton: XCUIElement { app.buttons["settings.addPath"] }
    var removePathButton: XCUIElement { app.buttons["settings.removePath"] }
    func open() { app.typeKey(",", modifierFlags: .command) }
}
```

### RepoPage

```swift
class RepoPage {
    private let app: XCUIApplication
    init(_ app: XCUIApplication) { self.app = app }

    var splitView: XCUIElement { app.otherElements["repo.splitView"] }
    var diffOverlay: XCUIElement { app.otherElements["repo.diffOverlay"] }
    func toggleDiff() { app.typeKey("d", modifierFlags: .command) }
    func splitVertical() { app.typeKey("d", modifierFlags: [.command, .shift]) }
    func splitHorizontal() { app.typeKey("e", modifierFlags: [.command, .shift]) }
    func closePane() { app.typeKey("w", modifierFlags: [.command, .shift]) }
}
```

### Helper Extensions

```swift
extension XCUIElement {
    func waitAndClick(timeout: TimeInterval = 5) {
        XCTAssertTrue(waitForExistence(timeout: timeout), "\(identifier) not found")
        click()
    }

    func waitForNonExistence(timeout: TimeInterval = 5) -> Bool {
        let predicate = NSPredicate(format: "exists == false")
        let expectation = XCTNSPredicateExpectation(predicate: predicate, object: self)
        return XCTWaiter.wait(for: [expectation], timeout: timeout) == .completed
    }
}
```

## Base Test Class

All UI tests inherit from `PmuxUITestCase` which handles launch, teardown, test config, and screenshot capture on failure.

```swift
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

## Test Coverage Plan

### Phase A — Core Navigation (first batch)

| Test | Assertion |
|------|-----------|
| `testDashboardShowsOnLaunch` | Dashboard grid visible after launch |
| `testDashboardCardsExist` | Card count > 0 with valid workspace config |
| `testTabBarShowsDashboard` | Dashboard tab exists and clickable |
| `testClickCardOpensRepo` | Double-click card switches to Repo view |
| `testSidebarWorktreeList` | Sidebar shows worktree list (requires repo tab active) |
| `testTabSwitchBackToDashboard` | Switch to repo then back to dashboard |

### Phase B — Feature Operations

| Test | Assertion |
|------|-----------|
| `testNewBranchDialogFlow` | Open dialog → type name → create button enabled |
| `testDeleteWorktreeContextMenu` | Right-click row → delete option appears |
| `testSettingsOpenAndClose` | Settings sheet opens/closes |
| `testSettingsWorkspacePaths` | Path list visible, add button functional |
| `testDiffOverlayToggle` | Cmd+D opens/closes diff overlay |
| `testSplitPaneCreation` | Cmd+Shift+D creates two panes |

### Phase C — Keyboard Shortcuts

| Test | Assertion |
|------|-----------|
| `testCmdCommaSettings` | Cmd+, opens settings |
| `testCmdPQuickSwitcher` | Cmd+P opens quick switcher |
| `testCmdNNewBranch` | Cmd+N opens new branch dialog |
| `testCmdDDiffOverlay` | Cmd+D toggles diff |
| `testCmdShiftDSplitVertical` | Cmd+Shift+D vertical split |
| `testCmdShiftWSplitClose` | Cmd+Shift+W closes split pane |
| `testEscClosesDialog` | Esc closes dialog/overlay |

## Test Configuration Strategy

Use launch arguments to inject test configuration without affecting user's real config:

```swift
// In app's AppDelegate or Config.swift
if let testConfigIndex = CommandLine.arguments.firstIndex(of: "-UITestConfig"),
   testConfigIndex + 1 < CommandLine.arguments.count {
    let testConfigPath = CommandLine.arguments[testConfigIndex + 1]
    // Load config from testConfigPath instead of default location
}
```

Test fixture: a minimal git repo created in a temp directory by test setUp(), with a config.json pointing to it.

## Run Script

```bash
#!/bin/bash
set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "$0")" && pwd)"
SCHEME="pmux"
DESTINATION="platform=macOS"
TEST_FILTER="${1:-}"

# Ensure Xcode project is up to date
if command -v xcodegen &> /dev/null; then
    echo "=== Generating Xcode project ==="
    cd "$PROJECT_DIR" && xcodegen generate
else
    if [ ! -d "$PROJECT_DIR/pmux.xcodeproj" ]; then
        echo "Error: pmux.xcodeproj not found and xcodegen is not installed."
        echo "Install with: brew install xcodegen"
        exit 1
    fi
fi

mkdir -p "$PROJECT_DIR/.build"
rm -rf "$PROJECT_DIR/.build/ui-test-results"

ARGS=(
    -project "$PROJECT_DIR/pmux.xcodeproj"
    -scheme "$SCHEME"
    -destination "$DESTINATION"
    -only-testing:pmuxUITests
    -resultBundlePath "$PROJECT_DIR/.build/ui-test-results"
)

if [ -n "$TEST_FILTER" ]; then
    ARGS=(-project "$PROJECT_DIR/pmux.xcodeproj"
          -scheme "$SCHEME"
          -destination "$DESTINATION"
          -only-testing:"pmuxUITests/$TEST_FILTER"
          -resultBundlePath "$PROJECT_DIR/.build/ui-test-results")
fi

echo "=== Building and running UI tests ==="
xcodebuild test "${ARGS[@]}" 2>&1 | tee "$PROJECT_DIR/.build/ui-test-output.log"
echo "=== UI tests complete ==="
echo "Results: $PROJECT_DIR/.build/ui-test-results"
```

Usage:
```bash
./run_ui_tests.sh                                          # all UI tests
./run_ui_tests.sh NavigationTests                          # one test class
./run_ui_tests.sh NavigationTests/testDashboardShowsOnLaunch  # one method
```
