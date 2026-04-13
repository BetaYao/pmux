import AppKit

final class MiniCardView: NSView {
    enum Typography {
        static let primaryPointSize: CGFloat = 12
        static let bodyPointSize: CGFloat = 11
        static let secondaryPointSize: CGFloat = 10
    }

    weak var delegate: AgentCardDelegate?
    private(set) var agentId: String = ""
    var isSelected: Bool = false { didSet { updateAppearance() } }
    var isKeyboardFocused: Bool = false { didSet { updateAppearance() } }

    // Line 1: project/repo name (title)
    private let projectLabel = NSTextField(labelWithString: "")

    // Line 2: duration
    private let durationLabel = NSTextField(labelWithString: "")

    // User prompt line (single line, above message)
    private let promptLabel = NSTextField(labelWithString: "")

    // Message area
    private let messageLabel = NSTextField(labelWithString: "")

    // Line 1: status dots (before repo name) + status text (right)
    private var statusDots: [NSView] = []
    private let statusTextLabel = NSTextField(labelWithString: "")

    // Bottom: worktree/branch name
    private let branchLabel = NSTextField(labelWithString: "")

    private var isHovered = false
    private var currentStatus: String = ""
    private var currentPaneStatuses: [AgentStatus] = []
    private var projectLeadingConstraint: NSLayoutConstraint?
    private var dimOverlayLayer: CALayer?

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        setup()
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) not supported")
    }

    func configure(id: String, project: String, thread: String, status: String, lastMessage: String, lastUserPrompt: String = "", totalDuration: String, roundDuration: String, paneStatuses: [AgentStatus] = [], isMainWorktree: Bool = false, tasks: [TaskItem] = [], activityEvents: [ActivityEvent] = []) {
        agentId = id
        currentStatus = status
        setAccessibilityIdentifier("dashboard.miniCard.\(id)")

        projectLabel.stringValue = project
        // SF Symbol icon: house for base repo, arrow.triangle.branch for worktree
        let symbolName = isMainWorktree ? "house" : "arrow.triangle.branch"
        let branchText = NSMutableAttributedString()
        if let symbolImage = NSImage(systemSymbolName: symbolName, accessibilityDescription: nil) {
            let config = NSImage.SymbolConfiguration(pointSize: Typography.secondaryPointSize - 1, weight: .regular)
            let sized = symbolImage.withSymbolConfiguration(config) ?? symbolImage
            let attachment = NSTextAttachment()
            attachment.image = sized
            branchText.append(NSAttributedString(attachment: attachment))
            branchText.append(NSAttributedString(string: " "))
        }
        branchText.append(NSAttributedString(string: thread, attributes: [
            .font: branchLabel.font as Any,
            .foregroundColor: SemanticColors.muted,
        ]))
        branchLabel.attributedStringValue = branchText
        if !lastUserPrompt.isEmpty {
            promptLabel.stringValue = "\u{276F} " + lastUserPrompt
            promptLabel.isHidden = false
        } else {
            promptLabel.stringValue = ""
            promptLabel.isHidden = true
        }
        // Content priority: tasks > activity feed > last message (same as grid card)
        if let taskAttr = TaskListRenderer.attributedString(for: tasks) {
            messageLabel.attributedStringValue = taskAttr
        } else if !activityEvents.isEmpty {
            let rendered = ActivityFeedRenderer.render(events: activityEvents, maxLines: 2)
            let combined = NSMutableAttributedString()
            for (i, line) in rendered.enumerated() {
                if i > 0 { combined.append(NSAttributedString(string: "\n")) }
                combined.append(line)
            }
            messageLabel.attributedStringValue = combined
        } else {
            messageLabel.stringValue = lastMessage
        }

        // Rebuild status dots on line 1 (before repo name)
        statusDots.forEach { $0.removeFromSuperview() }
        statusDots.removeAll()
        projectLeadingConstraint?.isActive = false

        let statuses = paneStatuses.isEmpty ? [AgentStatus(rawValue: status) ?? .unknown] : paneStatuses
        currentPaneStatuses = statuses
        let padding: CGFloat = 8
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
                dot.centerYAnchor.constraint(equalTo: projectLabel.centerYAnchor),
                dot.leadingAnchor.constraint(equalTo: previousDot?.trailingAnchor ?? leadingAnchor,
                                             constant: previousDot != nil ? 3 : padding),
            ])
            statusDots.append(dot)
            previousDot = dot
        }

        // Anchor repo name after the last dot
        if let lastDot = statusDots.last {
            projectLeadingConstraint = projectLabel.leadingAnchor.constraint(equalTo: lastDot.trailingAnchor, constant: 5)
            projectLeadingConstraint?.isActive = true
        }

        // Line 2: duration + relative time
        let compactTotal = AgentDisplayHelpers.compactDuration(totalDuration)
        let compactRound = AgentDisplayHelpers.compactDuration(roundDuration)
        durationLabel.stringValue = "\u{23F1} \(compactTotal) \u{00B7} \(compactRound)"

        // Status text
        statusTextLabel.stringValue = status.capitalized
        statusTextLabel.textColor = AgentDisplayHelpers.statusColor(status)

        updateAppearance()
    }

    private func setup() {
        wantsLayer = true
        layer?.cornerRadius = 4

        // 16:9 aspect ratio
        let aspectConstraint = widthAnchor.constraint(equalTo: heightAnchor, multiplier: 16.0 / 9.0)
        aspectConstraint.priority = .defaultHigh
        aspectConstraint.isActive = true

        // Line 1: repo name (leading set dynamically after dots in configure())
        projectLabel.font = NSFont.systemFont(ofSize: Typography.primaryPointSize, weight: .semibold)
        projectLabel.textColor = SemanticColors.text
        projectLabel.lineBreakMode = .byTruncatingTail
        projectLabel.maximumNumberOfLines = 1
        projectLabel.translatesAutoresizingMaskIntoConstraints = false
        // Resist compression so the repo name never collapses to 0 width when
        // layout runs mid-rebuild (dots removed, new leading constraint not yet active).
        projectLabel.setContentCompressionResistancePriority(.defaultHigh, for: .horizontal)
        addSubview(projectLabel)

        // Line 1: status text (right)
        statusTextLabel.font = NSFont.systemFont(ofSize: Typography.secondaryPointSize, weight: .regular)
        statusTextLabel.lineBreakMode = .byTruncatingTail
        statusTextLabel.maximumNumberOfLines = 1
        statusTextLabel.alignment = .right
        statusTextLabel.translatesAutoresizingMaskIntoConstraints = false
        statusTextLabel.setContentCompressionResistancePriority(.required, for: .horizontal)
        statusTextLabel.setContentHuggingPriority(.defaultHigh, for: .horizontal)
        addSubview(statusTextLabel)

        // Line 2: duration
        durationLabel.font = NSFont.systemFont(ofSize: Typography.secondaryPointSize, weight: .regular)
        durationLabel.textColor = SemanticColors.muted
        durationLabel.lineBreakMode = .byTruncatingTail
        durationLabel.maximumNumberOfLines = 1
        durationLabel.translatesAutoresizingMaskIntoConstraints = false
        durationLabel.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        addSubview(durationLabel)

        // User prompt line (single line above message)
        promptLabel.font = NSFont.monospacedSystemFont(ofSize: Typography.secondaryPointSize, weight: .regular)
        promptLabel.textColor = SemanticColors.text
        promptLabel.lineBreakMode = .byTruncatingTail
        promptLabel.maximumNumberOfLines = 1
        promptLabel.translatesAutoresizingMaskIntoConstraints = false
        promptLabel.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        promptLabel.setContentHuggingPriority(.defaultHigh, for: .vertical)
        promptLabel.isHidden = true
        addSubview(promptLabel)

        // Message area
        messageLabel.font = NSFont.monospacedSystemFont(ofSize: Typography.secondaryPointSize, weight: .regular)
        messageLabel.textColor = SemanticColors.muted
        messageLabel.lineBreakMode = .byTruncatingTail
        messageLabel.maximumNumberOfLines = 2
        messageLabel.cell?.wraps = true
        messageLabel.cell?.truncatesLastVisibleLine = true
        messageLabel.translatesAutoresizingMaskIntoConstraints = false
        messageLabel.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        messageLabel.setContentHuggingPriority(.defaultLow, for: .vertical)
        addSubview(messageLabel)

        // Bottom: worktree/branch name
        branchLabel.font = NSFont.systemFont(ofSize: Typography.secondaryPointSize, weight: .regular)
        branchLabel.textColor = SemanticColors.muted
        branchLabel.lineBreakMode = .byTruncatingTail
        branchLabel.maximumNumberOfLines = 1
        branchLabel.translatesAutoresizingMaskIntoConstraints = false
        branchLabel.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        addSubview(branchLabel)

        let padding: CGFloat = 8

        // Fallback leading constraint — active whenever the dynamic dot-anchored
        // constraint in configure() is not. Low priority so the dot-anchored
        // constraint wins when present, but guarantees projectLabel always has
        // a valid leading anchor (prevents 0-width collapse during rebuilds).
        let projectLabelFallbackLeading = projectLabel.leadingAnchor.constraint(equalTo: leadingAnchor, constant: padding)
        projectLabelFallbackLeading.priority = .defaultLow

        NSLayoutConstraint.activate([
            // Line 1: dots + repo name (left) + status (right)
            projectLabel.topAnchor.constraint(equalTo: topAnchor, constant: padding + 2),
            projectLabel.trailingAnchor.constraint(lessThanOrEqualTo: statusTextLabel.leadingAnchor, constant: -4),
            projectLabelFallbackLeading,

            statusTextLabel.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -padding),
            statusTextLabel.centerYAnchor.constraint(equalTo: projectLabel.centerYAnchor),

            // Line 2: duration
            durationLabel.topAnchor.constraint(equalTo: projectLabel.bottomAnchor, constant: 3),
            durationLabel.leadingAnchor.constraint(equalTo: leadingAnchor, constant: padding),
            durationLabel.trailingAnchor.constraint(lessThanOrEqualTo: trailingAnchor, constant: -padding),

            // User prompt line
            promptLabel.topAnchor.constraint(equalTo: durationLabel.bottomAnchor, constant: 4),
            promptLabel.leadingAnchor.constraint(equalTo: leadingAnchor, constant: padding),
            promptLabel.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -padding),

            // Message area
            messageLabel.topAnchor.constraint(equalTo: promptLabel.bottomAnchor, constant: 1),
            messageLabel.leadingAnchor.constraint(equalTo: leadingAnchor, constant: padding),
            messageLabel.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -padding),

            // Bottom: worktree name
            branchLabel.leadingAnchor.constraint(equalTo: leadingAnchor, constant: padding),
            branchLabel.trailingAnchor.constraint(lessThanOrEqualTo: trailingAnchor, constant: -padding),
            branchLabel.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -padding),

            // Message doesn't overlap branch
            messageLabel.bottomAnchor.constraint(lessThanOrEqualTo: branchLabel.topAnchor, constant: -4),
        ])

        // Click handler
        let click = NSClickGestureRecognizer(target: self, action: #selector(handleClick))
        addGestureRecognizer(click)

        // Hover tracking
        let trackingArea = NSTrackingArea(
            rect: .zero,
            options: [.mouseEnteredAndExited, .activeInActiveApp, .inVisibleRect],
            owner: self,
            userInfo: nil
        )
        addTrackingArea(trackingArea)

        updateAppearance()
    }

    @objc private func handleClick() {
        delegate?.agentCardClicked(agentId: agentId)
    }

    override func mouseEntered(with event: NSEvent) {
        isHovered = true
        updateAppearance()
    }

    override func mouseExited(with event: NSEvent) {
        isHovered = false
        updateAppearance()
    }

    // MARK: - Dim overlay

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

    override func updateLayer() {
        updateAppearance()
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        needsDisplay = true
    }

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
            return
        }

        if isSelected {
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

        projectLabel.textColor = SemanticColors.text
        branchLabel.textColor = SemanticColors.muted
        messageLabel.textColor = SemanticColors.muted
    }
}
