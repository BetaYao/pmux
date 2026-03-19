import XCTest
@testable import pmux

class NotificationNavigationTests: XCTestCase {

    func testWorktreePathLookup() {
        let worktrees = [
            WorktreeInfo(path: "/repos/main", branch: "main", commitHash: "abc12345", isMainWorktree: true),
            WorktreeInfo(path: "/repos/feature-a", branch: "feature-a", commitHash: "def67890", isMainWorktree: false),
            WorktreeInfo(path: "/repos/feature-b", branch: "feature-b", commitHash: "ghi11111", isMainWorktree: false),
        ]

        let index = worktrees.firstIndex(where: { $0.path == "/repos/feature-a" })
        XCTAssertEqual(index, 1)

        let missing = worktrees.firstIndex(where: { $0.path == "/repos/nonexistent" })
        XCTAssertNil(missing)
    }
}
