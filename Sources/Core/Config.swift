import Foundation

struct Config: Codable {
    var workspacePaths: [String]
    var activeWorkspaceIndex: Int
    var backend: String
    var terminalRowCacheSize: Int
    var agentDetect: AgentDetectConfig
    var webhook: WebhookConfig
    var autoUpdate: UpdateConfig

    enum CodingKeys: String, CodingKey {
        case workspacePaths = "workspace_paths"
        case activeWorkspaceIndex = "active_workspace_index"
        case backend
        case terminalRowCacheSize = "terminal_row_cache_size"
        case agentDetect = "agent_detect"
        case webhook
        case autoUpdate = "auto_update"
    }

    init() {
        workspacePaths = []
        activeWorkspaceIndex = 0
        backend = "tmux"
        terminalRowCacheSize = 200
        agentDetect = AgentDetectConfig.default
        webhook = WebhookConfig()
        autoUpdate = UpdateConfig()
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        workspacePaths = try container.decodeIfPresent([String].self, forKey: .workspacePaths) ?? []
        activeWorkspaceIndex = try container.decodeIfPresent(Int.self, forKey: .activeWorkspaceIndex) ?? 0
        backend = try container.decodeIfPresent(String.self, forKey: .backend) ?? "tmux"
        terminalRowCacheSize = try container.decodeIfPresent(Int.self, forKey: .terminalRowCacheSize) ?? 200
        agentDetect = try container.decodeIfPresent(AgentDetectConfig.self, forKey: .agentDetect) ?? .default
        webhook = try container.decodeIfPresent(WebhookConfig.self, forKey: .webhook) ?? WebhookConfig()
        autoUpdate = try container.decodeIfPresent(UpdateConfig.self, forKey: .autoUpdate) ?? UpdateConfig()
    }

    static let configDir = FileManager.default.homeDirectoryForCurrentUser
        .appendingPathComponent(".config/pmux")
    static let configPath = configDir.appendingPathComponent("config.json")

    static func load() -> Config {
        guard FileManager.default.fileExists(atPath: configPath.path) else {
            return Config()
        }
        do {
            let data = try Data(contentsOf: configPath)
            return try JSONDecoder().decode(Config.self, from: data)
        } catch {
            NSLog("Failed to load config: \(error)")
            return Config()
        }
    }

    func save() {
        do {
            try FileManager.default.createDirectory(at: Config.configDir, withIntermediateDirectories: true)
            let encoder = JSONEncoder()
            encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
            let data = try encoder.encode(self)
            try data.write(to: Config.configPath)
        } catch {
            NSLog("Failed to save config: \(error)")
        }
    }
}

struct AgentDetectConfig: Codable {
    var agents: [AgentDef]

    static let `default` = AgentDetectConfig(agents: [
        AgentDef(name: "claude", rules: [
            AgentRule(status: "Running", patterns: ["to interrupt"]),
            AgentRule(status: "Error", patterns: ["ERROR", "error:"]),
            AgentRule(status: "Waiting", patterns: ["?", "(y/n)", "(yes/no)"]),
        ], defaultStatus: "Idle", messageSkipPatterns: ["shift+tab", "accept edits", "to interrupt"]),
        AgentDef(name: "agent", rules: [
            AgentRule(status: "Running", patterns: ["to interrupt"]),
            AgentRule(status: "Error", patterns: ["error"]),
            AgentRule(status: "Waiting", patterns: ["?", "> "]),
        ], defaultStatus: "Idle", messageSkipPatterns: ["shift+tab", "accept edits", "to interrupt"]),
    ])
}

struct AgentDef: Codable {
    var name: String
    var rules: [AgentRule]
    var defaultStatus: String
    var messageSkipPatterns: [String]

    enum CodingKeys: String, CodingKey {
        case name, rules
        case defaultStatus = "default_status"
        case messageSkipPatterns = "message_skip_patterns"
    }
}

struct AgentRule: Codable {
    var status: String
    var patterns: [String]
}

struct WebhookConfig: Codable {
    var enabled: Bool = true
    var port: UInt16 = 7070
}

struct UpdateConfig: Codable {
    var enabled: Bool = true
    var checkIntervalHours: Int = 6

    enum CodingKeys: String, CodingKey {
        case enabled
        case checkIntervalHours = "check_interval_hours"
    }
}
