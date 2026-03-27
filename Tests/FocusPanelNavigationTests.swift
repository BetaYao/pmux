// Tests/FocusPanelNavigationTests.swift
import XCTest
@testable import amux

final class FocusPanelNavigationTests: XCTestCase {

    func testNavigationHiddenWhenTotalIsOne() {
        let panel = FocusPanelView()
        panel.configureNavigation(currentIndex: 0, total: 1)
        XCTAssertTrue(panel.prevButton.isHidden)
        XCTAssertTrue(panel.nextButton.isHidden)
        XCTAssertTrue(panel.counterLabel.isHidden)
    }

    func testNavigationVisibleWhenTotalGreaterThanOne() {
        let panel = FocusPanelView()
        panel.configureNavigation(currentIndex: 0, total: 4)
        XCTAssertFalse(panel.prevButton.isHidden)
        XCTAssertFalse(panel.nextButton.isHidden)
        XCTAssertFalse(panel.counterLabel.isHidden)
    }

    func testCounterLabelFormat() {
        let panel = FocusPanelView()
        panel.configureNavigation(currentIndex: 2, total: 5)
        XCTAssertEqual(panel.counterLabel.stringValue, "3/5")
    }

    func testPrevDisabledAtFirstIndex() {
        let panel = FocusPanelView()
        panel.configureNavigation(currentIndex: 0, total: 4)
        XCTAssertFalse(panel.prevButton.isEnabled)
        XCTAssertTrue(panel.nextButton.isEnabled)
    }

    func testNextDisabledAtLastIndex() {
        let panel = FocusPanelView()
        panel.configureNavigation(currentIndex: 3, total: 4)
        XCTAssertTrue(panel.prevButton.isEnabled)
        XCTAssertFalse(panel.nextButton.isEnabled)
    }

    func testBothEnabledInMiddle() {
        let panel = FocusPanelView()
        panel.configureNavigation(currentIndex: 1, total: 4)
        XCTAssertTrue(panel.prevButton.isEnabled)
        XCTAssertTrue(panel.nextButton.isEnabled)
    }
}
