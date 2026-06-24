import XCTest
@testable import seamux

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

    func testParseDiffPreservesAddedDeletedAndRenamedStatus() {
        let diff = """
        diff --git a/new.txt b/new.txt
        new file mode 100644
        --- /dev/null
        +++ b/new.txt
        @@ -0,0 +1 @@
        +new
        diff --git a/old.txt b/old.txt
        deleted file mode 100644
        --- a/old.txt
        +++ /dev/null
        @@ -1 +0,0 @@
        -old
        diff --git a/before.txt b/after.txt
        similarity index 92%
        rename from before.txt
        rename to after.txt
        --- a/before.txt
        +++ b/after.txt
        @@ -1 +1 @@
        -before
        +after
        """

        let files = GitDiff.parseDiff(diff, stage: .unstaged)

        XCTAssertEqual(files[0].status, .added)
        XCTAssertEqual(files[1].status, .deleted)
        XCTAssertEqual(files[2].status, .renamed)
        XCTAssertEqual(files[2].oldPath, "before.txt")
        XCTAssertEqual(files[2].path, "after.txt")
    }

    func testParseDiffPreservesPathContainingBSlash() {
        let diff = """
        diff --git a/foo b/bar.txt b/foo b/bar.txt
        --- a/foo b/bar.txt
        +++ b/foo b/bar.txt
        @@ -1 +1 @@
        -old
        +new
        """

        let files = GitDiff.parseDiff(diff)

        XCTAssertEqual(files[0].path, "foo b/bar.txt")
    }

    // MARK: - changedFiles parsing (via real git)

    func testChangedFilesWithRealRepo() {
        let tempDir = FileManager.default.temporaryDirectory
            .appendingPathComponent("amux-diff-test-\(UUID().uuidString)")
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
            .appendingPathComponent("amux-diff-test-\(UUID().uuidString)")
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

    func testDiffExcludesUntrackedFilesForBackwardCompatibility() throws {
        let tempDir = FileManager.default.temporaryDirectory
            .appendingPathComponent("amux-diff-test-\(UUID().uuidString)")
        let repoPath = tempDir.appendingPathComponent("repo").path
        defer { try? FileManager.default.removeItem(at: tempDir) }

        try FileManager.default.createDirectory(atPath: repoPath, withIntermediateDirectories: true)
        git(["init", "-b", "main"], in: repoPath)
        git(["-c", "user.email=t@t.com", "-c", "user.name=T", "commit", "--allow-empty", "-m", "init"], in: repoPath)
        try "untracked\n".write(toFile: "\(repoPath)/new.txt", atomically: true, encoding: .utf8)

        let files = GitDiff.diff(worktreePath: repoPath)

        XCTAssertTrue(files.isEmpty)
    }

    func testSnapshotIncludesUntrackedTextFileAsSyntheticAddition() throws {
        let tempDir = FileManager.default.temporaryDirectory
            .appendingPathComponent("amux-diff-test-\(UUID().uuidString)")
        let repoPath = tempDir.appendingPathComponent("repo").path
        defer { try? FileManager.default.removeItem(at: tempDir) }

        try FileManager.default.createDirectory(atPath: repoPath, withIntermediateDirectories: true)
        git(["init", "-b", "main"], in: repoPath)
        git(["-c", "user.email=t@t.com", "-c", "user.name=T", "commit", "--allow-empty", "-m", "init"], in: repoPath)
        try "hello\nworld\n".write(toFile: "\(repoPath)/new.txt", atomically: true, encoding: .utf8)

        let snapshot = GitDiff.snapshot(worktreePath: repoPath)
        let file = snapshot.files.first { $0.path == "new.txt" }
        let changedFile = snapshot.changedFiles.first { $0.path == "new.txt" }

        XCTAssertEqual(changedFile?.status, .added)
        XCTAssertEqual(changedFile?.stage, .untracked)
        XCTAssertEqual(file?.status, .added)
        XCTAssertEqual(file?.stage, .untracked)
        XCTAssertEqual(file?.additions, 2)
        XCTAssertEqual(file?.deletions, 0)
    }

    func testSnapshotExpandsUntrackedDirectoryFilesAsSyntheticAdditions() throws {
        let tempDir = FileManager.default.temporaryDirectory
            .appendingPathComponent("amux-diff-test-\(UUID().uuidString)")
        let repoPath = tempDir.appendingPathComponent("repo").path
        defer { try? FileManager.default.removeItem(at: tempDir) }

        try FileManager.default.createDirectory(atPath: repoPath, withIntermediateDirectories: true)
        git(["init", "-b", "main"], in: repoPath)
        git(["-c", "user.email=t@t.com", "-c", "user.name=T", "commit", "--allow-empty", "-m", "init"], in: repoPath)
        try FileManager.default.createDirectory(atPath: "\(repoPath)/newdir", withIntermediateDirectories: true)
        try "hello\nnested\n".write(toFile: "\(repoPath)/newdir/nested.txt", atomically: true, encoding: .utf8)

        let snapshot = GitDiff.snapshot(worktreePath: repoPath)
        let file = snapshot.files.first { $0.path == "newdir/nested.txt" }
        let changedFile = snapshot.changedFiles.first { $0.path == "newdir/nested.txt" }

        XCTAssertEqual(changedFile?.status, .added)
        XCTAssertEqual(changedFile?.stage, .untracked)
        XCTAssertEqual(file?.status, .added)
        XCTAssertEqual(file?.stage, .untracked)
        XCTAssertEqual(file?.additions, 2)
    }

    func testChangedFileEntriesPreservesRenamePathsContainingArrow() throws {
        let tempDir = FileManager.default.temporaryDirectory
            .appendingPathComponent("amux-diff-test-\(UUID().uuidString)")
        let repoPath = tempDir.appendingPathComponent("repo").path
        defer { try? FileManager.default.removeItem(at: tempDir) }

        try FileManager.default.createDirectory(atPath: repoPath, withIntermediateDirectories: true)
        git(["init", "-b", "main"], in: repoPath)
        try "old\n".write(toFile: "\(repoPath)/before -> original.txt", atomically: true, encoding: .utf8)
        git(["add", "before -> original.txt"], in: repoPath)
        git(["-c", "user.email=t@t.com", "-c", "user.name=T", "commit", "-m", "add"], in: repoPath)
        try FileManager.default.moveItem(
            atPath: "\(repoPath)/before -> original.txt",
            toPath: "\(repoPath)/after -> final.txt"
        )
        git(["add", "-A"], in: repoPath)

        let entries = GitDiff.changedFileEntries(worktreePath: repoPath)
        let rename = entries.first { $0.status == .renamed && $0.stage == .staged }

        XCTAssertEqual(rename?.oldPath, "before -> original.txt")
        XCTAssertEqual(rename?.path, "after -> final.txt")
    }

    func testChangedFilesPreservesRenameDestinationPathContainingArrow() throws {
        let tempDir = FileManager.default.temporaryDirectory
            .appendingPathComponent("amux-diff-test-\(UUID().uuidString)")
        let repoPath = tempDir.appendingPathComponent("repo").path
        defer { try? FileManager.default.removeItem(at: tempDir) }

        try FileManager.default.createDirectory(atPath: repoPath, withIntermediateDirectories: true)
        git(["init", "-b", "main"], in: repoPath)
        try "old\n".write(toFile: "\(repoPath)/before -> original.txt", atomically: true, encoding: .utf8)
        git(["add", "before -> original.txt"], in: repoPath)
        git(["-c", "user.email=t@t.com", "-c", "user.name=T", "commit", "-m", "add"], in: repoPath)
        try FileManager.default.moveItem(
            atPath: "\(repoPath)/before -> original.txt",
            toPath: "\(repoPath)/after -> final.txt"
        )
        git(["add", "-A"], in: repoPath)

        let files = GitDiff.changedFiles(worktreePath: repoPath)

        XCTAssertTrue(files.contains { $0.status == "R" && $0.path == "after -> final.txt" })
    }

    func testPorcelainCopyEntriesAreTreatedAsAdded() {
        let entries = GitDiff.parsePorcelainStatus("C  copy.txt\0source.txt\0")

        XCTAssertEqual(entries.first?.path, "copy.txt")
        XCTAssertEqual(entries.first?.oldPath, nil)
        XCTAssertEqual(entries.first?.status, .added)
        XCTAssertEqual(entries.first?.stage, .staged)
    }

    func testDiffPreservesModeOnlyPathContainingBSlash() throws {
        let tempDir = FileManager.default.temporaryDirectory
            .appendingPathComponent("amux-diff-test-\(UUID().uuidString)")
        let repoPath = tempDir.appendingPathComponent("repo").path
        defer { try? FileManager.default.removeItem(at: tempDir) }

        try FileManager.default.createDirectory(atPath: repoPath, withIntermediateDirectories: true)
        git(["init", "-b", "main"], in: repoPath)
        try FileManager.default.createDirectory(atPath: "\(repoPath)/foo b", withIntermediateDirectories: true)
        try "script\n".write(toFile: "\(repoPath)/foo b/bar.txt", atomically: true, encoding: .utf8)
        git(["add", "foo b/bar.txt"], in: repoPath)
        git(["-c", "user.email=t@t.com", "-c", "user.name=T", "commit", "-m", "add"], in: repoPath)
        chmod(["+x", "\(repoPath)/foo b/bar.txt"])

        let files = GitDiff.diff(worktreePath: repoPath)

        XCTAssertEqual(files.first?.path, "foo b/bar.txt")
    }

    func testSnapshotSeparatesStagedAndUnstagedChanges() throws {
        let tempDir = FileManager.default.temporaryDirectory
            .appendingPathComponent("amux-diff-test-\(UUID().uuidString)")
        let repoPath = tempDir.appendingPathComponent("repo").path
        defer { try? FileManager.default.removeItem(at: tempDir) }

        try FileManager.default.createDirectory(atPath: repoPath, withIntermediateDirectories: true)
        git(["init", "-b", "main"], in: repoPath)
        try "one\n".write(toFile: "\(repoPath)/tracked.txt", atomically: true, encoding: .utf8)
        git(["add", "tracked.txt"], in: repoPath)
        git(["-c", "user.email=t@t.com", "-c", "user.name=T", "commit", "-m", "add"], in: repoPath)

        try "one\ntwo\n".write(toFile: "\(repoPath)/tracked.txt", atomically: true, encoding: .utf8)
        git(["add", "tracked.txt"], in: repoPath)
        try "one\ntwo\nthree\n".write(toFile: "\(repoPath)/tracked.txt", atomically: true, encoding: .utf8)

        let snapshot = GitDiff.snapshot(worktreePath: repoPath)

        XCTAssertTrue(snapshot.changedFiles.contains { $0.path == "tracked.txt" && $0.stage == .staged })
        XCTAssertTrue(snapshot.changedFiles.contains { $0.path == "tracked.txt" && $0.stage == .unstaged })
        XCTAssertTrue(snapshot.files.contains { $0.path == "tracked.txt" && $0.stage == .staged })
        XCTAssertTrue(snapshot.files.contains { $0.path == "tracked.txt" && $0.stage == .unstaged })
    }

    func testSnapshotRepresentsUntrackedBinaryFileWithoutRenderingContents() throws {
        let tempDir = FileManager.default.temporaryDirectory
            .appendingPathComponent("amux-diff-test-\(UUID().uuidString)")
        let repoPath = tempDir.appendingPathComponent("repo").path
        defer { try? FileManager.default.removeItem(at: tempDir) }

        try FileManager.default.createDirectory(atPath: repoPath, withIntermediateDirectories: true)
        git(["init", "-b", "main"], in: repoPath)
        git(["-c", "user.email=t@t.com", "-c", "user.name=T", "commit", "--allow-empty", "-m", "init"], in: repoPath)
        FileManager.default.createFile(atPath: "\(repoPath)/image.bin", contents: Data([0, 1, 2, 3]))

        let snapshot = GitDiff.snapshot(worktreePath: repoPath)
        let file = snapshot.files.first { $0.path == "image.bin" }

        XCTAssertEqual(file?.status, .added)
        XCTAssertEqual(file?.stage, .untracked)
        XCTAssertEqual(file?.additions, 0)
        XCTAssertTrue(file?.hunks.isEmpty ?? false)
    }

    func testSnapshotRepresentsOversizedUntrackedTextWithoutRenderingContents() throws {
        let tempDir = FileManager.default.temporaryDirectory
            .appendingPathComponent("amux-diff-test-\(UUID().uuidString)")
        let repoPath = tempDir.appendingPathComponent("repo").path
        defer { try? FileManager.default.removeItem(at: tempDir) }

        try FileManager.default.createDirectory(atPath: repoPath, withIntermediateDirectories: true)
        git(["init", "-b", "main"], in: repoPath)
        git(["-c", "user.email=t@t.com", "-c", "user.name=T", "commit", "--allow-empty", "-m", "init"], in: repoPath)
        try "abcdef\n".write(toFile: "\(repoPath)/large.txt", atomically: true, encoding: .utf8)

        let snapshot = GitDiff.snapshot(worktreePath: repoPath, maxSyntheticFileBytes: 4)
        let file = snapshot.files.first { $0.path == "large.txt" }

        XCTAssertEqual(file?.status, .added)
        XCTAssertEqual(file?.stage, .untracked)
        XCTAssertEqual(file?.additions, 0)
        XCTAssertTrue(file?.hunks.isEmpty ?? false)
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

    private func chmod(_ args: [String]) {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/bin/chmod")
        process.arguments = args
        try? process.run()
        process.waitUntilExit()
    }
}
