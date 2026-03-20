import XCTest

/// Page object for the tab bar.
class TabBarPage {
    private let app: XCUIApplication

    init(_ app: XCUIApplication) {
        self.app = app
    }

    var dashboardTab: XCUIElement {
        // TabButtonView has accessibilityRole(.button)
        app.buttons["tabbar.dashboard"]
    }

    var statusBadge: XCUIElement {
        app.staticTexts["tabbar.statusBadge"]
    }

    var addButton: XCUIElement {
        app.buttons["tabbar.addButton"]
    }

    /// All repo tab elements (buttons with "tabbar.repo.*" identifiers)
    var repoTabs: XCUIElementQuery {
        app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'tabbar.repo.'"))
    }

    func repoTab(named name: String) -> XCUIElement {
        app.buttons["tabbar.repo.\(name)"]
    }

    /// Find the close button inside a repo tab
    func closeButton(forRepoNamed name: String) -> XCUIElement {
        repoTab(named: name).buttons.firstMatch
    }

    func clickDashboardTab() {
        dashboardTab.waitAndClick()
    }

    func clickRepoTab(named name: String) {
        repoTab(named: name).waitAndClick()
    }

    func clickAddButton() {
        addButton.waitAndClick()
    }
}
