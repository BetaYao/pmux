import XCTest

/// Tests for the project workspace view (sidebar + terminal).
class ProjectWorkspaceTests: PmuxUITestCase {

    func testProjectViewShowsTerminalOrEmptyState() {
        guard page.dashboard.dashboardView.waitForExistence(timeout: 10) else { return }

        // If cards exist, tap one to enter project view
        let firstCard = page.dashboard.cards.firstMatch
        guard firstCard.waitForExistence(timeout: 5) else { return }
        firstCard.click()

        // In non-grid layouts, use the enter-project button if available
        if page.dashboard.enterProjectButton.waitForExistence(timeout: 3) {
            page.dashboard.tapEnterProject()
        }

        // Project view should show either a terminal or empty state
        let terminal = page.repo.terminal
        let emptyState = page.repo.emptyState
        let hasTerminal = terminal.waitForExistence(timeout: 5)
        let hasEmptyState = emptyState.waitForExistence(timeout: 2)

        XCTAssertTrue(hasTerminal || hasEmptyState,
                      "Project view should show terminal or empty state")
    }

    func testProjectViewShowsSidebar() {
        guard page.dashboard.dashboardView.waitForExistence(timeout: 10) else { return }

        let firstCard = page.dashboard.cards.firstMatch
        guard firstCard.waitForExistence(timeout: 5) else { return }
        firstCard.click()

        if page.dashboard.enterProjectButton.waitForExistence(timeout: 3) {
            page.dashboard.tapEnterProject()
        }

        // Sidebar should be visible in project view
        XCTAssertTrue(page.sidebar.worktreeList.waitForExistence(timeout: 10),
                      "Sidebar should be visible in project view")
    }
}
