import XCTest

/// Worktree operation tests adapted for the redesigned UI.
class WorktreeTests: PmuxUITestCase {

    func testNewBranchDialogFlow() {
        // Open new branch dialog via Cmd+N
        page.dialog.openNewBranchDialog()

        let dialog = page.dialog.newBranchDialog
        XCTAssertTrue(dialog.waitForExistence(timeout: 5),
                      "New branch dialog should open with Cmd+N")

        // Verify branch name field exists
        let nameField = page.dialog.branchNameField
        XCTAssertTrue(nameField.waitForExistence(timeout: 3))

        // Verify create button exists
        let createBtn = page.dialog.createButton
        XCTAssertTrue(createBtn.waitForExistence(timeout: 3))

        // Cancel the dialog
        page.app.typeKey(.escape, modifierFlags: [])
        XCTAssertTrue(dialog.waitForNonExistence(timeout: 3),
                      "Dialog should close on Escape")
    }

    func testSidebarVisibleInProjectView() {
        guard page.dashboard.dashboardView.waitForExistence(timeout: 10) else { return }

        let firstCard = page.dashboard.cards.firstMatch
        guard firstCard.waitForExistence(timeout: 5) else { return }
        firstCard.click()

        if page.dashboard.enterProjectButton.waitForExistence(timeout: 3) {
            page.dashboard.tapEnterProject()
        }

        XCTAssertTrue(page.sidebar.worktreeList.waitForExistence(timeout: 10),
                      "Sidebar should be visible in project view")
    }
}
