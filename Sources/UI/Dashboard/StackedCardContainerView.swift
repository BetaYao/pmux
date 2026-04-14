import AppKit

protocol GridCardReorderDelegate: AnyObject {
    func gridCardReorderBegan(_ card: StackedCardContainerView)
    func gridCardReorderMoved(_ card: StackedCardContainerView, locationInContainer point: NSPoint)
    func gridCardReorderEnded(_ card: StackedCardContainerView)
}

final class StackedCardContainerView: NSView, NSGestureRecognizerDelegate {
    override var acceptsFirstResponder: Bool { false }

    let cardView = AgentCardView()
    private(set) var ghostViews: [NSView] = []

    /// The container owns click handling via its own gesture recognizer.
    /// cardView.delegate must remain nil to prevent double-firing.
    weak var delegate: AgentCardDelegate?
    /// Delegate for drag-to-reorder in grid. Set by DashboardViewController.
    weak var reorderDelegate: GridCardReorderDelegate?

    /// Stored so the NSGestureRecognizerDelegate can reference it for failure dependency.
    private var doubleClickRecognizer: NSClickGestureRecognizer?

    var agentId: String { cardView.agentId }

    var isSelected: Bool {
        get { cardView.isSelected }
        set { cardView.isSelected = newValue }
    }

    var isKeyboardFocused: Bool = false { didSet { updateKeyboardFocusAppearance() } }

    private func updateKeyboardFocusAppearance() {
        wantsLayer = true
        if isKeyboardFocused {
            layer?.borderColor = SemanticColors.accent.cgColor
            layer?.borderWidth = 2
            layer?.shadowColor = SemanticColors.accent.cgColor
            layer?.shadowOpacity = 0.6
            layer?.shadowRadius = 8
            layer?.shadowOffset = .zero
            layer?.masksToBounds = false
        } else {
            layer?.borderColor = nil
            layer?.borderWidth = 0
            layer?.shadowOpacity = 0
        }
    }

    override init(frame: NSRect) {
        super.init(frame: frame)
        setup()
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) not supported")
    }

    private func setup() {
        wantsLayer = true
        layer?.masksToBounds = false

        // cardView on top; ghost views are inserted below it
        addSubview(cardView)

        // Double-click fires navigation
        let doubleClick = NSClickGestureRecognizer(target: self, action: #selector(handleDoubleClick))
        doubleClick.numberOfClicksRequired = 2
        doubleClickRecognizer = doubleClick

        // Single-click fires selection; waits for the double-click recognizer to fail
        // before it fires. AppKit uses shouldRequireFailureOfGestureRecognizer (subclass
        // override) rather than UIKit-style require(toFail:).
        let singleClick = FailDependentClickRecognizer(
            failTarget: doubleClick,
            target: self,
            action: #selector(handleClick)
        )
        singleClick.numberOfClicksRequired = 1

        // cardView's own recognizer is functionally a no-op (delegate is nil), but without
        // this dependency it could still consume the first tap and prevent the container's
        // double-click from seeing the second tap. Wire it through the delegate.
        cardView.clickRecognizer.delegate = self

        addGestureRecognizer(doubleClick)
        addGestureRecognizer(singleClick)

        let press = NSPressGestureRecognizer(target: self, action: #selector(handleLongPress(_:)))
        press.minimumPressDuration = 0.3
        press.allowableMovement = 4
        addGestureRecognizer(press)
    }

    // MARK: - Drag-to-Reorder

    private var dragStartLocation: NSPoint = .zero

    @objc private func handleLongPress(_ gesture: NSPressGestureRecognizer) {
        guard let container = superview else { return }

        switch gesture.state {
        case .began:
            dragStartLocation = gesture.location(in: container)

            // Visual lift
            layer?.zPosition = 100
            NSAnimationContext.runAnimationGroup { ctx in
                ctx.duration = 0.15
                ctx.allowsImplicitAnimation = true
                self.alphaValue = 0.85
                self.layer?.shadowOpacity = 0.5
                self.layer?.shadowRadius = 12
                self.layer?.shadowOffset = CGSize(width: 0, height: -4)
                self.layer?.shadowColor = NSColor.black.cgColor
            }
            reorderDelegate?.gridCardReorderBegan(self)

        case .changed:
            let current = gesture.location(in: container)
            let dx = current.x - dragStartLocation.x
            let dy = current.y - dragStartLocation.y
            frame.origin.x += dx
            frame.origin.y += dy
            dragStartLocation = current
            reorderDelegate?.gridCardReorderMoved(self, locationInContainer: current)

        case .ended, .cancelled:
            layer?.zPosition = 0
            NSAnimationContext.runAnimationGroup { ctx in
                ctx.duration = 0.2
                ctx.allowsImplicitAnimation = true
                self.alphaValue = 1.0
                self.layer?.shadowOpacity = 0
                self.layer?.shadowRadius = 0
            }
            reorderDelegate?.gridCardReorderEnded(self)

        default:
            break
        }
    }

    // MARK: - Configure

    /// Updates ghost view count. Surplus ghosts are removed via removeFromSuperview().
    /// Needed ghosts are created and inserted below cardView.
    func configure(paneCount: Int) {
        let needed = min(max(paneCount - 1, 0), 2)

        // Remove surplus ghosts
        while ghostViews.count > needed {
            ghostViews.removeLast().removeFromSuperview()
        }

        // Add missing ghosts
        while ghostViews.count < needed {
            let ghost = makeGhostView(index: ghostViews.count)
            // Insert below cardView (index 0 = bottom of z-order)
            addSubview(ghost, positioned: .below, relativeTo: cardView)
            ghostViews.append(ghost)
        }
    }

    // MARK: - Layout

    override func resizeSubviews(withOldSize oldSize: NSSize) {
        super.resizeSubviews(withOldSize: oldSize)
        layoutChildren()
    }

    func layoutChildren() {
        let w = bounds.width
        let h = bounds.height

        // All cards (main + ghosts) share the same size, shrunken by maxOffset so the
        // entire stack fits within the container bounds without overflowing into
        // adjacent grid cells. Single-pane cards (no ghosts) fill the full cell.
        //
        // In AppKit Y-up coordinates, "visually top-left" = (x:0, y:maxOffset).
        // Each ghost shifts right (+dx) and down on screen (-dy in AppKit).
        let maxOffset = CGFloat(ghostViews.count * 6)  // 0, 6, or 12
        let cardW = w - maxOffset
        let cardH = h - maxOffset

        cardView.frame = NSRect(x: 0, y: maxOffset, width: cardW, height: cardH)

        for (i, ghost) in ghostViews.enumerated() {
            let offset = CGFloat((i + 1) * 6)
            ghost.frame = NSRect(x: offset, y: maxOffset - offset, width: cardW, height: cardH)
        }
    }

    // MARK: - Hit testing

    override func hitTest(_ point: NSPoint) -> NSView? {
        let localPoint = convert(point, from: superview)
        guard cardView.frame.contains(localPoint) else { return nil }
        return super.hitTest(point)
    }

    // MARK: - Click

    @objc private func handleClick() {
        delegate?.agentCardClicked(agentId: cardView.agentId)
    }

    @objc private func handleDoubleClick() {
        delegate?.agentCardDoubleClicked(agentId: cardView.agentId)
    }

    // MARK: - Context menu

    override func menu(for event: NSEvent) -> NSMenu? {
        let menu = NSMenu()
        let deleteItem = NSMenuItem(title: "Delete Worktree", action: #selector(deleteWorktreeAction), keyEquivalent: "")
        deleteItem.target = self
        menu.addItem(deleteItem)
        menu.addItem(NSMenuItem.separator())
        let closeRepoItem = NSMenuItem(title: "Close Repo", action: #selector(closeRepoAction), keyEquivalent: "")
        closeRepoItem.target = self
        menu.addItem(closeRepoItem)
        return menu
    }

    @objc private func deleteWorktreeAction() {
        delegate?.agentCardDidRequestDelete(agentId: cardView.agentId)
    }

    @objc private func closeRepoAction() {
        delegate?.agentCardDidRequestCloseRepo(agentId: cardView.agentId)
    }

    // MARK: - Test helpers (internal for @testable access)

    func simulateSingleClick() { handleClick() }
    func simulateDoubleClick() { handleDoubleClick() }

    // MARK: - NSGestureRecognizerDelegate

    /// Makes cardView's click recognizer wait for the double-click recognizer to fail,
    /// preventing it from consuming the first tap and blocking the container's double-click.
    func gestureRecognizer(
        _ gestureRecognizer: NSGestureRecognizer,
        shouldRequireFailureOf otherGestureRecognizer: NSGestureRecognizer
    ) -> Bool {
        otherGestureRecognizer === doubleClickRecognizer
    }

    // MARK: - Appearance

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        ghostViews.forEach { $0.needsDisplay = true }
    }

    // MARK: - Private helpers

    private func makeGhostView(index: Int) -> NSView {
        let v = GhostCardView()
        v.ghostIndex = index
        v.wantsLayer = true
        return v
    }
}

// MARK: - FailDependentClickRecognizer

/// An NSClickGestureRecognizer that requires another recognizer to fail before it fires.
/// This replicates UIKit's `require(toFail:)` behaviour using AppKit's subclass override.
private final class FailDependentClickRecognizer: NSClickGestureRecognizer {
    private weak var failTarget: NSGestureRecognizer?

    init(failTarget: NSGestureRecognizer, target: AnyObject?, action: Selector?) {
        self.failTarget = failTarget
        super.init(target: target, action: action)
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) not supported")
    }

    override func shouldRequireFailure(of otherGestureRecognizer: NSGestureRecognizer) -> Bool {
        otherGestureRecognizer === failTarget
    }
}

// MARK: - GhostCardView

/// A purely decorative view that renders a ghost card background and border.
private final class GhostCardView: NSView {
    var ghostIndex: Int = 0

    override var wantsUpdateLayer: Bool { true }

    override func updateLayer() {
        layer?.cornerRadius = 4
        layer?.masksToBounds = true
        let bg = ghostIndex == 0 ? SemanticColors.tileGhost1Bg : SemanticColors.tileGhost2Bg
        layer?.backgroundColor = resolvedCGColor(bg)
        layer?.borderColor = resolvedCGColor(SemanticColors.tileGhostBorder)
        layer?.borderWidth = 1
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        needsDisplay = true
    }
}
