import XCTest
@testable import amux

final class DiffReviewViewTests: XCTestCase {
    func testDiffReviewViewLoadsInjectedSnapshot() {
        let file = DiffFile(
            path: "Sources/App/main.swift",
            status: .modified,
            stage: .unstaged,
            additions: 1,
            deletions: 1,
            hunks: [
                DiffHunk(header: "@@ -1 +1 @@", lines: [
                    DiffLine(type: .deletion, content: "old"),
                    DiffLine(type: .addition, content: "new"),
                ])
            ]
        )
        let view = DiffReviewView(
            worktreePath: "/repo",
            loadSnapshot: { GitDiffSnapshot(changedFiles: [], files: [file]) }
        )

        view.frame = NSRect(x: 0, y: 0, width: 800, height: 500)
        view.loadDiffForTesting()

        XCTAssertTrue(view.renderedTextForTesting.contains("Sources/App/main.swift"))
        XCTAssertTrue(view.renderedTextForTesting.contains("+new"))
        XCTAssertTrue(view.renderedTextForTesting.contains("-old"))
    }

    func testDiffReviewViewShowsNoChangesStateForEmptySnapshot() {
        let view = DiffReviewView(
            worktreePath: "/repo",
            loadSnapshot: { GitDiffSnapshot(changedFiles: [], files: []) }
        )

        view.frame = NSRect(x: 0, y: 0, width: 800, height: 500)
        view.loadDiffForTesting()

        XCTAssertTrue(view.renderedTextForTesting.contains("No changes"))
    }
}
