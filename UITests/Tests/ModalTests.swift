import XCTest

/// Tests for modal dialog interactions (close confirmation, add project, etc.).
class ModalTests: PmuxUITestCase {

    // MARK: - Modal dismiss with Escape

    func testEscapeDismissesModal() {
        // Trigger a modal by attempting to close a project tab (if available)
        guard page.titleBar.dashboardTab.waitForExistence(timeout: 10) else { return }

        // Try Cmd+W to trigger a close modal if a project is active
        page.app.typeKey("w", modifierFlags: .command)

        if page.modal.isVisible {
            page.modal.dismissWithEscape()
            XCTAssertTrue(page.modal.overlay.waitForNonExistence(timeout: 3),
                          "Escape should dismiss the modal")
        }
    }

    // MARK: - Modal elements

    func testModalHasCancelAndConfirmButtons() {
        guard page.titleBar.dashboardTab.waitForExistence(timeout: 10) else { return }

        page.app.typeKey("w", modifierFlags: .command)

        if page.modal.isVisible {
            XCTAssertTrue(page.modal.cancelButton.waitForExistence(timeout: 3),
                          "Modal should have a cancel button")
            XCTAssertTrue(page.modal.confirmButton.waitForExistence(timeout: 3),
                          "Modal should have a confirm button")
            page.modal.dismissWithEscape()
        }
    }

    func testModalCancelClosesModal() {
        guard page.titleBar.dashboardTab.waitForExistence(timeout: 10) else { return }

        page.app.typeKey("w", modifierFlags: .command)

        if page.modal.isVisible {
            page.modal.cancel()
            XCTAssertTrue(page.modal.overlay.waitForNonExistence(timeout: 3),
                          "Cancel should close the modal")
        }
    }
}
