// Tests/WorktreeSidePanelViewControllerTests.swift
import XCTest
import AppKit
@testable import amux

final class WorktreeSidePanelViewControllerTests: XCTestCase {
    private func makeVC(worktreePath: String?) -> WorktreeSidePanelViewController {
        WorktreeSidePanelViewController(
            worktreePath: worktreePath,
            initialTab: .changes,
            makeDiffReviewView: { _ in DiffReviewView(worktreePath: "/tmp") { GitDiffSnapshot(changedFiles: [], files: []) } },
            makeYaziSurface: { _, _ in true }
        )
    }

    func testInitHoldsWorktreePath() {
        let vc = makeVC(worktreePath: "/tmp/wt-a")
        vc.loadViewIfNeeded()
        XCTAssertEqual(vc.worktreePathForTesting, "/tmp/wt-a")
        XCTAssertEqual(vc.selectedTabForTesting, .changes)
    }

    func testSetWorktreeUpdatesHeldPath() {
        let vc = makeVC(worktreePath: "/tmp/wt-a")
        vc.loadViewIfNeeded()
        vc.setWorktree("/tmp/wt-b")
        XCTAssertEqual(vc.worktreePathForTesting, "/tmp/wt-b")
    }

    func testNilWorktreeShowsPlaceholder() {
        let vc = makeVC(worktreePath: nil)
        vc.loadViewIfNeeded()
        let hasPlaceholder = vc.view.descendantViews().contains {
            $0.accessibilityIdentifier() == "sidePanel.emptyPlaceholder"
        }
        XCTAssertTrue(hasPlaceholder)
    }
}

extension NSView {
    func descendantViews() -> [NSView] {
        subviews + subviews.flatMap { $0.descendantViews() }
    }
}
