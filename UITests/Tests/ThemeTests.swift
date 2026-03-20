import XCTest

/// Tests for the theme toggle button.
class ThemeTests: PmuxUITestCase {

    func testThemeToggleExists() {
        XCTAssertTrue(page.titleBar.themeToggle.waitForExistence(timeout: 10),
                      "Theme toggle button should exist in the title bar")
    }

    func testThemeCycleDoesNotCrash() {
        guard page.titleBar.themeToggle.waitForExistence(timeout: 10) else { return }

        // Click theme toggle multiple times to cycle through themes
        page.titleBar.clickTheme()
        sleep(1)
        page.titleBar.clickTheme()
        sleep(1)

        // App should still be running — dashboard should still be visible
        XCTAssertTrue(page.dashboard.dashboardView.waitForExistence(timeout: 5),
                      "App should remain functional after cycling themes")
    }
}
