import AppKit

final class FocusPanelView: NSView {
    let terminalContainer = NSView()

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        setup()
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) not supported") }

    func setCornerMask(_ maskedCorners: CACornerMask, radius: CGFloat) {
        wantsLayer = true
        layer?.cornerRadius = radius
        layer?.maskedCorners = maskedCorners
        layer?.masksToBounds = true
    }

    private func setup() {
        wantsLayer = true
        layer?.cornerRadius = 10
        layer?.cornerCurve = .continuous
        layer?.masksToBounds = true
        layer?.borderWidth = 1
        setAccessibilityIdentifier("dashboard.focusPanel")
        setupTerminalContainer()
    }

    private func setupTerminalContainer() {
        terminalContainer.translatesAutoresizingMaskIntoConstraints = false
        addSubview(terminalContainer)
        NSLayoutConstraint.activate([
            terminalContainer.topAnchor.constraint(equalTo: topAnchor),
            terminalContainer.leadingAnchor.constraint(equalTo: leadingAnchor),
            terminalContainer.trailingAnchor.constraint(equalTo: trailingAnchor),
            terminalContainer.bottomAnchor.constraint(equalTo: bottomAnchor),
        ])
    }

    // MARK: - Focus restore

    /// Clicking anywhere on the focus panel (border, padding) should restore
    /// keyboard focus to the terminal inside.
    override func mouseDown(with event: NSEvent) {
        super.mouseDown(with: event)
        // Walk subviews to find the active split container and ask it to restore focus.
        if let split = terminalContainer.subviews.first(where: { $0 is SplitContainerView }) as? SplitContainerView {
            split.restoreFocusToActiveLeaf()
        }
    }

    override var wantsUpdateLayer: Bool { true }

    override func updateLayer() {
        applyColors()
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        needsDisplay = true
    }

    private func applyColors() {
        layer?.borderColor = resolvedCGColor(SemanticColors.lineAlpha70)
        layer?.backgroundColor = resolvedCGColor(SemanticColors.tileBg)
    }
}
