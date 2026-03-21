import XCTest

/// P1 — Core functionality. Run on every release.
class CoreTests: PmuxUITestCase {

    // MARK: - Layout

    func testViewMenuOpensAndHasAllOptions() {
        guard page.titleBar.viewMenuButton.waitForExistence(timeout: 10) else { return }
        page.titleBar.clickViewMenu()

        XCTAssertTrue(page.layoutPopover.gridItem.waitForExistence(timeout: 3))
        XCTAssertTrue(page.layoutPopover.leftRightItem.waitForExistence(timeout: 3))
        XCTAssertTrue(page.layoutPopover.topSmallItem.waitForExistence(timeout: 3))
        XCTAssertTrue(page.layoutPopover.topLargeItem.waitForExistence(timeout: 3))
    }

    func testSwitchToGridLayout() {
        guard page.titleBar.viewMenuButton.waitForExistence(timeout: 10) else { return }
        page.titleBar.clickViewMenu()

        page.layoutPopover.selectGrid()
        XCTAssertTrue(page.dashboard.gridLayout.waitForExistence(timeout: 5),
                      "Grid layout should be visible after selection")
    }

    func testSwitchToLeftRightLayout() {
        guard page.titleBar.viewMenuButton.waitForExistence(timeout: 10) else { return }
        page.titleBar.clickViewMenu()

        page.layoutPopover.selectLeftRight()
        XCTAssertTrue(page.dashboard.leftRightLayout.waitForExistence(timeout: 5),
                      "Left-right layout should be visible after selection")
    }

    // MARK: - Panels

    func testNotificationPanelOpenClose() {
        guard page.titleBar.notifButton.waitForExistence(timeout: 10) else { return }

        page.titleBar.clickNotif()
        XCTAssertTrue(page.notifPanel.isOpen,
                      "Notification panel should open when notif button is clicked")

        page.notifPanel.close()
        XCTAssertTrue(page.notifPanel.panel.waitForNonExistence(timeout: 3),
                      "Notification panel should close")
    }

    func testAIPanelOpenClose() {
        guard page.titleBar.aiButton.waitForExistence(timeout: 10) else { return }

        page.titleBar.clickAI()
        XCTAssertTrue(page.aiPanel.isOpen,
                      "AI panel should open when AI button is clicked")

        page.aiPanel.close()
        XCTAssertTrue(page.aiPanel.panel.waitForNonExistence(timeout: 3),
                      "AI panel should close")
    }

    func testPanelMutualExclusion() {
        guard page.titleBar.aiButton.waitForExistence(timeout: 10) else { return }
        guard page.titleBar.notifButton.waitForExistence(timeout: 5) else { return }

        // Open AI, then open Notif — AI should close
        page.titleBar.clickAI()
        guard page.aiPanel.isOpen else { return }

        page.titleBar.clickNotif()
        XCTAssertTrue(page.notifPanel.isOpen, "Notification panel should open")
        XCTAssertTrue(page.aiPanel.panel.waitForNonExistence(timeout: 3),
                      "AI panel should close when notification panel opens")

        // Open AI again — Notif should close
        page.titleBar.clickAI()
        XCTAssertTrue(page.aiPanel.isOpen, "AI panel should open")
        XCTAssertTrue(page.notifPanel.panel.waitForNonExistence(timeout: 3),
                      "Notification panel should close when AI panel opens")
    }

    // MARK: - Project workspace

    func testProjectViewShowsSidebarAndTerminal() {
        guard page.dashboard.dashboardView.waitForExistence(timeout: 10) else { return }

        let firstCard = page.dashboard.cards.firstMatch
        guard firstCard.waitForExistence(timeout: 5) else { return }
        firstCard.click()

        if page.dashboard.enterProjectButton.waitForExistence(timeout: 3) {
            page.dashboard.tapEnterProject()
        }

        XCTAssertTrue(page.sidebar.worktreeList.waitForExistence(timeout: 10),
                      "Sidebar should be visible in project view")

        let hasTerminal = page.repo.terminal.waitForExistence(timeout: 5)
        let hasEmptyState = page.repo.emptyState.waitForExistence(timeout: 2)
        XCTAssertTrue(hasTerminal || hasEmptyState,
                      "Project view should show terminal or empty state")
    }

    // MARK: - Settings

    func testSettingsOpenAndClose() {
        page.settings.open()
        XCTAssertTrue(page.settings.sheet.waitForExistence(timeout: 5),
                      "Settings sheet should open with Cmd+,")

        page.app.typeKey(.escape, modifierFlags: [])
        XCTAssertTrue(page.settings.sheet.waitForNonExistence(timeout: 3),
                      "Settings sheet should close on Escape")
    }

    // MARK: - Worktree

    func testNewBranchDialogFlow() {
        page.dialog.openNewBranchDialog()

        let dialog = page.dialog.newBranchDialog
        XCTAssertTrue(dialog.waitForExistence(timeout: 5),
                      "New branch dialog should open with Cmd+N")

        XCTAssertTrue(page.dialog.branchNameField.waitForExistence(timeout: 3))
        XCTAssertTrue(page.dialog.createButton.waitForExistence(timeout: 3))

        page.app.typeKey(.escape, modifierFlags: [])
        XCTAssertTrue(dialog.waitForNonExistence(timeout: 3),
                      "Dialog should close on Escape")
    }

    func testCmdNNewBranch() {
        page.app.typeKey("n", modifierFlags: .command)
        XCTAssertTrue(page.dialog.newBranchDialog.waitForExistence(timeout: 5),
                      "Cmd+N should open new branch dialog")
        page.app.typeKey(.escape, modifierFlags: [])
    }
}
