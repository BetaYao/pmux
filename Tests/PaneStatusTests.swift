import XCTest
@testable import pmux

final class PaneStatusTests: XCTestCase {

    func testWorktreeStatusStatuses() {
        let ws = WorktreeStatus(
            worktreePath: "/repo/main",
            panes: [
                PaneStatus(paneIndex: 1, terminalID: "t1", status: .running, lastMessage: "building", lastUpdated: Date()),
                PaneStatus(paneIndex: 2, terminalID: "t2", status: .idle, lastMessage: "done", lastUpdated: Date()),
            ],
            mostRecentPaneIndex: 1,
            mostRecentMessage: "building"
        )
        XCTAssertEqual(ws.statuses, [.running, .idle])
    }

    func testWorktreeStatusHasUrgent() {
        let ws = WorktreeStatus(
            worktreePath: "/repo/main",
            panes: [
                PaneStatus(paneIndex: 1, terminalID: "t1", status: .running, lastMessage: "", lastUpdated: Date()),
                PaneStatus(paneIndex: 2, terminalID: "t2", status: .error, lastMessage: "failed", lastUpdated: Date()),
            ],
            mostRecentPaneIndex: 2,
            mostRecentMessage: "failed"
        )
        XCTAssertTrue(ws.hasUrgent)
    }

    func testWorktreeStatusNotUrgent() {
        let ws = WorktreeStatus(
            worktreePath: "/repo/main",
            panes: [
                PaneStatus(paneIndex: 1, terminalID: "t1", status: .running, lastMessage: "", lastUpdated: Date()),
            ],
            mostRecentPaneIndex: 1,
            mostRecentMessage: ""
        )
        XCTAssertFalse(ws.hasUrgent)
    }

    func testWorktreeStatusHighestPriority() {
        let ws = WorktreeStatus(
            worktreePath: "/repo/main",
            panes: [
                PaneStatus(paneIndex: 1, terminalID: "t1", status: .idle, lastMessage: "", lastUpdated: Date()),
                PaneStatus(paneIndex: 2, terminalID: "t2", status: .waiting, lastMessage: "?", lastUpdated: Date()),
                PaneStatus(paneIndex: 3, terminalID: "t3", status: .running, lastMessage: "", lastUpdated: Date()),
            ],
            mostRecentPaneIndex: 2,
            mostRecentMessage: "?"
        )
        XCTAssertEqual(ws.highestPriority, .waiting)
    }

    func testSinglePaneWorktreeStatus() {
        let ws = WorktreeStatus(
            worktreePath: "/repo/main",
            panes: [
                PaneStatus(paneIndex: 1, terminalID: "t1", status: .running, lastMessage: "working", lastUpdated: Date()),
            ],
            mostRecentPaneIndex: 1,
            mostRecentMessage: "working"
        )
        XCTAssertEqual(ws.statuses.count, 1)
        XCTAssertEqual(ws.highestPriority, .running)
        XCTAssertFalse(ws.hasUrgent)
    }
}
