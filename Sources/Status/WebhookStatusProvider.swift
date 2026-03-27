import Foundation

class WebhookStatusProvider {
    private let queue = DispatchQueue(label: "amux.webhook-status")
    private var sessions: [String: SessionState] = [:]
    private var knownWorktrees: [String] = []

    /// Called when a WorktreeCreate event arrives with a path not in knownWorktrees
    var onNewWorktreeDetected: ((String) -> Void)?

    /// Called when a WorktreeCreate event arrives, with source worktree path and worktree name.
    /// Fires before the new worktree is discoverable (the git operation may still be in progress).
    var onWorktreeCreateReceived: ((_ sourceWorktreePath: String, _ worktreeName: String, _ sessionId: String) -> Void)?

    struct SessionState {
        let sessionId: String
        let worktreePath: String
        var status: AgentStatus
        var lastEvent: Date
        var lastMessage: String?
    }

    func updateWorktrees(_ paths: [String]) {
        queue.sync {
            knownWorktrees = paths.map { canonicalize($0) }
            // Remove sessions for worktrees no longer tracked
            sessions = sessions.filter { (_, state) in
                knownWorktrees.contains(state.worktreePath)
            }
            // Prune stale sessions (no events for >1 hour)
            let cutoff = Date().addingTimeInterval(-3600)
            sessions = sessions.filter { $0.value.lastEvent > cutoff }
        }
    }

    func handleEvent(_ event: WebhookEvent) {
        queue.sync {
            let canonCwd = canonicalize(event.cwd)

            // WorktreeCreate: record transfer intent before new worktree is discoverable
            if event.event == .worktreeCreate {
                let worktreeName = event.data?["worktree_name"] as? String ?? ""
                if !worktreeName.isEmpty {
                    let sourcePath = canonCwd
                    NSLog("[WebhookStatusProvider] WorktreeCreate from \(sourcePath): \(worktreeName)")
                    DispatchQueue.main.async { [weak self] in
                        self?.onWorktreeCreateReceived?(sourcePath, worktreeName, event.sessionId)
                    }
                }
                return
            }

            // CwdChanged with unknown path → notify upstream to discover it
            if event.event == .cwdChanged {
                if matchWorktree(canonCwd) == nil {
                    NSLog("[WebhookStatusProvider] New worktree detected via CwdChanged: \(event.cwd)")
                    DispatchQueue.main.async { [weak self] in
                        self?.onNewWorktreeDetected?(canonCwd)
                    }
                }
                // CwdChanged falls through to update session status
            }

            guard let worktreePath = matchWorktree(canonCwd) else {
                NSLog("[WebhookStatusProvider] No worktree match for cwd: \(event.cwd)")
                return
            }

            let status = event.event.agentStatus(data: event.data)
            let message = Self.extractMessage(from: event)

            if var existing = sessions[event.sessionId] {
                existing.status = status
                existing.lastEvent = Date()
                if let message { existing.lastMessage = message }
                sessions[event.sessionId] = existing
            } else {
                sessions[event.sessionId] = SessionState(
                    sessionId: event.sessionId,
                    worktreePath: worktreePath,
                    status: status,
                    lastEvent: Date(),
                    lastMessage: message
                )
            }
        }
    }

    func status(for worktreePath: String) -> AgentStatus {
        queue.sync {
            let canon = canonicalize(worktreePath)
            let sessionStatuses = sessions.values
                .filter { $0.worktreePath == canon }
                .map { $0.status }
            return AgentStatus.highestPriority(sessionStatuses)
        }
    }

    /// Returns the most recent webhook-derived message for a worktree, or nil
    func lastMessage(for worktreePath: String) -> String? {
        queue.sync {
            let canon = canonicalize(worktreePath)
            // Pick the session with the most recent event
            return sessions.values
                .filter { $0.worktreePath == canon }
                .max(by: { $0.lastEvent < $1.lastEvent })?
                .lastMessage
        }
    }

    /// Extract a human-readable message from a webhook event
    private static func extractMessage(from event: WebhookEvent) -> String? {
        let data = event.data
        switch event.event {
        case .toolUseStart:
            if let toolName = data?["tool_name"] as? String {
                return "Using \(toolName)"
            }
            return nil
        case .toolUseEnd:
            if let toolName = data?["tool_name"] as? String {
                return "Done: \(toolName)"
            }
            return nil
        case .agentStop:
            let reason = data?["stop_reason"] as? String ?? "done"
            return "Stopped: \(reason)"
        case .error:
            let message = data?["message"] as? String ?? "Error occurred"
            return message
        case .prompt:
            let message = data?["message"] as? String ?? "Waiting for input"
            return message
        case .notification:
            if let message = data?["message"] as? String {
                return message
            }
            if let title = data?["title"] as? String {
                return title
            }
            return nil
        case .sessionStart:
            return "Session started"
        case .worktreeCreate:
            return "Creating worktree"
        case .userPrompt:
            return "Processing prompt"
        case .toolUseFailed:
            if let toolName = data?["tool_name"] as? String {
                return "Failed: \(toolName)"
            }
            return "Tool failed"
        case .stopFailure:
            return data?["error"] as? String ?? "API error"
        case .subagentStart:
            return "Subagent started"
        case .cwdChanged:
            return nil
        }
    }

    private func matchWorktree(_ canonCwd: String) -> String? {
        // Exact match first
        if knownWorktrees.contains(canonCwd) {
            return canonCwd
        }
        // Prefix match (agent in subdirectory)
        for worktree in knownWorktrees {
            if canonCwd.hasPrefix(worktree + "/") {
                return worktree
            }
        }
        return nil
    }

    private func canonicalize(_ path: String) -> String {
        // Resolve symlinks (e.g. /var → /private/var on macOS) so that
        // worktree paths and webhook cwd values match reliably.
        let resolved = (path as NSString).resolvingSymlinksInPath
        var cleaned = resolved
        while cleaned.hasSuffix("/") && cleaned.count > 1 {
            cleaned = String(cleaned.dropLast())
        }
        return cleaned
    }
}
