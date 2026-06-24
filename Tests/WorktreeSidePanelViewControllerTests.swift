// Tests/WorktreeSidePanelViewControllerTests.swift
import XCTest
import AppKit
@testable import seahelm

final class WorktreeSidePanelViewControllerTests: XCTestCase {
    private func makeVC(worktreePath: String?) -> WorktreeSidePanelViewController {
        WorktreeSidePanelViewController(worktreePath: worktreePath)
    }

    func testInitHoldsWorktreePath() {
        let vc = makeVC(worktreePath: "/tmp/wt-a")
        vc.loadViewIfNeeded()
        XCTAssertEqual(vc.worktreePathForTesting, "/tmp/wt-a")
        XCTAssertEqual(vc.selectedTabForTesting, .files)
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

    // MARK: - Delegate spy tests

    private class SpyDelegate: WorktreeSidePanelDelegate {
        var selectedFilePath: String?
        var selectedChangePath: String?

        func sidePanel(_ vc: WorktreeSidePanelViewController, didSelectFile path: String) {
            selectedFilePath = path
        }

        func sidePanel(_ vc: WorktreeSidePanelViewController, didSelectChange path: String) {
            selectedChangePath = path
        }
    }

    func testHandleFileSelectionForwardsToDelegate() {
        let vc = makeVC(worktreePath: "/tmp/wt-a")
        let spy = SpyDelegate()
        vc.delegate = spy
        vc.loadViewIfNeeded()

        vc.handleFileSelection("/x/y.txt")

        XCTAssertEqual(spy.selectedFilePath, "/x/y.txt")
    }

    func testHandleChangeSelectionForwardsToDelegate() {
        let vc = makeVC(worktreePath: "/tmp/wt-a")
        let spy = SpyDelegate()
        vc.delegate = spy
        vc.loadViewIfNeeded()

        vc.handleChangeSelection("/x/z.txt")

        XCTAssertEqual(spy.selectedChangePath, "/x/z.txt")
    }
}
