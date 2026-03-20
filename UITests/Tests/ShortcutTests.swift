import XCTest

/// Phase C: Keyboard shortcut tests.
class ShortcutTests: PmuxUITestCase {

    func testCmdCommaSettings() {
        page.app.typeKey(",", modifierFlags: .command)
        XCTAssertTrue(page.settings.sheet.waitForExistence(timeout: 5),
                      "Cmd+, should open settings")
        page.app.typeKey(.escape, modifierFlags: [])
    }

    func testCmdPQuickSwitcher() {
        page.app.typeKey("p", modifierFlags: .command)
        XCTAssertTrue(page.dialog.quickSwitcher.waitForExistence(timeout: 5),
                      "Cmd+P should open quick switcher")
        page.app.typeKey(.escape, modifierFlags: [])
    }

    func testCmdNNewBranch() {
        page.app.typeKey("n", modifierFlags: .command)
        XCTAssertTrue(page.dialog.newBranchDialog.waitForExistence(timeout: 5),
                      "Cmd+N should open new branch dialog")
        page.app.typeKey(.escape, modifierFlags: [])
    }

    func testCmdDDiffOverlay() {
        // Need to be in repo view for Cmd+D to work
        guard page.dashboard.grid.waitForExistence(timeout: 10) else { return }
        let firstCard = page.dashboard.cards.firstMatch
        guard firstCard.waitForExistence(timeout: 5) else { return }
        firstCard.doubleClick()
        guard page.sidebar.worktreeList.waitForExistence(timeout: 5) else { return }

        // Toggle diff on
        page.app.typeKey("d", modifierFlags: .command)
        XCTAssertTrue(page.repo.diffOverlay.waitForExistence(timeout: 5),
                      "Cmd+D should open diff overlay")

        // Toggle diff off
        page.app.typeKey("d", modifierFlags: .command)
        XCTAssertTrue(page.repo.diffOverlay.waitForNonExistence(timeout: 5),
                      "Cmd+D again should close diff overlay")
    }

    func testCmdShiftDSplitVertical() {
        guard page.dashboard.grid.waitForExistence(timeout: 10) else { return }
        let firstCard = page.dashboard.cards.firstMatch
        guard firstCard.waitForExistence(timeout: 5) else { return }
        firstCard.doubleClick()
        guard page.sidebar.worktreeList.waitForExistence(timeout: 5) else { return }

        page.app.typeKey("d", modifierFlags: [.command, .shift])
        let secondPane = page.repo.pane(at: 1)
        XCTAssertTrue(secondPane.waitForExistence(timeout: 5),
                      "Cmd+Shift+D should create a split pane")
    }

    func testCmdShiftWSplitClose() {
        guard page.dashboard.grid.waitForExistence(timeout: 10) else { return }
        let firstCard = page.dashboard.cards.firstMatch
        guard firstCard.waitForExistence(timeout: 5) else { return }
        firstCard.doubleClick()
        guard page.sidebar.worktreeList.waitForExistence(timeout: 5) else { return }

        // First split, then close
        page.app.typeKey("d", modifierFlags: [.command, .shift])
        let secondPane = page.repo.pane(at: 1)
        guard secondPane.waitForExistence(timeout: 5) else { return }

        page.app.typeKey("w", modifierFlags: [.command, .shift])
        XCTAssertTrue(secondPane.waitForNonExistence(timeout: 5),
                      "Cmd+Shift+W should close the split pane")
    }

    func testEscClosesDialog() {
        // Open quick switcher
        page.app.typeKey("p", modifierFlags: .command)
        XCTAssertTrue(page.dialog.quickSwitcher.waitForExistence(timeout: 5))

        // Escape should close it
        page.app.typeKey(.escape, modifierFlags: [])
        XCTAssertTrue(page.dialog.quickSwitcher.waitForNonExistence(timeout: 3),
                      "Escape should close the quick switcher")
    }
}
