import XCTest
@testable import amux

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
}
