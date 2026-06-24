import XCTest
@testable import seamux

final class TabSelectionTests: XCTestCase {
    func testTabIndexForWorktreeMatchesPath() {
        let paths = ["/wt/a", "/wt/b", "/wt/c"]
        XCTAssertEqual(TabCoordinator.tabIndex(forWorktree: "/wt/b", in: paths), 1)
        XCTAssertNil(TabCoordinator.tabIndex(forWorktree: "/wt/z", in: paths))
    }
}
