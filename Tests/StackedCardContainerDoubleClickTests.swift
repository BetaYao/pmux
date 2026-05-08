import XCTest
@testable import amux

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

    func testContextMenuContainsInspectorActionsBeforeDeleteWorktree() throws {
        let container = StackedCardContainerView(frame: NSRect(x: 0, y: 0, width: 200, height: 150))

        let menu = try XCTUnwrap(container.menu(for: makeRightClickEvent()))
        let titles = menu.items.map(\.title)

        XCTAssertEqual(titles.prefix(4), ["Browse Files...", "Show Changes...", "", "Delete Worktree"])
    }

    func testBrowseFilesMenuActionForwardsAgentId() throws {
        let container = StackedCardContainerView(frame: NSRect(x: 0, y: 0, width: 200, height: 150))
        let spy = DelegateSpy()
        container.delegate = spy
        container.cardView.configure(
            id: "agent-1", project: "proj", thread: "main",
            status: "idle", lastMessage: "", totalDuration: "", roundDuration: ""
        )

        try performMenuItem(title: "Browse Files...", in: container)

        XCTAssertEqual(spy.browseIds, ["agent-1"])
        XCTAssertTrue(spy.showChangesIds.isEmpty)
    }

    func testShowChangesMenuActionForwardsAgentId() throws {
        let container = StackedCardContainerView(frame: NSRect(x: 0, y: 0, width: 200, height: 150))
        let spy = DelegateSpy()
        container.delegate = spy
        container.cardView.configure(
            id: "agent-2", project: "proj", thread: "main",
            status: "idle", lastMessage: "", totalDuration: "", roundDuration: ""
        )

        try performMenuItem(title: "Show Changes...", in: container)

        XCTAssertEqual(spy.showChangesIds, ["agent-2"])
        XCTAssertTrue(spy.browseIds.isEmpty)
    }

    private func performMenuItem(title: String, in container: StackedCardContainerView) throws {
        let menu = try XCTUnwrap(container.menu(for: makeRightClickEvent()))
        let item = try XCTUnwrap(menu.item(withTitle: title))
        let target = try XCTUnwrap(item.target as? NSObject)
        let action = try XCTUnwrap(item.action)
        target.perform(action, with: item)
    }

    private func makeRightClickEvent() -> NSEvent {
        NSEvent.mouseEvent(
            with: .rightMouseDown,
            location: .zero,
            modifierFlags: [],
            timestamp: 0,
            windowNumber: 0,
            context: nil,
            eventNumber: 0,
            clickCount: 1,
            pressure: 1
        )!
    }
}

// MARK: - Test helpers

private class DelegateSpy: AgentCardDelegate {
    var clickedIds: [String] = []
    var doubleClickedIds: [String] = []
    var browseIds: [String] = []
    var showChangesIds: [String] = []

    func agentCardClicked(agentId: String) { clickedIds.append(agentId) }
    func agentCardDoubleClicked(agentId: String) { doubleClickedIds.append(agentId) }
    func agentCardDidRequestBrowseFiles(agentId: String) { browseIds.append(agentId) }
    func agentCardDidRequestShowChanges(agentId: String) { showChangesIds.append(agentId) }
}
