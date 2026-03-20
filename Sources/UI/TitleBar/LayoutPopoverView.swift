import AppKit

enum DashboardLayout: String, CaseIterable {
    case grid = "grid"
    case leftRight = "left-right"
    case topSmall = "top-small"
    case topLarge = "top-large"

    var displayName: String {
        switch self {
        case .grid: return "1 Grid"
        case .leftRight: return "2 左大右列"
        case .topSmall: return "3 上小下大"
        case .topLarge: return "4 上大下小"
        }
    }
}

protocol LayoutPopoverDelegate: AnyObject {
    func layoutPopover(_ popover: LayoutPopoverView, didSelect layout: DashboardLayout)
}

final class LayoutPopoverView: NSView {
    weak var delegate: LayoutPopoverDelegate?
    private(set) var currentLayout: DashboardLayout = .leftRight
    private var itemViews: [LayoutItemView] = []
    private let contentStack = NSStackView()

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        setup()
    }

    required init?(coder: NSCoder) {
        super.init(coder: coder)
        setup()
    }

    func setLayout(_ layout: DashboardLayout) {
        currentLayout = layout
        updateItemStates()
    }

    func toggle() {
        isHidden = !isHidden
    }

    func dismiss() {
        isHidden = true
    }

    // MARK: - Setup

    private func setup() {
        identifier = NSUserInterfaceItemIdentifier("layout.popover")
        wantsLayer = true
        isHidden = true
        translatesAutoresizingMaskIntoConstraints = false

        layer?.cornerRadius = 8
        layer?.borderWidth = 1

        // Shadow
        shadow = NSShadow()
        layer?.shadowOpacity = 1
        layer?.shadowRadius = 14
        layer?.shadowOffset = NSSize(width: 0, height: -6)
        layer?.shadowColor = NSColor.black.withAlphaComponent(0.18).cgColor

        // Content stack
        contentStack.orientation = .vertical
        contentStack.spacing = 1
        contentStack.edgeInsets = NSEdgeInsets(top: 4, left: 4, bottom: 4, right: 4)
        contentStack.translatesAutoresizingMaskIntoConstraints = false
        addSubview(contentStack)

        // Build items
        for layout in DashboardLayout.allCases {
            let item = LayoutItemView(layout: layout)
            item.target = self
            item.action = #selector(itemClicked(_:))
            item.identifier = NSUserInterfaceItemIdentifier("layout.item.\(layout.rawValue)")
            contentStack.addArrangedSubview(item)
            itemViews.append(item)
        }

        NSLayoutConstraint.activate([
            widthAnchor.constraint(equalToConstant: 200),

            contentStack.topAnchor.constraint(equalTo: topAnchor),
            contentStack.leadingAnchor.constraint(equalTo: leadingAnchor),
            contentStack.trailingAnchor.constraint(equalTo: trailingAnchor),
            contentStack.bottomAnchor.constraint(equalTo: bottomAnchor),
        ])

        updateItemStates()
    }

    @objc private func itemClicked(_ sender: LayoutItemView) {
        currentLayout = sender.layout
        updateItemStates()
        delegate?.layoutPopover(self, didSelect: sender.layout)
        dismiss()
    }

    private func updateItemStates() {
        for item in itemViews {
            item.setActive(item.layout == currentLayout)
        }
    }

    // MARK: - Theme

    override func updateLayer() {
        layer?.backgroundColor = SemanticColors.panel.cgColor
        layer?.borderColor = SemanticColors.line.withAlphaComponent(0.4).cgColor
    }
}

// MARK: - LayoutItemView

private final class LayoutItemView: NSView {
    let layout: DashboardLayout
    var target: AnyObject?
    var action: Selector?

    private let titleLabel = NSTextField(labelWithString: "")
    private var isActive = false
    private var isHovered = false
    private var trackingArea: NSTrackingArea?

    init(layout: DashboardLayout) {
        self.layout = layout
        super.init(frame: .zero)
        setup()
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) not supported")
    }

    private func setup() {
        wantsLayer = true
        layer?.cornerRadius = 5
        translatesAutoresizingMaskIntoConstraints = false

        titleLabel.stringValue = layout.displayName
        titleLabel.font = NSFont.systemFont(ofSize: 12)
        titleLabel.lineBreakMode = .byTruncatingTail
        titleLabel.translatesAutoresizingMaskIntoConstraints = false
        addSubview(titleLabel)

        NSLayoutConstraint.activate([
            heightAnchor.constraint(equalToConstant: 30),
            titleLabel.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 10),
            titleLabel.trailingAnchor.constraint(lessThanOrEqualTo: trailingAnchor, constant: -10),
            titleLabel.centerYAnchor.constraint(equalTo: centerYAnchor),
        ])

        let click = NSClickGestureRecognizer(target: self, action: #selector(handleClick))
        addGestureRecognizer(click)
    }

    func setActive(_ active: Bool) {
        isActive = active
        titleLabel.font = active
            ? NSFont.systemFont(ofSize: 12, weight: .semibold)
            : NSFont.systemFont(ofSize: 12)
        needsDisplay = true
        updateBackground()
    }

    @objc private func handleClick() {
        guard let target = target, let action = action else { return }
        NSApp.sendAction(action, to: target, from: self)
    }

    // MARK: - Tracking

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        if let existing = trackingArea {
            removeTrackingArea(existing)
        }
        let area = NSTrackingArea(
            rect: bounds,
            options: [.mouseEnteredAndExited, .activeInActiveApp],
            owner: self,
            userInfo: nil
        )
        addTrackingArea(area)
        trackingArea = area
    }

    override func mouseEntered(with event: NSEvent) {
        isHovered = true
        updateBackground()
    }

    override func mouseExited(with event: NSEvent) {
        isHovered = false
        updateBackground()
    }

    private func updateBackground() {
        if isActive {
            layer?.backgroundColor = SemanticColors.accent.withAlphaComponent(0.12).cgColor
        } else if isHovered {
            layer?.backgroundColor = SemanticColors.line.withAlphaComponent(0.18).cgColor
        } else {
            layer?.backgroundColor = NSColor.clear.cgColor
        }
        titleLabel.textColor = SemanticColors.text
    }

    override func updateLayer() {
        updateBackground()
    }

    override func becomeFirstResponder() -> Bool {
        true
    }

    override var focusRingMaskBounds: NSRect {
        bounds
    }

    override func drawFocusRingMask() {
        let path = NSBezierPath(roundedRect: bounds, xRadius: 5, yRadius: 5)
        path.fill()
    }

    override var focusRingType: NSFocusRingType {
        get { .exterior }
        set { }
    }
}
