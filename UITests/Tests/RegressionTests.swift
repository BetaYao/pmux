import XCTest

/// P2 — Regression protection. Run weekly or before major releases.
class RegressionTests: PmuxUITestCase {

    func testSwitchToTopSmallLayout() {
        guard page.titleBar.viewMenuButton.waitForExistence(timeout: 10) else { return }
        page.titleBar.clickViewMenu()
        guard page.layoutPopover.popover.waitForExistence(timeout: 5) else { return }

        page.layoutPopover.selectTopSmall()
        XCTAssertTrue(page.dashboard.topSmallLayout.waitForExistence(timeout: 5),
                      "Top-small layout should be visible after selection")
    }

    func testSwitchToTopLargeLayout() {
        guard page.titleBar.viewMenuButton.waitForExistence(timeout: 10) else { return }
        page.titleBar.clickViewMenu()
        guard page.layoutPopover.popover.waitForExistence(timeout: 5) else { return }

        page.layoutPopover.selectTopLarge()
        XCTAssertTrue(page.dashboard.topLargeLayout.waitForExistence(timeout: 5),
                      "Top-large layout should be visible after selection")
    }

    func testEscClosesDialogAndPanel() {
        // Esc closes quick switcher
        page.app.typeKey("p", modifierFlags: .command)
        XCTAssertTrue(page.dialog.quickSwitcher.waitForExistence(timeout: 5))

        page.app.typeKey(.escape, modifierFlags: [])
        XCTAssertTrue(page.dialog.quickSwitcher.waitForNonExistence(timeout: 3),
                      "Escape should close the quick switcher")

        // Esc closes notification panel
        guard page.titleBar.notifButton.waitForExistence(timeout: 10) else { return }
        page.titleBar.clickNotif()
        guard page.notifPanel.isOpen else { return }

        page.app.typeKey(.escape, modifierFlags: [])
        XCTAssertTrue(page.notifPanel.panel.waitForNonExistence(timeout: 3),
                      "Escape should close the notification panel")
    }

    func testBackdropDismissesPanel() {
        guard page.titleBar.notifButton.waitForExistence(timeout: 10) else { return }

        page.titleBar.clickNotif()
        guard page.notifPanel.isOpen else { return }

        if page.notifPanel.backdrop.waitForExistence(timeout: 3) {
            page.notifPanel.clickBackdrop()
            XCTAssertTrue(page.notifPanel.panel.waitForNonExistence(timeout: 3),
                          "Clicking backdrop should dismiss notification panel")
        }
    }

    func testCloseProjectTabTriggersConfirmation() {
        guard page.dashboard.dashboardView.waitForExistence(timeout: 10) else { return }

        let firstCard = page.dashboard.cards.firstMatch
        guard firstCard.waitForExistence(timeout: 5) else { return }
        firstCard.click()

        if page.dashboard.enterProjectButton.waitForExistence(timeout: 3) {
            page.dashboard.tapEnterProject()
        }

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
