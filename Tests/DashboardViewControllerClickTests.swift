import XCTest
@testable import amux

final class DashboardViewControllerClickTests: XCTestCase {

    // MARK: - Grid single-click

    func testGridSingleClickUpdatesSelectedAgentId() {
        let vc = DashboardViewController()
        vc.currentLayout = .grid
        vc.agentCardClicked(agentId: "agent-1")
        XCTAssertEqual(vc.selectedAgentId, "agent-1")
    }

    func testGridSingleClickDoesNotChangeLayout() {
        let vc = DashboardViewController()
        vc.currentLayout = .grid
        vc.agentCardClicked(agentId: "agent-1")
        XCTAssertEqual(vc.currentLayout, .grid,
                       "Single click in grid must not switch to another layout")
    }

    func testGridSingleClickDoesNotCallDelegate() {
        let vc = DashboardViewController()
        let spy = DashboardDelegateSpy()
        vc.dashboardDelegate = spy
        vc.currentLayout = .grid
        vc.agentCardClicked(agentId: "agent-1")
        XCTAssertFalse(spy.didSelectProjectCalled,
                       "Single click in grid must not call dashboardDidSelectProject")
    }

    // MARK: - Double-click on unknown agentId (guard path)

    func testDoubleClickWithUnknownAgentIdIsNoop() {
        let vc = DashboardViewController()
        let spy = DashboardDelegateSpy()
        vc.dashboardDelegate = spy
        vc.agentCardDoubleClicked(agentId: "nonexistent")
        XCTAssertFalse(spy.didSelectProjectCalled,
                       "Double click on unknown agentId must not call delegate")
    }
}

// MARK: - Test helpers

private class DashboardDelegateSpy: DashboardDelegate {
    var didSelectProjectCalled = false
    var lastProject: String?
    var lastThread: String?

    func dashboardDidSelectProject(_ project: String, thread: String) {
        didSelectProjectCalled = true
        lastProject = project
        lastThread = thread
    }
    func dashboardDidRequestEnterProject(_ project: String) {}
    func dashboardDidReorderCards(order: [String]) {}
    func dashboardDidRequestDelete(_ terminalID: String) {}
    func dashboardDidRequestAddProject() {}
    func dashboardDidChangeSelection(_ dashboard: DashboardViewController) {}
}
