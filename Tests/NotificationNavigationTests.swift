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

    func testTabLookupByWorktreePath() {
        let manager = WorkspaceManager()
        let worktrees1 = [
            WorktreeInfo(path: "/repos/alpha/main", branch: "main", commitHash: "aaa", isMainWorktree: true),
            WorktreeInfo(path: "/repos/alpha-worktrees/feat", branch: "feat", commitHash: "bbb", isMainWorktree: false),
        ]
        let worktrees2 = [
            WorktreeInfo(path: "/repos/beta/main", branch: "main", commitHash: "ccc", isMainWorktree: true),
        ]
        _ = manager.addTab(repoPath: "/repos/alpha", worktrees: worktrees1)
        _ = manager.addTab(repoPath: "/repos/beta", worktrees: worktrees2)

        // Find tab containing a worktree path
        let tabIndex = manager.tabs.firstIndex(where: { tab in
            tab.worktrees.contains(where: { $0.path == "/repos/alpha-worktrees/feat" })
        })
        XCTAssertEqual(tabIndex, 0)
        XCTAssertEqual(manager.tabs[tabIndex!].repoPath, "/repos/alpha")

        // Find tab for second repo
        let tabIndex2 = manager.tabs.firstIndex(where: { tab in
            tab.worktrees.contains(where: { $0.path == "/repos/beta/main" })
        })
        XCTAssertEqual(tabIndex2, 1)

        // Missing worktree returns nil
        let missing = manager.tabs.firstIndex(where: { tab in
            tab.worktrees.contains(where: { $0.path == "/nonexistent" })
        })
        XCTAssertNil(missing)
    }
}
