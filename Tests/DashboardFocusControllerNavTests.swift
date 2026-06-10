import XCTest
@testable import amux

final class DashboardFocusControllerNavTests: XCTestCase {
    private func gridController(_ ids: [String], focus: String) -> DashboardFocusController {
        let c = DashboardFocusController()
        c.enterGrid(cardIds: ids, initialId: focus)
        return c
    }

    func testJumpToIndexGrid() {
        let c = gridController(["a","b","c","d"], focus: "a")
        c.jump(toIndex: 2)
        XCTAssertEqual(c.focusedTarget, .card("c"))
    }

    func testJumpOutOfRangeIsNoop() {
        let c = gridController(["a","b"], focus: "a")
        c.jump(toIndex: 9)
        XCTAssertEqual(c.focusedTarget, .card("a"))
    }

    func testGridMoveRightAdvancesByOne() {
        let c = gridController(["a","b","c","d"], focus: "a")
        c.move(.right, columns: 2)
        XCTAssertEqual(c.focusedTarget, .card("b"))
    }

    func testGridMoveDownAdvancesByColumns() {
        let c = gridController(["a","b","c","d"], focus: "a")  // 2 cols: a b / c d
        c.move(.down, columns: 2)
        XCTAssertEqual(c.focusedTarget, .card("c"))
    }

    func testGridMoveUpFromTopRowIsNoop() {
        let c = gridController(["a","b","c","d"], focus: "b")
        c.move(.up, columns: 2)
        XCTAssertEqual(c.focusedTarget, .card("b"))
    }

    func testGridMoveRightAtRowEndIsNoop() {
        let c = gridController(["a","b","c","d"], focus: "b")  // b is end of row 0
        c.move(.right, columns: 2)
        XCTAssertEqual(c.focusedTarget, .card("b"))
    }
}
