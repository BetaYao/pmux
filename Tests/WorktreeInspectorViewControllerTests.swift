import XCTest
@testable import amux

final class WorktreeInspectorViewControllerTests: XCTestCase {
    func testYaziCommandUsesCurrentDirectory() {
        XCTAssertEqual(WorktreeInspectorViewController.yaziCommand, "yazi .")
    }

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

    func testFilesTabShowsYaziContainerWhenAvailable() {
        var capturedPath: String?
        var capturedCommand: String?

        let vc = WorktreeInspectorViewController(
            worktreePath: "/repo/project",
            initialTab: .files,
            yaziAvailability: { true },
            makeDiffReviewView: { path in
                DiffReviewView(
                    worktreePath: path,
                    loadSnapshot: { GitDiffSnapshot(changedFiles: [], files: []) }
                )
            },
            createYaziSurface: { _, path, command in
                capturedPath = path
                capturedCommand = command
                return true
            }
        )

        vc.loadViewIfNeeded()

        XCTAssertNotNil(vc.view.viewWithAccessibilityIdentifier("worktreeInspector.yaziContainer"))
        XCTAssertEqual(capturedPath, "/repo/project")
        XCTAssertEqual(capturedCommand, "yazi .")
    }

    func testFilesTabShowsFailureMessageWhenYaziSurfaceCannotStart() {
        let vc = WorktreeInspectorViewController(
            worktreePath: "/repo/project",
            initialTab: .files,
            yaziAvailability: { true },
            makeDiffReviewView: { path in
                DiffReviewView(
                    worktreePath: path,
                    loadSnapshot: { GitDiffSnapshot(changedFiles: [], files: []) }
                )
            },
            createYaziSurface: { _, _, _ in false }
        )

        vc.loadViewIfNeeded()

        XCTAssertNil(vc.view.viewWithAccessibilityIdentifier("worktreeInspector.yaziContainer"))
        XCTAssertNotNil(vc.view.viewWithAccessibilityIdentifier("worktreeInspector.filesYaziFailed"))
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
