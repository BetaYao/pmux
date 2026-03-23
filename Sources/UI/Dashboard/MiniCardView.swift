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

    // Line 1: status dot + branch
    private let statusDot = NSView()
    private let branchLabel = NSTextField(labelWithString: "")

    // Line 2: duration
    private let durationLabel = NSTextField(labelWithString: "")

    // Message area
    private let messageLabel = NSTextField(labelWithString: "")

    // Bottom line: project (left) + status (right)
    private let projectLabel = NSTextField(labelWithString: "")
    private let statusTextLabel = NSTextField(labelWithString: "")

    private var isHovered = false
    private var currentStatus: String = ""

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        setup()
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) not supported")
    }

    func configure(id: String, project: String, thread: String, status: String, lastMessage: String, totalDuration: String, roundDuration: String) {
        agentId = id
        currentStatus = status
        setAccessibilityIdentifier("dashboard.miniCard.\(id)")

        projectLabel.stringValue = project
        branchLabel.stringValue = thread
        messageLabel.stringValue = lastMessage

        statusDot.layer?.backgroundColor = AgentDisplayHelpers.statusColor(status).cgColor

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

        // Line 1: status dot
        statusDot.wantsLayer = true
        statusDot.layer?.cornerRadius = 3
        statusDot.translatesAutoresizingMaskIntoConstraints = false
        addSubview(statusDot)

        // Line 1: branch label
        branchLabel.font = NSFont.systemFont(ofSize: Typography.primaryPointSize, weight: .semibold)
        branchLabel.textColor = SemanticColors.text
        branchLabel.lineBreakMode = .byTruncatingTail
        branchLabel.maximumNumberOfLines = 1
        branchLabel.translatesAutoresizingMaskIntoConstraints = false
        branchLabel.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        addSubview(branchLabel)

        // Line 2: duration
        durationLabel.font = NSFont.systemFont(ofSize: Typography.secondaryPointSize, weight: .regular)
        durationLabel.textColor = SemanticColors.muted
        durationLabel.lineBreakMode = .byTruncatingTail
        durationLabel.maximumNumberOfLines = 1
        durationLabel.translatesAutoresizingMaskIntoConstraints = false
        durationLabel.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        addSubview(durationLabel)

        // Message area
        messageLabel.font = NSFont.monospacedSystemFont(ofSize: Typography.secondaryPointSize, weight: .regular)
        messageLabel.textColor = SemanticColors.muted
        messageLabel.lineBreakMode = .byTruncatingTail
        messageLabel.maximumNumberOfLines = 3
        messageLabel.cell?.wraps = true
        messageLabel.cell?.truncatesLastVisibleLine = true
        messageLabel.translatesAutoresizingMaskIntoConstraints = false
        messageLabel.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        messageLabel.setContentHuggingPriority(.defaultLow, for: .vertical)
        addSubview(messageLabel)

        // Bottom line: project (left)
        projectLabel.font = NSFont.systemFont(ofSize: Typography.secondaryPointSize, weight: .regular)
        projectLabel.textColor = SemanticColors.muted
        projectLabel.lineBreakMode = .byTruncatingTail
        projectLabel.maximumNumberOfLines = 1
        projectLabel.translatesAutoresizingMaskIntoConstraints = false
        projectLabel.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        addSubview(projectLabel)

        // Bottom line: status (right)
        statusTextLabel.font = NSFont.systemFont(ofSize: Typography.secondaryPointSize, weight: .regular)
        statusTextLabel.lineBreakMode = .byTruncatingTail
        statusTextLabel.maximumNumberOfLines = 1
        statusTextLabel.alignment = .right
        statusTextLabel.translatesAutoresizingMaskIntoConstraints = false
        statusTextLabel.setContentCompressionResistancePriority(.required, for: .horizontal)
        statusTextLabel.setContentHuggingPriority(.defaultHigh, for: .horizontal)
        addSubview(statusTextLabel)

        let padding: CGFloat = 8

        NSLayoutConstraint.activate([
            // Line 1: status dot
            statusDot.leadingAnchor.constraint(equalTo: leadingAnchor, constant: padding),
            statusDot.topAnchor.constraint(equalTo: topAnchor, constant: padding + 2),
            statusDot.widthAnchor.constraint(equalToConstant: 6),
            statusDot.heightAnchor.constraint(equalToConstant: 6),

            // Line 1: branch name
            branchLabel.leadingAnchor.constraint(equalTo: statusDot.trailingAnchor, constant: 5),
            branchLabel.centerYAnchor.constraint(equalTo: statusDot.centerYAnchor),
            branchLabel.trailingAnchor.constraint(lessThanOrEqualTo: trailingAnchor, constant: -padding),

            // Line 2: duration
            durationLabel.topAnchor.constraint(equalTo: branchLabel.bottomAnchor, constant: 3),
            durationLabel.leadingAnchor.constraint(equalTo: leadingAnchor, constant: padding),
            durationLabel.trailingAnchor.constraint(lessThanOrEqualTo: trailingAnchor, constant: -padding),

            // Message area
            messageLabel.topAnchor.constraint(equalTo: durationLabel.bottomAnchor, constant: 4),
            messageLabel.leadingAnchor.constraint(equalTo: leadingAnchor, constant: padding),
            messageLabel.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -padding),

            // Bottom line: project (left)
            projectLabel.leadingAnchor.constraint(equalTo: leadingAnchor, constant: padding),
            projectLabel.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -padding),
            projectLabel.trailingAnchor.constraint(lessThanOrEqualTo: statusTextLabel.leadingAnchor, constant: -4),

            // Bottom line: status (right)
            statusTextLabel.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -padding),
            statusTextLabel.centerYAnchor.constraint(equalTo: projectLabel.centerYAnchor),

            // Message bottom connects to project top
            messageLabel.bottomAnchor.constraint(lessThanOrEqualTo: projectLabel.topAnchor, constant: -4),
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

        branchLabel.textColor = SemanticColors.text
        messageLabel.textColor = SemanticColors.muted
    }
}
