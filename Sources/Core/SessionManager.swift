import Foundation
import CommonCrypto

enum SessionManager {
    /// Maximum session name length to stay within tmux socket path limits.
    private static let maxSessionNameLength = 40

    /// Generate a stable persistent session name from a worktree path.
    /// Format: amux-<parent>-<name>, with dots and colons replaced by underscores.
    /// Names exceeding maxSessionNameLength are truncated with a hash suffix for uniqueness.
    static func persistentSessionName(for path: String) -> String {
        let url = URL(fileURLWithPath: path)
        let parent = url.deletingLastPathComponent().lastPathComponent
        let name = url.lastPathComponent
        let raw = "amux-\(parent)-\(name)"
            .replacingOccurrences(of: ".", with: "_")
            .replacingOccurrences(of: ":", with: "_")

        if raw.count <= maxSessionNameLength {
            return raw
        }

        let hash = shortHash(raw)
        let truncated = String(raw.prefix(maxSessionNameLength - hash.count - 1))
        return "\(truncated)-\(hash)"
    }

    /// Generate an indexed session name for an additional pane.
    static func indexedSessionName(base: String, index: Int) -> String {
        "\(base)-\(index)"
    }

    /// Kill a persistent session (tmux or zmx)
    static func killSession(_ name: String, backend: String) {
        DispatchQueue.global(qos: .utility).async {
            if backend == "tmux" {
                ProcessRunner.runSync(["tmux", "kill-session", "-t", name])
            } else {
                TerminalSurface.forceKillZmxSession(name)
            }
        }
    }

    /// Produce a short deterministic hash (6 hex chars) for session name deduplication.
    private static func shortHash(_ input: String) -> String {
        let data = Data(input.utf8)
        var digest = [UInt8](repeating: 0, count: Int(CC_SHA256_DIGEST_LENGTH))
        data.withUnsafeBytes { CC_SHA256($0.baseAddress, CC_LONG(data.count), &digest) }
        return digest.prefix(3).map { String(format: "%02x", $0) }.joined()
    }

    /// Resize a tmux session to match terminal grid size
    static func resizeTmuxSession(_ sessionName: String, cols: Int, rows: Int) {
        ProcessRunner.runSync(["tmux", "resize-window", "-t", sessionName, "-x", "\(cols)", "-y", "\(rows)"])
        ProcessRunner.runSync(["tmux", "refresh-client", "-t", sessionName, "-S"])
    }

    /// Refresh a tmux client display (auto-resize + refresh)
    static func refreshTmuxClient(_ sessionName: String) {
        ProcessRunner.runSync(["tmux", "resize-window", "-t", sessionName, "-A"])
        ProcessRunner.runSync(["tmux", "refresh-client", "-t", sessionName, "-S"])
    }
}
