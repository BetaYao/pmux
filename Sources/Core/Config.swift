import Foundation

struct Config: Codable {
    var workspacePaths: [String]
    var activeWorkspaceIndex: Int
    var backend: String
    var terminalRowCacheSize: Int
    var agentDetect: AgentDetectConfig
    var webhook: WebhookConfig
    var autoUpdate: UpdateConfig
    var cardOrder: [String]
    var zoomIndex: Int
    var dashboardLayout: String
    var themeMode: String
    var worktreeStartedAt: [String: String]

    enum CodingKeys: String, CodingKey {
        case workspacePaths = "workspace_paths"
        case activeWorkspaceIndex = "active_workspace_index"
        case backend
        case terminalRowCacheSize = "terminal_row_cache_size"
        case agentDetect = "agent_detect"
        case webhook
        case autoUpdate = "auto_update"
        case cardOrder = "card_order"
        case zoomIndex = "zoom_index"
        case dashboardLayout = "dashboard_layout"
        case themeMode = "theme_mode"
        case worktreeStartedAt = "worktree_started_at"
    }

    init() {
        workspacePaths = []
        activeWorkspaceIndex = 0
        backend = "tmux"
        terminalRowCacheSize = 200
        agentDetect = AgentDetectConfig.default
        webhook = WebhookConfig()
        autoUpdate = UpdateConfig()
        cardOrder = []
        zoomIndex = 3
        dashboardLayout = "left-right"
        themeMode = "dark"
        worktreeStartedAt = [:]
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
        cardOrder = try container.decodeIfPresent([String].self, forKey: .cardOrder) ?? []
        zoomIndex = try container.decodeIfPresent(Int.self, forKey: .zoomIndex) ?? 3
        dashboardLayout = try container.decodeIfPresent(String.self, forKey: .dashboardLayout) ?? "left-right"
        themeMode = try container.decodeIfPresent(String.self, forKey: .themeMode) ?? "dark"
        worktreeStartedAt = try container.decodeIfPresent([String: String].self, forKey: .worktreeStartedAt) ?? [:]
    }

    static let configDir = FileManager.default.homeDirectoryForCurrentUser
        .appendingPathComponent(".config/pmux")
    static let configPath = configDir.appendingPathComponent("config.json")

    static func load() -> Config {
        // Support UI test config override via launch argument
        if let idx = CommandLine.arguments.firstIndex(of: "-UITestConfig"),
           idx + 1 < CommandLine.arguments.count {
            let testPath = CommandLine.arguments[idx + 1]
            if let data = FileManager.default.contents(atPath: testPath) {
                return (try? JSONDecoder().decode(Config.self, from: data)) ?? Config()
            }
        }

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

    private static let saveQueue = DispatchQueue(label: "com.pmux.config-save", qos: .utility)
    private static var pendingSaveWorkItem: DispatchWorkItem?

    func save() {
        // Debounced async save: coalesces rapid saves into a single write
        Config.pendingSaveWorkItem?.cancel()
        let configCopy = self
        let workItem = DispatchWorkItem {
            do {
                try FileManager.default.createDirectory(at: Config.configDir, withIntermediateDirectories: true)
                let encoder = JSONEncoder()
                encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
                let data = try encoder.encode(configCopy)
                try data.write(to: Config.configPath, options: .atomic)
            } catch {
                NSLog("Failed to save config: \(error)")
            }
        }
        Config.pendingSaveWorkItem = workItem
        Config.saveQueue.asyncAfter(deadline: .now() + 0.3, execute: workItem)
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
    var skippedVersion: String? = nil

    enum CodingKeys: String, CodingKey {
        case enabled
        case checkIntervalHours = "check_interval_hours"
        case skippedVersion = "skipped_version"
    }

    init() {}

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        enabled = try container.decodeIfPresent(Bool.self, forKey: .enabled) ?? true
        checkIntervalHours = try container.decodeIfPresent(Int.self, forKey: .checkIntervalHours) ?? 6
        skippedVersion = try container.decodeIfPresent(String.self, forKey: .skippedVersion)
    }
}
