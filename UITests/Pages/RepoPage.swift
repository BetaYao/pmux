import XCTest

/// Page object for the repo view (terminal split panes).
class RepoPage {
    private let app: XCUIApplication

    init(_ app: XCUIApplication) {
        self.app = app
    }

    var diffOverlay: XCUIElement {
        app.groups["repo.diffOverlay"]
    }

    func pane(at index: Int) -> XCUIElement {
        app.groups["repo.pane.\(index)"]
    }

    /// Count visible terminal panes.
    var panes: XCUIElementQuery {
        app.groups.matching(NSPredicate(format: "identifier BEGINSWITH 'repo.pane.'"))
    }

    func toggleDiff() {
        app.typeKey("d", modifierFlags: .command)
    }

    func splitVertical() {
        app.typeKey("d", modifierFlags: [.command, .shift])
    }

    func splitHorizontal() {
        app.typeKey("e", modifierFlags: [.command, .shift])
    }

    func closePane() {
        app.typeKey("w", modifierFlags: [.command, .shift])
    }
}
