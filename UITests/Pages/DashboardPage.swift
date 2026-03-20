import XCTest

/// Page object for Dashboard grid/spotlight views.
class DashboardPage {
    private let app: XCUIApplication

    init(_ app: XCUIApplication) {
        self.app = app
    }

    var grid: XCUIElement {
        app.groups["dashboard.grid"]
    }

    /// All dashboard card elements (groups with "dashboard.card.*" identifiers).
    var cards: XCUIElementQuery {
        app.groups.matching(NSPredicate(format: "identifier MATCHES 'dashboard\\.card\\.[^.]+$'"))
    }

    func card(named name: String) -> XCUIElement {
        app.groups["dashboard.card.\(name)"]
    }

    func cardStatus(named name: String) -> String {
        app.staticTexts["dashboard.cardStatus.\(name)"].label
    }

    func cardMessage(named name: String) -> String {
        app.staticTexts["dashboard.cardMessage.\(name)"].label
    }

    func tapCard(named name: String) {
        card(named: name).click()
    }

    func doubleClickCard(named name: String) {
        card(named: name).doubleClick()
    }
}
