import Foundation

class WebhookStatusProvider {
    private let queue = DispatchQueue(label: "pmux.webhook-status")
    private var sessions: [String: SessionState] = [:]
    private var knownWorktrees: [String] = []

    struct SessionState {
        let sessionId: String
        let worktreePath: String
        var status: AgentStatus
        var lastEvent: Date
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
            guard let worktreePath = matchWorktree(canonCwd) else {
                NSLog("[WebhookStatusProvider] No worktree match for cwd: \(event.cwd)")
                return
            }

            let status = event.event.agentStatus(data: event.data)
            if var existing = sessions[event.sessionId] {
                existing.status = status
                existing.lastEvent = Date()
                sessions[event.sessionId] = existing
            } else {
                sessions[event.sessionId] = SessionState(
                    sessionId: event.sessionId,
                    worktreePath: worktreePath,
                    status: status,
                    lastEvent: Date()
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
        var cleaned = path
        while cleaned.hasSuffix("/") && cleaned.count > 1 {
            cleaned = String(cleaned.dropLast())
        }
        return cleaned
    }
}
