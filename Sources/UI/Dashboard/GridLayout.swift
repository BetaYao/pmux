import Foundation

/// Pure layout calculator for the Dashboard grid. No UI dependencies — fully testable.
struct GridLayout {
    let availableWidth: CGFloat
    let availableHeight: CGFloat
    let cardCount: Int
    let minCardWidth: CGFloat
    let spacing: CGFloat
    let aspectRatio: CGFloat  // height / width

    /// Number of columns that fit.
    var columns: Int {
        max(1, Int(availableWidth / minCardWidth))
    }

    /// Number of rows needed.
    var rows: Int {
        guard cardCount > 0 else { return 0 }
        return Int(ceil(Double(cardCount) / Double(columns)))
    }

    /// Actual card width (fills available space evenly).
    var cardWidth: CGFloat {
        let cols = CGFloat(columns)
        return (availableWidth - spacing * (cols - 1)) / cols
    }

    /// Actual card height based on aspect ratio.
    var cardHeight: CGFloat {
        cardWidth * aspectRatio
    }

    /// Total content height (for scroll view).
    var totalHeight: CGFloat {
        guard rows > 0 else { return 0 }
        return CGFloat(rows) * cardHeight + CGFloat(rows - 1) * spacing
    }

    /// Scroll content height (at least fills the visible area).
    var scrollContentHeight: CGFloat {
        max(totalHeight, availableHeight)
    }

    /// Frame for card at a given index.
    func cardFrame(at index: Int) -> CGRect {
        guard index >= 0, index < cardCount else { return .zero }
        let col = index % columns
        let row = index / columns
        let x = CGFloat(col) * (cardWidth + spacing)
        let y = scrollContentHeight - CGFloat(row + 1) * cardHeight - CGFloat(row) * spacing
        return CGRect(x: x, y: y, width: cardWidth, height: cardHeight)
    }

    /// Drop indicator position: a thin vertical bar at the left edge of the target index slot.
    func dropIndicatorFrame(at index: Int) -> CGRect {
        let clampedIndex = min(index, cardCount)
        let col = clampedIndex % columns
        let row = clampedIndex / columns
        let x = CGFloat(col) * (cardWidth + spacing) - 3
        let y: CGFloat
        if row < rows {
            y = scrollContentHeight - CGFloat(row + 1) * cardHeight - CGFloat(row) * spacing
        } else {
            y = 0
        }
        return CGRect(x: max(0, x), y: y, width: 4, height: cardHeight)
    }

    /// Grid index closest to a point (for drag & drop).
    func gridIndex(for point: CGPoint) -> Int {
        guard cardCount > 0 else { return 0 }
        let col = min(columns - 1, max(0, Int(point.x / (cardWidth + spacing))))
        let row = min(rows - 1, max(0, Int((scrollContentHeight - point.y) / (cardHeight + spacing))))
        return min(cardCount, row * columns + col)
    }

    // MARK: - Zoom Levels

    /// Predefined zoom levels (min card widths).
    static let zoomLevels: [CGFloat] = [180, 220, 260, 300, 380, 480]
    static let defaultZoomIndex = 3  // 300

    /// Clamped zoom index.
    static func clampZoomIndex(_ index: Int) -> Int {
        max(0, min(zoomLevels.count - 1, index))
    }
}
