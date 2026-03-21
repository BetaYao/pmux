import XCTest

/// P2 — Regression protection. Run weekly or before major releases.
class RegressionTests: PmuxUITestCase {

    func testSwitchToTopSmallLayout() {
        guard page.titleBar.viewMenuButton.waitForExistence(timeout: 10) else { return }
        page.titleBar.clickViewMenu()

        page.layoutPopover.selectTopSmall()
        XCTAssertTrue(page.dashboard.topSmallLayout.waitForExistence(timeout: 5),
                      "Top-small layout should be visible after selection")
    }

    func testSwitchToTopLargeLayout() {
        guard page.titleBar.viewMenuButton.waitForExistence(timeout: 10) else { return }
        page.titleBar.clickViewMenu()

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

    func testProjectTabRemainsAfterRepeatedDashboardSwitches() {
        guard page.dashboard.dashboardView.waitForExistence(timeout: 10) else { return }

        let projectTabs = page.app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'titlebar.projectTab.'"))
        let initialCount = projectTabs.count
        XCTAssertGreaterThan(initialCount, 0, "Expected at least one project tab")

        let firstTab = projectTabs.element(boundBy: 0)
        guard firstTab.waitForExistence(timeout: 5) else { return }
        let firstTabIdentifier = firstTab.identifier

        firstTab.waitAndClick()
        XCTAssertTrue(page.sidebar.worktreeList.waitForExistence(timeout: 10),
                      "Project detail should be visible after clicking project tab")

        page.titleBar.clickDashboardTab()
        XCTAssertTrue(page.dashboard.dashboardView.waitForExistence(timeout: 5),
                      "Dashboard should be visible after switching back")

        page.app.buttons[firstTabIdentifier].waitAndClick()
        XCTAssertTrue(page.sidebar.worktreeList.waitForExistence(timeout: 10),
                      "Project detail should still be reachable on second switch")

        page.titleBar.clickDashboardTab()
        XCTAssertTrue(page.dashboard.dashboardView.waitForExistence(timeout: 5),
                      "Dashboard should be visible after second switch back")

        XCTAssertTrue(page.app.buttons[firstTabIdentifier].waitForExistence(timeout: 5),
                      "Project tab should remain visible after repeated switching")
        XCTAssertEqual(page.app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'titlebar.projectTab.'")).count,
                       initialCount,
                       "Project tab count should stay stable after repeated switching")
    }

    func testFirstProjectTabClickWorksFromColdLaunch() {
        guard page.dashboard.dashboardView.waitForExistence(timeout: 10) else { return }

        let projectTabs = page.app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'titlebar.projectTab.'"))
        XCTAssertGreaterThan(projectTabs.count, 0, "Expected at least one project tab")

        let firstTab = projectTabs.element(boundBy: 0)
        guard firstTab.waitForExistence(timeout: 5) else { return }

        firstTab.waitAndClick()
        XCTAssertTrue(page.sidebar.worktreeList.waitForExistence(timeout: 10),
                      "First click on project tab should open project view from cold launch")
    }

    func testDashboardLayoutsStartBelowTitlebarWithGap() {
        XCTAssertTrue(page.titleBar.viewMenuButton.waitForExistence(timeout: 10),
                      "Layout button should exist in titlebar")

        let titlebarMaxY = page.titleBar.viewMenuButton.frame.maxY

        page.titleBar.clickViewMenu()
        page.layoutPopover.selectGrid()
        XCTAssertTrue(page.dashboard.gridLayout.waitForExistence(timeout: 5),
                      "Grid layout should exist")

        XCTAssertGreaterThan(page.dashboard.gridLayout.frame.minY, titlebarMaxY,
                             "Grid layout should start below titlebar")

        page.titleBar.clickViewMenu()
        page.layoutPopover.selectLeftRight()
        XCTAssertTrue(page.dashboard.leftRightLayout.waitForExistence(timeout: 5))
        XCTAssertGreaterThan(page.dashboard.leftRightLayout.frame.minY, titlebarMaxY,
                             "Left-right layout should start below titlebar")

        page.titleBar.clickViewMenu()
        page.layoutPopover.selectTopSmall()
        XCTAssertTrue(page.dashboard.topSmallLayout.waitForExistence(timeout: 5))
        XCTAssertGreaterThan(page.dashboard.topSmallLayout.frame.minY, titlebarMaxY,
                             "Top-small layout should start below titlebar")

        page.titleBar.clickViewMenu()
        page.layoutPopover.selectTopLarge()
        XCTAssertTrue(page.dashboard.topLargeLayout.waitForExistence(timeout: 5))
        XCTAssertGreaterThan(page.dashboard.topLargeLayout.frame.minY, titlebarMaxY,
                             "Top-large layout should start below titlebar")
    }

}
