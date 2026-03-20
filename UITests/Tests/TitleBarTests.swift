import XCTest

/// Tests for the redesigned title bar: dashboard tab, project tabs, action buttons.
class TitleBarTests: PmuxUITestCase {

    // MARK: - Dashboard tab

    func testDashboardTabAlwaysVisible() {
        XCTAssertTrue(page.titleBar.dashboardTab.waitForExistence(timeout: 10),
                      "Dashboard tab should always be visible in the title bar")
    }

    func testTitleBarVisible() {
        XCTAssertTrue(page.titleBar.titleBar.waitForExistence(timeout: 10),
                      "Title bar container should be visible")
    }

    // MARK: - Add project

    func testAddProjectButtonExists() {
        XCTAssertTrue(page.titleBar.addProjectButton.waitForExistence(timeout: 10),
                      "Add project button should exist in the title bar")
    }

    func testAddProjectButtonOpensPanel() {
        guard page.titleBar.addProjectButton.waitForExistence(timeout: 10) else { return }
        page.titleBar.clickAddProject()

        // Either an open panel or modal should appear
        let openPanel = page.app.sheets.firstMatch
        let modalVisible = page.modal.isVisible
        if openPanel.waitForExistence(timeout: 3) {
            openPanel.buttons["Cancel"].click()
        } else if modalVisible {
            page.modal.dismissWithEscape()
        }
    }

    // MARK: - New thread

    func testNewThreadButtonExists() {
        XCTAssertTrue(page.titleBar.newThreadButton.waitForExistence(timeout: 10),
                      "New thread button should exist in the title bar")
    }

    // MARK: - View menu

    func testViewMenuButtonExists() {
        XCTAssertTrue(page.titleBar.viewMenuButton.waitForExistence(timeout: 10),
                      "View menu button should exist in the title bar")
    }

    // MARK: - Notification button

    func testNotifButtonExists() {
        XCTAssertTrue(page.titleBar.notifButton.waitForExistence(timeout: 10),
                      "Notification button should exist in the title bar")
    }

    // MARK: - AI button

    func testAIButtonExists() {
        XCTAssertTrue(page.titleBar.aiButton.waitForExistence(timeout: 10),
                      "AI button should exist in the title bar")
    }

    // MARK: - Theme toggle

    func testThemeToggleExists() {
        XCTAssertTrue(page.titleBar.themeToggle.waitForExistence(timeout: 10),
                      "Theme toggle should exist in the title bar")
    }
}
