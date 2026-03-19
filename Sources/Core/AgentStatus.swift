import AppKit

enum AgentStatus: String, Codable {
    case running = "Running"
    case idle = "Idle"
    case waiting = "Waiting"
    case error = "Error"
    case exited = "Exited"
    case unknown = "Unknown"

    var color: NSColor {
        switch self {
        case .running:  return NSColor.systemGreen
        case .idle:     return NSColor.systemGray
        case .waiting:  return NSColor.systemYellow
        case .error:    return NSColor.systemRed
        case .exited:   return NSColor(white: 0.4, alpha: 1.0)
        case .unknown:  return NSColor(white: 0.5, alpha: 1.0)
        }
    }

    var icon: String {
        switch self {
        case .running:  return "●"
        case .idle:     return "○"
        case .waiting:  return "◐"
        case .error:    return "✕"
        case .exited:   return "◻"
        case .unknown:  return "?"
        }
    }
}
