import XCTest

/// Phase B: Split pane and diff overlay tests.
class SplitPaneTests: PmuxUITestCase {

    private func navigateToRepo() -> Bool {
        guard page.dashboard.grid.waitForExistence(timeout: 10) else { return false }
        let firstCard = page.dashboard.cards.firstMatch
        guard firstCard.waitForExistence(timeout: 5) else { return false }
        firstCard.doubleClick()
        return page.sidebar.worktreeList.waitForExistence(timeout: 5)
    }

    func testDiffOverlayToggle() {
        guard navigateToRepo() else { return }

        // Open diff overlay with Cmd+D
        page.repo.toggleDiff()
        XCTAssertTrue(page.repo.diffOverlay.waitForExistence(timeout: 5),
                      "Diff overlay should appear with Cmd+D")

        // Close diff overlay with Cmd+D again
        page.repo.toggleDiff()
        XCTAssertTrue(page.repo.diffOverlay.waitForNonExistence(timeout: 5),
                      "Diff overlay should close with Cmd+D")
    }

    func testSplitPaneCreation() {
        guard navigateToRepo() else { return }

        // Initial state: one pane
        let initialPane = page.repo.pane(at: 0)
        XCTAssertTrue(initialPane.waitForExistence(timeout: 5),
                      "Initial terminal pane should exist")

        // Split vertical with Cmd+Shift+D
        page.repo.splitVertical()

        // After split, there should be a second pane
        let secondPane = page.repo.pane(at: 1)
        XCTAssertTrue(secondPane.waitForExistence(timeout: 5),
                      "Second pane should appear after vertical split")
    }
}
