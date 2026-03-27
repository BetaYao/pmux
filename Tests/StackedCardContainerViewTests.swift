import XCTest
@testable import amux

final class StackedCardContainerViewTests: XCTestCase {

    func testNoPanesProducesNoGhosts() {
        let container = StackedCardContainerView()
        container.configure(paneCount: 1)
        XCTAssertEqual(container.ghostViews.count, 0)
    }

    func testTwoPanesProducesOneGhost() {
        let container = StackedCardContainerView()
        container.configure(paneCount: 2)
        XCTAssertEqual(container.ghostViews.count, 1)
    }

    func testThreePanesProducesTwoGhosts() {
        let container = StackedCardContainerView()
        container.configure(paneCount: 3)
        XCTAssertEqual(container.ghostViews.count, 2)
    }

    func testFivePanesProducesTwoGhosts() {
        let container = StackedCardContainerView()
        container.configure(paneCount: 5)
        XCTAssertEqual(container.ghostViews.count, 2)
    }

    func testGhostsRemovedWhenPaneCountDrops() {
        let container = StackedCardContainerView()
        container.configure(paneCount: 3)
        XCTAssertEqual(container.ghostViews.count, 2)
        container.configure(paneCount: 1)
        XCTAssertEqual(container.ghostViews.count, 0)
        // Verify they were removed from the view hierarchy too
        XCTAssertEqual(container.subviews.count, 1) // only cardView remains
    }

    func testGhostsAreNotInSubviewsWhenZero() {
        let container = StackedCardContainerView()
        container.configure(paneCount: 1)
        // cardView is the only subview
        XCTAssertEqual(container.subviews.count, 1)
        XCTAssertTrue(container.subviews.first === container.cardView)
    }

    func testHitTestOutsideCardViewReturnsNil() {
        let container = StackedCardContainerView()
        container.frame = NSRect(x: 0, y: 0, width: 200, height: 130)
        container.cardView.frame = NSRect(x: 0, y: 0, width: 200, height: 130)
        container.configure(paneCount: 2)
        // Point in ghost overflow zone (below card in AppKit = negative y)
        let ghostPoint = NSPoint(x: 10, y: -5)
        XCTAssertNil(container.hitTest(ghostPoint))
    }

    func testHitTestInsideCardViewReturnsNonNil() {
        let container = StackedCardContainerView()
        container.frame = NSRect(x: 0, y: 0, width: 200, height: 130)
        container.cardView.frame = NSRect(x: 0, y: 0, width: 200, height: 130)
        let centerPoint = NSPoint(x: 100, y: 65)
        XCTAssertNotNil(container.hitTest(centerPoint))
    }

    func testAgentIdForwarding() {
        let container = StackedCardContainerView()
        container.cardView.configure(
            id: "test-id", project: "proj", thread: "main",
            status: "idle", lastMessage: "", totalDuration: "", roundDuration: ""
        )
        XCTAssertEqual(container.agentId, "test-id")
    }

    func testIsSelectedForwarding() {
        let container = StackedCardContainerView()
        container.isSelected = true
        XCTAssertTrue(container.cardView.isSelected)
        container.isSelected = false
        XCTAssertFalse(container.cardView.isSelected)
    }
}
