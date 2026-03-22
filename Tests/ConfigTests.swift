import XCTest
@testable import pmux

final class ConfigTests: XCTestCase {

    // MARK: - Default Config

    func testDefaultConfig() {
        let config = Config()
        XCTAssertTrue(config.workspacePaths.isEmpty)
        XCTAssertEqual(config.activeWorkspaceIndex, 0)
        XCTAssertEqual(config.backend, "zmx")
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
        XCTAssertEqual(config.backend, "zmx")  // default
        XCTAssertEqual(config.terminalRowCacheSize, 200)  // default
    }

    func testDecodeEmptyJSON_UsesDefaults() throws {
        let json = "{}".data(using: .utf8)!
        let config = try JSONDecoder().decode(Config.self, from: json)
        XCTAssertTrue(config.workspacePaths.isEmpty)
        XCTAssertEqual(config.backend, "zmx")
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

    // MARK: - Save/Load to File

    func testSaveAndLoadFromFile() throws {
        let tempDir = FileManager.default.temporaryDirectory
            .appendingPathComponent("pmux-config-test-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: tempDir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: tempDir) }

        let filePath = tempDir.appendingPathComponent("config.json")

        // Create and save config
        var config = Config()
        config.workspacePaths = ["/Users/dev/project-a", "/Users/dev/project-b"]
        config.backend = "local"
        config.terminalRowCacheSize = 500

        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        let data = try encoder.encode(config)
        try data.write(to: filePath)

        // Load it back
        let loadedData = try Data(contentsOf: filePath)
        let loaded = try JSONDecoder().decode(Config.self, from: loadedData)

        XCTAssertEqual(loaded.workspacePaths, ["/Users/dev/project-a", "/Users/dev/project-b"])
        XCTAssertEqual(loaded.backend, "local")
        XCTAssertEqual(loaded.terminalRowCacheSize, 500)
    }

    func testConfigModification_WorkspacePaths() {
        var config = Config()
        XCTAssertTrue(config.workspacePaths.isEmpty)

        config.workspacePaths.append("/new/path")
        XCTAssertEqual(config.workspacePaths.count, 1)

        config.workspacePaths.append("/another/path")
        XCTAssertEqual(config.workspacePaths.count, 2)

        config.workspacePaths.remove(at: 0)
        XCTAssertEqual(config.workspacePaths, ["/another/path"])
    }

    func testConfigModification_Backend() {
        var config = Config()
        XCTAssertEqual(config.backend, "zmx")

        config.backend = "local"
        XCTAssertEqual(config.backend, "local")
    }

    func testAgentDetectConfig_EncodeDecodeRoundtrip() throws {
        let original = AgentDetectConfig.default
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        let data = try encoder.encode(original)
        let decoded = try JSONDecoder().decode(AgentDetectConfig.self, from: data)

        XCTAssertEqual(decoded.agents.count, original.agents.count)
        for (orig, dec) in zip(original.agents, decoded.agents) {
            XCTAssertEqual(orig.name, dec.name)
            XCTAssertEqual(orig.defaultStatus, dec.defaultStatus)
            XCTAssertEqual(orig.rules.count, dec.rules.count)
        }
    }

    func testCardOrder_DefaultEmpty() {
        let config = Config()
        XCTAssertTrue(config.cardOrder.isEmpty)
    }

    func testCardOrder_RoundTrip() throws {
        var config = Config()
        config.cardOrder = ["/path/a", "/path/b", "/path/c"]

        let data = try JSONEncoder().encode(config)
        let decoded = try JSONDecoder().decode(Config.self, from: data)
        XCTAssertEqual(decoded.cardOrder, ["/path/a", "/path/b", "/path/c"])
    }

    func testCardOrder_SortsWorktrees() {
        let order = ["/c", "/a", "/b"]
        var items = [
            (path: "/a", index: 0),
            (path: "/b", index: 1),
            (path: "/c", index: 2),
        ]
        items.sort { a, b in
            let ai = order.firstIndex(of: a.path) ?? Int.max
            let bi = order.firstIndex(of: b.path) ?? Int.max
            return ai < bi
        }
        XCTAssertEqual(items.map { $0.path }, ["/c", "/a", "/b"])
    }

    func testCardOrder_UnknownPathsGoToEnd() {
        let order = ["/b", "/a"]
        var items = [
            (path: "/a", index: 0),
            (path: "/b", index: 1),
            (path: "/unknown", index: 2),
        ]
        items.sort { a, b in
            let ai = order.firstIndex(of: a.path) ?? Int.max
            let bi = order.firstIndex(of: b.path) ?? Int.max
            return ai < bi
        }
        XCTAssertEqual(items.map { $0.path }, ["/b", "/a", "/unknown"])
    }

    func testDefaultDashboardLayout() {
        let config = Config()
        XCTAssertEqual(config.dashboardLayout, "left-right")
    }

    func testDefaultThemeMode() {
        let config = Config()
        XCTAssertEqual(config.themeMode, "system")
    }

    func testDecodeMissingNewFields() throws {
        let json = """
        {
            "workspace_paths": ["/path/a"],
            "backend": "tmux"
        }
        """.data(using: .utf8)!

        let config = try JSONDecoder().decode(Config.self, from: json)
        XCTAssertEqual(config.backend, "zmx")
        XCTAssertEqual(config.dashboardLayout, "left-right")
        XCTAssertEqual(config.themeMode, "system")
    }

    func testConfigMigration_TmuxToZmx() throws {
        let json = """
        {
            "workspace_paths": ["/path/a"],
            "backend": "tmux"
        }
        """.data(using: .utf8)!

        let config = try JSONDecoder().decode(Config.self, from: json)
        XCTAssertEqual(config.backend, "zmx")
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
