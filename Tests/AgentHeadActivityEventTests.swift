// Tests/AgentHeadActivityEventTests.swift
import XCTest
@testable import amux

final class AgentHeadActivityEventTests: XCTestCase {

    func testAppendActivityEventAddsToFront() {
        var events: [ActivityEvent] = []
        let event1 = ActivityEvent(tool: "Read", detail: "a.swift", isError: false, timestamp: Date())
        let event2 = ActivityEvent(tool: "Edit", detail: "b.swift", isError: false, timestamp: Date())

        AgentHead.appendToRingBuffer(&events, event: event1, maxSize: 20)
        AgentHead.appendToRingBuffer(&events, event: event2, maxSize: 20)

        XCTAssertEqual(events.count, 2)
        XCTAssertEqual(events[0].tool, "Edit")
        XCTAssertEqual(events[1].tool, "Read")
    }

    func testRingBufferCapsAtMaxSize() {
        var events: [ActivityEvent] = []
        for i in 0..<25 {
            let event = ActivityEvent(tool: "Read", detail: "file\(i).swift", isError: false, timestamp: Date())
            AgentHead.appendToRingBuffer(&events, event: event, maxSize: 20)
        }
        XCTAssertEqual(events.count, 20)
        XCTAssertEqual(events[0].detail, "file24.swift")
    }

    func testClearActivityEventsEmptiesBuffer() {
        var events: [ActivityEvent] = []
        let event = ActivityEvent(tool: "Read", detail: "a.swift", isError: false, timestamp: Date())
        AgentHead.appendToRingBuffer(&events, event: event, maxSize: 20)
        XCTAssertEqual(events.count, 1)
        events.removeAll()
        XCTAssertTrue(events.isEmpty)
    }
}
