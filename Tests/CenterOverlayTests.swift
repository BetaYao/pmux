import XCTest
import AppKit
@testable import seahelm

final class CenterOverlayTests: XCTestCase {
    func testShowThenDismiss() {
        let vc = DashboardViewController()
        vc.loadViewIfNeeded()
        vc.showCenterOverlay(NSView(), title: "Detail")
        XCTAssertTrue(vc.view.descendantViews().contains { $0 is CenterOverlayView })
        vc.dismissCenterOverlay()
        XCTAssertFalse(vc.view.descendantViews().contains { $0 is CenterOverlayView })
    }
}
