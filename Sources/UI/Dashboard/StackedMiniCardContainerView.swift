// Sources/UI/Dashboard/StackedMiniCardContainerView.swift
import AppKit

final class StackedMiniCardContainerView: NSView {
    let miniCardView = MiniCardView()
    private(set) var ghostViews: [NSView] = []

    /// The container owns click handling. miniCardView.delegate must remain nil.
    weak var delegate: AgentCardDelegate?

    var agentId: String { miniCardView.agentId }

    var isSelected: Bool {
        get { miniCardView.isSelected }
        set { miniCardView.isSelected = newValue }
    }

    override init(frame: NSRect) {
        super.init(frame: frame)
        setup()
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) not supported") }

    private func setup() {
        wantsLayer = true
        layer?.masksToBounds = false

        // Disable MiniCardView's own click handler to prevent double-firing.
        // Remove its gesture recognizers and use the container's instead.
        miniCardView.gestureRecognizers.forEach { miniCardView.removeGestureRecognizer($0) }
        miniCardView.translatesAutoresizingMaskIntoConstraints = true
        addSubview(miniCardView)

        let click = NSClickGestureRecognizer(target: self, action: #selector(handleClick))
        addGestureRecognizer(click)
    }

    // MARK: - Configure

    func configure(paneCount: Int) {
        let needed = min(max(paneCount - 1, 0), 2)

        while ghostViews.count > needed {
            ghostViews.removeLast().removeFromSuperview()
        }

        while ghostViews.count < needed {
            let ghost = MiniGhostCardView()
            ghost.ghostIndex = ghostViews.count
            ghost.wantsLayer = true
            addSubview(ghost, positioned: .below, relativeTo: miniCardView)
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
        let ghostOffset: CGFloat = 3
        let maxOffset = CGFloat(ghostViews.count) * ghostOffset
        let cardW = w - maxOffset
        let cardH = h - maxOffset

        miniCardView.frame = NSRect(x: 0, y: maxOffset, width: cardW, height: cardH)

        for (i, ghost) in ghostViews.enumerated() {
            let offset = CGFloat(i + 1) * ghostOffset
            ghost.frame = NSRect(x: offset, y: maxOffset - offset, width: cardW, height: cardH)
        }
    }

    // MARK: - Hit testing

    override func hitTest(_ point: NSPoint) -> NSView? {
        guard miniCardView.frame.contains(point) else { return nil }
        return super.hitTest(point)
    }

    // MARK: - Click

    @objc private func handleClick() {
        delegate?.agentCardClicked(agentId: miniCardView.agentId)
    }

    // MARK: - Appearance

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        ghostViews.forEach { $0.needsDisplay = true }
    }
}

// MARK: - MiniGhostCardView

private final class MiniGhostCardView: NSView {
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
