import XCTest

/// Keyboard shortcut tests for the redesigned dashboard.
class ShortcutTests: PmuxUITestCase {

    func testCmdCommaSettings() {
        page.app.typeKey(",", modifierFlags: .command)
        XCTAssertTrue(page.settings.sheet.waitForExistence(timeout: 5),
                      "Cmd+, should open settings")
        page.app.typeKey(.escape, modifierFlags: [])
    }

    func testCmdPQuickSwitcher() {
        page.app.typeKey("p", modifierFlags: .command)
        XCTAssertTrue(page.dialog.quickSwitcher.waitForExistence(timeout: 5),
                      "Cmd+P should open quick switcher")
        page.app.typeKey(.escape, modifierFlags: [])
    }

    func testCmdNNewBranch() {
        page.app.typeKey("n", modifierFlags: .command)
        XCTAssertTrue(page.dialog.newBranchDialog.waitForExistence(timeout: 5),
                      "Cmd+N should open new branch dialog")
        page.app.typeKey(.escape, modifierFlags: [])
    }

    func testEscClosesDialog() {
        // Open quick switcher
        page.app.typeKey("p", modifierFlags: .command)
        XCTAssertTrue(page.dialog.quickSwitcher.waitForExistence(timeout: 5))

        // Escape should close it
        page.app.typeKey(.escape, modifierFlags: [])
        XCTAssertTrue(page.dialog.quickSwitcher.waitForNonExistence(timeout: 3),
                      "Escape should close the quick switcher")
    }

    func testEscClosesModal() {
        guard page.titleBar.dashboardTab.waitForExistence(timeout: 10) else { return }

        // Try Cmd+W to trigger close modal
        page.app.typeKey("w", modifierFlags: .command)

        if page.modal.isVisible {
            page.app.typeKey(.escape, modifierFlags: [])
            XCTAssertTrue(page.modal.overlay.waitForNonExistence(timeout: 3),
                          "Escape should close the modal")
        }
    }

    func testEscClosesPanel() {
        guard page.titleBar.notifButton.waitForExistence(timeout: 10) else { return }

        page.titleBar.clickNotif()
        guard page.notifPanel.isOpen else { return }

        page.app.typeKey(.escape, modifierFlags: [])
        XCTAssertTrue(page.notifPanel.panel.waitForNonExistence(timeout: 3),
                      "Escape should close the notification panel")
    }
}
