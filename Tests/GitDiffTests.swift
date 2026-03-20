import XCTest
@testable import pmux

final class GitDiffTests: XCTestCase {

    // MARK: - parseDiff

    func testParseEmptyDiff() {
        let files = GitDiff.parseDiff("")
        XCTAssertTrue(files.isEmpty)
    }

    func testParseSingleFileAddition() {
        let diff = """
        diff --git a/hello.txt b/hello.txt
        new file mode 100644
        --- /dev/null
        +++ b/hello.txt
        @@ -0,0 +1,3 @@
        +line 1
        +line 2
        +line 3
        """
        let files = GitDiff.parseDiff(diff)
        XCTAssertEqual(files.count, 1)
        XCTAssertEqual(files[0].path, "hello.txt")
        XCTAssertEqual(files[0].additions, 3)
        XCTAssertEqual(files[0].deletions, 0)
        XCTAssertEqual(files[0].hunks.count, 1)
        XCTAssertEqual(files[0].hunks[0].lines.count, 3)
    }

    func testParseSingleFileDeletion() {
        let diff = """
        diff --git a/old.txt b/old.txt
        deleted file mode 100644
        --- a/old.txt
        +++ /dev/null
        @@ -1,2 +0,0 @@
        -removed line 1
        -removed line 2
        """
        let files = GitDiff.parseDiff(diff)
        XCTAssertEqual(files.count, 1)
        XCTAssertEqual(files[0].additions, 0)
        XCTAssertEqual(files[0].deletions, 2)
    }

    func testParseModifiedFile() {
        let diff = """
        diff --git a/file.txt b/file.txt
        --- a/file.txt
        +++ b/file.txt
        @@ -1,3 +1,3 @@
         context line
        -old line
        +new line
         another context
        """
        let files = GitDiff.parseDiff(diff)
        XCTAssertEqual(files.count, 1)
        XCTAssertEqual(files[0].additions, 1)
        XCTAssertEqual(files[0].deletions, 1)

        let lines = files[0].hunks[0].lines
        XCTAssertEqual(lines.count, 4)
        XCTAssertEqual(lines[0].type, .context)
        XCTAssertEqual(lines[0].content, "context line")
        XCTAssertEqual(lines[1].type, .deletion)
        XCTAssertEqual(lines[1].content, "old line")
        XCTAssertEqual(lines[2].type, .addition)
        XCTAssertEqual(lines[2].content, "new line")
        XCTAssertEqual(lines[3].type, .context)
    }

    func testParseMultipleFiles() {
        let diff = """
        diff --git a/a.txt b/a.txt
        --- a/a.txt
        +++ b/a.txt
        @@ -1 +1 @@
        -old a
        +new a
        diff --git a/b.txt b/b.txt
        --- a/b.txt
        +++ b/b.txt
        @@ -1 +1,2 @@
        -old b
        +new b
        +extra line
        """
        let files = GitDiff.parseDiff(diff)
        XCTAssertEqual(files.count, 2)
        XCTAssertEqual(files[0].path, "a.txt")
        XCTAssertEqual(files[1].path, "b.txt")
        XCTAssertEqual(files[0].additions, 1)
        XCTAssertEqual(files[1].additions, 2)
    }

    func testParseMultipleHunks() {
        let diff = """
        diff --git a/file.txt b/file.txt
        --- a/file.txt
        +++ b/file.txt
        @@ -1,3 +1,3 @@
         top
        -old top
        +new top
         mid
        @@ -10,3 +10,3 @@
         bottom
        -old bottom
        +new bottom
         end
        """
        let files = GitDiff.parseDiff(diff)
        XCTAssertEqual(files.count, 1)
        XCTAssertEqual(files[0].hunks.count, 2)
        XCTAssertTrue(files[0].hunks[0].header.contains("-1,3"))
        XCTAssertTrue(files[0].hunks[1].header.contains("-10,3"))
    }

    func testParsePathWithSpaces() {
        let diff = """
        diff --git a/path with spaces/file.txt b/path with spaces/file.txt
        --- a/path with spaces/file.txt
        +++ b/path with spaces/file.txt
        @@ -1 +1 @@
        -old
        +new
        """
        let files = GitDiff.parseDiff(diff)
        XCTAssertEqual(files.count, 1)
        XCTAssertEqual(files[0].path, "path with spaces/file.txt")
    }

    func testParseNestedPath() {
        let diff = """
        diff --git a/src/module/deep/file.rs b/src/module/deep/file.rs
        --- a/src/module/deep/file.rs
        +++ b/src/module/deep/file.rs
        @@ -1 +1 @@
        -old
        +new
        """
        let files = GitDiff.parseDiff(diff)
        XCTAssertEqual(files[0].path, "src/module/deep/file.rs")
    }

    // MARK: - changedFiles parsing (via real git)

    func testChangedFilesWithRealRepo() {
        let tempDir = FileManager.default.temporaryDirectory
            .appendingPathComponent("pmux-diff-test-\(UUID().uuidString)")
        let repoPath = tempDir.appendingPathComponent("repo").path

        defer { try? FileManager.default.removeItem(at: tempDir) }

        // Set up repo
        try? FileManager.default.createDirectory(atPath: repoPath, withIntermediateDirectories: true)
        git(["init", "-b", "main"], in: repoPath)
        git(["-c", "user.email=t@t.com", "-c", "user.name=T", "commit", "--allow-empty", "-m", "init"], in: repoPath)

        // Create a new file (untracked)
        let filePath = tempDir.appendingPathComponent("repo/new.txt").path
        FileManager.default.createFile(atPath: filePath, contents: "hello".data(using: .utf8))

        let changed = GitDiff.changedFiles(worktreePath: repoPath)
        XCTAssertEqual(changed.count, 1)
        XCTAssertEqual(changed[0].status, "??")
        XCTAssertEqual(changed[0].path, "new.txt")
    }

    func testDiffWithRealRepo() {
        let tempDir = FileManager.default.temporaryDirectory
            .appendingPathComponent("pmux-diff-test-\(UUID().uuidString)")
        let repoPath = tempDir.appendingPathComponent("repo").path

        defer { try? FileManager.default.removeItem(at: tempDir) }

        // Set up repo with a file
        try? FileManager.default.createDirectory(atPath: repoPath, withIntermediateDirectories: true)
        git(["init", "-b", "main"], in: repoPath)
        let filePath = tempDir.appendingPathComponent("repo/tracked.txt").path
        FileManager.default.createFile(atPath: filePath, contents: "original".data(using: .utf8))
        git(["add", "tracked.txt"], in: repoPath)
        git(["-c", "user.email=t@t.com", "-c", "user.name=T", "commit", "-m", "add"], in: repoPath)

        // Modify the file
        try? "modified content".write(toFile: filePath, atomically: true, encoding: .utf8)

        let files = GitDiff.diff(worktreePath: repoPath)
        XCTAssertEqual(files.count, 1)
        XCTAssertEqual(files[0].path, "tracked.txt")
        XCTAssertTrue(files[0].additions > 0 || files[0].deletions > 0)
    }

    // MARK: - Helpers

    @discardableResult
    private func git(_ args: [String], in directory: String) -> String {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/git")
        process.arguments = args
        process.currentDirectoryURL = URL(fileURLWithPath: directory)
        let pipe = Pipe()
        process.standardOutput = pipe
        process.standardError = Pipe()
        try? process.run()
        process.waitUntilExit()
        return String(data: pipe.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
    }
}
