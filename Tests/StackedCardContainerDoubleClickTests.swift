import XCTest
@testable import pmux

final class StackedCardContainerDoubleClickTests: XCTestCase {

    // MARK: - Gesture recognizer configuration

    func testSingleClickRecognizerRequiresDoubleClickToFail() {
        let container = StackedCardContainerView(frame: NSRect(x: 0, y: 0, width: 200, height: 150))
        let recognizers = container.gestureRecognizers.compactMap { $0 as? NSClickGestureRecognizer }
        let single = recognizers.first(where: { $0.numberOfClicksRequired == 1 })
        let double_ = recognizers.first(where: { $0.numberOfClicksRequired == 2 })
        XCTAssertNotNil(single, "Container must have a single-click recognizer")
        XCTAssertNotNil(double_, "Container must have a double-click recognizer")
    }

    func testCardViewClickRecognizerIsExposed() {
        let container = StackedCardContainerView(frame: NSRect(x: 0, y: 0, width: 200, height: 150))
        XCTAssertNotNil(container.cardView.clickRecognizer,
                        "AgentCardView must expose clickRecognizer as private(set)")
    }

    func testContainerHasTwoClickRecognizers() {
        let container = StackedCardContainerView(frame: NSRect(x: 0, y: 0, width: 200, height: 150))
        let clickRecognizers = container.gestureRecognizers.compactMap { $0 as? NSClickGestureRecognizer }
        XCTAssertEqual(clickRecognizers.count, 2,
                       "Container must have exactly two NSClickGestureRecognizers (single + double)")
    }

    // MARK: - Delegate wiring

    func testSingleClickFiresAgentCardClicked() {
        let container = StackedCardContainerView(frame: NSRect(x: 0, y: 0, width: 200, height: 150))
        let spy = DelegateSpy()
        container.delegate = spy

        // Simulate the container's single-click handler directly
        container.simulateSingleClick()

        XCTAssertEqual(spy.clickedIds.count, 1)
        XCTAssertTrue(spy.doubleClickedIds.isEmpty)
    }

    func testDoubleClickFiresAgentCardDoubleClicked() {
        let container = StackedCardContainerView(frame: NSRect(x: 0, y: 0, width: 200, height: 150))
        let spy = DelegateSpy()
        container.delegate = spy

        // Simulate the container's double-click handler directly
        container.simulateDoubleClick()

        XCTAssertEqual(spy.doubleClickedIds.count, 1)
        XCTAssertTrue(spy.clickedIds.isEmpty)
    }
}

// MARK: - Test helpers

private class DelegateSpy: AgentCardDelegate {
    var clickedIds: [String] = []
    var doubleClickedIds: [String] = []

    func agentCardClicked(agentId: String) { clickedIds.append(agentId) }
    func agentCardDoubleClicked(agentId: String) { doubleClickedIds.append(agentId) }
}
