import XCTest
@testable import amux

private class MockTabCoordinatorDelegate: TabCoordinatorDelegate {
    var embeddedVC: NSViewController?
    var switchTabCalled = false
    var updateTitleBarCalled = false
    var showNewBranchCalled = false
    var showDiffPath: String?
    var clearContentCalled = false

    func tabCoordinator(_ coordinator: TabCoordinator, embedViewController vc: NSViewController) {
        embeddedVC = vc
    }
    func tabCoordinatorDidSwitchTab(_ coordinator: TabCoordinator) {
        switchTabCalled = true
    }
    func tabCoordinatorRequestUpdateTitleBar(_ coordinator: TabCoordinator) {
        updateTitleBarCalled = true
    }
    func tabCoordinatorRequestShowNewBranchDialog(_ coordinator: TabCoordinator) {
        showNewBranchCalled = true
    }
    func tabCoordinatorRequestShowDiff(_ coordinator: TabCoordinator, worktreePath: String) {
        showDiffPath = worktreePath
    }
    func tabCoordinatorRequestClearContentContainer(_ coordinator: TabCoordinator) {
        clearContentCalled = true
    }
}

final class TabCoordinatorTests: XCTestCase {

    func testInitialActiveTabIsZero() {
        let coordinator = TabCoordinator(config: Config())
        XCTAssertEqual(coordinator.activeTabIndex, 0)
    }

    func testSwitchToSameTabIsNoop() {
        let coordinator = TabCoordinator(config: Config())
        let mockDelegate = MockTabCoordinatorDelegate()
        coordinator.delegate = mockDelegate
        coordinator.switchToTab(0)
        XCTAssertFalse(mockDelegate.switchTabCalled)
    }

    func testBuildAgentDisplayInfosEmptyByDefault() {
        let coordinator = TabCoordinator(config: Config())
        coordinator.terminalCoordinator = TerminalCoordinator(config: Config(), currentRepoVC: { nil })
        coordinator.statusAggregator = WorktreeStatusAggregator()
        let infos = coordinator.buildAgentDisplayInfos()
        XCTAssertTrue(infos.isEmpty)
    }

    func testWorktreeDidDeleteRemovesFromList() {
        let coordinator = TabCoordinator(config: Config())
        coordinator.terminalCoordinator = TerminalCoordinator(config: Config(), currentRepoVC: { nil })
        coordinator.statusPublisher = StatusPublisher(agentConfig: Config().agentDetect)
        coordinator.statusAggregator = WorktreeStatusAggregator()
        let info = WorktreeInfo(path: "/tmp/test-wt", branch: "feature", commitHash: "", isMainWorktree: false)
        let tree = SplitTree(worktreePath: info.path, rootLeafId: "leaf-1", surfaceId: "surface-1", sessionName: "test")
        coordinator.allWorktrees.append((info: info, tree: tree))

        coordinator.worktreeDidDelete(info)
        XCTAssertTrue(coordinator.allWorktrees.isEmpty)
    }

    func testCurrentRepoVCNilAtDashboard() {
        let coordinator = TabCoordinator(config: Config())
        XCTAssertNil(coordinator.currentRepoVC)
    }
}
