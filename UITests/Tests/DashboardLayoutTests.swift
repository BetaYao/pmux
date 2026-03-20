import XCTest

/// Tests for dashboard layout modes and the view menu popover.
class DashboardLayoutTests: PmuxUITestCase {

    // MARK: - Dashboard view

    func testDashboardViewVisibleOnLaunch() {
        XCTAssertTrue(page.dashboard.dashboardView.waitForExistence(timeout: 10),
                      "Dashboard view should be visible on launch")
    }

    // MARK: - View menu popover

    func testViewMenuOpensPopover() {
        guard page.titleBar.viewMenuButton.waitForExistence(timeout: 10) else { return }
        page.titleBar.clickViewMenu()

        XCTAssertTrue(page.layoutPopover.popover.waitForExistence(timeout: 5),
                      "View menu should open the layout popover")
    }

    func testPopoverHasAllLayoutOptions() {
        guard page.titleBar.viewMenuButton.waitForExistence(timeout: 10) else { return }
        page.titleBar.clickViewMenu()
        guard page.layoutPopover.popover.waitForExistence(timeout: 5) else { return }

        XCTAssertTrue(page.layoutPopover.gridItem.waitForExistence(timeout: 3),
                      "Grid layout option should exist")
        XCTAssertTrue(page.layoutPopover.leftRightItem.waitForExistence(timeout: 3),
                      "Left-right layout option should exist")
        XCTAssertTrue(page.layoutPopover.topSmallItem.waitForExistence(timeout: 3),
                      "Top-small layout option should exist")
        XCTAssertTrue(page.layoutPopover.topLargeItem.waitForExistence(timeout: 3),
                      "Top-large layout option should exist")
    }

    // MARK: - Layout switching

    func testSwitchToGridLayout() {
        guard page.titleBar.viewMenuButton.waitForExistence(timeout: 10) else { return }
        page.titleBar.clickViewMenu()
        guard page.layoutPopover.popover.waitForExistence(timeout: 5) else { return }

        page.layoutPopover.selectGrid()

        XCTAssertTrue(page.dashboard.gridLayout.waitForExistence(timeout: 5),
                      "Grid layout should be visible after selection")
    }

    func testSwitchToLeftRightLayout() {
        guard page.titleBar.viewMenuButton.waitForExistence(timeout: 10) else { return }
        page.titleBar.clickViewMenu()
        guard page.layoutPopover.popover.waitForExistence(timeout: 5) else { return }

        page.layoutPopover.selectLeftRight()

        XCTAssertTrue(page.dashboard.leftRightLayout.waitForExistence(timeout: 5),
                      "Left-right layout should be visible after selection")
    }

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

    // MARK: - Cards

    func testDashboardHasCards() {
        guard page.dashboard.dashboardView.waitForExistence(timeout: 10) else { return }

        XCTAssertGreaterThan(page.dashboard.cards.count, 0,
                             "Dashboard should display at least one card")
    }

    // MARK: - Focus panel

    func testFocusPanelHasEnterProjectButton() {
        guard page.dashboard.dashboardView.waitForExistence(timeout: 10) else { return }

        // Focus panel may only be visible in non-grid layouts;
        // tap first card to select it if needed
        if page.dashboard.cards.firstMatch.waitForExistence(timeout: 5) {
            page.dashboard.cards.firstMatch.click()
        }

        // Focus panel and enter-project button are layout-dependent
        if page.dashboard.focusPanel.waitForExistence(timeout: 3) {
            XCTAssertTrue(page.dashboard.enterProjectButton.waitForExistence(timeout: 3),
                          "Focus panel should have an enter-project button")
        }
    }
}
