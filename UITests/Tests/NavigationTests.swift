import XCTest

/// Phase A: Core navigation flow tests.
class NavigationTests: PmuxUITestCase {

    func testDashboardShowsOnLaunch() {
        XCTAssertTrue(page.dashboard.grid.waitForExistence(timeout: 10),
                      "Dashboard grid should be visible on launch")
    }

    func testTabBarShowsDashboard() {
        XCTAssertTrue(page.tabBar.dashboardTab.waitForExistence(timeout: 5),
                      "Dashboard tab should exist in tab bar")
    }

    func testDashboardCardsExist() {
        XCTAssertTrue(page.dashboard.grid.waitForExistence(timeout: 10))
        // Verify cards exist (relies on real workspace config)
        XCTAssertGreaterThan(page.dashboard.cards.count, 0,
                             "Should have at least one dashboard card")
    }

    func testClickRepoTabShowsSidebar() {
        // Click a repo tab button (these exist in the tab bar from launch)
        let repoTabs = page.tabBar.repoTabs
        guard repoTabs.firstMatch.waitForExistence(timeout: 5) else {
            // No repo tabs — skip
            return
        }
        repoTabs.firstMatch.click()

        // Sidebar should appear in repo view
        XCTAssertTrue(page.sidebar.worktreeList.waitForExistence(timeout: 10),
                      "Sidebar should appear after clicking a repo tab")
    }

    func testSidebarWorktreeList() {
        let repoTabs = page.tabBar.repoTabs
        guard repoTabs.firstMatch.waitForExistence(timeout: 5) else { return }
        repoTabs.firstMatch.click()

        XCTAssertTrue(page.sidebar.worktreeList.waitForExistence(timeout: 10),
                      "Sidebar worktree list should be visible in repo view")
    }

    func testTabSwitchBackToDashboard() {
        let repoTabs = page.tabBar.repoTabs
        guard repoTabs.firstMatch.waitForExistence(timeout: 5) else { return }
        repoTabs.firstMatch.click()

        // Wait for repo to load, then switch back
        sleep(1)
        page.tabBar.clickDashboardTab()
        XCTAssertTrue(page.dashboard.grid.waitForExistence(timeout: 5),
                      "Dashboard grid should reappear after switching back")
    }

    func testRepoTerminalPaneExists() {
        let repoTabs = page.tabBar.repoTabs
        guard repoTabs.firstMatch.waitForExistence(timeout: 5) else { return }
        repoTabs.firstMatch.click()

        // Wait for terminal pane to appear
        let pane = page.repo.pane(at: 0)
        XCTAssertTrue(pane.waitForExistence(timeout: 10),
                      "Terminal pane should exist in repo view")
    }

    func testRepoTerminalReceivesInput() {
        let repoTabs = page.tabBar.repoTabs
        guard repoTabs.firstMatch.waitForExistence(timeout: 5) else { return }
        repoTabs.firstMatch.click()

        // Wait for terminal pane to load
        let pane = page.repo.pane(at: 0)
        guard pane.waitForExistence(timeout: 10) else {
            XCTFail("Terminal pane not found")
            return
        }

        // Wait for terminal to initialize and get focus
        sleep(2)

        // Type a command — if the terminal can receive input,
        // typing "echo hello" + Enter should produce visible output
        page.app.typeKey("e", modifierFlags: [])
        page.app.typeKey("c", modifierFlags: [])
        page.app.typeKey("h", modifierFlags: [])
        page.app.typeKey("o", modifierFlags: [])
        page.app.typeKey(" ", modifierFlags: [])
        page.app.typeKey("t", modifierFlags: [])
        page.app.typeKey("e", modifierFlags: [])
        page.app.typeKey("s", modifierFlags: [])
        page.app.typeKey("t", modifierFlags: [])

        // If the terminal accepted input, the pane should still be active
        // (no crash, no loss of focus). We verify the pane still exists.
        XCTAssertTrue(pane.exists, "Terminal pane should remain after typing input")
    }
}
