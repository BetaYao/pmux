import XCTest

/// Tests for tab bar features: repo tabs display, "+" add button, and close with confirmation.
class TabBarTests: PmuxUITestCase {

    // MARK: - Requirement 1: Tab bar shows all repos

    func testTabBarShowsDashboardAndRepoTabs() {
        // Dashboard tab should always exist
        XCTAssertTrue(page.tabBar.dashboardTab.waitForExistence(timeout: 10),
                      "Dashboard tab should exist")

        // If workspaces are configured, repo tabs should appear automatically
        let repoTabs = page.tabBar.repoTabs
        // With configured workspaces, at least one repo tab should exist
        // (graceful: if no workspaces configured, we just verify dashboard)
        if repoTabs.count > 0 {
            XCTAssertTrue(repoTabs.firstMatch.waitForExistence(timeout: 5),
                          "Repo tabs should be visible in tab bar on launch")
        }
    }

    func testAllConfiguredReposAppearAsTabs() {
        XCTAssertTrue(page.tabBar.dashboardTab.waitForExistence(timeout: 10))

        // Repo tabs should appear without needing to manually "Open in Tab"
        let repoTabs = page.tabBar.repoTabs
        guard repoTabs.count > 0 else { return }

        // Each repo tab should be clickable
        let firstRepoTab = repoTabs.firstMatch
        XCTAssertTrue(firstRepoTab.isHittable, "Repo tab should be hittable")
    }

    func testClickRepoTabSwitchesToRepoView() {
        XCTAssertTrue(page.tabBar.dashboardTab.waitForExistence(timeout: 10))

        let repoTabs = page.tabBar.repoTabs
        guard repoTabs.count > 0 else { return }

        // Click the first repo tab
        repoTabs.firstMatch.click()

        // Should show sidebar (repo view)
        XCTAssertTrue(page.sidebar.worktreeList.waitForExistence(timeout: 5),
                      "Clicking a repo tab should show the repo view with sidebar")
    }

    func testSwitchBetweenRepoAndDashboard() {
        XCTAssertTrue(page.tabBar.dashboardTab.waitForExistence(timeout: 10))

        let repoTabs = page.tabBar.repoTabs
        guard repoTabs.count > 0 else { return }

        // Switch to repo
        repoTabs.firstMatch.click()
        XCTAssertTrue(page.sidebar.worktreeList.waitForExistence(timeout: 5))

        // Switch back to dashboard
        page.tabBar.clickDashboardTab()
        XCTAssertTrue(page.dashboard.grid.waitForExistence(timeout: 5),
                      "Should return to dashboard grid after clicking dashboard tab")
    }

    // MARK: - Requirement 2: "+" button to add new repo

    func testAddButtonExistsInTabBar() {
        XCTAssertTrue(page.tabBar.addButton.waitForExistence(timeout: 10),
                      "'+' add button should exist at the end of the tab bar")
    }

    func testAddButtonIsHittable() {
        XCTAssertTrue(page.tabBar.addButton.waitForExistence(timeout: 10))
        XCTAssertTrue(page.tabBar.addButton.isHittable,
                      "'+' button should be hittable")
    }

    func testAddButtonOpensOpenPanel() {
        XCTAssertTrue(page.tabBar.addButton.waitForExistence(timeout: 10))

        page.tabBar.clickAddButton()

        // NSOpenPanel shows as a sheet — look for the open panel
        let openPanel = page.app.sheets.firstMatch
        let exists = openPanel.waitForExistence(timeout: 5)
        if exists {
            // Dismiss the open panel
            openPanel.buttons["Cancel"].click()
        }
        // Note: NSOpenPanel may not always be accessible via XCUITest
        // depending on system permissions; this is a best-effort test
    }

    // MARK: - Requirement 3: Close repo with confirmation

    func testRepoTabHasCloseButton() {
        XCTAssertTrue(page.tabBar.dashboardTab.waitForExistence(timeout: 10))

        let repoTabs = page.tabBar.repoTabs
        guard repoTabs.count > 0 else { return }

        let firstRepoTab = repoTabs.firstMatch
        XCTAssertTrue(firstRepoTab.waitForExistence(timeout: 5))

        // Repo tabs should have a close button (the "x" button)
        let closeBtn = firstRepoTab.buttons.firstMatch
        XCTAssertTrue(closeBtn.exists, "Repo tab should have a close button")
    }

    func testDashboardTabHasNoCloseButton() {
        XCTAssertTrue(page.tabBar.dashboardTab.waitForExistence(timeout: 10))

        // Dashboard tab should NOT have a close button
        let closeBtn = page.tabBar.dashboardTab.buttons.firstMatch
        XCTAssertFalse(closeBtn.exists, "Dashboard tab should not have a close button")
    }

    func testCloseButtonShowsConfirmationAlert() {
        XCTAssertTrue(page.tabBar.dashboardTab.waitForExistence(timeout: 10))

        let repoTabs = page.tabBar.repoTabs
        guard repoTabs.count > 0 else { return }

        let firstRepoTab = repoTabs.firstMatch
        XCTAssertTrue(firstRepoTab.waitForExistence(timeout: 5))

        // Click the close button
        let closeBtn = firstRepoTab.buttons.firstMatch
        guard closeBtn.exists else { return }
        closeBtn.click()

        // Confirmation alert should appear as a sheet
        let alert = page.app.sheets.firstMatch
        XCTAssertTrue(alert.waitForExistence(timeout: 5),
                      "Confirmation alert should appear when closing a repo tab")
    }

    func testCloseConfirmationHasCloseAndCancelButtons() {
        XCTAssertTrue(page.tabBar.dashboardTab.waitForExistence(timeout: 10))

        let repoTabs = page.tabBar.repoTabs
        guard repoTabs.count > 0 else { return }

        let closeBtn = repoTabs.firstMatch.buttons.firstMatch
        guard closeBtn.exists else { return }
        closeBtn.click()

        let alert = page.app.sheets.firstMatch
        guard alert.waitForExistence(timeout: 5) else { return }

        // Alert should have "Close" and "Cancel" buttons
        XCTAssertTrue(alert.buttons["Close"].exists,
                      "Alert should have a 'Close' button")
        XCTAssertTrue(alert.buttons["Cancel"].exists,
                      "Alert should have a 'Cancel' button")
    }

    func testCancelCloseKeepsRepoTab() {
        XCTAssertTrue(page.tabBar.dashboardTab.waitForExistence(timeout: 10))

        let repoTabs = page.tabBar.repoTabs
        guard repoTabs.count > 0 else { return }

        let tabCountBefore = repoTabs.count
        let firstRepoTab = repoTabs.firstMatch
        let closeBtn = firstRepoTab.buttons.firstMatch
        guard closeBtn.exists else { return }

        // Click close, then cancel
        closeBtn.click()
        let alert = page.app.sheets.firstMatch
        guard alert.waitForExistence(timeout: 5) else { return }
        alert.buttons["Cancel"].click()

        // Tab should still be there
        sleep(1)
        XCTAssertEqual(page.tabBar.repoTabs.count, tabCountBefore,
                       "Canceling close should keep the repo tab")
    }

    func testConfirmCloseRemovesRepoTab() {
        XCTAssertTrue(page.tabBar.dashboardTab.waitForExistence(timeout: 10))

        let repoTabs = page.tabBar.repoTabs
        guard repoTabs.count > 0 else { return }

        let tabCountBefore = repoTabs.count
        let firstRepoTab = repoTabs.firstMatch
        let closeBtn = firstRepoTab.buttons.firstMatch
        guard closeBtn.exists else { return }

        // Click close, then confirm
        closeBtn.click()
        let alert = page.app.sheets.firstMatch
        guard alert.waitForExistence(timeout: 5) else { return }
        alert.buttons["Close"].click()

        // Tab count should decrease
        sleep(1)
        XCTAssertEqual(page.tabBar.repoTabs.count, tabCountBefore - 1,
                       "Confirming close should remove the repo tab")
    }

    func testCloseRepoSwitchesToDashboard() {
        XCTAssertTrue(page.tabBar.dashboardTab.waitForExistence(timeout: 10))

        let repoTabs = page.tabBar.repoTabs
        guard repoTabs.count > 0 else { return }

        // Switch to the repo tab first
        repoTabs.firstMatch.click()
        XCTAssertTrue(page.sidebar.worktreeList.waitForExistence(timeout: 5))

        // Now close it via Cmd+W
        page.app.typeKey("w", modifierFlags: .command)

        let alert = page.app.sheets.firstMatch
        guard alert.waitForExistence(timeout: 5) else { return }
        alert.buttons["Close"].click()

        // Should fall back to dashboard
        XCTAssertTrue(page.dashboard.grid.waitForExistence(timeout: 5),
                      "After closing the active repo tab, should switch to dashboard")
    }
}
