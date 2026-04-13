import Foundation

enum WorktreeDeleterError: Error, LocalizedError {
    case gitFailed(String)
    case isMainWorktree
    case pathNotFound(String)

    var errorDescription: String? {
        switch self {
        case .gitFailed(let msg): return "Git error: \(msg)"
        case .isMainWorktree: return "Cannot delete the main worktree"
        case .pathNotFound(let path): return "Worktree not found: \(path)"
        }
    }
}

enum WorktreeDeleter {

    /// Remove a git worktree and optionally delete its branch.
    /// - Parameters:
    ///   - worktreePath: Absolute path to the worktree directory
    ///   - repoPath: Root repo path (for running git commands)
    ///   - deleteBranch: If true, also deletes the local branch
    ///   - force: If true, uses --force for dirty worktrees
    static func deleteWorktree(
        worktreePath: String,
        repoPath: String,
        branchName: String,
        deleteBranch: Bool = false,
        force: Bool = false
    ) throws {
        // Don't allow deleting the main worktree.
        // Use the first entry from `git worktree list` which is always the main worktree.
        // Note: `git rev-parse --show-toplevel` returns the worktree's own path when run
        // inside a linked worktree, so it cannot reliably identify the main worktree.
        let listOutput = runGit(args: ["worktree", "list", "--porcelain"], in: repoPath) ?? ""
        if let firstLine = listOutput.components(separatedBy: "\n").first,
           firstLine.hasPrefix("worktree ") {
            let mainPath = String(firstLine.dropFirst("worktree ".count))
            if worktreePath == mainPath {
                throw WorktreeDeleterError.isMainWorktree
            }
        }

        // git worktree remove <path> [--force]
        var args = ["worktree", "remove", worktreePath]
        if force { args.append("--force") }

        let (success, stderr) = runGitWithStderr(args: args, in: repoPath)
        if !success {
            throw WorktreeDeleterError.gitFailed(stderr)
        }

        // Optionally delete the branch
        if deleteBranch {
            let flag = force ? "-D" : "-d"
            let (branchOk, branchErr) = runGitWithStderr(args: ["branch", flag, branchName], in: repoPath)
            if !branchOk {
                // Non-fatal: worktree removed but branch delete failed
                NSLog("Warning: worktree removed but branch delete failed: \(branchErr)")
            }
        }
    }

    /// Check if a worktree has uncommitted changes
    static func hasUncommittedChanges(worktreePath: String) -> Bool {
        let output = runGit(args: ["status", "--porcelain"], in: worktreePath) ?? ""
        return !output.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    private static func runGit(args: [String], in directory: String) -> String? {
        let (success, _, stdout) = runGitFull(args: args, in: directory)
        return success ? stdout : nil
    }

    private static func runGitWithStderr(args: [String], in directory: String) -> (success: Bool, stderr: String) {
        let (success, stderr, _) = runGitFull(args: args, in: directory)
        return (success, stderr)
    }

    private static func runGitFull(args: [String], in directory: String) -> (success: Bool, stderr: String, stdout: String) {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/git")
        process.arguments = args
        process.currentDirectoryURL = URL(fileURLWithPath: directory)

        let outPipe = Pipe()
        let errPipe = Pipe()
        process.standardOutput = outPipe
        process.standardError = errPipe

        do {
            try process.run()
            process.waitUntilExit()
        } catch {
            return (false, error.localizedDescription, "")
        }

        let stdout = String(data: outPipe.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
        let stderr = String(data: errPipe.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
        return (process.terminationStatus == 0, stderr.trimmingCharacters(in: .whitespacesAndNewlines), stdout)
    }
}
