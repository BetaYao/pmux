import XCTest
@testable import pmux

class StatusPublisherThreadTests: XCTestCase {
    func testConcurrentUpdateAndPollDoesNotCrash() {
        let publisher = StatusPublisher()
        let expectation = expectation(description: "concurrent access")
        expectation.expectedFulfillmentCount = 10

        for _ in 0..<10 {
            DispatchQueue.global().async {
                publisher.updateSurfaces([String: SplitTree]())
                expectation.fulfill()
            }
        }

        wait(for: [expectation], timeout: 5.0)
    }
}
