import AppKit

final class MiniCardView: NSView {
    enum Typography {
        static let primaryPointSize: CGFloat = 12
        static let secondaryPointSize: CGFloat = 10
    }

    weak var delegate: AgentCardDelegate?
    private(set) var agentId: String = ""
    var isSelected: Bool = false { didSet { updateAppearance() } }
    var isKeyboardFocused: Bool = false { didSet { updateAppearance() } }

    // Line 1–2: title (wraps up to 2 lines)
    private let titleLabel = NSTextField(labelWithString: "")
    // Line 3: status text (right) + duration (left), with leading status dots
    private let durationLabel = NSTextField(labelWithString: "")
    private let statusTextLabel = NSTextField(labelWithString: "")
    private var statusDots: [NSView] = []
    private var durationLeadingConstraint: NSLayoutConstraint?
    // Line 4: repo · worktree
    private let repoWorktreeLabel = NSTextField(labelWithString: "")

    private var isHovered = false
    private var dimOverlayLayer: CALayer?

    // Test hooks
    var titleTextForTesting: String { titleLabel.stringValue }
    var repoWorktreeTextForTesting: String { repoWorktreeLabel.stringValue }

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        setup()
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) not supported") }

    func configure(id: String, project: String, thread: String, status: String, lastMessage: String, lastUserPrompt: String = "", totalDuration: String, roundDuration: String, paneStatuses: [AgentStatus] = [], isMainWorktree: Bool = false, tasks: [TaskItem] = [], activityEvents: [ActivityEvent] = []) {
        agentId = id
        setAccessibilityIdentifier("dashboard.miniCard.\(id)")

        let title = lastUserPrompt.isEmpty ? thread : lastUserPrompt
        titleLabel.stringValue = title

        // Line 4: repo · worktree
        repoWorktreeLabel.stringValue = "\(project)  \u{00B7}  \(thread)"

        // Status dots before the duration line
        statusDots.forEach { $0.removeFromSuperview() }
        statusDots.removeAll()
        durationLeadingConstraint?.isActive = false
        let statuses = paneStatuses.isEmpty ? [AgentStatus(rawValue: status) ?? .unknown] : paneStatuses
        var previousDot: NSView? = nil
        for agentStatus in statuses {
            let dot = NSView()
            dot.wantsLayer = true
            dot.layer?.cornerRadius = 3
            dot.layer?.backgroundColor = agentStatus.color.cgColor
            dot.translatesAutoresizingMaskIntoConstraints = false
            addSubview(dot)
            NSLayoutConstraint.activate([
                dot.widthAnchor.constraint(equalToConstant: 6),
                dot.heightAnchor.constraint(equalToConstant: 6),
                dot.centerYAnchor.constraint(equalTo: durationLabel.centerYAnchor),
                dot.leadingAnchor.constraint(equalTo: previousDot?.trailingAnchor ?? leadingAnchor,
                                             constant: previousDot != nil ? 3 : 8),
            ])
            statusDots.append(dot)
            previousDot = dot
        }
        if let lastDot = statusDots.last {
            durationLeadingConstraint = durationLabel.leadingAnchor.constraint(equalTo: lastDot.trailingAnchor, constant: 5)
            durationLeadingConstraint?.isActive = true
        }

        let compactTotal = AgentDisplayHelpers.compactDuration(totalDuration)
        let compactRound = AgentDisplayHelpers.compactDuration(roundDuration)
        durationLabel.stringValue = "\u{23F1} \(compactTotal) \u{00B7} \(compactRound)"

        statusTextLabel.stringValue = status.capitalized
        statusTextLabel.textColor = AgentDisplayHelpers.statusColor(status)

        updateAppearance()
    }

    private func setup() {
        wantsLayer = true
        layer?.cornerRadius = 6

        titleLabel.font = NSFont.systemFont(ofSize: Typography.primaryPointSize, weight: .semibold)
        titleLabel.textColor = SemanticColors.text
        titleLabel.lineBreakMode = .byTruncatingTail
        titleLabel.maximumNumberOfLines = 2
        titleLabel.cell?.wraps = true
        titleLabel.translatesAutoresizingMaskIntoConstraints = false
        titleLabel.setContentCompressionResistancePriority(.defaultHigh, for: .vertical)
        addSubview(titleLabel)

        statusTextLabel.font = NSFont.systemFont(ofSize: Typography.secondaryPointSize, weight: .regular)
        statusTextLabel.lineBreakMode = .byTruncatingTail
        statusTextLabel.maximumNumberOfLines = 1
        statusTextLabel.alignment = .right
        statusTextLabel.translatesAutoresizingMaskIntoConstraints = false
        statusTextLabel.setContentCompressionResistancePriority(.required, for: .horizontal)
        statusTextLabel.setContentHuggingPriority(.defaultHigh, for: .horizontal)
        addSubview(statusTextLabel)

        durationLabel.font = NSFont.systemFont(ofSize: Typography.secondaryPointSize, weight: .regular)
        durationLabel.textColor = SemanticColors.muted
        durationLabel.lineBreakMode = .byTruncatingTail
        durationLabel.maximumNumberOfLines = 1
        durationLabel.translatesAutoresizingMaskIntoConstraints = false
        durationLabel.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        addSubview(durationLabel)

        repoWorktreeLabel.font = NSFont.systemFont(ofSize: Typography.secondaryPointSize, weight: .regular)
        repoWorktreeLabel.textColor = SemanticColors.muted
        repoWorktreeLabel.lineBreakMode = .byTruncatingTail
        repoWorktreeLabel.maximumNumberOfLines = 1
        repoWorktreeLabel.translatesAutoresizingMaskIntoConstraints = false
        addSubview(repoWorktreeLabel)

        let padding: CGFloat = 8
        let durationFallbackLeading = durationLabel.leadingAnchor.constraint(equalTo: leadingAnchor, constant: padding)
        durationFallbackLeading.priority = .defaultLow

        NSLayoutConstraint.activate([
            titleLabel.topAnchor.constraint(equalTo: topAnchor, constant: padding),
            titleLabel.leadingAnchor.constraint(equalTo: leadingAnchor, constant: padding),
            titleLabel.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -padding),

            durationLabel.topAnchor.constraint(equalTo: titleLabel.bottomAnchor, constant: 4),
            durationFallbackLeading,
            durationLabel.trailingAnchor.constraint(lessThanOrEqualTo: statusTextLabel.leadingAnchor, constant: -4),

            statusTextLabel.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -padding),
            statusTextLabel.centerYAnchor.constraint(equalTo: durationLabel.centerYAnchor),

            repoWorktreeLabel.topAnchor.constraint(equalTo: durationLabel.bottomAnchor, constant: 4),
            repoWorktreeLabel.leadingAnchor.constraint(equalTo: leadingAnchor, constant: padding),
            repoWorktreeLabel.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -padding),
            repoWorktreeLabel.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -padding),
        ])

        let click = NSClickGestureRecognizer(target: self, action: #selector(handleClick))
        addGestureRecognizer(click)
        addTrackingArea(NSTrackingArea(
            rect: .zero,
            options: [.mouseEnteredAndExited, .activeInActiveApp, .inVisibleRect],
            owner: self, userInfo: nil
        ))
        updateAppearance()
    }

    @objc private func handleClick() { delegate?.agentCardClicked(agentId: agentId) }
    override func mouseEntered(with event: NSEvent) { isHovered = true; updateAppearance() }
    override func mouseExited(with event: NSEvent) { isHovered = false; updateAppearance() }

    func showDimOverlay(opacity: CGFloat) {
        if dimOverlayLayer == nil {
            let overlay = CALayer()
            overlay.backgroundColor = NSColor.white.withAlphaComponent(opacity).cgColor
            overlay.frame = bounds
            overlay.autoresizingMask = [.layerWidthSizable, .layerHeightSizable]
            layer?.addSublayer(overlay)
            dimOverlayLayer = overlay
        }
    }

    func hideDimOverlay() {
        dimOverlayLayer?.removeFromSuperlayer()
        dimOverlayLayer = nil
    }

    override var acceptsFirstResponder: Bool { false }
    override var wantsUpdateLayer: Bool { true }
    override func updateLayer() { updateAppearance() }
    override func viewDidChangeEffectiveAppearance() { super.viewDidChangeEffectiveAppearance(); needsDisplay = true }

    private func updateAppearance() {
        guard let layer = layer else { return }
        if isKeyboardFocused {
            layer.backgroundColor = resolvedCGColor(SemanticColors.panel2)
            layer.borderColor = resolvedCGColor(SemanticColors.accent)
            layer.borderWidth = 2
            layer.shadowColor = resolvedCGColor(SemanticColors.accent)
            layer.shadowOpacity = 0.6
            layer.shadowRadius = 8
            layer.shadowOffset = .zero
            layer.masksToBounds = false
        } else if isSelected {
            layer.backgroundColor = resolvedCGColor(SemanticColors.panel2)
            layer.borderColor = resolvedCGColor(SemanticColors.accent)
            layer.borderWidth = 1.5
            layer.shadowOpacity = 0
        } else if isHovered {
            layer.backgroundColor = resolvedCGColor(SemanticColors.arcBlockHover)
            layer.borderColor = resolvedCGColor(SemanticColors.lineAlpha40)
            layer.borderWidth = 1.5
            layer.shadowOpacity = 0
        } else {
            layer.backgroundColor = resolvedCGColor(SemanticColors.tileBarBg)
            layer.borderColor = resolvedCGColor(SemanticColors.lineAlpha45)
            layer.borderWidth = 1
            layer.shadowColor = resolvedCGColor(SemanticColors.miniCardShadowDefault)
            layer.shadowOpacity = 1
            layer.shadowRadius = 8
            layer.shadowOffset = NSSize(width: 0, height: -2)
        }
        titleLabel.textColor = SemanticColors.text
        repoWorktreeLabel.textColor = SemanticColors.muted
    }
}
