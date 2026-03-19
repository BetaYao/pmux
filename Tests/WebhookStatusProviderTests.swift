import XCTest
@testable import pmux

final class WebhookStatusProviderTests: XCTestCase {

    var provider: WebhookStatusProvider!

    override func setUp() {
        super.setUp()
        provider = WebhookStatusProvider()
        provider.updateWorktrees(["/projects/repo/main", "/projects/repo/feature"])
    }

    // MARK: - Basic event handling

    func testNoEventsReturnsUnknown() {
        XCTAssertEqual(provider.status(for: "/projects/repo/main"), .unknown)
    }

    func testSessionStartSetsRunning() {
        let event = makeEvent(sessionId: "s1", event: .sessionStart, cwd: "/projects/repo/main")
        provider.handleEvent(event)
        XCTAssertEqual(provider.status(for: "/projects/repo/main"), .running)
    }

    func testAgentStopSetsIdle() {
        provider.handleEvent(makeEvent(sessionId: "s1", event: .sessionStart, cwd: "/projects/repo/main"))
        provider.handleEvent(makeEvent(sessionId: "s1", event: .agentStop, cwd: "/projects/repo/main"))
        XCTAssertEqual(provider.status(for: "/projects/repo/main"), .idle)
    }

    func testToolUseKeepsRunning() {
        provider.handleEvent(makeEvent(sessionId: "s1", event: .sessionStart, cwd: "/projects/repo/main"))
        provider.handleEvent(makeEvent(sessionId: "s1", event: .toolUseStart, cwd: "/projects/repo/main"))
        XCTAssertEqual(provider.status(for: "/projects/repo/main"), .running)
    }

    func testErrorEvent() {
        provider.handleEvent(makeEvent(sessionId: "s1", event: .error, cwd: "/projects/repo/main"))
        XCTAssertEqual(provider.status(for: "/projects/repo/main"), .error)
    }

    func testPromptEvent() {
        provider.handleEvent(makeEvent(sessionId: "s1", event: .prompt, cwd: "/projects/repo/main"))
        XCTAssertEqual(provider.status(for: "/projects/repo/main"), .waiting)
    }

    // MARK: - cwd matching

    func testExactCwdMatch() {
        provider.handleEvent(makeEvent(sessionId: "s1", event: .sessionStart, cwd: "/projects/repo/feature"))
        XCTAssertEqual(provider.status(for: "/projects/repo/feature"), .running)
        XCTAssertEqual(provider.status(for: "/projects/repo/main"), .unknown)
    }

    func testPrefixCwdMatch() {
        // Agent running in a subdirectory of the worktree
        provider.handleEvent(makeEvent(sessionId: "s1", event: .sessionStart, cwd: "/projects/repo/main/src/lib"))
        XCTAssertEqual(provider.status(for: "/projects/repo/main"), .running)
    }

    func testUnknownCwdDiscarded() {
        provider.handleEvent(makeEvent(sessionId: "s1", event: .sessionStart, cwd: "/unknown/path"))
        XCTAssertEqual(provider.status(for: "/projects/repo/main"), .unknown)
        XCTAssertEqual(provider.status(for: "/projects/repo/feature"), .unknown)
    }

    // MARK: - Multi-session aggregation

    func testMultipleSessionsSameWorktreeAggregates() {
        provider.handleEvent(makeEvent(sessionId: "s1", event: .agentStop, cwd: "/projects/repo/main"))
        provider.handleEvent(makeEvent(sessionId: "s2", event: .sessionStart, cwd: "/projects/repo/main"))
        // s1=idle, s2=running → aggregated = running (higher priority)
        XCTAssertEqual(provider.status(for: "/projects/repo/main"), .running)
    }

    func testMultipleSessionsBothIdle() {
        provider.handleEvent(makeEvent(sessionId: "s1", event: .agentStop, cwd: "/projects/repo/main"))
        provider.handleEvent(makeEvent(sessionId: "s2", event: .agentStop, cwd: "/projects/repo/main"))
        XCTAssertEqual(provider.status(for: "/projects/repo/main"), .idle)
    }

    func testSessionsIsolatedBetweenWorktrees() {
        provider.handleEvent(makeEvent(sessionId: "s1", event: .sessionStart, cwd: "/projects/repo/main"))
        provider.handleEvent(makeEvent(sessionId: "s2", event: .error, cwd: "/projects/repo/feature"))
        XCTAssertEqual(provider.status(for: "/projects/repo/main"), .running)
        XCTAssertEqual(provider.status(for: "/projects/repo/feature"), .error)
    }

    // MARK: - Notification level mapping

    func testNotificationErrorLevel() {
        let event = makeEvent(sessionId: "s1", event: .notification, cwd: "/projects/repo/main", data: ["level": "error"])
        provider.handleEvent(event)
        XCTAssertEqual(provider.status(for: "/projects/repo/main"), .error)
    }

    func testNotificationWarningLevel() {
        let event = makeEvent(sessionId: "s1", event: .notification, cwd: "/projects/repo/main", data: ["level": "warning"])
        provider.handleEvent(event)
        XCTAssertEqual(provider.status(for: "/projects/repo/main"), .waiting)
    }

    func testNotificationInfoLevel() {
        let event = makeEvent(sessionId: "s1", event: .notification, cwd: "/projects/repo/main", data: ["level": "info"])
        provider.handleEvent(event)
        XCTAssertEqual(provider.status(for: "/projects/repo/main"), .idle)
    }

    // MARK: - Path normalization

    func testTrailingSlashNormalized() {
        provider.updateWorktrees(["/projects/repo/main/"])
        provider.handleEvent(makeEvent(sessionId: "s1", event: .sessionStart, cwd: "/projects/repo/main"))
        XCTAssertEqual(provider.status(for: "/projects/repo/main"), .running)
        XCTAssertEqual(provider.status(for: "/projects/repo/main/"), .running)
    }

    // MARK: - Worktree cleanup

    func testUpdateWorktreesRemovesStaleSessions() {
        provider.handleEvent(makeEvent(sessionId: "s1", event: .sessionStart, cwd: "/projects/repo/main"))
        provider.handleEvent(makeEvent(sessionId: "s2", event: .sessionStart, cwd: "/projects/repo/feature"))
        // Remove "feature" worktree
        provider.updateWorktrees(["/projects/repo/main"])
        XCTAssertEqual(provider.status(for: "/projects/repo/main"), .running)
        XCTAssertEqual(provider.status(for: "/projects/repo/feature"), .unknown)
    }

    // MARK: - Helpers

    private func makeEvent(
        sessionId: String,
        event: WebhookEventType,
        cwd: String,
        data: [String: Any]? = nil
    ) -> WebhookEvent {
        WebhookEvent(
            source: "claude-code",
            sessionId: sessionId,
            event: event,
            cwd: cwd,
            timestamp: nil,
            data: data
        )
    }
}
