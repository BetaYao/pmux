import Foundation

enum SessionManager {
    /// Generate a stable persistent session name from a worktree path.
    /// Format: pmux-<parent>-<name>, with dots and colons replaced by underscores.
    static func persistentSessionName(for path: String) -> String {
        let url = URL(fileURLWithPath: path)
        let parent = url.deletingLastPathComponent().lastPathComponent
        let name = url.lastPathComponent
        return "pmux-\(parent)-\(name)"
            .replacingOccurrences(of: ".", with: "_")
            .replacingOccurrences(of: ":", with: "_")
    }

    /// Kill a persistent session (tmux or zmx)
    static func killSession(_ name: String, backend: String) {
        DispatchQueue.global(qos: .utility).async {
            if backend == "tmux" {
                ProcessRunner.runSync(["tmux", "kill-session", "-t", name])
            } else {
                ProcessRunner.runSync(["zmx", "kill", name])
            }
        }
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
