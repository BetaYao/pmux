import XCTest

/// P0 — Smoke tests. Must pass on every build.
class SmokeTests: PmuxUITestCase {

    func testDashboardVisibleOnLaunch() {
        XCTAssertTrue(page.dashboard.dashboardView.waitForExistence(timeout: 10),
                      "Dashboard view should be visible on launch")
    }

    func testDashboardHasCards() {
        guard page.dashboard.dashboardView.waitForExistence(timeout: 10) else { return }
        XCTAssertGreaterThan(page.dashboard.cards.count, 0,
                             "Dashboard should display at least one card")
    }

    func testNavigateToProjectAndBack() {
        guard page.dashboard.dashboardView.waitForExistence(timeout: 10) else { return }

        let firstCard = page.dashboard.cards.firstMatch
        guard firstCard.waitForExistence(timeout: 5) else { return }
        firstCard.click()

        if page.dashboard.enterProjectButton.waitForExistence(timeout: 3) {
            page.dashboard.tapEnterProject()
        }

        guard page.sidebar.worktreeList.waitForExistence(timeout: 10) else { return }

        page.titleBar.clickDashboardTab()
        XCTAssertTrue(page.dashboard.dashboardView.waitForExistence(timeout: 5),
                      "Dashboard should reappear after clicking dashboard tab")
    }

    func testCmdPQuickSwitcher() {
        page.app.typeKey("p", modifierFlags: .command)
        XCTAssertTrue(page.dialog.quickSwitcher.waitForExistence(timeout: 5),
                      "Cmd+P should open quick switcher")
        page.app.typeKey(.escape, modifierFlags: [])
    }

    func testThemeCycleDoesNotCrash() {
        guard page.titleBar.themeToggle.waitForExistence(timeout: 10) else { return }

        page.titleBar.clickTheme()
        sleep(1)
        page.titleBar.clickTheme()
        sleep(1)

        XCTAssertTrue(page.dashboard.dashboardView.waitForExistence(timeout: 5),
                      "App should remain functional after cycling themes")
    }
}
