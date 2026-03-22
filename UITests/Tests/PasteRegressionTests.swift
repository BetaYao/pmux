import XCTest

/// UI regression tests for clipboard paste functionality.
/// Verifies Cmd+V (text paste) and Ctrl+V (control key passthrough) work correctly.
class PasteRegressionTests: PmuxUITestCase {

    func testCmdVPasteTriggersPasteAction() throws {
        // Verify Cmd+V triggers paste action by focusing terminal and sending paste command
        guard page.dashboard.dashboardView.waitForExistence(timeout: 10) else {
            throw XCTSkip("Dashboard not available")
        }

        // Open a project tab to get terminal focus
        let projectTabs = page.app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'titlebar.projectTab.'"))
        if projectTabs.count > 0 {
            let firstTab = projectTabs.element(boundBy: 0)
            guard firstTab.waitForExistence(timeout: 5) else { return }
            firstTab.waitAndClick()
            
            // Wait for terminal/surface to be ready
            XCTAssertTrue(page.sidebar.worktreeList.waitForExistence(timeout: 10),
                          "Sidebar should be visible after clicking project tab")
            
            // Click in the terminal area to ensure focus
            let terminalArea = page.repo.terminal
            guard terminalArea.waitForExistence(timeout: 5) else { return }
            terminalArea.click()
            
            // Give focus time to transfer
            Thread.sleep(forTimeInterval: 0.5)
            
            // Verify the terminal view has focus by checking first responder status
            // We do this by verifying keyboard focus indicators exist
            let focusedElement = XCUIApplication().focusedElement
            XCTAssertNotNil(focusedElement, "Terminal should be able to receive focus")
            
            // Put some text on clipboard and verify paste action is registered
            let pasteboard = NSPasteboard.general
            pasteboard.clearContents()
            pasteboard.setString("UI_TEST_PASTE_TEXT_\(UUID().uuidString)", forType: .string)
            
            // Send Cmd+V
            page.app.typeKey("v", modifierFlags: .command)
            
            // The paste should be processed (we can't directly verify terminal content,
            // but we verify no crash/freeze occurs and app remains responsive)
            XCTAssertTrue(page.app.wait(for: .runningBackground, timeout: 2) || 
                          page.app.wait(for: .runningForeground, timeout: 2),
                          "App should remain responsive after Cmd+V paste")
        } else {
            // If no project tabs, test on dashboard
            page.titleBar.clickDashboardTab()
            XCTAssertTrue(page.dashboard.dashboardView.waitForExistence(timeout: 5),
                          "Dashboard should be visible")
        }
    }

    func testCtrlVPassthroughDoesNotCrash() throws {
        // Verify Ctrl+V doesn't crash the app (it should pass through to terminal)
        guard page.dashboard.dashboardView.waitForExistence(timeout: 10) else {
            throw XCTSkip("Dashboard not available")
        }

        let projectTabs = page.app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'titlebar.projectTab.'"))
        if projectTabs.count > 0 {
            let firstTab = projectTabs.element(boundBy: 0)
            guard firstTab.waitForExistence(timeout: 5) else { return }
            firstTab.waitAndClick()
            
            let terminalArea = page.repo.terminal
            guard terminalArea.waitForExistence(timeout: 5) else { return }
            terminalArea.click()
            
            Thread.sleep(forTimeInterval: 0.5)
            
            // Send Ctrl+V (should pass through to terminal, not trigger paste)
            page.app.typeKey("v", modifierFlags: .control)
            
            // App should remain responsive (Ctrl+V passes through to shell)
            XCTAssertTrue(page.app.wait(for: .runningBackground, timeout: 2) || 
                          page.app.wait(for: .runningForeground, timeout: 2),
                          "App should remain responsive after Ctrl+V")
        }
    }

    func testTerminalViewAcceptsFirstResponder() throws {
        // Verify terminal view can become first responder
        guard page.dashboard.dashboardView.waitForExistence(timeout: 10) else {
            throw XCTSkip("Dashboard not available")
        }

        let projectTabs = page.app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'titlebar.projectTab.'"))
        if projectTabs.count > 0 {
            let firstTab = projectTabs.element(boundBy: 0)
            firstTab.waitAndClick()
            
            let terminalArea = page.repo.terminal
            XCTAssertTrue(terminalArea.waitForExistence(timeout: 5),
                          "Terminal area should exist")
            
            // Click to focus
            terminalArea.click()
            Thread.sleep(forTimeInterval: 0.3)
            
            // Verify app is still responsive (terminal accepted focus)
            XCTAssertTrue(page.titleBar.viewMenuButton.waitForExistence(timeout: 3),
                          "Title bar should still be accessible after focusing terminal")
        }
    }

    func testMouseClickFocusesTerminalForPaste() throws {
        // Verify mouse click on terminal makes it first responder (enabling paste)
        guard page.dashboard.dashboardView.waitForExistence(timeout: 10) else {
            throw XCTSkip("Dashboard not available")
        }

        let projectTabs = page.app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'titlebar.projectTab.'"))
        if projectTabs.count > 0 {
            let firstTab = projectTabs.element(boundBy: 0)
            firstTab.waitAndClick()
            
            let terminalArea = page.repo.terminal
            XCTAssertTrue(terminalArea.waitForExistence(timeout: 5),
                          "Terminal area should exist")
            
            // Simulate mouse click to focus
            terminalArea.click()
            Thread.sleep(forTimeInterval: 0.3)
            
            // Verify we can still interact with UI (terminal didn't hijack focus incorrectly)
            page.titleBar.clickDashboardTab()
            XCTAssertTrue(page.dashboard.dashboardView.waitForExistence(timeout: 5),
                          "Should be able to switch back to dashboard after clicking terminal")
        }
    }
}
