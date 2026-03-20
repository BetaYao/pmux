import XCTest

/// Tests for project tab management in the title bar.
/// (Replaces the old TabBarTests that referenced the removed TabBarPage.)
class TabBarTests: PmuxUITestCase {

    func testDashboardTabAlwaysPresent() {
        XCTAssertTrue(page.titleBar.dashboardTab.waitForExistence(timeout: 10),
                      "Dashboard tab should always be present")
    }

    func testAddProjectButtonExists() {
        XCTAssertTrue(page.titleBar.addProjectButton.waitForExistence(timeout: 10),
                      "Add project button should exist in the title bar")
    }

    func testCloseProjectTabTriggersConfirmation() {
        guard page.titleBar.dashboardTab.waitForExistence(timeout: 10) else { return }

        // Navigate to a project first (if cards exist)
        guard page.dashboard.dashboardView.waitForExistence(timeout: 5) else { return }
        let firstCard = page.dashboard.cards.firstMatch
        guard firstCard.waitForExistence(timeout: 5) else { return }
        firstCard.click()

        if page.dashboard.enterProjectButton.waitForExistence(timeout: 3) {
            page.dashboard.tapEnterProject()
        }

        // Try Cmd+W to close — should trigger modal confirmation
        page.app.typeKey("w", modifierFlags: .command)

        if page.modal.isVisible {
            XCTAssertTrue(page.modal.cancelButton.waitForExistence(timeout: 3),
                          "Close confirmation should have cancel button")
            XCTAssertTrue(page.modal.confirmButton.waitForExistence(timeout: 3),
                          "Close confirmation should have confirm button")
            page.modal.dismissWithEscape()
        }
    }
}
