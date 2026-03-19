import Foundation
import Network

class WebhookServer {
    private var listener: NWListener?
    private let port: UInt16
    private let onEvent: (WebhookEvent) -> Void
    private let queue = DispatchQueue(label: "pmux.webhook-server")

    init(port: UInt16, onEvent: @escaping (WebhookEvent) -> Void) {
        self.port = port
        self.onEvent = onEvent
    }

    func start() {
        do {
            let params = NWParameters.tcp
            params.requiredLocalEndpoint = NWEndpoint.hostPort(host: .ipv4(.loopback), port: NWEndpoint.Port(rawValue: port)!)
            listener = try NWListener(using: params, on: NWEndpoint.Port(rawValue: port)!)
        } catch {
            NSLog("[WebhookServer] Failed to create listener: \(error)")
            return
        }

        listener?.newConnectionHandler = { [weak self] connection in
            self?.handleConnection(connection)
        }

        listener?.stateUpdateHandler = { [weak self] state in
            guard let self = self else { return }
            switch state {
            case .ready:
                NSLog("[WebhookServer] Listening on port \(self.port)")
            case .failed(let error):
                NSLog("[WebhookServer] Failed: \(error)")
            default:
                break
            }
        }

        listener?.start(queue: queue)
    }

    func stop() {
        listener?.cancel()
        listener = nil
    }

    private func handleConnection(_ connection: NWConnection) {
        connection.start(queue: queue)
        receiveData(connection: connection, buffer: Data())
    }

    private func receiveData(connection: NWConnection, buffer: Data) {
        connection.receive(minimumIncompleteLength: 1, maximumLength: 65536) { [weak self] data, _, isComplete, error in
            guard let self = self else { return }

            var accumulated = buffer
            if let data = data {
                accumulated.append(data)
            }

            if isComplete || error != nil {
                self.processHTTPRequest(data: accumulated, connection: connection)
            } else {
                if self.hasCompleteHTTPRequest(accumulated) {
                    self.processHTTPRequest(data: accumulated, connection: connection)
                } else {
                    self.receiveData(connection: connection, buffer: accumulated)
                }
            }
        }
    }

    private func hasCompleteHTTPRequest(_ data: Data) -> Bool {
        let message = CFHTTPMessageCreateEmpty(kCFAllocatorDefault, true).takeRetainedValue()
        CFHTTPMessageAppendBytes(message, [UInt8](data), data.count)
        guard CFHTTPMessageIsHeaderComplete(message) else { return false }
        guard let contentLengthStr = CFHTTPMessageCopyHeaderFieldValue(message, "Content-Length" as CFString)?.takeRetainedValue() as String?,
              let contentLength = Int(contentLengthStr) else {
            return true  // No Content-Length means no body expected
        }
        let body = CFHTTPMessageCopyBody(message)?.takeRetainedValue() as Data?
        return (body?.count ?? 0) >= contentLength
    }

    private func processHTTPRequest(data: Data, connection: NWConnection) {
        let message = CFHTTPMessageCreateEmpty(kCFAllocatorDefault, true).takeRetainedValue()
        CFHTTPMessageAppendBytes(message, [UInt8](data), data.count)

        guard CFHTTPMessageIsHeaderComplete(message) else {
            sendResponse(connection: connection, statusCode: 400, body: "Bad Request")
            return
        }

        let method = CFHTTPMessageCopyRequestMethod(message)?.takeRetainedValue() as String? ?? ""
        let url = CFHTTPMessageCopyRequestURL(message)?.takeRetainedValue() as URL?
        let path = url?.path ?? ""
        let body = CFHTTPMessageCopyBody(message)?.takeRetainedValue() as Data?

        guard method == "POST", path == "/webhook" else {
            sendResponse(connection: connection, statusCode: 404, body: "Not Found")
            return
        }

        guard let body = body else {
            sendResponse(connection: connection, statusCode: 400, body: "Missing body")
            return
        }

        do {
            let event = try WebhookEvent.parse(from: body)
            onEvent(event)
            sendResponse(connection: connection, statusCode: 200, body: "")
        } catch {
            NSLog("[WebhookServer] Parse error: \(error)")
            sendResponse(connection: connection, statusCode: 400, body: "Bad Request")
        }
    }

    private func sendResponse(connection: NWConnection, statusCode: Int, body: String) {
        let statusText: String
        switch statusCode {
        case 200: statusText = "OK"
        case 400: statusText = "Bad Request"
        case 404: statusText = "Not Found"
        default: statusText = "Error"
        }

        let response = "HTTP/1.1 \(statusCode) \(statusText)\r\nContent-Length: \(body.utf8.count)\r\nConnection: close\r\n\r\n\(body)"
        let responseData = response.data(using: .utf8)!

        connection.send(content: responseData, completion: .contentProcessed { _ in
            connection.cancel()
        })
    }

    deinit {
        stop()
    }
}
