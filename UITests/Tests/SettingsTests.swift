import XCTest

/// Phase B: Settings UI tests.
class SettingsTests: PmuxUITestCase {

    func testSettingsOpenAndClose() {
        // Open settings with Cmd+,
        page.settings.open()

        let sheet = page.settings.sheet
        XCTAssertTrue(sheet.waitForExistence(timeout: 5),
                      "Settings sheet should open with Cmd+,")

        // Close with Escape
        page.app.typeKey(.escape, modifierFlags: [])
        XCTAssertTrue(sheet.waitForNonExistence(timeout: 3),
                      "Settings sheet should close on Escape")
    }

    func testSettingsWorkspacePaths() {
        page.settings.open()
        XCTAssertTrue(page.settings.sheet.waitForExistence(timeout: 5))

        // Verify workspace paths list exists
        XCTAssertTrue(page.settings.workspacePaths.waitForExistence(timeout: 3),
                      "Workspace paths table should be visible")

        // Verify add/remove buttons exist
        XCTAssertTrue(page.settings.addPathButton.waitForExistence(timeout: 3),
                      "Add path button should exist")
        XCTAssertTrue(page.settings.removePathButton.waitForExistence(timeout: 3),
                      "Remove path button should exist")

        // Clean up
        page.app.typeKey(.escape, modifierFlags: [])
    }
}
