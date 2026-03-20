import AppKit

final class MiniCardView: NSView {
    weak var delegate: AgentCardDelegate?
    private(set) var agentId: String = ""
    var isSelected: Bool = false { didSet { updateAppearance() } }

    // Line 1: status dot + project / branch
    private let statusDot = NSView()
    private let projectLabel = NSTextField(labelWithString: "")
    private let separatorLabel = NSTextField(labelWithString: "/")
    private let branchLabel = NSTextField(labelWithString: "")

    // Line 2: duration + relative time + status text
    private let durationLabel = NSTextField(labelWithString: "")
    private let statusTextLabel = NSTextField(labelWithString: "")

    // Message area
    private let messageLabel = NSTextField(labelWithString: "")

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

        // Status dot (6px circle)
        statusDot.wantsLayer = true
        statusDot.layer?.cornerRadius = 3
        statusDot.translatesAutoresizingMaskIntoConstraints = false
        addSubview(statusDot)

        // Project label
        projectLabel.font = NSFont.systemFont(ofSize: 9, weight: .regular)
        projectLabel.textColor = NSColor(hex: 0x777777)
        projectLabel.lineBreakMode = .byTruncatingTail
        projectLabel.maximumNumberOfLines = 1
        projectLabel.translatesAutoresizingMaskIntoConstraints = false
        projectLabel.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        addSubview(projectLabel)

        // Separator
        separatorLabel.font = NSFont.systemFont(ofSize: 7, weight: .regular)
        separatorLabel.textColor = NSColor(hex: 0x333333)
        separatorLabel.translatesAutoresizingMaskIntoConstraints = false
        separatorLabel.setContentCompressionResistancePriority(.required, for: .horizontal)
        separatorLabel.setContentHuggingPriority(.required, for: .horizontal)
        addSubview(separatorLabel)

        // Branch label
        branchLabel.font = NSFont.systemFont(ofSize: 9, weight: .medium)
        branchLabel.textColor = NSColor.white
        branchLabel.lineBreakMode = .byTruncatingTail
        branchLabel.maximumNumberOfLines = 1
        branchLabel.translatesAutoresizingMaskIntoConstraints = false
        branchLabel.setContentCompressionResistancePriority(.defaultLow - 1, for: .horizontal)
        addSubview(branchLabel)

        // Duration label (line 2 left)
        durationLabel.font = NSFont.systemFont(ofSize: 8, weight: .regular)
        durationLabel.textColor = NSColor(hex: 0x444444)
        durationLabel.lineBreakMode = .byTruncatingTail
        durationLabel.maximumNumberOfLines = 1
        durationLabel.translatesAutoresizingMaskIntoConstraints = false
        durationLabel.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        addSubview(durationLabel)

        // Status text label (line 2 right)
        statusTextLabel.font = NSFont.systemFont(ofSize: 8, weight: .regular)
        statusTextLabel.lineBreakMode = .byTruncatingTail
        statusTextLabel.maximumNumberOfLines = 1
        statusTextLabel.alignment = .right
        statusTextLabel.translatesAutoresizingMaskIntoConstraints = false
        statusTextLabel.setContentCompressionResistancePriority(.defaultHigh, for: .horizontal)
        statusTextLabel.setContentHuggingPriority(.defaultHigh, for: .horizontal)
        addSubview(statusTextLabel)

        // Message area
        messageLabel.font = NSFont.monospacedSystemFont(ofSize: 8, weight: .regular)
        messageLabel.textColor = NSColor(hex: 0x666666)
        messageLabel.lineBreakMode = .byTruncatingTail
        messageLabel.maximumNumberOfLines = 3
        messageLabel.cell?.wraps = true
        messageLabel.cell?.truncatesLastVisibleLine = true
        messageLabel.translatesAutoresizingMaskIntoConstraints = false
        messageLabel.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        messageLabel.setContentHuggingPriority(.defaultLow, for: .vertical)
        addSubview(messageLabel)

        let padding: CGFloat = 8

        NSLayoutConstraint.activate([
            // Line 1: status dot
            statusDot.leadingAnchor.constraint(equalTo: leadingAnchor, constant: padding),
            statusDot.topAnchor.constraint(equalTo: topAnchor, constant: padding),
            statusDot.widthAnchor.constraint(equalToConstant: 6),
            statusDot.heightAnchor.constraint(equalToConstant: 6),

            // Line 1: project name
            projectLabel.leadingAnchor.constraint(equalTo: statusDot.trailingAnchor, constant: 4),
            projectLabel.centerYAnchor.constraint(equalTo: statusDot.centerYAnchor),

            // Line 1: separator
            separatorLabel.leadingAnchor.constraint(equalTo: projectLabel.trailingAnchor, constant: 2),
            separatorLabel.centerYAnchor.constraint(equalTo: statusDot.centerYAnchor),

            // Line 1: branch name
            branchLabel.leadingAnchor.constraint(equalTo: separatorLabel.trailingAnchor, constant: 2),
            branchLabel.centerYAnchor.constraint(equalTo: statusDot.centerYAnchor),
            branchLabel.trailingAnchor.constraint(lessThanOrEqualTo: trailingAnchor, constant: -padding),

            // Line 2: duration
            durationLabel.topAnchor.constraint(equalTo: statusDot.bottomAnchor, constant: 4),
            durationLabel.leadingAnchor.constraint(equalTo: leadingAnchor, constant: padding),

            // Line 2: status text (right-aligned)
            statusTextLabel.centerYAnchor.constraint(equalTo: durationLabel.centerYAnchor),
            statusTextLabel.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -padding),
            statusTextLabel.leadingAnchor.constraint(greaterThanOrEqualTo: durationLabel.trailingAnchor, constant: 4),

            // Message area
            messageLabel.topAnchor.constraint(equalTo: durationLabel.bottomAnchor, constant: 4),
            messageLabel.leadingAnchor.constraint(equalTo: leadingAnchor, constant: padding),
            messageLabel.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -padding),
            messageLabel.bottomAnchor.constraint(lessThanOrEqualTo: bottomAnchor, constant: -padding),
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
            layer.backgroundColor = NSColor(hex: 0x1a1a1a).cgColor
            layer.borderColor = NSColor(hex: 0x33c17b).cgColor
            layer.borderWidth = 1.5
            layer.shadowOpacity = 0
        } else if isHovered {
            layer.backgroundColor = NSColor(hex: 0x222222).cgColor
            layer.borderColor = NSColor.white.withAlphaComponent(0.08).cgColor
            layer.borderWidth = 1.5
            layer.shadowOpacity = 0
            // Brighten text on hover
            branchLabel.textColor = NSColor.white
            messageLabel.textColor = NSColor(hex: 0x888888)
        } else {
            layer.backgroundColor = SemanticColors.tileBarBg.cgColor
            layer.borderColor = NSColor.clear.cgColor
            layer.borderWidth = 0
            layer.shadowOpacity = 0
        }

        // Reset text colors for non-hover states
        if !isHovered {
            branchLabel.textColor = NSColor.white
            messageLabel.textColor = NSColor(hex: 0x666666)
        }
    }
}
