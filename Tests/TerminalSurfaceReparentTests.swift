import XCTest
@testable import pmux

/// Tests that terminal surfaces fill their container after reparent.
class TerminalSurfaceReparentTests: XCTestCase {

    // MARK: - GhosttyNSView Auto Layout Tests

    func testGhosttyNSViewFillsContainerViaConstraints() {
        let container = NSView(frame: NSRect(x: 0, y: 0, width: 800, height: 600))
        container.wantsLayer = true

        let ghosttyView = GhosttyNSView(frame: .zero)
        ghosttyView.translatesAutoresizingMaskIntoConstraints = false
        container.addSubview(ghosttyView)

        NSLayoutConstraint.activate([
            ghosttyView.topAnchor.constraint(equalTo: container.topAnchor),
            ghosttyView.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            ghosttyView.trailingAnchor.constraint(equalTo: container.trailingAnchor),
            ghosttyView.bottomAnchor.constraint(equalTo: container.bottomAnchor),
        ])

        container.layoutSubtreeIfNeeded()

        XCTAssertEqual(ghosttyView.frame.size.width, 800, accuracy: 1)
        XCTAssertEqual(ghosttyView.frame.size.height, 600, accuracy: 1)
        XCTAssertEqual(ghosttyView.frame.origin.x, 0, accuracy: 1)
        XCTAssertEqual(ghosttyView.frame.origin.y, 0, accuracy: 1)
    }

    func testGhosttyNSViewResizesWithContainer() {
        let container = NSView(frame: NSRect(x: 0, y: 0, width: 800, height: 600))
        container.wantsLayer = true

        let ghosttyView = GhosttyNSView(frame: .zero)
        ghosttyView.translatesAutoresizingMaskIntoConstraints = false
        container.addSubview(ghosttyView)

        NSLayoutConstraint.activate([
            ghosttyView.topAnchor.constraint(equalTo: container.topAnchor),
            ghosttyView.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            ghosttyView.trailingAnchor.constraint(equalTo: container.trailingAnchor),
            ghosttyView.bottomAnchor.constraint(equalTo: container.bottomAnchor),
        ])

        container.layoutSubtreeIfNeeded()
        XCTAssertEqual(ghosttyView.frame.size, NSSize(width: 800, height: 600))

        container.frame = NSRect(x: 0, y: 0, width: 1200, height: 900)
        container.layoutSubtreeIfNeeded()

        XCTAssertEqual(ghosttyView.frame.size.width, 1200, accuracy: 1)
        XCTAssertEqual(ghosttyView.frame.size.height, 900, accuracy: 1)
    }

    // MARK: - Reparent Constraint Tests

    func testReparentRemovesOldConstraintsAndAddsNew() {
        let oldContainer = NSView(frame: NSRect(x: 0, y: 0, width: 200, height: 150))
        let newContainer = NSView(frame: NSRect(x: 0, y: 0, width: 800, height: 600))

        let view = GhosttyNSView(frame: .zero)
        view.translatesAutoresizingMaskIntoConstraints = false
        oldContainer.addSubview(view)

        NSLayoutConstraint.activate([
            view.topAnchor.constraint(equalTo: oldContainer.topAnchor),
            view.leadingAnchor.constraint(equalTo: oldContainer.leadingAnchor),
            view.trailingAnchor.constraint(equalTo: oldContainer.trailingAnchor),
            view.bottomAnchor.constraint(equalTo: oldContainer.bottomAnchor),
        ])
        oldContainer.layoutSubtreeIfNeeded()
        XCTAssertEqual(view.frame.size.width, 200, accuracy: 1)

        // Reparent
        view.removeFromSuperview()
        view.translatesAutoresizingMaskIntoConstraints = false
        newContainer.addSubview(view)
        NSLayoutConstraint.activate([
            view.topAnchor.constraint(equalTo: newContainer.topAnchor),
            view.leadingAnchor.constraint(equalTo: newContainer.leadingAnchor),
            view.trailingAnchor.constraint(equalTo: newContainer.trailingAnchor),
            view.bottomAnchor.constraint(equalTo: newContainer.bottomAnchor),
        ])
        newContainer.layoutSubtreeIfNeeded()

        XCTAssertEqual(view.frame.size.width, 800, accuracy: 1,
                       "After reparent, view should fill new container width")
        XCTAssertEqual(view.frame.size.height, 600, accuracy: 1,
                       "After reparent, view should fill new container height")
    }

    // MARK: - lastSyncedSize reset on reparent

    /// This test verifies that lastSyncedSize is properly reset when
    /// the view is reparented, so that syncSurfaceSize isn't debounced
    /// into skipping the size update for the new container.
    func testLastSyncedSizeResetsOnReparent() {
        // Simulate: dashboard card (432x259) → spotlight (1000x700)
        let cardContainer = NSView(frame: NSRect(x: 0, y: 0, width: 432, height: 259))
        let spotlightContainer = NSView(frame: NSRect(x: 0, y: 0, width: 1000, height: 700))

        let view = GhosttyNSView(frame: .zero)

        // Phase 1: embed in card
        view.translatesAutoresizingMaskIntoConstraints = false
        cardContainer.addSubview(view)
        NSLayoutConstraint.activate([
            view.topAnchor.constraint(equalTo: cardContainer.topAnchor),
            view.leadingAnchor.constraint(equalTo: cardContainer.leadingAnchor),
            view.trailingAnchor.constraint(equalTo: cardContainer.trailingAnchor),
            view.bottomAnchor.constraint(equalTo: cardContainer.bottomAnchor),
        ])
        cardContainer.layoutSubtreeIfNeeded()
        XCTAssertEqual(view.bounds.size.width, 432, accuracy: 1, "View should match card width")

        // Record that a sync happened at this size (simulating syncSurfaceSize)
        // The internal lastSyncedSize is now (432, 259)
        let cardSize = view.bounds.size

        // Phase 2: reparent to spotlight
        view.removeFromSuperview()
        view.translatesAutoresizingMaskIntoConstraints = false
        spotlightContainer.addSubview(view)
        NSLayoutConstraint.activate([
            view.topAnchor.constraint(equalTo: spotlightContainer.topAnchor),
            view.leadingAnchor.constraint(equalTo: spotlightContainer.leadingAnchor),
            view.trailingAnchor.constraint(equalTo: spotlightContainer.trailingAnchor),
            view.bottomAnchor.constraint(equalTo: spotlightContainer.bottomAnchor),
        ])
        spotlightContainer.layoutSubtreeIfNeeded()

        let spotlightSize = view.bounds.size
        XCTAssertEqual(spotlightSize.width, 1000, accuracy: 1,
                       "After reparent, view width should match spotlight container")
        XCTAssertEqual(spotlightSize.height, 700, accuracy: 1,
                       "After reparent, view height should match spotlight container")

        // The critical check: lastSyncedSize must have been updated to the new size.
        // Access it via the test-accessible property.
        XCTAssertEqual(view.lastSyncedSizeForTesting, spotlightSize,
                       "lastSyncedSize should match spotlight container size, not stale card size")
        XCTAssertNotEqual(view.lastSyncedSizeForTesting, cardSize,
                          "lastSyncedSize should NOT still be the old card size")
    }

    /// Verify that reparent from a small container to a large one that has the SAME WIDTH
    /// (but different height) still triggers a size sync — no stale debounce.
    func testReparentSameWidthDifferentHeight() {
        let small = NSView(frame: NSRect(x: 0, y: 0, width: 800, height: 200))
        let large = NSView(frame: NSRect(x: 0, y: 0, width: 800, height: 700))

        let view = GhosttyNSView(frame: .zero)

        // Embed in small
        view.translatesAutoresizingMaskIntoConstraints = false
        small.addSubview(view)
        NSLayoutConstraint.activate([
            view.topAnchor.constraint(equalTo: small.topAnchor),
            view.leadingAnchor.constraint(equalTo: small.leadingAnchor),
            view.trailingAnchor.constraint(equalTo: small.trailingAnchor),
            view.bottomAnchor.constraint(equalTo: small.bottomAnchor),
        ])
        small.layoutSubtreeIfNeeded()

        // Force a sync at the small size
        view.resetLastSyncedSizeForTesting(to: view.bounds.size)

        // Reparent to large
        view.removeFromSuperview()
        view.translatesAutoresizingMaskIntoConstraints = false
        large.addSubview(view)
        NSLayoutConstraint.activate([
            view.topAnchor.constraint(equalTo: large.topAnchor),
            view.leadingAnchor.constraint(equalTo: large.leadingAnchor),
            view.trailingAnchor.constraint(equalTo: large.trailingAnchor),
            view.bottomAnchor.constraint(equalTo: large.bottomAnchor),
        ])
        large.layoutSubtreeIfNeeded()

        XCTAssertEqual(view.lastSyncedSizeForTesting, NSSize(width: 800, height: 700),
                       "Size sync must happen even when width is the same but height differs")
    }

    // MARK: - Scroll Events

    func testGhosttyNSViewAcceptsScrollEvents() {
        let view = GhosttyNSView(frame: NSRect(x: 0, y: 0, width: 800, height: 600))
        XCTAssertTrue(view.responds(to: #selector(NSView.scrollWheel(with:))))
    }

    func testGhosttyNSViewAcceptsFirstResponder() {
        let view = GhosttyNSView(frame: NSRect(x: 0, y: 0, width: 800, height: 600))
        XCTAssertTrue(view.acceptsFirstResponder)
        XCTAssertTrue(view.canBecomeKeyView)
    }
}
