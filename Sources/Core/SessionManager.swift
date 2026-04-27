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

    static func sessionNames(in layout: CodableSplitNode) -> [String] {
        switch layout {
        case .leaf(let sessionName):
            return [sessionName]
        case .split(_, _, let first, let second):
            return sessionNames(in: first) + sessionNames(in: second)
        }
    }

    static func expectedSessionNames(config: Config, discoveredWorktreePaths: [String]) -> Set<String> {
        var names = Set(discoveredWorktreePaths.map { persistentSessionName(for: $0) })
        for layout in config.splitLayouts.values {
            names.formUnion(sessionNames(in: layout))
        }
        return names.filter { $0.hasPrefix("amux-") }
    }

    static func parseZmxSessionNames(listOutput: String) -> [String] {
        listOutput
            .components(separatedBy: .newlines)
            .compactMap { line in
                let trimmed = line.trimmingCharacters(in: .whitespacesAndNewlines)
                guard !trimmed.isEmpty else { return nil }

                if let range = trimmed.range(of: "name=") {
                    let suffix = trimmed[range.upperBound...]
                    let end = suffix.firstIndex(where: \.isWhitespace) ?? suffix.endIndex
                    let name = String(suffix[..<end])
                    return name.isEmpty ? nil : name
                }

                let fields = trimmed.split(whereSeparator: \.isWhitespace)
                guard let first = fields.first else { return nil }
                let candidate = String(first)
                return candidate.isEmpty ? nil : candidate
            }
    }

    static func orphanZmxSessionNames(activeSessionNames: Set<String>, listOutput: String) -> [String] {
        parseZmxSessionNames(listOutput: listOutput)
            .filter { $0.hasPrefix("amux-") && !activeSessionNames.contains($0) }
    }

    @discardableResult
    static func cleanupOrphanZmxSessions(
        activeSessionNames: Set<String>,
        listOutput: String? = nil
    ) -> [String] {
        let output = listOutput ?? ProcessRunner.output(["zmx", "list"]) ?? ""
        let orphaned = orphanZmxSessionNames(activeSessionNames: activeSessionNames, listOutput: output)
        for sessionName in orphaned {
            TerminalSurface.forceKillZmxSession(sessionName)
        }
        return orphaned
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
