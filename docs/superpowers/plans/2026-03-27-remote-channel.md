# RemoteChannel — External Platform Gateway for AgentHead

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enable AMUX to connect to remote AI agents (e.g. OpenCode running on a server) via SSE event streams, so that AgentHead can track and communicate with agents that don't run in a local terminal.

**Architecture:** A new `RemoteChannel` conforming to `AgentChannel` connects to a remote agent's SSE `/event` endpoint, translates incoming SSE events into `WebhookEvent`s, and sends commands via HTTP POST. A `GatewayState` enum tracks connection lifecycle (`disconnected → connecting → connected → error`). `AgentHead` gains a `registerRemote()` path that creates virtual agent entries without a `TerminalSurface`. The design borrows the SSE streaming pattern and gateway state machine from TeamClaw while fitting cleanly into AMUX's existing `AgentChannel` / `WebhookEvent` / `AgentHead` architecture.

**Tech Stack:** Swift 5.10, Foundation (URLSession for SSE streaming), NWConnection (optional), XCTest

---

## File Structure

| Action | File | Responsibility |
|--------|------|----------------|
| Create | `Sources/Core/GatewayState.swift` | Gateway lifecycle enum + state machine |
| Create | `Sources/Core/RemoteChannel.swift` | SSE client + HTTP command sender, conforms to `AgentChannel` |
| Create | `Sources/Core/RemoteAgentConfig.swift` | Codable config for remote agent connections |
| Modify | `Sources/Core/AgentChannel.swift` | Add `.remote` case to `AgentChannelType` |
| Modify | `Sources/Core/AgentHead.swift` | Add `registerRemote()` / remote agent lifecycle |
| Modify | `Sources/Core/AgentInfo.swift` | Make `surface` optional in spirit (already `weak`); no code change needed |
| Create | `tests/GatewayStateTests.swift` | Tests for state machine transitions |
| Create | `tests/RemoteChannelTests.swift` | Tests for SSE parsing, event translation, reconnect |
| Create | `tests/RemoteAgentRegistrationTests.swift` | Tests for AgentHead remote registration |

---

### Task 1: GatewayState — Connection Lifecycle Enum

**Files:**
- Create: `Sources/Core/GatewayState.swift`
- Create: `tests/GatewayStateTests.swift`

- [ ] **Step 1: Write failing tests for GatewayState**

```swift
// tests/GatewayStateTests.swift
import XCTest
@testable import amux

final class GatewayStateTests: XCTestCase {

    func testInitialStateIsDisconnected() {
        let sm = GatewayStateMachine()
        XCTAssertEqual(sm.state, .disconnected)
    }

    func testDisconnectedToConnecting() {
        var sm = GatewayStateMachine()
        let changed = sm.transition(to: .connecting)
        XCTAssertTrue(changed)
        XCTAssertEqual(sm.state, .connecting)
    }

    func testConnectingToConnected() {
        var sm = GatewayStateMachine()
        sm.transition(to: .connecting)
        let changed = sm.transition(to: .connected)
        XCTAssertTrue(changed)
        XCTAssertEqual(sm.state, .connected)
    }

    func testConnectingToError() {
        var sm = GatewayStateMachine()
        sm.transition(to: .connecting)
        let changed = sm.transition(to: .error("timeout"))
        XCTAssertTrue(changed)
        if case .error(let msg) = sm.state {
            XCTAssertEqual(msg, "timeout")
        } else {
            XCTFail("Expected error state")
        }
    }

    func testErrorToConnecting() {
        var sm = GatewayStateMachine()
        sm.transition(to: .connecting)
        sm.transition(to: .error("fail"))
        let changed = sm.transition(to: .connecting)
        XCTAssertTrue(changed)
        XCTAssertEqual(sm.state, .connecting)
    }

    func testConnectedToDisconnected() {
        var sm = GatewayStateMachine()
        sm.transition(to: .connecting)
        sm.transition(to: .connected)
        let changed = sm.transition(to: .disconnected)
        XCTAssertTrue(changed)
        XCTAssertEqual(sm.state, .disconnected)
    }

    func testSameStateReturnsFalse() {
        var sm = GatewayStateMachine()
        let changed = sm.transition(to: .disconnected)
        XCTAssertFalse(changed)
    }

    func testCannotGoDirectlyToConnected() {
        var sm = GatewayStateMachine()
        let changed = sm.transition(to: .connected)
        XCTAssertFalse(changed)
        XCTAssertEqual(sm.state, .disconnected)
    }

    func testIsConnectedProperty() {
        var sm = GatewayStateMachine()
        XCTAssertFalse(sm.isConnected)
        sm.transition(to: .connecting)
        sm.transition(to: .connected)
        XCTAssertTrue(sm.isConnected)
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/GatewayStateTests 2>&1 | tail -5`
Expected: Compilation error — `GatewayStateMachine` not defined

- [ ] **Step 3: Implement GatewayState**

```swift
// Sources/Core/GatewayState.swift
import Foundation

enum GatewayState: Equatable {
    case disconnected
    case connecting
    case connected
    case error(String)

    static func == (lhs: GatewayState, rhs: GatewayState) -> Bool {
        switch (lhs, rhs) {
        case (.disconnected, .disconnected),
             (.connecting, .connecting),
             (.connected, .connected):
            return true
        case (.error(let a), .error(let b)):
            return a == b
        default:
            return false
        }
    }
}

struct GatewayStateMachine {
    private(set) var state: GatewayState = .disconnected

    var isConnected: Bool { state == .connected }

    /// Transition to a new state. Returns true if the state changed.
    /// Enforces valid transitions:
    ///   disconnected → connecting
    ///   connecting   → connected | error | disconnected
    ///   connected    → disconnected | error
    ///   error        → connecting | disconnected
    @discardableResult
    mutating func transition(to newState: GatewayState) -> Bool {
        guard newState != state else { return false }

        let valid: Bool
        switch (state, newState) {
        case (.disconnected, .connecting):
            valid = true
        case (.connecting, .connected),
             (.connecting, .disconnected),
             (.connecting, .error):
            valid = true
        case (.connected, .disconnected),
             (.connected, .error):
            valid = true
        case (.error, .connecting),
             (.error, .disconnected):
            valid = true
        default:
            valid = false
        }

        guard valid else { return false }
        state = newState
        return true
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/GatewayStateTests 2>&1 | tail -5`
Expected: All tests PASS

- [ ] **Step 5: Commit**

```bash
git add Sources/Core/GatewayState.swift tests/GatewayStateTests.swift
git commit -m "feat: add GatewayState enum and state machine for remote channel lifecycle"
```

---

### Task 2: Extend AgentChannelType with `.remote`

**Files:**
- Modify: `Sources/Core/AgentChannel.swift:19-23`

- [ ] **Step 1: Add `.remote` case to AgentChannelType**

In `Sources/Core/AgentChannel.swift`, add the new case:

```swift
enum AgentChannelType: String {
    case zmx        // Default: read/write via zmx commands
    case tmux       // Fallback: read/write via tmux commands
    case hooks      // Claude Code hooks: structured events via webhook + backend input channel
    case remote     // Remote agent: SSE events + HTTP commands
}
```

- [ ] **Step 2: Verify build**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -3`
Expected: BUILD SUCCEEDED

- [ ] **Step 3: Commit**

```bash
git add Sources/Core/AgentChannel.swift
git commit -m "feat: add .remote case to AgentChannelType"
```

---

### Task 3: RemoteAgentConfig — Connection Configuration

**Files:**
- Create: `Sources/Core/RemoteAgentConfig.swift`

- [ ] **Step 1: Create RemoteAgentConfig**

```swift
// Sources/Core/RemoteAgentConfig.swift
import Foundation

/// Configuration for a remote agent connection.
/// Stored in the AMUX config file under "remoteAgents".
struct RemoteAgentConfig: Codable, Equatable {
    /// Display name for this remote agent (shown in dashboard)
    let name: String
    /// Base URL of the agent's API (e.g. "http://localhost:3000")
    let baseURL: String
    /// Worktree path this remote agent is associated with
    let worktreePath: String
    /// Project name for display
    let project: String
    /// Branch name
    let branch: String
    /// Agent type (defaults to openCode)
    var agentType: String?
    /// Whether to auto-connect on app launch
    var autoConnect: Bool?
    /// Reconnect interval in seconds (0 = no auto-reconnect)
    var reconnectInterval: TimeInterval?

    /// Resolved agent type enum
    var resolvedAgentType: AgentType {
        guard let raw = agentType else { return .openCode }
        return AgentType(rawValue: raw) ?? .openCode
    }

    /// Resolved reconnect interval (default 30s)
    var resolvedReconnectInterval: TimeInterval {
        reconnectInterval ?? 30.0
    }

    /// SSE event endpoint
    var eventURL: URL? {
        URL(string: baseURL)?.appendingPathComponent("event")
    }

    /// Session prompt endpoint
    func promptURL(sessionId: String) -> URL? {
        URL(string: baseURL)?
            .appendingPathComponent("session")
            .appendingPathComponent(sessionId)
            .appendingPathComponent("prompt_async")
    }
}
```

- [ ] **Step 2: Verify build**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -3`
Expected: BUILD SUCCEEDED

- [ ] **Step 3: Commit**

```bash
git add Sources/Core/RemoteAgentConfig.swift
git commit -m "feat: add RemoteAgentConfig for remote agent connection settings"
```

---

### Task 4: RemoteChannel — SSE Client + HTTP Command Sender

**Files:**
- Create: `Sources/Core/RemoteChannel.swift`
- Create: `tests/RemoteChannelTests.swift`

- [ ] **Step 1: Write failing tests for SSE line parsing and event translation**

```swift
// tests/RemoteChannelTests.swift
import XCTest
@testable import amux

final class RemoteChannelTests: XCTestCase {

    // MARK: - SSE Line Parsing

    func testParseSSEDataLine() {
        let line = "data: {\"type\":\"message.updated\",\"session_id\":\"s1\"}"
        let result = SSEParser.parseDataLine(line)
        XCTAssertNotNil(result)
        XCTAssertEqual(result?["type"] as? String, "message.updated")
    }

    func testParseSSEIgnoresNonDataLines() {
        XCTAssertNil(SSEParser.parseDataLine("event: message"))
        XCTAssertNil(SSEParser.parseDataLine(": comment"))
        XCTAssertNil(SSEParser.parseDataLine(""))
    }

    func testParseSSEIgnoresMalformedJSON() {
        let line = "data: not-json"
        XCTAssertNil(SSEParser.parseDataLine(line))
    }

    // MARK: - SSE Event to WebhookEvent Translation

    func testTranslateSessionStartEvent() {
        let sseData: [String: Any] = [
            "type": "session.created",
            "properties": ["session_id": "sess_123", "cwd": "/tmp/project"]
        ]
        let event = SSEEventTranslator.translate(sseData)
        XCTAssertNotNil(event)
        XCTAssertEqual(event?.event, .sessionStart)
        XCTAssertEqual(event?.sessionId, "sess_123")
        XCTAssertEqual(event?.cwd, "/tmp/project")
    }

    func testTranslatePermissionAskedEvent() {
        let sseData: [String: Any] = [
            "type": "permission.asked",
            "properties": [
                "session_id": "sess_123",
                "permission_id": "perm_1",
                "tool_name": "Bash"
            ]
        ]
        let event = SSEEventTranslator.translate(sseData)
        // permission.asked maps to .prompt (waiting for approval)
        XCTAssertNotNil(event)
        XCTAssertEqual(event?.event, .prompt)
    }

    func testTranslateMessageUpdatedEvent() {
        let sseData: [String: Any] = [
            "type": "message.updated",
            "properties": [
                "session_id": "sess_123",
                "role": "assistant",
                "cwd": "/tmp/project"
            ]
        ]
        let event = SSEEventTranslator.translate(sseData)
        XCTAssertNotNil(event)
        XCTAssertEqual(event?.event, .toolUseEnd)
        XCTAssertEqual(event?.sessionId, "sess_123")
    }

    func testTranslateUnknownEventReturnsNil() {
        let sseData: [String: Any] = [
            "type": "unknown.event",
            "properties": ["session_id": "s1"]
        ]
        XCTAssertNil(SSEEventTranslator.translate(sseData))
    }

    // MARK: - SSE Buffer Splitting

    func testBufferSplitByDoubleNewline() {
        let buffer = "data: {\"a\":1}\n\ndata: {\"b\":2}\n\n"
        let events = SSEParser.splitEvents(buffer)
        XCTAssertEqual(events.count, 2)
    }

    func testBufferIncompleteEventNotEmitted() {
        let result = SSEParser.splitEventsWithRemainder("data: {\"a\":1}\n\ndata: {\"b\":")
        XCTAssertEqual(result.complete.count, 1)
        XCTAssertEqual(result.remainder, "data: {\"b\":")
    }

    // MARK: - Gateway State Integration

    func testChannelTypeIsRemote() {
        let config = RemoteAgentConfig(
            name: "test", baseURL: "http://localhost:3000",
            worktreePath: "/tmp/repo", project: "TestProject", branch: "main"
        )
        let channel = RemoteChannel(config: config)
        XCTAssertEqual(channel.channelType, .remote)
        XCTAssertTrue(channel.supportsStructuredEvents)
    }

    func testInitialGatewayStateIsDisconnected() {
        let config = RemoteAgentConfig(
            name: "test", baseURL: "http://localhost:3000",
            worktreePath: "/tmp/repo", project: "TestProject", branch: "main"
        )
        let channel = RemoteChannel(config: config)
        XCTAssertEqual(channel.gatewayState, .disconnected)
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/RemoteChannelTests 2>&1 | tail -5`
Expected: Compilation error — `SSEParser`, `SSEEventTranslator`, `RemoteChannel` not defined

- [ ] **Step 3: Implement SSEParser**

```swift
// Sources/Core/RemoteChannel.swift
import Foundation

// MARK: - SSE Parsing

/// Parses Server-Sent Events (SSE) text stream into JSON payloads.
enum SSEParser {
    /// Extract JSON from a single "data: {...}" line. Returns nil for non-data lines.
    static func parseDataLine(_ line: String) -> [String: Any]? {
        guard line.hasPrefix("data: ") else { return nil }
        let jsonStr = String(line.dropFirst(6))
        guard let data = jsonStr.data(using: .utf8),
              let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            return nil
        }
        return json
    }

    /// Split a buffer into complete SSE events (separated by "\n\n").
    static func splitEvents(_ buffer: String) -> [String] {
        buffer.components(separatedBy: "\n\n").filter { !$0.isEmpty }
    }

    /// Split buffer, returning complete events and the incomplete remainder.
    static func splitEventsWithRemainder(_ buffer: String) -> (complete: [String], remainder: String) {
        let parts = buffer.components(separatedBy: "\n\n")
        guard parts.count > 1 else {
            return (complete: [], remainder: buffer)
        }
        let complete = Array(parts.dropLast()).filter { !$0.isEmpty }
        let remainder = parts.last ?? ""
        return (complete: complete, remainder: remainder)
    }
}

// MARK: - SSE → WebhookEvent Translation

/// Translates SSE event payloads (OpenCode format) into AMUX WebhookEvents.
/// OpenCode SSE events have: {"type": "...", "properties": {...}}
/// TeamClaw maps these through session tracking and permission approval.
/// AMUX maps them to WebhookEventType for unified status tracking.
enum SSEEventTranslator {
    /// Map from OpenCode SSE event type to AMUX WebhookEventType
    private static let typeMapping: [String: WebhookEventType] = [
        "session.created": .sessionStart,
        "message.updated": .toolUseEnd,
        "permission.asked": .prompt,
        "question.asked": .prompt,
    ]

    /// Translate an SSE JSON payload to a WebhookEvent.
    /// Returns nil for unmapped event types.
    static func translate(_ sseData: [String: Any]) -> WebhookEvent? {
        guard let type = sseData["type"] as? String,
              let eventType = typeMapping[type] else {
            return nil
        }

        let properties = sseData["properties"] as? [String: Any] ?? [:]
        let sessionId = properties["session_id"] as? String ?? ""
        let cwd = properties["cwd"] as? String ?? ""

        return WebhookEvent(
            source: "remote",
            sessionId: sessionId,
            event: eventType,
            cwd: cwd,
            timestamp: nil,
            data: properties
        )
    }
}

// MARK: - RemoteChannel

/// Communication channel for a remote AI agent via SSE event stream + HTTP commands.
/// Mirrors TeamClaw's gateway pattern: connect SSE first, send commands via HTTP POST,
/// auto-approve permissions, track session state.
class RemoteChannel: AgentChannel {
    let channelType: AgentChannelType = .remote
    let supportsStructuredEvents = true

    let config: RemoteAgentConfig
    private(set) var gatewayState: GatewayState = .disconnected
    private var stateMachine = GatewayStateMachine()

    /// Accumulated events (same pattern as HooksChannel)
    private let lock = NSLock()
    private(set) var events: [HookEvent] = []

    /// SSE streaming state
    private var sseTask: URLSessionDataTask?
    private var sseSession: URLSession?
    private var sseBuffer = ""
    private var reconnectTimer: Timer?

    /// Current OpenCode session ID (learned from SSE events or set externally)
    var sessionId: String?

    /// Callback when gateway state changes
    var onStateChange: ((GatewayState) -> Void)?

    /// Callback when a WebhookEvent is translated from SSE
    var onEvent: ((WebhookEvent) -> Void)?

    init(config: RemoteAgentConfig) {
        self.config = config
    }

    deinit {
        disconnect()
    }

    // MARK: - AgentChannel

    /// Send a command to the remote agent via HTTP POST.
    /// Uses the OpenCode prompt_async endpoint (same as TeamClaw).
    func sendCommand(_ command: String) {
        guard let sid = sessionId,
              let url = config.promptURL(sessionId: sid) else {
            NSLog("[RemoteChannel] Cannot send command: no session ID or invalid URL")
            return
        }

        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")

        let body: [String: Any] = [
            "parts": [["type": "text", "text": command]]
        ]
        request.httpBody = try? JSONSerialization.data(withJSONObject: body)

        URLSession.shared.dataTask(with: request) { _, response, error in
            if let error {
                NSLog("[RemoteChannel] Send command error: \(error)")
            }
        }.resume()
    }

    /// Remote channels don't have a terminal to read from.
    /// Returns the last event messages as a substitute.
    func readOutput(lines: Int) -> String? {
        lock.lock()
        defer { lock.unlock() }
        let recent = events.suffix(lines)
        let messages = recent.compactMap(\.message)
        return messages.isEmpty ? nil : messages.joined(separator: "\n")
    }

    // MARK: - Connection Lifecycle

    func connect() {
        guard stateMachine.transition(to: .connecting) else { return }
        gatewayState = stateMachine.state
        onStateChange?(gatewayState)

        guard let url = config.eventURL else {
            updateState(.error("Invalid base URL: \(config.baseURL)"))
            return
        }

        NSLog("[RemoteChannel] Connecting to SSE: \(url)")

        let sessionConfig = URLSessionConfiguration.default
        sessionConfig.timeoutIntervalForRequest = 900 // 15 min (matches TeamClaw)
        sessionConfig.timeoutIntervalForResource = 0  // No resource timeout for SSE

        let delegate = SSEDelegate(channel: self)
        sseSession = URLSession(configuration: sessionConfig, delegate: delegate, delegateQueue: nil)

        var request = URLRequest(url: url)
        request.setValue("text/event-stream", forHTTPHeaderField: "Accept")
        request.setValue("no-cache", forHTTPHeaderField: "Cache-Control")

        sseTask = sseSession?.dataTask(with: request)
        sseTask?.resume()
    }

    func disconnect() {
        reconnectTimer?.invalidate()
        reconnectTimer = nil
        sseTask?.cancel()
        sseTask = nil
        sseSession?.invalidateAndCancel()
        sseSession = nil
        sseBuffer = ""
        updateState(.disconnected)
    }

    // MARK: - SSE Data Processing

    /// Called by SSEDelegate when data arrives.
    func handleSSEData(_ data: Data) {
        guard let text = String(data: data, encoding: .utf8) else { return }

        sseBuffer += text
        let result = SSEParser.splitEventsWithRemainder(sseBuffer)
        sseBuffer = result.remainder

        for eventText in result.complete {
            // Each SSE event block may have multiple lines; find the data line
            for line in eventText.components(separatedBy: "\n") {
                guard let json = SSEParser.parseDataLine(line) else { continue }
                processSSEPayload(json)
            }
        }

        // Mark connected on first successful data
        if stateMachine.state == .connecting {
            updateState(.connected)
        }
    }

    /// Called by SSEDelegate on stream completion or error.
    func handleSSEComplete(error: Error?) {
        if let error {
            NSLog("[RemoteChannel] SSE stream error: \(error)")
            updateState(.error(error.localizedDescription))
        } else {
            updateState(.disconnected)
        }
        scheduleReconnect()
    }

    // MARK: - Private

    private func processSSEPayload(_ json: [String: Any]) {
        guard let event = SSEEventTranslator.translate(json) else { return }

        // Track session ID from session.created events
        if event.event == .sessionStart, !event.sessionId.isEmpty {
            sessionId = event.sessionId
        }

        // Store as HookEvent (same as HooksChannel)
        let hookEvent = HookEvent(
            timestamp: Date(),
            type: event.event,
            toolName: event.data?["tool_name"] as? String,
            message: extractMessage(from: event),
            rawData: event.data
        )

        lock.lock()
        events.append(hookEvent)
        if events.count > 200 {
            events.removeFirst(events.count - 200)
        }
        lock.unlock()

        // Notify AgentHead
        onEvent?(event)
    }

    private func extractMessage(from event: WebhookEvent) -> String? {
        switch event.event {
        case .sessionStart:  return "Session started"
        case .toolUseStart:  return event.data?["tool_name"] as? String
        case .toolUseEnd:    return "Processing"
        case .agentStop:     return "Stopped"
        case .prompt:        return "Waiting for input"
        case .error:         return event.data?["message"] as? String
        default:             return nil
        }
    }

    private func updateState(_ newState: GatewayState) {
        guard stateMachine.transition(to: newState) else { return }
        gatewayState = stateMachine.state
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            self.onStateChange?(self.gatewayState)
        }
    }

    private func scheduleReconnect() {
        let interval = config.resolvedReconnectInterval
        guard interval > 0 else { return }

        NSLog("[RemoteChannel] Scheduling reconnect in \(interval)s")
        DispatchQueue.main.async { [weak self] in
            self?.reconnectTimer = Timer.scheduledTimer(withTimeInterval: interval, repeats: false) { [weak self] _ in
                self?.connect()
            }
        }
    }

    // MARK: - Event Query (mirrors HooksChannel)

    var lastEvent: HookEvent? {
        lock.lock()
        defer { lock.unlock() }
        return events.last
    }

    func eventsSince(_ date: Date) -> [HookEvent] {
        lock.lock()
        defer { lock.unlock() }
        return events.filter { $0.timestamp >= date }
    }

    func clearEvents() {
        lock.lock()
        events.removeAll()
        lock.unlock()
    }
}

// MARK: - SSE URLSession Delegate

/// Handles streaming SSE data from URLSession.
private class SSEDelegate: NSObject, URLSessionDataDelegate {
    weak var channel: RemoteChannel?

    init(channel: RemoteChannel) {
        self.channel = channel
    }

    func urlSession(_ session: URLSession, dataTask: URLSessionDataTask, didReceive data: Data) {
        channel?.handleSSEData(data)
    }

    func urlSession(_ session: URLSession, task: URLSessionTask, didCompleteWithError error: Error?) {
        // Don't treat cancellation as an error
        if let urlError = error as? URLError, urlError.code == .cancelled {
            return
        }
        channel?.handleSSEComplete(error: error)
    }

    func urlSession(_ session: URLSession, dataTask: URLSessionDataTask,
                     didReceive response: URLResponse,
                     completionHandler: @escaping (URLSession.ResponseDisposition) -> Void) {
        // Accept the response and allow streaming
        completionHandler(.allow)
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/RemoteChannelTests 2>&1 | tail -5`
Expected: All tests PASS

- [ ] **Step 5: Commit**

```bash
git add Sources/Core/RemoteChannel.swift tests/RemoteChannelTests.swift
git commit -m "feat: implement RemoteChannel with SSE streaming and event translation"
```

---

### Task 5: AgentHead — Remote Agent Registration

**Files:**
- Modify: `Sources/Core/AgentHead.swift`
- Create: `tests/RemoteAgentRegistrationTests.swift`

- [ ] **Step 1: Write failing tests for remote registration**

```swift
// tests/RemoteAgentRegistrationTests.swift
import XCTest
@testable import amux

final class RemoteAgentRegistrationTests: XCTestCase {

    override func setUp() {
        super.setUp()
        for agent in AgentHead.shared.allAgents() {
            AgentHead.shared.unregister(terminalID: agent.id)
        }
    }

    override func tearDown() {
        for agent in AgentHead.shared.allAgents() {
            AgentHead.shared.unregister(terminalID: agent.id)
        }
        super.tearDown()
    }

    func testRegisterRemoteAgent() {
        let config = RemoteAgentConfig(
            name: "test-remote", baseURL: "http://localhost:3000",
            worktreePath: "/tmp/remote-repo", project: "RemoteProject", branch: "main"
        )
        let id = AgentHead.shared.registerRemote(config: config)

        let agent = AgentHead.shared.agent(for: id)
        XCTAssertNotNil(agent)
        XCTAssertEqual(agent?.worktreePath, "/tmp/remote-repo")
        XCTAssertEqual(agent?.project, "RemoteProject")
        XCTAssertEqual(agent?.branch, "main")
        XCTAssertEqual(agent?.agentType, .openCode)
        XCTAssertNil(agent?.surface)
        XCTAssertNotNil(agent?.channel)
        XCTAssertEqual(agent?.channel?.channelType, .remote)
    }

    func testRemoteAgentAppearsInAllAgents() {
        let config = RemoteAgentConfig(
            name: "test-remote", baseURL: "http://localhost:3000",
            worktreePath: "/tmp/remote-repo", project: "RemoteProject", branch: "main"
        )
        AgentHead.shared.registerRemote(config: config)

        XCTAssertEqual(AgentHead.shared.allAgents().count, 1)
    }

    func testRemoteAgentWorktreeIndexLookup() {
        let config = RemoteAgentConfig(
            name: "test-remote", baseURL: "http://localhost:3000",
            worktreePath: "/tmp/remote-repo", project: "RemoteProject", branch: "main"
        )
        AgentHead.shared.registerRemote(config: config)

        let agent = AgentHead.shared.agent(forWorktree: "/tmp/remote-repo")
        XCTAssertNotNil(agent)
        XCTAssertEqual(agent?.project, "RemoteProject")
    }

    func testUnregisterRemoteAgent() {
        let config = RemoteAgentConfig(
            name: "test-remote", baseURL: "http://localhost:3000",
            worktreePath: "/tmp/remote-repo", project: "RemoteProject", branch: "main"
        )
        let id = AgentHead.shared.registerRemote(config: config)
        AgentHead.shared.unregister(terminalID: id)

        XCTAssertEqual(AgentHead.shared.allAgents().count, 0)
    }

    func testRemoteChannelReceivesEventCallback() {
        let config = RemoteAgentConfig(
            name: "test-remote", baseURL: "http://localhost:3000",
            worktreePath: "/tmp/remote-repo", project: "RemoteProject", branch: "main"
        )
        let id = AgentHead.shared.registerRemote(config: config)

        let channel = AgentHead.shared.channel(for: id) as? RemoteChannel
        XCTAssertNotNil(channel)
        XCTAssertNotNil(channel?.onEvent, "onEvent callback should be wired up")
    }

    func testRemoteAgentCustomAgentType() {
        let config = RemoteAgentConfig(
            name: "test-remote", baseURL: "http://localhost:3000",
            worktreePath: "/tmp/remote-repo", project: "RemoteProject", branch: "main",
            agentType: "claudeCode"
        )
        let id = AgentHead.shared.registerRemote(config: config)

        let agent = AgentHead.shared.agent(for: id)
        XCTAssertEqual(agent?.agentType, .claudeCode)
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/RemoteAgentRegistrationTests 2>&1 | tail -5`
Expected: Compilation error — `registerRemote` not defined on AgentHead

- [ ] **Step 3: Add `registerRemote()` to AgentHead**

Add the following to `Sources/Core/AgentHead.swift`, after the existing `register()` method (after line 73):

```swift
    // MARK: - Remote Agent Registration

    /// Register a remote agent (no TerminalSurface).
    /// Returns a synthetic terminal ID for use in subsequent calls.
    @discardableResult
    func registerRemote(config: RemoteAgentConfig) -> String {
        lock.lock()
        defer { lock.unlock() }

        let remoteID = "remote-\(UUID().uuidString)"

        let channel = RemoteChannel(config: config)
        channels[remoteID] = channel

        // Wire up event callback to route through AgentHead
        channel.onEvent = { [weak self] event in
            self?.handleWebhookEvent(event)
        }

        let info = AgentInfo(
            id: remoteID,
            worktreePath: config.worktreePath,
            agentType: config.resolvedAgentType,
            project: config.project,
            branch: config.branch,
            status: .unknown,
            lastMessage: "",
            commandLine: nil,
            roundDuration: 0,
            startedAt: Date(),
            surface: nil,
            channel: channel,
            taskProgress: TaskProgress()
        )
        agents[remoteID] = info

        var ids = worktreeIndex[config.worktreePath] ?? []
        if !ids.contains(remoteID) {
            ids.append(remoteID)
        }
        worktreeIndex[config.worktreePath] = ids

        if !orderedIDs.contains(remoteID) {
            orderedIDs.append(remoteID)
        }

        return remoteID
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/RemoteAgentRegistrationTests 2>&1 | tail -5`
Expected: All tests PASS

- [ ] **Step 5: Run full test suite to verify no regressions**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test 2>&1 | grep -E "(Test Suite|Tests|Passed|Failed)" | tail -10`
Expected: All existing tests still PASS

- [ ] **Step 6: Commit**

```bash
git add Sources/Core/AgentHead.swift tests/RemoteAgentRegistrationTests.swift
git commit -m "feat: add registerRemote() to AgentHead for remote agent lifecycle"
```

---

### Task 6: Wire Remote Channel Events into StatusPublisher

**Files:**
- Modify: `Sources/Status/StatusPublisher.swift` (minor — remote agents route through `AgentHead.handleWebhookEvent` which already feeds the status pipeline)
- Modify: `Sources/Status/WebhookStatusProvider.swift` (add remote source recognition)

This task ensures the existing status pipeline handles events from RemoteChannel correctly. The key insight is that `RemoteChannel.onEvent` calls `AgentHead.handleWebhookEvent()`, which already routes to `HooksChannel` via worktree matching. For remote agents, we need the event to route to the `RemoteChannel` instead.

- [ ] **Step 1: Update `AgentHead.handleWebhookEvent` to handle RemoteChannel**

In `Sources/Core/AgentHead.swift`, modify `handleWebhookEvent()` (currently line 272-286) to also check for `RemoteChannel`:

Replace the existing `handleWebhookEvent` method:

```swift
    /// Route a webhook event to the appropriate channel based on cwd matching.
    /// Supports both HooksChannel (local) and RemoteChannel (remote).
    func handleWebhookEvent(_ event: WebhookEvent) {
        lock.lock()
        // Find the agent whose worktree path matches the event's cwd
        let matchingTIDs = worktreeIndex.first { (worktreePath, _) in
            event.cwd == worktreePath || event.cwd.hasPrefix(worktreePath + "/")
        }?.value
        guard let tid = matchingTIDs?.first,
              let channel = channels[tid] else {
            lock.unlock()
            return
        }
        lock.unlock()

        if let hooks = channel as? HooksChannel {
            hooks.handleWebhookEvent(event)
        } else if let remote = channel as? RemoteChannel {
            // RemoteChannel already processed the event internally;
            // update AgentHead status directly
            let status = event.event.agentStatus(data: event.data)
            let message = extractRemoteMessage(from: event)
            updateStatus(terminalID: tid, status: status, lastMessage: message, roundDuration: 0)
        }
    }

    private func extractRemoteMessage(from event: WebhookEvent) -> String {
        switch event.event {
        case .sessionStart:  return "Session started"
        case .toolUseStart:
            return event.data?["tool_name"] as? String ?? "Working"
        case .toolUseEnd:
            return event.data?["tool_name"] as? String ?? "Processing"
        case .agentStop:
            return event.data?["stop_reason"] as? String ?? "Done"
        case .prompt:
            return event.data?["message"] as? String ?? "Waiting for input"
        case .error:
            return event.data?["message"] as? String ?? "Error"
        case .notification:
            return event.data?["message"] as? String ?? ""
        default:
            return ""
        }
    }
```

- [ ] **Step 2: Run full test suite**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test 2>&1 | grep -E "(Test Suite|Passed|Failed)" | tail -10`
Expected: All tests PASS

- [ ] **Step 3: Commit**

```bash
git add Sources/Core/AgentHead.swift
git commit -m "feat: route remote channel SSE events through AgentHead status pipeline"
```

---

### Task 7: Add Remote Agents to Config Persistence

**Files:**
- Modify: `Sources/Core/Config.swift` (add `remoteAgents` array)

- [ ] **Step 1: Find the Config struct and add remoteAgents field**

Read `Sources/Core/Config.swift` to find the struct definition. Add:

```swift
    var remoteAgents: [RemoteAgentConfig]?
```

Use `decodeIfPresent` in the decoder (following the existing pattern in Config) so older config files without this field still load.

- [ ] **Step 2: Verify build**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -3`
Expected: BUILD SUCCEEDED

- [ ] **Step 3: Run existing ConfigTests**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/ConfigTests 2>&1 | tail -5`
Expected: All tests PASS (backward compatible due to `decodeIfPresent`)

- [ ] **Step 4: Commit**

```bash
git add Sources/Core/Config.swift
git commit -m "feat: add remoteAgents config for persistent remote agent connections"
```

---

## Summary

| Task | What it builds | Key pattern borrowed from TeamClaw |
|------|---------------|-------------------------------------|
| 1 | `GatewayStateMachine` | Gateway status lifecycle (`disconnected → connecting → connected → error`) |
| 2 | `.remote` channel type | Channel type extensibility |
| 3 | `RemoteAgentConfig` | Per-channel config with baseURL, reconnect settings |
| 4 | `RemoteChannel` | SSE streaming client, event translation, auto-reconnect |
| 5 | `AgentHead.registerRemote()` | Virtual agent registration without terminal |
| 6 | Event routing | SSE events → status pipeline (like TeamClaw's SSE → session → response) |
| 7 | Config persistence | Store remote connections in config (like TeamClaw's `teamclaw.json`) |

**Not included (future work):**
- Permission auto-approval (TeamClaw's `POST /permission/{id}/reply`) — add when we have a concrete remote agent that needs it
- Message deduplication (`ProcessedMessageTracker`) — add if we see duplicate events in practice
- Keep-alive health check — the reconnect timer covers basic resilience; periodic pings can be added later
- UI for managing remote agent connections — dashboard cards already render from `AgentInfo`; adding a config dialog is a separate feature
