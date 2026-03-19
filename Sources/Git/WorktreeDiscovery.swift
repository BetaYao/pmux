import Foundation

struct WorktreeInfo {
    let path: String
    let branch: String
    let commitHash: String
    let isMainWorktree: Bool

    var displayName: String {
        if isMainWorktree {
            return URL(fileURLWithPath: path).lastPathComponent
        }
        return branch.isEmpty ? URL(fileURLWithPath: path).lastPathComponent : branch
    }
}

enum WorktreeDiscovery {
    /// Find the git toplevel (repo root) from any path inside the repo
    static func findRepoRoot(from path: String) -> String? {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/git")
        process.arguments = ["rev-parse", "--show-toplevel"]
        process.currentDirectoryURL = URL(fileURLWithPath: path)

        let pipe = Pipe()
        process.standardOutput = pipe
        process.standardError = Pipe()

        do {
            try process.run()
            process.waitUntilExit()
        } catch {
            return nil
        }

        guard process.terminationStatus == 0 else { return nil }
        let data = pipe.fileHandleForReading.readDataToEndOfFile()
        return String(data: data, encoding: .utf8)?.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    /// Discover all worktrees for a given repository path
    static func discover(repoPath: String) -> [WorktreeInfo] {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/git")
        process.arguments = ["worktree", "list", "--porcelain"]
        process.currentDirectoryURL = URL(fileURLWithPath: repoPath)

        let pipe = Pipe()
        process.standardOutput = pipe
        process.standardError = Pipe()

        do {
            try process.run()
            process.waitUntilExit()
        } catch {
            NSLog("Failed to run git worktree list: \(error)")
            return []
        }

        guard process.terminationStatus == 0 else {
            return []
        }

        let data = pipe.fileHandleForReading.readDataToEndOfFile()
        guard let output = String(data: data, encoding: .utf8) else { return [] }

        return parsePorcelain(output)
    }

    /// Parse `git worktree list --porcelain` output
    static func parsePorcelain(_ output: String) -> [WorktreeInfo] {
        var worktrees: [WorktreeInfo] = []
        var currentPath: String?
        var currentBranch = ""
        var currentCommit = ""
        var isMainWorktree = false

        for line in output.components(separatedBy: "\n") {
            if line.isEmpty {
                // End of entry
                if let path = currentPath {
                    worktrees.append(WorktreeInfo(
                        path: path,
                        branch: currentBranch,
                        commitHash: currentCommit,
                        isMainWorktree: isMainWorktree
                    ))
                }
                currentPath = nil
                currentBranch = ""
                currentCommit = ""
                isMainWorktree = false
            } else if line.hasPrefix("worktree ") {
                currentPath = String(line.dropFirst("worktree ".count))
                // First worktree entry is always the main worktree
                if worktrees.isEmpty && currentPath != nil {
                    isMainWorktree = true
                }
            } else if line.hasPrefix("HEAD ") {
                currentCommit = String(line.dropFirst("HEAD ".count).prefix(8))
            } else if line.hasPrefix("branch ") {
                let fullRef = String(line.dropFirst("branch ".count))
                // Strip refs/heads/ prefix
                if fullRef.hasPrefix("refs/heads/") {
                    currentBranch = String(fullRef.dropFirst("refs/heads/".count))
                } else {
                    currentBranch = fullRef
                }
            } else if line == "bare" {
                // bare worktree, skip
            } else if line == "detached" {
                currentBranch = "(detached)"
            }
        }

        // Handle last entry if no trailing newline
        if let path = currentPath {
            worktrees.append(WorktreeInfo(
                path: path,
                branch: currentBranch,
                commitHash: currentCommit,
                isMainWorktree: isMainWorktree
            ))
        }

        return worktrees
    }
}
