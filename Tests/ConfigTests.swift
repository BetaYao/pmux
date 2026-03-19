import XCTest
@testable import pmux

final class ConfigTests: XCTestCase {

    // MARK: - Default Config

    func testDefaultConfig() {
        let config = Config()
        XCTAssertTrue(config.workspacePaths.isEmpty)
        XCTAssertEqual(config.activeWorkspaceIndex, 0)
        XCTAssertEqual(config.backend, "tmux")
        XCTAssertEqual(config.terminalRowCacheSize, 200)
        XCTAssertFalse(config.agentDetect.agents.isEmpty)
    }

    // MARK: - JSON Decode

    func testDecodeFullConfig() throws {
        let json = """
        {
            "workspace_paths": ["/path/a", "/path/b"],
            "active_workspace_index": 1,
            "backend": "local",
            "terminal_row_cache_size": 500
        }
        """.data(using: .utf8)!

        let config = try JSONDecoder().decode(Config.self, from: json)
        XCTAssertEqual(config.workspacePaths, ["/path/a", "/path/b"])
        XCTAssertEqual(config.activeWorkspaceIndex, 1)
        XCTAssertEqual(config.backend, "local")
        XCTAssertEqual(config.terminalRowCacheSize, 500)
    }

    func testDecodePartialConfig_UsesDefaults() throws {
        let json = """
        {
            "workspace_paths": ["/path/a"]
        }
        """.data(using: .utf8)!

        let config = try JSONDecoder().decode(Config.self, from: json)
        XCTAssertEqual(config.workspacePaths, ["/path/a"])
        XCTAssertEqual(config.backend, "tmux")  // default
        XCTAssertEqual(config.terminalRowCacheSize, 200)  // default
    }

    func testDecodeEmptyJSON_UsesDefaults() throws {
        let json = "{}".data(using: .utf8)!
        let config = try JSONDecoder().decode(Config.self, from: json)
        XCTAssertTrue(config.workspacePaths.isEmpty)
        XCTAssertEqual(config.backend, "tmux")
    }

    // MARK: - JSON Roundtrip

    func testEncodeDecodeRoundtrip() throws {
        var original = Config()
        original.workspacePaths = ["/a", "/b"]
        original.backend = "local"

        let encoder = JSONEncoder()
        let data = try encoder.encode(original)
        let decoded = try JSONDecoder().decode(Config.self, from: data)

        XCTAssertEqual(decoded.workspacePaths, original.workspacePaths)
        XCTAssertEqual(decoded.backend, original.backend)
        XCTAssertEqual(decoded.terminalRowCacheSize, original.terminalRowCacheSize)
    }

    // MARK: - Agent Detect Config

    func testDefaultAgentDetect_HasClaude() {
        let agents = AgentDetectConfig.default.agents
        XCTAssertTrue(agents.contains(where: { $0.name == "claude" }))
    }

    func testClaudeAgent_HasRules() {
        let claude = AgentDetectConfig.default.agents.first(where: { $0.name == "claude" })!
        XCTAssertFalse(claude.rules.isEmpty)
        XCTAssertEqual(claude.defaultStatus, "Idle")
        XCTAssertFalse(claude.messageSkipPatterns.isEmpty)
    }

    func testAgentDetectConfig_Decode() throws {
        let json = """
        {
            "agents": [
                {
                    "name": "myagent",
                    "rules": [{"status": "Running", "patterns": ["working"]}],
                    "default_status": "Idle",
                    "message_skip_patterns": []
                }
            ]
        }
        """.data(using: .utf8)!

        let config = try JSONDecoder().decode(AgentDetectConfig.self, from: json)
        XCTAssertEqual(config.agents.count, 1)
        XCTAssertEqual(config.agents[0].name, "myagent")
        XCTAssertEqual(config.agents[0].rules[0].status, "Running")
        XCTAssertEqual(config.agents[0].rules[0].patterns, ["working"])
    }
}
