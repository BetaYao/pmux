import Foundation

struct DiffFile {
    let path: String
    let status: FileStatus
    let additions: Int
    let deletions: Int
    let hunks: [DiffHunk]

    enum FileStatus: String {
        case added = "A"
        case modified = "M"
        case deleted = "D"
        case renamed = "R"
        case unknown = "?"
    }
}

struct DiffHunk {
    let header: String       // @@ -1,5 +1,7 @@
    let lines: [DiffLine]
}

struct DiffLine {
    let type: LineType
    let content: String

    enum LineType {
        case context    // unchanged
        case addition   // +
        case deletion   // -
    }
}

enum GitDiff {
    /// Get diff for a worktree (staged + unstaged)
    static func diff(worktreePath: String) -> [DiffFile] {
        // Get unstaged diff
        let unstagedOutput = runGit(args: ["diff", "--no-color"], in: worktreePath) ?? ""
        // Get staged diff
        let stagedOutput = runGit(args: ["diff", "--cached", "--no-color"], in: worktreePath) ?? ""

        let combined = stagedOutput + unstagedOutput
        return parseDiff(combined)
    }

    /// Get short stat summary
    static func diffStat(worktreePath: String) -> String {
        let output = runGit(args: ["diff", "--stat", "--no-color", "HEAD"], in: worktreePath) ?? ""
        return output.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    /// List changed files with status
    static func changedFiles(worktreePath: String) -> [(status: String, path: String)] {
        let output = runGit(args: ["status", "--porcelain"], in: worktreePath) ?? ""
        return output
            .components(separatedBy: .newlines)
            .filter { !$0.isEmpty }
            .map { line in
                let status = String(line.prefix(2)).trimmingCharacters(in: .whitespaces)
                let path = String(line.dropFirst(3))
                return (status: status, path: path)
            }
    }

    // MARK: - Diff Parser

    static func parseDiff(_ output: String) -> [DiffFile] {
        var files: [DiffFile] = []
        var currentPath = ""
        var currentHunks: [DiffHunk] = []
        var currentHunkHeader = ""
        var currentLines: [DiffLine] = []
        var additions = 0
        var deletions = 0

        func flushHunk() {
            if !currentHunkHeader.isEmpty {
                currentHunks.append(DiffHunk(header: currentHunkHeader, lines: currentLines))
                currentLines = []
                currentHunkHeader = ""
            }
        }

        func flushFile() {
            flushHunk()
            if !currentPath.isEmpty {
                files.append(DiffFile(
                    path: currentPath,
                    status: .modified,
                    additions: additions,
                    deletions: deletions,
                    hunks: currentHunks
                ))
            }
            currentPath = ""
            currentHunks = []
            additions = 0
            deletions = 0
        }

        for line in output.components(separatedBy: .newlines) {
            if line.hasPrefix("diff --git") {
                flushFile()
                // Extract path: "diff --git a/path b/path"
                let parts = line.components(separatedBy: " b/")
                if parts.count >= 2 {
                    currentPath = parts.last ?? ""
                }
            } else if line.hasPrefix("@@") {
                flushHunk()
                currentHunkHeader = line
            } else if line.hasPrefix("+") && !line.hasPrefix("+++") {
                currentLines.append(DiffLine(type: .addition, content: String(line.dropFirst())))
                additions += 1
            } else if line.hasPrefix("-") && !line.hasPrefix("---") {
                currentLines.append(DiffLine(type: .deletion, content: String(line.dropFirst())))
                deletions += 1
            } else if line.hasPrefix(" ") {
                currentLines.append(DiffLine(type: .context, content: String(line.dropFirst())))
            }
        }
        flushFile()

        return files
    }

    private static func runGit(args: [String], in directory: String) -> String? {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/git")
        process.arguments = args
        process.currentDirectoryURL = URL(fileURLWithPath: directory)

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
        return String(data: data, encoding: .utf8)
    }
}
