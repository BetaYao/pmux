import XCTest
@testable import amux

final class KeyboardModeControllerTests: XCTestCase {
    func testStartsInNormal() {
        let c = KeyboardModeController()
        XCTAssertEqual(c.mode, .normal)
        XCTAssertEqual(c.substate, .none)
    }

    func testEnterInsertSetsMode() {
        let c = KeyboardModeController()
        c.enterInsert()
        XCTAssertEqual(c.mode, .insert)
    }

    func testEnterNormalFromInsert() {
        let c = KeyboardModeController()
        c.enterInsert()
        c.enterNormal()
        XCTAssertEqual(c.mode, .normal)
    }

    func testModeChangeNotifiesDelegate() {
        let c = KeyboardModeController()
        let spy = ModeSpy()
        c.delegate = spy
        c.enterInsert()
        XCTAssertEqual(spy.modeChangeCount, 1)
        XCTAssertEqual(spy.lastMode, .insert)
    }
}

final class ModeSpy: KeyboardModeDelegate {
    var modeChangeCount = 0
    var lastMode: KeyboardMode?
    var lastHint: String?
    func keyboardModeDidChange(_ mode: KeyboardMode, substate: KeyboardSubstate) {
        modeChangeCount += 1
        lastMode = mode
    }
    func keyboardHintDidChange(_ hint: String) { lastHint = hint }
}
