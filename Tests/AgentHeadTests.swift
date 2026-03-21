import XCTest
@testable import pmux

/// Tests for AgentHead singleton.
/// Note: We avoid creating TerminalSurface instances in tests because they
/// require Ghostty/Metal initialization. Instead we test the data management
/// logic by registering agents and verifying queries.
final class AgentHeadTests: XCTestCase {

    /// Helper: register a test agent without a real surface.
    /// Uses the internal register path that AgentHead provides.
    private func registerTestAgent(
        path: String, branch: String = "main", project: String = "TestProject",
        startedAt: Date? = nil
    ) {
        // We need a TerminalSurface for the API, but the tests focus on
        // data management (status, type, ordering) not rendering.
        // TerminalSurface() without Ghostty init is just an NSView wrapper.
        // If this hangs in CI, we'll need a mock — for now it works headlessly.
        let surface = TerminalSurface()
        AgentHead.shared.register(
            worktreePath: path, branch: branch, project: project,
            surface: surface, startedAt: startedAt
        )
    }

    override func setUp() {
        super.setUp()
        // Clear shared state between tests
        for agent in AgentHead.shared.allAgents() {
            AgentHead.shared.unregister(worktreePath: agent.id)
        }
    }

    override func tearDown() {
        for agent in AgentHead.shared.allAgents() {
            AgentHead.shared.unregister(worktreePath: agent.id)
        }
        super.tearDown()
    }

    // MARK: - Registration

    func testRegisterAndQuery() {
        registerTestAgent(path: "/tmp/repo/main", project: "MyProject")

        let agents = AgentHead.shared.allAgents()
        XCTAssertEqual(agents.count, 1)
        XCTAssertEqual(agents[0].id, "/tmp/repo/main")
        XCTAssertEqual(agents[0].branch, "main")
        XCTAssertEqual(agents[0].project, "MyProject")
        XCTAssertEqual(agents[0].agentType, .unknown)
        XCTAssertEqual(agents[0].status, .unknown)
    }

    func testUnregister() {
        registerTestAgent(path: "/tmp/repo/main")
        AgentHead.shared.unregister(worktreePath: "/tmp/repo/main")

        XCTAssertEqual(AgentHead.shared.allAgents().count, 0)
        XCTAssertNil(AgentHead.shared.agent(for: "/tmp/repo/main"))
    }

    // MARK: - Status Updates

    func testUpdateStatus() {
        registerTestAgent(path: "/tmp/repo/main")

        AgentHead.shared.updateStatus(
            worktreePath: "/tmp/repo/main",
            status: .running,
            lastMessage: "Editing file.swift",
            roundDuration: 30.0
        )

        let agent = AgentHead.shared.agent(for: "/tmp/repo/main")
        XCTAssertEqual(agent?.status, .running)
        XCTAssertEqual(agent?.lastMessage, "Editing file.swift")
        XCTAssertEqual(agent?.roundDuration, 30.0)
    }

    func testUpdateStatusForUnknownPath() {
        // Should not crash when updating non-existent path
        AgentHead.shared.updateStatus(
            worktreePath: "/nonexistent",
            status: .running,
            lastMessage: "test",
            roundDuration: 0
        )
        XCTAssertNil(AgentHead.shared.agent(for: "/nonexistent"))
    }

    // MARK: - Agent Type Detection

    func testUpdateAgentType() {
        registerTestAgent(path: "/tmp/repo/main")

        AgentHead.shared.updateAgentType(worktreePath: "/tmp/repo/main", type: .claudeCode)

        let agent = AgentHead.shared.agent(for: "/tmp/repo/main")
        XCTAssertEqual(agent?.agentType, .claudeCode)
    }

    func testUpdateAgentTypeOnlyWhenUnknown() {
        registerTestAgent(path: "/tmp/repo/main")

        AgentHead.shared.updateAgentType(worktreePath: "/tmp/repo/main", type: .claudeCode)
        // Should not overwrite once set
        AgentHead.shared.updateAgentType(worktreePath: "/tmp/repo/main", type: .codex)

        let agent = AgentHead.shared.agent(for: "/tmp/repo/main")
        XCTAssertEqual(agent?.agentType, .claudeCode)
    }

    func testUpdateAgentTypeIgnoresUnknown() {
        registerTestAgent(path: "/tmp/repo/main")

        AgentHead.shared.updateAgentType(worktreePath: "/tmp/repo/main", type: .unknown)

        let agent = AgentHead.shared.agent(for: "/tmp/repo/main")
        XCTAssertEqual(agent?.agentType, .unknown)
    }

    // MARK: - Ordering

    func testAllAgentsPreservesInsertionOrder() {
        registerTestAgent(path: "/a", branch: "a")
        registerTestAgent(path: "/b", branch: "b")
        registerTestAgent(path: "/c", branch: "c")

        let paths = AgentHead.shared.allAgents().map { $0.id }
        XCTAssertEqual(paths, ["/a", "/b", "/c"])
    }

    func testReorder() {
        registerTestAgent(path: "/a", branch: "a")
        registerTestAgent(path: "/b", branch: "b")
        registerTestAgent(path: "/c", branch: "c")

        AgentHead.shared.reorder(paths: ["/c", "/a", "/b"])

        let paths = AgentHead.shared.allAgents().map { $0.id }
        XCTAssertEqual(paths, ["/c", "/a", "/b"])
    }

    // MARK: - Project Filtering

    func testAgentsForProject() {
        registerTestAgent(path: "/repo1/main", branch: "main", project: "Repo1")
        registerTestAgent(path: "/repo2/main", branch: "main", project: "Repo2")
        registerTestAgent(path: "/repo1/feature", branch: "feature", project: "Repo1")

        let repo1Agents = AgentHead.shared.agentsForProject("Repo1")
        XCTAssertEqual(repo1Agents.count, 2)
        XCTAssertTrue(repo1Agents.allSatisfy { $0.project == "Repo1" })
    }

    // MARK: - Total Duration

    func testTotalDurationComputedFromStartedAt() {
        let fiveMinutesAgo = Date().addingTimeInterval(-300)
        registerTestAgent(path: "/tmp/repo/main", startedAt: fiveMinutesAgo)

        let agent = AgentHead.shared.agent(for: "/tmp/repo/main")!
        XCTAssertGreaterThan(agent.totalDuration, 299)
        XCTAssertLessThan(agent.totalDuration, 302)
    }

    func testTotalDurationZeroWhenNoStartedAt() {
        registerTestAgent(path: "/tmp/repo/main", startedAt: nil)

        let agent = AgentHead.shared.agent(for: "/tmp/repo/main")!
        XCTAssertEqual(agent.totalDuration, 0)
    }
}
