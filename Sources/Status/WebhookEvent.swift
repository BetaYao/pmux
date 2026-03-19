import Foundation

enum WebhookEventType: String {
    case sessionStart = "session_start"
    case toolUseStart = "tool_use_start"
    case toolUseEnd = "tool_use_end"
    case agentStop = "agent_stop"
    case notification = "notification"
    case error = "error"
    case prompt = "prompt"

    func agentStatus(data: [String: Any]?) -> AgentStatus {
        switch self {
        case .sessionStart, .toolUseStart, .toolUseEnd:
            return .running
        case .agentStop:
            return .idle
        case .error:
            return .error
        case .prompt:
            return .waiting
        case .notification:
            let level = data?["level"] as? String
            switch level {
            case "error": return .error
            case "warning": return .waiting
            default: return .idle
            }
        }
    }

    /// Map Claude Code hook_event_name to generic event type
    static func fromClaudeCode(_ hookEventName: String) -> WebhookEventType? {
        switch hookEventName {
        case "SessionStart": return .sessionStart
        case "PreToolUse": return .toolUseStart
        case "PostToolUse": return .toolUseEnd
        case "Stop", "SubagentStop": return .agentStop
        case "Notification": return .notification
        default: return nil
        }
    }
}

struct WebhookEvent {
    let source: String
    let sessionId: String
    let event: WebhookEventType
    let cwd: String
    let timestamp: String?
    let data: [String: Any]?

    /// Parse from JSON data. Supports both generic protocol and Claude Code native format.
    static func parse(from jsonData: Data) throws -> WebhookEvent {
        guard let json = try JSONSerialization.jsonObject(with: jsonData) as? [String: Any] else {
            throw WebhookEventError.invalidJSON
        }

        // Detect format: Claude Code native has "hook_event_name", generic has "event"
        if let hookEventName = json["hook_event_name"] as? String {
            return try parseClaudeCode(json: json, hookEventName: hookEventName)
        } else {
            return try parseGeneric(json: json)
        }
    }

    private static func parseGeneric(json: [String: Any]) throws -> WebhookEvent {
        guard let source = json["source"] as? String,
              let sessionId = json["session_id"] as? String,
              let eventRaw = json["event"] as? String,
              let cwd = json["cwd"] as? String else {
            throw WebhookEventError.missingRequiredField
        }
        guard let event = WebhookEventType(rawValue: eventRaw) else {
            throw WebhookEventError.unknownEventType(eventRaw)
        }
        return WebhookEvent(
            source: source,
            sessionId: sessionId,
            event: event,
            cwd: cwd,
            timestamp: json["timestamp"] as? String,
            data: json["data"] as? [String: Any]
        )
    }

    private static func parseClaudeCode(json: [String: Any], hookEventName: String) throws -> WebhookEvent {
        guard let sessionId = json["session_id"] as? String,
              let cwd = json["cwd"] as? String else {
            throw WebhookEventError.missingRequiredField
        }
        guard let event = WebhookEventType.fromClaudeCode(hookEventName) else {
            throw WebhookEventError.unknownEventType(hookEventName)
        }

        // Collect remaining fields as data
        var data: [String: Any] = [:]
        let reservedKeys: Set<String> = ["hook_event_name", "session_id", "cwd", "transcript_path", "permission_mode"]
        for (key, value) in json where !reservedKeys.contains(key) {
            data[key] = value
        }

        return WebhookEvent(
            source: "claude-code",
            sessionId: sessionId,
            event: event,
            cwd: cwd,
            timestamp: nil,
            data: data.isEmpty ? nil : data
        )
    }
}

enum WebhookEventError: Error {
    case invalidJSON
    case missingRequiredField
    case unknownEventType(String)
}
