// Tests/FileContentViewTests.swift
import XCTest
@testable import amux

final class FileContentViewTests: XCTestCase {
    func testReadsUTF8AndRejectsOversizeAndMissing() throws {
        let fm = FileManager.default
        let f = fm.temporaryDirectory.appendingPathComponent("amux-fcv-\(ProcessInfo.processInfo.globallyUniqueString).txt")
        try "hello".write(to: f, atomically: true, encoding: .utf8)
        defer { try? fm.removeItem(at: f) }
        XCTAssertEqual(FileContentView.readContent(at: f.path), "hello")
        XCTAssertNil(FileContentView.readContent(at: f.path, maxBytes: 2)) // oversize
        XCTAssertNil(FileContentView.readContent(at: f.path + ".nope"))    // missing
    }
}
