import AppKit

enum DashboardLayout: String, CaseIterable {
    case grid = "grid"
    case leftRight = "left-right"
    case topSmall = "top-small"
    case topLarge = "top-large"

    var displayName: String {
        switch self {
        case .grid: return "Grid"
        case .leftRight: return "Left-Right"
        case .topSmall: return "Top-Small"
        case .topLarge: return "Top-Large"
        }
    }

    var iconName: String {
        switch self {
        case .grid: return "square.grid.2x2"
        case .leftRight: return "rectangle.lefthalf.filled"
        case .topSmall: return "rectangle.tophalf.inset.filled"
        case .topLarge: return "rectangle.bottomhalf.inset.filled"
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

        // Build items with dividers
        var topAnchorRef = topAnchor
        let allCases = DashboardLayout.allCases
        for (index, layoutCase) in allCases.enumerated() {
            // Divider between items (not before first)
            if index > 0 {
                let divider = NSView()
                divider.wantsLayer = true
                divider.translatesAutoresizingMaskIntoConstraints = false
                divider.identifier = NSUserInterfaceItemIdentifier("layout.divider.\(index)")
                addSubview(divider)
                NSLayoutConstraint.activate([
                    divider.topAnchor.constraint(equalTo: topAnchorRef),
                    divider.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 8),
                    divider.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -8),
                    divider.heightAnchor.constraint(equalToConstant: 1),
                ])
                topAnchorRef = divider.bottomAnchor
            }

            let item = LayoutItemView(layout: layoutCase)
            item.target = self
            item.action = #selector(itemClicked(_:))
            item.identifier = NSUserInterfaceItemIdentifier("layout.item.\(layoutCase.rawValue)")
            addSubview(item)
            itemViews.append(item)

            NSLayoutConstraint.activate([
                item.topAnchor.constraint(equalTo: topAnchorRef),
                item.leadingAnchor.constraint(equalTo: leadingAnchor),
                item.trailingAnchor.constraint(equalTo: trailingAnchor),
            ])
            topAnchorRef = item.bottomAnchor
        }

        NSLayoutConstraint.activate([
            widthAnchor.constraint(equalToConstant: 180),
            bottomAnchor.constraint(equalTo: topAnchorRef),
        ])

        applyColors()
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

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        applyColors()
        // Update dividers
        for sub in subviews where (sub.identifier?.rawValue ?? "").hasPrefix("layout.divider.") {
            sub.layer?.backgroundColor = SemanticColors.line.cgColor
        }
    }

    override func layout() {
        super.layout()
        layer?.shadowPath = CGPath(roundedRect: bounds, cornerWidth: 8, cornerHeight: 8, transform: nil)
    }

    private func applyColors() {
        layer?.backgroundColor = SemanticColors.panel.cgColor
        layer?.borderColor = SemanticColors.line.cgColor
        for sub in subviews where (sub.identifier?.rawValue ?? "").hasPrefix("layout.divider.") {
            sub.layer?.backgroundColor = SemanticColors.line.cgColor
        }
    }
}

// MARK: - LayoutItemView

private final class LayoutItemView: NSView {
    let layout: DashboardLayout
    var target: AnyObject?
    var action: Selector?

    private let iconView = NSImageView()
    private let titleLabel = NSTextField(labelWithString: "")
    private let checkLabel = NSTextField(labelWithString: "\u{2713}")
    private var isActive = false
    private var isHovered = false
    private var trackingArea: NSTrackingArea?

    // Default text color #aaa
    private static let defaultTextColor = NSColor(srgbRed: 0xAA / 255.0,
                                                   green: 0xAA / 255.0,
                                                    blue: 0xAA / 255.0,
                                                   alpha: 1.0)

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
        translatesAutoresizingMaskIntoConstraints = false

        // Icon
        iconView.translatesAutoresizingMaskIntoConstraints = false
        iconView.imageScaling = .scaleProportionallyDown
        if let img = NSImage(systemSymbolName: layout.iconName, accessibilityDescription: layout.displayName) {
            iconView.image = img
        }
        iconView.contentTintColor = Self.defaultTextColor
        addSubview(iconView)

        // Label
        titleLabel.stringValue = layout.displayName
        titleLabel.font = NSFont.systemFont(ofSize: 12)
        titleLabel.textColor = Self.defaultTextColor
        titleLabel.lineBreakMode = .byTruncatingTail
        titleLabel.translatesAutoresizingMaskIntoConstraints = false
        addSubview(titleLabel)

        // Checkmark
        checkLabel.font = NSFont.systemFont(ofSize: 12, weight: .medium)
        checkLabel.textColor = SemanticColors.accent
        checkLabel.translatesAutoresizingMaskIntoConstraints = false
        checkLabel.isHidden = true
        addSubview(checkLabel)

        NSLayoutConstraint.activate([
            heightAnchor.constraint(equalToConstant: 32),

            iconView.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 12),
            iconView.centerYAnchor.constraint(equalTo: centerYAnchor),
            iconView.widthAnchor.constraint(equalToConstant: 16),
            iconView.heightAnchor.constraint(equalToConstant: 16),

            titleLabel.leadingAnchor.constraint(equalTo: iconView.trailingAnchor, constant: 8),
            titleLabel.centerYAnchor.constraint(equalTo: centerYAnchor),
            titleLabel.trailingAnchor.constraint(lessThanOrEqualTo: checkLabel.leadingAnchor, constant: -4),

            checkLabel.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -12),
            checkLabel.centerYAnchor.constraint(equalTo: centerYAnchor),
        ])

        let click = NSClickGestureRecognizer(target: self, action: #selector(handleClick))
        addGestureRecognizer(click)
    }

    func setActive(_ active: Bool) {
        isActive = active
        checkLabel.isHidden = !active
        applyAppearance()
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
        applyAppearance()
    }

    override func mouseExited(with event: NSEvent) {
        isHovered = false
        applyAppearance()
    }

    private func applyAppearance() {
        // Background
        if isHovered && !isActive {
            layer?.backgroundColor = NSColor.white.withAlphaComponent(0.03).cgColor
        } else {
            layer?.backgroundColor = NSColor.clear.cgColor
        }

        // Text/icon color
        if isActive {
            titleLabel.textColor = SemanticColors.accent
            iconView.contentTintColor = SemanticColors.accent
            checkLabel.textColor = SemanticColors.accent
        } else if isHovered {
            titleLabel.textColor = NSColor.white
            iconView.contentTintColor = NSColor.white
        } else {
            titleLabel.textColor = Self.defaultTextColor
            iconView.contentTintColor = Self.defaultTextColor
        }
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        applyAppearance()
    }
}
