import XCTest
@testable import pmux

final class GridLayoutTests: XCTestCase {

    // MARK: - Helper

    private func makeLayout(
        width: CGFloat = 900,
        height: CGFloat = 600,
        cardCount: Int = 6,
        minCardWidth: CGFloat = 300,
        spacing: CGFloat = 12,
        aspectRatio: CGFloat = 0.6
    ) -> GridLayout {
        GridLayout(
            availableWidth: width,
            availableHeight: height,
            cardCount: cardCount,
            minCardWidth: minCardWidth,
            spacing: spacing,
            aspectRatio: aspectRatio
        )
    }

    // MARK: - Columns

    func testColumns_ExactFit() {
        let layout = makeLayout(width: 900, minCardWidth: 300)
        XCTAssertEqual(layout.columns, 3)
    }

    func testColumns_PartialFit() {
        let layout = makeLayout(width: 700, minCardWidth: 300)
        XCTAssertEqual(layout.columns, 2)
    }

    func testColumns_NarrowWidth_AtLeastOne() {
        let layout = makeLayout(width: 100, minCardWidth: 300)
        XCTAssertEqual(layout.columns, 1)
    }

    func testColumns_WideWidth() {
        let layout = makeLayout(width: 1800, minCardWidth: 300)
        XCTAssertEqual(layout.columns, 6)
    }

    // MARK: - Rows

    func testRows_ExactDivision() {
        let layout = makeLayout(width: 900, cardCount: 6, minCardWidth: 300)
        // 3 columns, 6 cards = 2 rows
        XCTAssertEqual(layout.rows, 2)
    }

    func testRows_PartialRow() {
        let layout = makeLayout(width: 900, cardCount: 7, minCardWidth: 300)
        // 3 columns, 7 cards = 3 rows
        XCTAssertEqual(layout.rows, 3)
    }

    func testRows_ZeroCards() {
        let layout = makeLayout(cardCount: 0)
        XCTAssertEqual(layout.rows, 0)
    }

    func testRows_SingleCard() {
        let layout = makeLayout(width: 900, cardCount: 1, minCardWidth: 300)
        XCTAssertEqual(layout.rows, 1)
    }

    // MARK: - Card Dimensions

    func testCardWidth_FillsSpace() {
        let layout = makeLayout(width: 900, minCardWidth: 300, spacing: 12)
        // 3 columns: (900 - 12*2) / 3 = 876/3 = 292
        XCTAssertEqual(layout.cardWidth, 292, accuracy: 0.01)
    }

    func testCardWidth_SingleColumn() {
        let layout = makeLayout(width: 200, minCardWidth: 300, spacing: 12)
        // 1 column: (200 - 0) / 1 = 200
        XCTAssertEqual(layout.cardWidth, 200, accuracy: 0.01)
    }

    func testCardHeight_AspectRatio() {
        let layout = makeLayout(width: 900, minCardWidth: 300, spacing: 12, aspectRatio: 0.6)
        let expectedWidth: CGFloat = 292
        XCTAssertEqual(layout.cardHeight, expectedWidth * 0.6, accuracy: 0.01)
    }

    // MARK: - Total Height

    func testTotalHeight_TwoRows() {
        let layout = makeLayout(width: 900, cardCount: 6, minCardWidth: 300, spacing: 12, aspectRatio: 0.6)
        // 2 rows, cardHeight = 292*0.6 = 175.2
        // totalHeight = 2*175.2 + 1*12 = 362.4
        let expectedCardHeight = 292.0 * 0.6
        let expected = 2 * expectedCardHeight + 12
        XCTAssertEqual(layout.totalHeight, expected, accuracy: 0.01)
    }

    func testTotalHeight_ZeroCards() {
        let layout = makeLayout(cardCount: 0)
        XCTAssertEqual(layout.totalHeight, 0)
    }

    // MARK: - Scroll Content Height

    func testScrollContentHeight_AtLeastAvailableHeight() {
        let layout = makeLayout(width: 900, height: 800, cardCount: 1, minCardWidth: 300, spacing: 12, aspectRatio: 0.6)
        // Single row, totalHeight small, should return availableHeight
        XCTAssertEqual(layout.scrollContentHeight, 800, accuracy: 0.01)
    }

    func testScrollContentHeight_LargeContent() {
        let layout = makeLayout(width: 900, height: 200, cardCount: 30, minCardWidth: 300, spacing: 12, aspectRatio: 0.6)
        // 10 rows, totalHeight >> 200
        XCTAssertGreaterThan(layout.scrollContentHeight, 200)
        XCTAssertEqual(layout.scrollContentHeight, layout.totalHeight)
    }

    // MARK: - Card Frame

    func testCardFrame_FirstCard() {
        let layout = makeLayout(width: 900, cardCount: 6, minCardWidth: 300, spacing: 12, aspectRatio: 0.6)
        let frame = layout.cardFrame(at: 0)
        // First card: col=0, row=0
        XCTAssertEqual(frame.origin.x, 0, accuracy: 0.01)
        XCTAssertEqual(frame.size.width, layout.cardWidth, accuracy: 0.01)
        XCTAssertEqual(frame.size.height, layout.cardHeight, accuracy: 0.01)
    }

    func testCardFrame_SecondCard() {
        let layout = makeLayout(width: 900, cardCount: 6, minCardWidth: 300, spacing: 12, aspectRatio: 0.6)
        let frame = layout.cardFrame(at: 1)
        // Second card: col=1, row=0
        let expectedX = 1 * (layout.cardWidth + 12)
        XCTAssertEqual(frame.origin.x, expectedX, accuracy: 0.01)
    }

    func testCardFrame_SecondRow() {
        let layout = makeLayout(width: 900, cardCount: 6, minCardWidth: 300, spacing: 12, aspectRatio: 0.6)
        let frame0 = layout.cardFrame(at: 0)
        let frame3 = layout.cardFrame(at: 3)
        // Card 3: col=0, row=1 — should be below row 0
        XCTAssertEqual(frame3.origin.x, 0, accuracy: 0.01)
        XCTAssertLessThan(frame3.origin.y, frame0.origin.y)
    }

    func testCardFrame_OutOfBounds() {
        let layout = makeLayout(cardCount: 3)
        XCTAssertEqual(layout.cardFrame(at: -1), .zero)
        XCTAssertEqual(layout.cardFrame(at: 3), .zero)
    }

    // MARK: - Grid Index from Point

    func testGridIndex_TopLeftCorner() {
        let layout = makeLayout(width: 900, cardCount: 6, minCardWidth: 300, spacing: 12, aspectRatio: 0.6)
        // Top-left of first card (top row in flipped coords = high y in unflipped)
        let topY = layout.totalHeight - 1
        let index = layout.gridIndex(for: CGPoint(x: 10, y: topY))
        XCTAssertEqual(index, 0)
    }

    func testGridIndex_BottomRow() {
        let layout = makeLayout(width: 900, cardCount: 6, minCardWidth: 300, spacing: 12, aspectRatio: 0.6)
        // Bottom row (low y) = row 1
        let index = layout.gridIndex(for: CGPoint(x: 10, y: 10))
        XCTAssertEqual(index, 3)
    }

    func testGridIndex_EmptyGrid() {
        let layout = makeLayout(cardCount: 0)
        XCTAssertEqual(layout.gridIndex(for: CGPoint(x: 100, y: 100)), 0)
    }

    // MARK: - Drop Indicator

    func testDropIndicator_FirstPosition() {
        let layout = makeLayout(width: 900, cardCount: 3, minCardWidth: 300, spacing: 12, aspectRatio: 0.6)
        let frame = layout.dropIndicatorFrame(at: 0)
        XCTAssertEqual(frame.origin.x, 0, accuracy: 0.01)  // clamped to 0 from -3
        XCTAssertEqual(frame.size.width, 4)
        XCTAssertEqual(frame.size.height, layout.cardHeight, accuracy: 0.01)
    }

    func testDropIndicator_MiddlePosition() {
        let layout = makeLayout(width: 900, cardCount: 3, minCardWidth: 300, spacing: 12, aspectRatio: 0.6)
        let frame = layout.dropIndicatorFrame(at: 1)
        let expectedX = 1 * (layout.cardWidth + 12) - 3
        XCTAssertEqual(frame.origin.x, expectedX, accuracy: 0.01)
    }

    // MARK: - Zoom Levels

    func testZoomLevels_Count() {
        XCTAssertEqual(GridLayout.zoomLevels.count, 6)
    }

    func testZoomLevels_DefaultIndex() {
        XCTAssertEqual(GridLayout.defaultZoomIndex, 3)
        XCTAssertEqual(GridLayout.zoomLevels[3], 300)
    }

    func testClampZoomIndex_InRange() {
        XCTAssertEqual(GridLayout.clampZoomIndex(0), 0)
        XCTAssertEqual(GridLayout.clampZoomIndex(3), 3)
        XCTAssertEqual(GridLayout.clampZoomIndex(5), 5)
    }

    func testClampZoomIndex_OutOfRange() {
        XCTAssertEqual(GridLayout.clampZoomIndex(-1), 0)
        XCTAssertEqual(GridLayout.clampZoomIndex(-10), 0)
        XCTAssertEqual(GridLayout.clampZoomIndex(6), 5)
        XCTAssertEqual(GridLayout.clampZoomIndex(100), 5)
    }

    // MARK: - Zoom changes columns

    func testZoom_SmallerMinWidth_MoreColumns() {
        let small = makeLayout(width: 900, minCardWidth: 180)  // zoom level 0
        let large = makeLayout(width: 900, minCardWidth: 480)  // zoom level 5
        XCTAssertGreaterThan(small.columns, large.columns)
    }

    func testZoom_AllLevelsProduceValidLayout() {
        for level in GridLayout.zoomLevels {
            let layout = makeLayout(width: 1200, cardCount: 10, minCardWidth: level)
            XCTAssertGreaterThanOrEqual(layout.columns, 1)
            XCTAssertGreaterThanOrEqual(layout.rows, 1)
            XCTAssertGreaterThan(layout.cardWidth, 0)
            XCTAssertGreaterThan(layout.cardHeight, 0)
        }
    }
}
