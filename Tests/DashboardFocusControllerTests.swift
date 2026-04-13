import XCTest
@testable import amux

final class DashboardFocusControllerTests: XCTestCase {

    // MARK: - Grid ring

    func testGridRingNextWrapsAround() {
        let ctrl = DashboardFocusController()
        ctrl.enterGrid(cardIds: ["a", "b", "c"], initialId: "a")
        XCTAssertEqual(ctrl.focusedTarget, .card("a"))
        ctrl.next()
        XCTAssertEqual(ctrl.focusedTarget, .card("b"))
        ctrl.next()
        XCTAssertEqual(ctrl.focusedTarget, .card("c"))
        ctrl.next()
        XCTAssertEqual(ctrl.focusedTarget, .card("a"))
    }

    func testGridRingPrevWrapsAround() {
        let ctrl = DashboardFocusController()
        ctrl.enterGrid(cardIds: ["a", "b", "c"], initialId: "a")
        ctrl.prev()
        XCTAssertEqual(ctrl.focusedTarget, .card("c"))
        ctrl.prev()
        XCTAssertEqual(ctrl.focusedTarget, .card("b"))
    }

    func testGridInitialFallsBackToFirstWhenInitialNotFound() {
        let ctrl = DashboardFocusController()
        ctrl.enterGrid(cardIds: ["a", "b", "c"], initialId: "zzz")
        XCTAssertEqual(ctrl.focusedTarget, .card("a"))
    }

    func testGridInitialHandlesEmptyRing() {
        let ctrl = DashboardFocusController()
        ctrl.enterGrid(cardIds: [], initialId: nil)
        XCTAssertEqual(ctrl.focusedTarget, .none)
    }

    // MARK: - Focus-layout ring

    func testFocusLayoutRingStartsAtBigPanel() {
        let ctrl = DashboardFocusController()
        ctrl.enterFocusLayout(cardIds: ["a", "b"])
        XCTAssertEqual(ctrl.focusedTarget, .bigPanel)
    }

    func testFocusLayoutRingCyclesPanelThenCards() {
        let ctrl = DashboardFocusController()
        ctrl.enterFocusLayout(cardIds: ["a", "b"])
        ctrl.next() // first card
        XCTAssertEqual(ctrl.focusedTarget, .card("a"))
        ctrl.next()
        XCTAssertEqual(ctrl.focusedTarget, .card("b"))
        ctrl.next() // back to big panel
        XCTAssertEqual(ctrl.focusedTarget, .bigPanel)
    }

    func testFocusLayoutPrevFromBigPanelGoesToLastCard() {
        let ctrl = DashboardFocusController()
        ctrl.enterFocusLayout(cardIds: ["a", "b"])
        ctrl.prev()
        XCTAssertEqual(ctrl.focusedTarget, .card("b"))
    }

    // MARK: - Delete shifts focus

    func testDeleteShiftsFocusToNextCardInGrid() {
        let ctrl = DashboardFocusController()
        ctrl.enterGrid(cardIds: ["a", "b", "c"], initialId: "b")
        ctrl.removeCurrentCard()
        // After removing "b", ring is ["a", "c"] and focus advances to "c"
        XCTAssertEqual(ctrl.focusedTarget, .card("c"))
    }

    func testDeleteLastCardWrapsToFirstInGrid() {
        let ctrl = DashboardFocusController()
        ctrl.enterGrid(cardIds: ["a", "b", "c"], initialId: "c")
        ctrl.removeCurrentCard()
        XCTAssertEqual(ctrl.focusedTarget, .card("a"))
    }

    func testDeleteOnlyCardBecomesNone() {
        let ctrl = DashboardFocusController()
        ctrl.enterGrid(cardIds: ["a"], initialId: "a")
        ctrl.removeCurrentCard()
        XCTAssertEqual(ctrl.focusedTarget, .none)
    }

    func testDeleteInFocusLayoutFallsBackToBigPanelIfNoCardsLeft() {
        let ctrl = DashboardFocusController()
        ctrl.enterFocusLayout(cardIds: ["a"])
        ctrl.next() // focus card "a"
        ctrl.removeCurrentCard()
        XCTAssertEqual(ctrl.focusedTarget, .bigPanel)
    }
}
