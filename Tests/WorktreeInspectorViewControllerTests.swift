import XCTest
@testable import amux

final class WorktreeInspectorViewControllerTests: XCTestCase {
    func testInitialTabFilesSelectsFilesSegment() {
        let vc = WorktreeInspectorViewController(
            worktreePath: "/repo/project",
            initialTab: .files,
            yaziAvailability: { false }
        )

        vc.loadViewIfNeeded()

        XCTAssertEqual(vc.selectedTabForTesting, .files)
        XCTAssertNotNil(vc.view.viewWithAccessibilityIdentifier("worktreeInspector.filesMissingYazi"))
    }

    func testInitialTabChangesSelectsChangesSegment() {
        let vc = WorktreeInspectorViewController(
            worktreePath: "/repo/project",
            initialTab: .changes,
            yaziAvailability: { false }
        )

        vc.loadViewIfNeeded()

        XCTAssertEqual(vc.selectedTabForTesting, .changes)
        XCTAssertNotNil(vc.view.viewWithAccessibilityIdentifier("worktreeInspector.changesPlaceholder"))
    }
}

private extension NSView {
    func viewWithAccessibilityIdentifier(_ identifier: String) -> NSView? {
        if accessibilityIdentifier() == identifier { return self }
        for subview in subviews {
            if let found = subview.viewWithAccessibilityIdentifier(identifier) {
                return found
            }
        }
        return nil
    }
}
