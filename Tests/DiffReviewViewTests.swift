import XCTest
@testable import amux

final class DiffReviewViewTests: XCTestCase {
    func testDiffReviewViewLoadsInjectedSnapshot() {
        let file = DiffFile(
            path: "Sources/App/main.swift",
            status: .modified,
            stage: .unstaged,
            additions: 1,
            deletions: 1,
            hunks: [
                DiffHunk(header: "@@ -1 +1 @@", lines: [
                    DiffLine(type: .deletion, content: "old"),
                    DiffLine(type: .addition, content: "new"),
                ])
            ]
        )
        let view = DiffReviewView(
            worktreePath: "/repo",
            loadSnapshot: { GitDiffSnapshot(changedFiles: [], files: [file]) }
        )

        view.frame = NSRect(x: 0, y: 0, width: 800, height: 500)
        view.loadDiffForTesting()

        XCTAssertTrue(view.renderedTextForTesting.contains("Sources/App/main.swift"))
        XCTAssertTrue(view.renderedTextForTesting.contains("+new"))
        XCTAssertTrue(view.renderedTextForTesting.contains("-old"))
    }

    func testDiffReviewViewShowsNoChangesStateForEmptySnapshot() {
        let view = DiffReviewView(
            worktreePath: "/repo",
            loadSnapshot: { GitDiffSnapshot(changedFiles: [], files: []) }
        )

        view.frame = NSRect(x: 0, y: 0, width: 800, height: 500)
        view.loadDiffForTesting()

        XCTAssertTrue(view.renderedTextForTesting.contains("No changes"))
    }

    func testDiffReviewViewKeepsDiffColorWhileHighlightingCodeTokens() throws {
        let file = DiffFile(
            path: "apps/api/src/routes/user-course-reservation.ts",
            status: .modified,
            stage: .unstaged,
            additions: 1,
            deletions: 0,
            hunks: [
                DiffHunk(header: "@@ -0,0 +1 @@", lines: [
                    DiffLine(type: .addition, content: "const result = \"reserved\";"),
                ])
            ]
        )
        let view = DiffReviewView(
            worktreePath: "/repo",
            loadSnapshot: { GitDiffSnapshot(changedFiles: [], files: [file]) }
        )

        view.frame = NSRect(x: 0, y: 0, width: 800, height: 500)
        view.loadDiffForTesting()

        let textView = try XCTUnwrap(findTextView(in: view))
        let storage = try XCTUnwrap(textView.textStorage)
        let rendered = storage.string as NSString
        let addedLineRange = rendered.range(of: "+const result = \"reserved\";")
        XCTAssertNotEqual(addedLineRange.location, NSNotFound)

        let prefixColor = try foregroundColor(in: storage, at: addedLineRange.location)
        assertColorsApproximatelyEqual(prefixColor, NSColor.systemGreen)

        let keywordColor = try foregroundColor(in: storage, at: addedLineRange.location + 1)
        XCTAssertFalse(colorsApproximatelyEqual(keywordColor, NSColor.systemGreen))

        let stringRange = rendered.range(of: "\"reserved\"")
        XCTAssertNotEqual(stringRange.location, NSNotFound)
        let stringColor = try foregroundColor(in: storage, at: stringRange.location)
        XCTAssertFalse(colorsApproximatelyEqual(stringColor, NSColor.systemGreen))
        XCTAssertFalse(colorsApproximatelyEqual(stringColor, keywordColor))
    }
}

private func findTextView(in view: NSView) -> NSTextView? {
    if let textView = view as? NSTextView {
        return textView
    }

    for subview in view.subviews {
        if let textView = findTextView(in: subview) {
            return textView
        }
    }

    return nil
}

private func foregroundColor(in storage: NSTextStorage, at index: Int) throws -> NSColor {
    let color = storage.attribute(.foregroundColor, at: index, effectiveRange: nil) as? NSColor
    return try XCTUnwrap(color)
}

private func assertColorsApproximatelyEqual(
    _ lhs: NSColor,
    _ rhs: NSColor,
    file: StaticString = #filePath,
    line: UInt = #line
) {
    XCTAssertTrue(colorsApproximatelyEqual(lhs, rhs), file: file, line: line)
}

private func colorsApproximatelyEqual(_ lhs: NSColor, _ rhs: NSColor) -> Bool {
    guard
        let left = lhs.usingColorSpace(.deviceRGB),
        let right = rhs.usingColorSpace(.deviceRGB)
    else {
        return lhs == rhs
    }

    return abs(left.redComponent - right.redComponent) < 0.01
        && abs(left.greenComponent - right.greenComponent) < 0.01
        && abs(left.blueComponent - right.blueComponent) < 0.01
        && abs(left.alphaComponent - right.alphaComponent) < 0.01
}
