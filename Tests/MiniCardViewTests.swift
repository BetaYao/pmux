import XCTest
import AppKit
@testable import amux

final class MiniCardViewTests: XCTestCase {

    func testMiniCardShowsPromptAndNewestToolOnly() {
        let card = MiniCardView(frame: NSRect(x: 0, y: 0, width: 320, height: 180))
        let events = [
            ActivityEvent(tool: "Bash", detail: "swift test", isError: false, timestamp: Date()),
            ActivityEvent(tool: "Read", detail: "main.swift", isError: false, timestamp: Date(timeIntervalSinceNow: -5)),
        ]

        card.configure(
            id: "agent-1",
            project: "repo",
            thread: "main",
            status: "running",
            lastMessage: "fallback",
            lastUserPrompt: "fix the failing tests",
            totalDuration: "00:10:00",
            roundDuration: "00:00:10",
            activityEvents: events
        )

        let labels = card.subviews.compactMap { $0 as? NSTextField }
        XCTAssertTrue(labels.contains { $0.stringValue.contains("fix the failing tests") })

        let toolLabels = labels.filter { $0.attributedStringValue.string.contains("swift test") }
        XCTAssertEqual(toolLabels.count, 1)
        XCTAssertFalse(toolLabels[0].attributedStringValue.string.contains("\n"))
        XCTAssertFalse(labels.contains { $0.attributedStringValue.string.contains("main.swift") })
    }
}
