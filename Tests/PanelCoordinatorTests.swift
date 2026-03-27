import XCTest
@testable import amux

private class MockPanelCoordinatorDelegate: PanelCoordinatorDelegate {
    var navigateCalled = false
    var lastWorktreePath: String?

    func panelCoordinator(_ coordinator: PanelCoordinator, navigateToWorktreePath path: String) {
        navigateCalled = true
        lastWorktreePath = path
    }
}

final class PanelCoordinatorTests: XCTestCase {

    func testCloseBothPanelsSetsOpenFalse() {
        let coordinator = PanelCoordinator()
        coordinator.closeBothPanels()
        XCTAssertFalse(coordinator.notificationPopover.isShown)
        XCTAssertFalse(coordinator.aiPopover.isShown)
    }

    func testToggleNotificationPanelWithoutTitleBarIsNoop() {
        let coordinator = PanelCoordinator()
        coordinator.titleBar = nil
        // Should not crash when titleBar is nil
        coordinator.toggleNotificationPanel()
    }

    func testToggleAIPanelWithoutTitleBarIsNoop() {
        let coordinator = PanelCoordinator()
        coordinator.titleBar = nil
        // Should not crash when titleBar is nil
        coordinator.toggleAIPanel()
    }
}
