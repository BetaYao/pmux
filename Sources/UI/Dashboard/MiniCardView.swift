import AppKit

final class MiniCardView: NSView {
    weak var delegate: AgentCardDelegate?
    private(set) var agentId: String = ""
    private var currentStatus: String = ""
    var isSelected: Bool = false { didSet { updateAppearance() } }

    private let statusDot = NSView()
    private let titleLabel = NSTextField(labelWithString: "")
    private let messageLabel = NSTextField(labelWithString: "")
    private let timeLabel = NSTextField(labelWithString: "")
    private var isHovered = false

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        setup()
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) not supported")
    }

    func configure(id: String, project: String, thread: String, status: String, lastMessage: String, totalDuration: String, roundDuration: String) {
        agentId = id
        setAccessibilityIdentifier("dashboard.miniCard.\(id)")

        titleLabel.stringValue = "\(project) - \(thread)"
        messageLabel.stringValue = lastMessage
        statusDot.layer?.backgroundColor = AgentDisplayHelpers.statusColor(status).cgColor

        let compactTotal = AgentDisplayHelpers.compactDuration(totalDuration)
        let compactRound = AgentDisplayHelpers.compactDuration(roundDuration)
        timeLabel.stringValue = "\u{03A3} \(compactTotal) \u{00B7} \u{27F3} \(compactRound)"

        if status != currentStatus {
            currentStatus = status
            updateAppearance()
        }
    }

    private func setup() {
        wantsLayer = true
        layer?.cornerRadius = 9
        layer?.borderWidth = 1

        // 16:9 aspect ratio
        let aspectConstraint = widthAnchor.constraint(equalTo: heightAnchor, multiplier: 16.0 / 9.0)
        aspectConstraint.priority = .defaultHigh
        aspectConstraint.isActive = true

        // Status dot
        statusDot.wantsLayer = true
        statusDot.layer?.cornerRadius = 4
        statusDot.translatesAutoresizingMaskIntoConstraints = false
        addSubview(statusDot)

        // Title
        titleLabel.font = NSFont.systemFont(ofSize: 11, weight: .bold)
        titleLabel.textColor = SemanticColors.text
        titleLabel.lineBreakMode = .byTruncatingTail
        titleLabel.maximumNumberOfLines = 1
        titleLabel.translatesAutoresizingMaskIntoConstraints = false
        addSubview(titleLabel)

        // Message
        messageLabel.font = NSFont.systemFont(ofSize: 11)
        messageLabel.textColor = SemanticColors.muted
        messageLabel.lineBreakMode = .byTruncatingTail
        messageLabel.maximumNumberOfLines = 2
        messageLabel.cell?.wraps = true
        messageLabel.cell?.truncatesLastVisibleLine = true
        messageLabel.translatesAutoresizingMaskIntoConstraints = false
        messageLabel.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        messageLabel.setContentHuggingPriority(.defaultLow, for: .vertical)
        addSubview(messageLabel)

        // Time
        timeLabel.font = NSFont.systemFont(ofSize: 10)
        timeLabel.textColor = SemanticColors.muted
        timeLabel.lineBreakMode = .byTruncatingTail
        timeLabel.maximumNumberOfLines = 1
        timeLabel.translatesAutoresizingMaskIntoConstraints = false
        addSubview(timeLabel)

        NSLayoutConstraint.activate([
            // Status dot
            statusDot.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 8),
            statusDot.topAnchor.constraint(equalTo: topAnchor, constant: 8),
            statusDot.widthAnchor.constraint(equalToConstant: 8),
            statusDot.heightAnchor.constraint(equalToConstant: 8),

            // Title
            titleLabel.leadingAnchor.constraint(equalTo: statusDot.trailingAnchor, constant: 6),
            titleLabel.centerYAnchor.constraint(equalTo: statusDot.centerYAnchor),
            titleLabel.trailingAnchor.constraint(lessThanOrEqualTo: trailingAnchor, constant: -8),

            // Message
            messageLabel.topAnchor.constraint(equalTo: statusDot.bottomAnchor, constant: 6),
            messageLabel.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 8),
            messageLabel.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -8),

            // Time
            timeLabel.topAnchor.constraint(greaterThanOrEqualTo: messageLabel.bottomAnchor, constant: 6),
            timeLabel.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 8),
            timeLabel.trailingAnchor.constraint(lessThanOrEqualTo: trailingAnchor, constant: -8),
            timeLabel.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -8),
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

    private func updateAppearance() {
        guard let layer = layer else { return }
        let accent = SemanticColors.accent

        layer.backgroundColor = SemanticColors.panel2.cgColor

        if isSelected {
            layer.borderColor = accent.withAlphaComponent(0.65).cgColor
            layer.borderWidth = 1
            // Inset shadow effect via inner shadow
            layer.shadowColor = accent.withAlphaComponent(0.25).cgColor
            layer.shadowOffset = .zero
            layer.shadowRadius = 4
            layer.shadowOpacity = 1.0
        } else if isHovered {
            layer.borderColor = accent.withAlphaComponent(0.45).cgColor
            layer.borderWidth = 1
            layer.shadowOpacity = 0
        } else {
            layer.borderColor = SemanticColors.line.withAlphaComponent(0.58).cgColor
            layer.borderWidth = 1
            layer.shadowOpacity = 0
        }
    }
}
