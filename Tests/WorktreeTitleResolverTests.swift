import XCTest
@testable import amux

final class WorktreeTitleResolverTests: XCTestCase {
    func testFallsBackToPromptWhenNoSummary() {
        let title = WorktreeTitleResolver.resolve(
            worktreePath: "/nonexistent/path",
            lastUserPrompt: "Fix the login bug",
            branch: "feature/login",
            sessionTitle: { _ in nil }
        )
        XCTAssertEqual(title, "Fix the login bug")
    }

    func testPrefersSessionTitle() {
        let title = WorktreeTitleResolver.resolve(
            worktreePath: "/p",
            lastUserPrompt: "prompt",
            branch: "br",
            sessionTitle: { _ in "Session Title" }
        )
        XCTAssertEqual(title, "Session Title")
    }

    func testFallsBackToBranchWhenEmpty() {
        let title = WorktreeTitleResolver.resolve(
            worktreePath: "/p",
            lastUserPrompt: "",
            branch: "feature/x",
            sessionTitle: { _ in nil }
        )
        XCTAssertEqual(title, "feature/x")
    }
}
