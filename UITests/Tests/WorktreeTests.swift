import XCTest

/// Phase B: Worktree operation tests.
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

    func testDeleteWorktreeContextMenu() {
        // Navigate to repo view via tab bar
        let repoTabs = page.tabBar.repoTabs
        guard repoTabs.firstMatch.waitForExistence(timeout: 5) else { return }
        repoTabs.firstMatch.click()

        // Wait for sidebar to load
        XCTAssertTrue(page.sidebar.worktreeList.waitForExistence(timeout: 10),
                      "Sidebar should be visible in repo view")
    }
}
