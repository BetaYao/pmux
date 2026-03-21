import Foundation

enum AgentType: String, Codable, CaseIterable {
    case claudeCode
    case codex
    case openCode
    case gemini
    case cline
    case goose
    case amp
    case aider
    case cursor
    case kiro
    case unknown

    var displayName: String {
        switch self {
        case .claudeCode: return "Claude Code"
        case .codex:      return "Codex"
        case .openCode:   return "OpenCode"
        case .gemini:     return "Gemini"
        case .cline:      return "Cline"
        case .goose:      return "Goose"
        case .amp:        return "Amp"
        case .aider:      return "Aider"
        case .cursor:     return "Cursor"
        case .kiro:       return "Kiro"
        case .unknown:    return "Unknown"
        }
    }

    // Ordered by specificity to avoid false matches (e.g., "opencode" before "code")
    private static let detectionPatterns: [(pattern: String, type: AgentType)] = [
        ("opencode", .openCode),
        ("claude", .claudeCode),
        ("codex", .codex),
        ("gemini", .gemini),
        ("cline", .cline),
        ("goose", .goose),
        ("aider", .aider),
        ("cursor", .cursor),
        ("kiro", .kiro),
        ("amp ", .amp),
    ]

    /// Detect agent type from lowercased terminal content
    static func detect(fromLowercased content: String) -> AgentType {
        for (pattern, type) in detectionPatterns {
            if content.contains(pattern) {
                return type
            }
        }
        return .unknown
    }
}
