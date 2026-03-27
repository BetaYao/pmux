// Tests/StackedMiniCardContainerViewTests.swift
import XCTest
@testable import amux

final class StackedMiniCardContainerViewTests: XCTestCase {

    func testNoPanesProducesNoGhosts() {
        let container = StackedMiniCardContainerView()
        container.configure(paneCount: 1)
        XCTAssertEqual(container.ghostViews.count, 0)
    }

    func testTwoPanesProducesOneGhost() {
        let container = StackedMiniCardContainerView()
        container.configure(paneCount: 2)
        XCTAssertEqual(container.ghostViews.count, 1)
    }

    func testThreePanesProducesTwoGhosts() {
        let container = StackedMiniCardContainerView()
        container.configure(paneCount: 3)
        XCTAssertEqual(container.ghostViews.count, 2)
    }

    func testFivePanesCapsAtTwoGhosts() {
        let container = StackedMiniCardContainerView()
        container.configure(paneCount: 5)
        XCTAssertEqual(container.ghostViews.count, 2)
    }

    func testGhostsRemovedWhenPaneCountDrops() {
        let container = StackedMiniCardContainerView()
        container.configure(paneCount: 3)
        XCTAssertEqual(container.ghostViews.count, 2)
        container.configure(paneCount: 1)
        XCTAssertEqual(container.ghostViews.count, 0)
        XCTAssertEqual(container.subviews.count, 1)
    }

    func testGhostOffset3px() {
        let container = StackedMiniCardContainerView()
        container.frame = NSRect(x: 0, y: 0, width: 220, height: 128)
        container.configure(paneCount: 3)
        container.layoutChildren()
        XCTAssertEqual(container.miniCardView.frame.origin.x, 0)
        XCTAssertEqual(container.miniCardView.frame.origin.y, 6)
        XCTAssertEqual(container.miniCardView.frame.width, 214)
        XCTAssertEqual(container.miniCardView.frame.height, 122)
        XCTAssertEqual(container.ghostViews[0].frame.origin.x, 3)
        XCTAssertEqual(container.ghostViews[0].frame.origin.y, 3)
        XCTAssertEqual(container.ghostViews[1].frame.origin.x, 6)
        XCTAssertEqual(container.ghostViews[1].frame.origin.y, 0)
    }

    func testHitTestOutsideMiniCardReturnsNil() {
        let container = StackedMiniCardContainerView()
        container.frame = NSRect(x: 0, y: 0, width: 220, height: 128)
        container.miniCardView.frame = NSRect(x: 0, y: 6, width: 214, height: 122)
        container.configure(paneCount: 2)
        let ghostPoint = NSPoint(x: 10, y: 2)
        XCTAssertNil(container.hitTest(ghostPoint))
    }

    func testAgentIdForwarding() {
        let container = StackedMiniCardContainerView()
        container.miniCardView.configure(
            id: "test-id", project: "proj", thread: "main",
            status: "idle", lastMessage: "", totalDuration: "", roundDuration: ""
        )
        XCTAssertEqual(container.agentId, "test-id")
    }

    func testIsSelectedForwarding() {
        let container = StackedMiniCardContainerView()
        container.isSelected = true
        XCTAssertTrue(container.miniCardView.isSelected)
        container.isSelected = false
        XCTAssertFalse(container.miniCardView.isSelected)
    }
}
