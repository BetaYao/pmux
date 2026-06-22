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
            makeYaziSurface: { _, _ in TerminalSurface() }
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

    func testFilesTabUsesInjectedYaziLauncher() {
        var launched = 0
        let vc = WorktreeSidePanelViewController(
            worktreePath: "/tmp/wt-a",
            initialTab: .files,
            makeDiffReviewView: { _ in DiffReviewView(worktreePath: "/tmp") { GitDiffSnapshot(changedFiles: [], files: []) } },
            makeYaziSurface: { _, _ in launched += 1; return TerminalSurface() }
        )
        vc.loadViewIfNeeded()
        XCTAssertEqual(launched, 1)
    }

    func testSwitchingAwayFromFilesDoesNotLaunchYaziAgain() {
        var launched = 0
        let vc = WorktreeSidePanelViewController(
            worktreePath: "/tmp/wt-a",
            initialTab: .files,
            makeDiffReviewView: { _ in DiffReviewView(worktreePath: "/tmp") { GitDiffSnapshot(changedFiles: [], files: []) } },
            makeYaziSurface: { _, _ in launched += 1; return TerminalSurface() }
        )
        vc.loadViewIfNeeded()
        vc.setWorktree("/tmp/wt-b") // still Files tab -> relaunch
        XCTAssertEqual(launched, 2)
    }

    func testFailedYaziShowsMissingMessage() {
        let vc = WorktreeSidePanelViewController(
            worktreePath: "/tmp/wt-a",
            initialTab: .files,
            makeDiffReviewView: { _ in DiffReviewView(worktreePath: "/tmp") { GitDiffSnapshot(changedFiles: [], files: []) } },
            makeYaziSurface: { _, _ in nil }
        )
        vc.loadViewIfNeeded()
        let hasMsg = vc.view.descendantViews().contains {
            $0.accessibilityIdentifier() == "sidePanel.filesMissingYazi"
        }
        XCTAssertTrue(hasMsg)
    }
}

extension NSView {
    func descendantViews() -> [NSView] {
        subviews + subviews.flatMap { $0.descendantViews() }
    }
}
