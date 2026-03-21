import Foundation

/// Communication channel for Claude Code via Hooks.
/// Receives structured events through the existing WebhookServer,
/// sends commands via tmux (same as TmuxChannel).
class HooksChannel: AgentChannel {
    let channelType: AgentChannelType = .hooks
    let supportsStructuredEvents = true

    private let tmux: TmuxChannel
    private let lock = NSLock()

    /// Accumulated hook events for this agent session
    private(set) var events: [HookEvent] = []

    init(sessionName: String) {
        self.tmux = TmuxChannel(sessionName: sessionName)
    }

    // MARK: - AgentChannel

    /// Send command via tmux (hooks don't provide an input channel)
    func sendCommand(_ command: String) {
        tmux.sendCommand(command)
    }

    /// Read output via tmux (hooks provide events, not raw output)
    func readOutput(lines: Int) -> String? {
        tmux.readOutput(lines: lines)
    }

    // MARK: - Hook Events

    /// Called by AgentHead when a WebhookEvent arrives for this agent
    func handleWebhookEvent(_ event: WebhookEvent) {
        let hookEvent = HookEvent(
            timestamp: Date(),
            type: event.event,
            toolName: event.data?["tool_name"] as? String,
            message: extractMessage(from: event),
            rawData: event.data
        )

        lock.lock()
        events.append(hookEvent)
        // Keep last 200 events to prevent unbounded growth
        if events.count > 200 {
            events.removeFirst(events.count - 200)
        }
        lock.unlock()
    }

    /// Get the most recent event
    var lastEvent: HookEvent? {
        lock.lock()
        defer { lock.unlock() }
        return events.last
    }

    /// Get events since a given date
    func eventsSince(_ date: Date) -> [HookEvent] {
        lock.lock()
        defer { lock.unlock() }
        return events.filter { $0.timestamp >= date }
    }

    /// Clear event history
    func clearEvents() {
        lock.lock()
        events.removeAll()
        lock.unlock()
    }

    // MARK: - Private

    private func extractMessage(from event: WebhookEvent) -> String? {
        switch event.event {
        case .toolUseStart:
            if let tool = event.data?["tool_name"] as? String {
                return "Using \(tool)"
            }
        case .toolUseEnd:
            if let tool = event.data?["tool_name"] as? String {
                return "Done: \(tool)"
            }
        case .agentStop:
            if let reason = event.data?["stop_reason"] as? String {
                return "Stopped: \(reason)"
            }
        case .error:
            return event.data?["message"] as? String
        case .prompt:
            return event.data?["message"] as? String ?? "Waiting for input"
        case .notification:
            return event.data?["message"] as? String ?? event.data?["title"] as? String
        case .sessionStart:
            return "Session started"
        }
        return nil
    }
}

/// A structured event received through hooks
struct HookEvent {
    let timestamp: Date
    let type: WebhookEventType
    let toolName: String?
    let message: String?
    let rawData: [String: Any]?
}
