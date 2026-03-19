import Foundation

enum ProcessStatus {
    case running    // Process is alive
    case exited     // Exited with code 0
    case error      // Exited with code != 0
    case unknown    // Not started or unavailable
}

/// Deterministic status detection: ProcessStatus > ShellPhase > TextPatterns > Unknown
class StatusDetector {

    /// Detect agent status from available signals (priority order)
    func detect(
        processStatus: ProcessStatus,
        shellInfo: ShellPhaseInfo?,
        content: String,
        agentDef: AgentDef?
    ) -> AgentStatus {
        // Priority 1: Process lifecycle overrides everything
        switch processStatus {
        case .exited: return .exited
        case .error:  return .error
        case .unknown: break
        case .running: break
        }

        // Priority 2: OSC 133 shell phase (authoritative when available)
        if let info = shellInfo {
            switch info.phase {
            case .running:
                return .running
            case .input, .prompt:
                return .idle
            case .output:
                if let code = info.lastExitCode, code != 0 {
                    return .error
                }
                return .idle
            }
        }

        // Priority 3: Text pattern matching (fallback)
        if let agent = agentDef, !content.isEmpty {
            return agent.detectStatus(from: content)
        }

        return .unknown
    }
}

// MARK: - AgentDef status detection

extension AgentDef {
    /// Apply rules in order; first match wins
    func detectStatus(from content: String) -> AgentStatus {
        let lower = content.lowercased()
        for rule in rules {
            for pattern in rule.patterns {
                if lower.contains(pattern.lowercased()) {
                    return AgentStatus(rawValue: rule.status) ?? .unknown
                }
            }
        }
        return AgentStatus(rawValue: defaultStatus) ?? .idle
    }

    func extractLastMessage(from content: String, maxLen: Int) -> String {
        let lines = content.components(separatedBy: .newlines)
        for line in lines.reversed() {
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            if trimmed.isEmpty { continue }
            if isChromeLine(trimmed) { continue }
            if messageSkipPatterns.contains(where: { trimmed.lowercased().contains($0.lowercased()) }) {
                continue
            }
            if trimmed.count > maxLen {
                return String(trimmed.prefix(maxLen - 3)) + "..."
            }
            return trimmed
        }
        return ""
    }

    private func isChromeLine(_ line: String) -> Bool {
        // Box-drawing characters and decorative lines
        let chromeChars: Set<Character> = ["─", "│", "┌", "┐", "└", "┘", "├", "┤", "┬", "┴", "┼",
                                           "═", "║", "╔", "╗", "╚", "╝", "╠", "╣", "╦", "╩", "╬",
                                           "━", "┃", "┏", "┓", "┗", "┛", "┣", "┫", "┳", "┻", "╋"]
        if line.count <= 2 { return true }
        return line.allSatisfy { chromeChars.contains($0) || $0 == " " }
    }
}

// MARK: - DebouncedStatusTracker

/// Tracks status changes; Unknown preserves current state
class DebouncedStatusTracker {
    private(set) var currentStatus: AgentStatus = .unknown

    /// Update with detected status. Returns true if status changed.
    @discardableResult
    func update(status: AgentStatus) -> Bool {
        // Unknown means "no data" — don't change
        guard status != .unknown else { return false }
        guard status != currentStatus else { return false }
        currentStatus = status
        return true
    }

    func forceStatus(_ status: AgentStatus) {
        currentStatus = status
    }

    func reset() {
        currentStatus = .unknown
    }
}

// MARK: - AgentStatus extensions

extension AgentStatus {
    var priority: UInt8 {
        switch self {
        case .error:   return 6
        case .exited:  return 5
        case .waiting: return 4
        case .running: return 3
        case .idle:    return 2
        case .unknown: return 1
        }
    }

    var isUrgent: Bool {
        self == .error || self == .waiting
    }

    var isActive: Bool {
        self == .running || self == .waiting
    }

    static func highestPriority(_ statuses: [AgentStatus]) -> AgentStatus {
        statuses.max(by: { $0.priority < $1.priority }) ?? .unknown
    }
}
