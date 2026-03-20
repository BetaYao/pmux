import XCTest

/// Tests for notification and AI side panels.
class PanelTests: PmuxUITestCase {

    // MARK: - Notification panel

    func testNotificationPanelOpenClose() {
        guard page.titleBar.notifButton.waitForExistence(timeout: 10) else { return }

        page.titleBar.clickNotif()
        XCTAssertTrue(page.notifPanel.isOpen,
                      "Notification panel should open when notif button is clicked")

        page.notifPanel.close()
        XCTAssertTrue(page.notifPanel.panel.waitForNonExistence(timeout: 3),
                      "Notification panel should close when close button is clicked")
    }

    // MARK: - AI panel

    func testAIPanelOpenClose() {
        guard page.titleBar.aiButton.waitForExistence(timeout: 10) else { return }

        page.titleBar.clickAI()
        XCTAssertTrue(page.aiPanel.isOpen,
                      "AI panel should open when AI button is clicked")

        page.aiPanel.close()
        XCTAssertTrue(page.aiPanel.panel.waitForNonExistence(timeout: 3),
                      "AI panel should close when close button is clicked")
    }

    // MARK: - Mutual exclusion

    func testOpeningNotifPanelClosesAIPanel() {
        guard page.titleBar.aiButton.waitForExistence(timeout: 10) else { return }
        guard page.titleBar.notifButton.waitForExistence(timeout: 5) else { return }

        // Open AI panel first
        page.titleBar.clickAI()
        guard page.aiPanel.isOpen else { return }

        // Open notification panel — should close AI panel
        page.titleBar.clickNotif()
        XCTAssertTrue(page.notifPanel.isOpen,
                      "Notification panel should open")
        XCTAssertTrue(page.aiPanel.panel.waitForNonExistence(timeout: 3),
                      "AI panel should close when notification panel opens")
    }

    func testOpeningAIPanelClosesNotifPanel() {
        guard page.titleBar.notifButton.waitForExistence(timeout: 10) else { return }
        guard page.titleBar.aiButton.waitForExistence(timeout: 5) else { return }

        // Open notification panel first
        page.titleBar.clickNotif()
        guard page.notifPanel.isOpen else { return }

        // Open AI panel — should close notification panel
        page.titleBar.clickAI()
        XCTAssertTrue(page.aiPanel.isOpen,
                      "AI panel should open")
        XCTAssertTrue(page.notifPanel.panel.waitForNonExistence(timeout: 3),
                      "Notification panel should close when AI panel opens")
    }

    // MARK: - Backdrop dismiss

    func testBackdropDismissesNotifPanel() {
        guard page.titleBar.notifButton.waitForExistence(timeout: 10) else { return }

        page.titleBar.clickNotif()
        guard page.notifPanel.isOpen else { return }

        // Click the backdrop to dismiss
        if page.notifPanel.backdrop.waitForExistence(timeout: 3) {
            page.notifPanel.clickBackdrop()
            XCTAssertTrue(page.notifPanel.panel.waitForNonExistence(timeout: 3),
                          "Clicking backdrop should dismiss notification panel")
        }
    }
}
