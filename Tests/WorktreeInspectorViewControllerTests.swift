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
            yaziAvailability: { false },
            makeDiffReviewView: { path in
                DiffReviewView(
                    worktreePath: path,
                    loadSnapshot: { GitDiffSnapshot(changedFiles: [], files: []) }
                )
            }
        )

        vc.loadViewIfNeeded()

        XCTAssertEqual(vc.selectedTabForTesting, .changes)
        XCTAssertNotNil(vc.view.viewWithAccessibilityIdentifier("diffReview"))
    }

    func testChangesTabEmbedsDiffReviewView() {
        let vc = WorktreeInspectorViewController(
            worktreePath: "/repo/project",
            initialTab: .changes,
            yaziAvailability: { false },
            makeDiffReviewView: { path in
                XCTAssertEqual(path, "/repo/project")
                return DiffReviewView(
                    worktreePath: path,
                    loadSnapshot: { GitDiffSnapshot(changedFiles: [], files: []) }
                )
            }
        )

        vc.loadViewIfNeeded()

        XCTAssertNotNil(vc.view.viewWithAccessibilityIdentifier("diffReview"))
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
