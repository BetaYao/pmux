import XCTest
@testable import amux

final class WorktreeTaskStoreTests: XCTestCase {
    func testSetAndGetRoundTrip() {
        let path = "/tmp/amux-test-worktree-\(UUID().uuidString)"
        WorktreeTaskStore.shared.set("fix the login bug", forWorktree: path)
        XCTAssertEqual(WorktreeTaskStore.shared.task(forWorktree: path), "fix the login bug")
    }

    func testMissingPathReturnsNil() {
        XCTAssertNil(WorktreeTaskStore.shared.task(forWorktree: "/tmp/amux-never-set-\(UUID().uuidString)"))
    }
}
