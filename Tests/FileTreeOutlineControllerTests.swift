import XCTest
@testable import seahelm

final class FileTreeOutlineControllerTests: XCTestCase {
    func testChildNodesHidesDotfilesAndSortsDirsFirst() throws {
        let fm = FileManager.default
        let root = fm.temporaryDirectory.appendingPathComponent("amux-filetree-test-\(ProcessInfo.processInfo.globallyUniqueString)")
        try fm.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? fm.removeItem(at: root) }
        try "x".write(to: root.appendingPathComponent("a.txt"), atomically: true, encoding: .utf8)
        try "x".write(to: root.appendingPathComponent(".hidden"), atomically: true, encoding: .utf8)
        try fm.createDirectory(at: root.appendingPathComponent("sub"), withIntermediateDirectories: true)

        let names = FileTreeOutlineController.childNodes(of: root).map { $0.url.lastPathComponent }
        XCTAssertEqual(names, ["sub", "a.txt"])
    }

    func testChildNodesShowHiddenIncludesDotfiles() throws {
        let fm = FileManager.default
        let root = fm.temporaryDirectory.appendingPathComponent("amux-filetree-hidden-\(ProcessInfo.processInfo.globallyUniqueString)")
        try fm.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? fm.removeItem(at: root) }
        try "x".write(to: root.appendingPathComponent("a.txt"), atomically: true, encoding: .utf8)
        try "x".write(to: root.appendingPathComponent(".env"), atomically: true, encoding: .utf8)

        let names = FileTreeOutlineController.childNodes(of: root, showHidden: true).map { $0.url.lastPathComponent }
        XCTAssertEqual(names.sorted(), [".env", "a.txt"])
    }

    func testIconVariesByFileType() {
        func icon(_ name: String, dir: Bool = false) -> String {
            FileTreeOutlineController.icon(
                for: FileTreeNode(url: URL(fileURLWithPath: "/x/\(name)"), isDirectory: dir)
            ).0
        }
        XCTAssertEqual(icon("src", dir: true), "folder.fill")
        XCTAssertEqual(icon("main.swift"), "swift")
        XCTAssertEqual(icon("config.json"), "curlybraces")
        XCTAssertEqual(icon("photo.png"), "photo.fill")
        XCTAssertEqual(icon("README.md"), "book.fill")
        XCTAssertEqual(icon("notes.md"), "doc.text.fill")
        XCTAssertEqual(icon("script.py"), "chevron.left.forwardslash.chevron.right")
        XCTAssertEqual(icon("mystery.qwerty"), "doc")
    }

    func testExpansionCaptureAndRestore() throws {
        let fm = FileManager.default
        let root = fm.temporaryDirectory.appendingPathComponent("amux-filetree-expand-\(ProcessInfo.processInfo.globallyUniqueString)")
        let a = root.appendingPathComponent("a")
        let ab = a.appendingPathComponent("b")
        try fm.createDirectory(at: ab, withIntermediateDirectories: true)
        defer { try? fm.removeItem(at: root) }
        try "x".write(to: ab.appendingPathComponent("f.txt"), atomically: true, encoding: .utf8)

        let c1 = FileTreeOutlineController(rootPath: root.path)
        // Expand a, then a/b.
        let aNode = c1.outlineView(c1.outlineView, child: 0, ofItem: nil) as! FileTreeNode
        c1.outlineView.expandItem(aNode)
        let bNode = c1.outlineView(c1.outlineView, child: 0, ofItem: aNode) as! FileTreeNode
        c1.outlineView.expandItem(bNode)

        let saved = c1.currentExpandedPaths()
        XCTAssertTrue(saved.contains(a.standardizedFileURL.path))
        XCTAssertTrue(saved.contains(ab.standardizedFileURL.path))

        // Fresh controller starts collapsed; restore re-expands both levels.
        let c2 = FileTreeOutlineController(rootPath: root.path)
        XCTAssertFalse(c2.outlineView.isItemExpanded(c2.outlineView.item(atRow: 0)!))
        c2.restoreExpansion(saved)
        let a2 = c2.outlineView.item(atRow: 0) as! FileTreeNode
        XCTAssertTrue(c2.outlineView.isItemExpanded(a2))
        let b2 = c2.outlineView.item(atRow: 1) as! FileTreeNode
        XCTAssertEqual(b2.url.lastPathComponent, "b")
        XCTAssertTrue(c2.outlineView.isItemExpanded(b2))
    }

    func testRelativePathFromRoot() {
        let root = "/Users/me/proj"
        XCTAssertEqual(FileTreeOutlineController.relativePath(of: URL(fileURLWithPath: "/Users/me/proj/src/a.swift"), from: root), "src/a.swift")
        XCTAssertEqual(FileTreeOutlineController.relativePath(of: URL(fileURLWithPath: "/Users/me/proj"), from: root), ".")
        XCTAssertEqual(FileTreeOutlineController.relativePath(of: URL(fileURLWithPath: "/elsewhere/x.txt"), from: root), "x.txt")
    }

    func testFilterKeepsMatchingDescendantsAndHidesOthers() throws {
        let fm = FileManager.default
        let root = fm.temporaryDirectory.appendingPathComponent("amux-filetree-filter-\(ProcessInfo.processInfo.globallyUniqueString)")
        let sub = root.appendingPathComponent("sub")
        try fm.createDirectory(at: sub, withIntermediateDirectories: true)
        defer { try? fm.removeItem(at: root) }
        try "x".write(to: sub.appendingPathComponent("needle.swift"), atomically: true, encoding: .utf8)
        try "x".write(to: root.appendingPathComponent("other.txt"), atomically: true, encoding: .utf8)

        let controller = FileTreeOutlineController(rootPath: root.path)
        controller.filterText = "needle"

        // Root should keep only "sub" (contains the match); "other.txt" filtered out.
        let topCount = controller.outlineView(controller.outlineView, numberOfChildrenOfItem: nil)
        XCTAssertEqual(topCount, 1)
        let top = controller.outlineView(controller.outlineView, child: 0, ofItem: nil) as! FileTreeNode
        XCTAssertEqual(top.url.lastPathComponent, "sub")
        let child = controller.outlineView(controller.outlineView, child: 0, ofItem: top) as! FileTreeNode
        XCTAssertEqual(child.url.lastPathComponent, "needle.swift")
    }
}
