import XCTest
@testable import amux

final class MiniCardViewTests: XCTestCase {
    func testConfigureSetsTitleAndRepoWorktree() {
        let card = MiniCardView(frame: NSRect(x: 0, y: 0, width: 220, height: 80))
        card.configure(
            id: "t1", project: "teamclaw-next", thread: "test-trsyt",
            status: "running", lastMessage: "ignored",
            lastUserPrompt: "Why are classics unread?",
            totalDuration: "120", roundDuration: "30"
        )
        XCTAssertEqual(card.agentId, "t1")
        XCTAssertEqual(card.titleTextForTesting, "Why are classics unread?")
        XCTAssertTrue(card.repoWorktreeTextForTesting.contains("teamclaw-next"))
        XCTAssertTrue(card.repoWorktreeTextForTesting.contains("test-trsyt"))
    }

    func testTitleFallsBackToProject() {
        let card = MiniCardView(frame: .zero)
        card.configure(
            id: "t2", project: "repo", thread: "wt",
            status: "idle", lastMessage: "",
            lastUserPrompt: "",
            totalDuration: "0", roundDuration: "0"
        )
        XCTAssertEqual(card.titleTextForTesting, "repo")
    }
}
