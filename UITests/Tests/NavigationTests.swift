import XCTest

/// Core navigation flow tests for the redesigned dashboard.
class NavigationTests: PmuxUITestCase {

    func testDashboardShowsOnLaunch() {
        XCTAssertTrue(page.dashboard.dashboardView.waitForExistence(timeout: 10),
                      "Dashboard view should be visible on launch")
    }

    func testTitleBarExistsOnLaunch() {
        XCTAssertTrue(page.titleBar.titleBar.waitForExistence(timeout: 10),
                      "Title bar should exist on launch")
    }

    func testStatusBarExistsOnLaunch() {
        XCTAssertTrue(page.statusBar.statusBar.waitForExistence(timeout: 10),
                      "Status bar should exist on launch")
    }

    func testDashboardCardsExist() {
        guard page.dashboard.dashboardView.waitForExistence(timeout: 10) else { return }

        XCTAssertGreaterThan(page.dashboard.cards.count, 0,
                             "Should have at least one dashboard card")
    }

    func testNavigateToProjectAndBack() {
        guard page.dashboard.dashboardView.waitForExistence(timeout: 10) else { return }

        let firstCard = page.dashboard.cards.firstMatch
        guard firstCard.waitForExistence(timeout: 5) else { return }
        firstCard.click()

        // Use enter-project if available (non-grid layouts)
        if page.dashboard.enterProjectButton.waitForExistence(timeout: 3) {
            page.dashboard.tapEnterProject()
        }

        // Wait for project view to load
        guard page.sidebar.worktreeList.waitForExistence(timeout: 10) else { return }

        // Navigate back to dashboard
        page.titleBar.clickDashboardTab()
        XCTAssertTrue(page.dashboard.dashboardView.waitForExistence(timeout: 5),
                      "Dashboard should reappear after clicking dashboard tab")
    }

    func testDashboardTabClickReturnsToDashboard() {
        guard page.titleBar.dashboardTab.waitForExistence(timeout: 10) else { return }

        page.titleBar.clickDashboardTab()
        XCTAssertTrue(page.dashboard.dashboardView.waitForExistence(timeout: 5),
                      "Clicking dashboard tab should show dashboard")
    }
}
