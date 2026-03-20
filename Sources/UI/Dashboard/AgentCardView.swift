import AppKit

protocol AgentCardDelegate: AnyObject {
    func agentCardClicked(agentId: String)
}

final class AgentCardView: NSView {
    weak var delegate: AgentCardDelegate?
    private(set) var agentId: String = ""
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
        setAccessibilityIdentifier("dashboard.card.\(id)")

        titleLabel.stringValue = "\(project) - \(thread)"
        messageLabel.stringValue = lastMessage
        statusDot.layer?.backgroundColor = AgentDisplayHelpers.statusColor(status).cgColor

        let compactTotal = AgentDisplayHelpers.compactDuration(totalDuration)
        let compactRound = AgentDisplayHelpers.compactDuration(roundDuration)
        timeLabel.stringValue = "\u{03A3} \(compactTotal) \u{00B7} \u{27F3} \(compactRound)"

        updateAppearance()
    }

    private func setup() {
        wantsLayer = true
        layer?.cornerRadius = 10
        layer?.borderWidth = 1

        // Status dot
        statusDot.wantsLayer = true
        statusDot.layer?.cornerRadius = 4
        statusDot.translatesAutoresizingMaskIntoConstraints = false
        addSubview(statusDot)

        // Title
        titleLabel.font = NSFont.systemFont(ofSize: 12, weight: .bold)
        titleLabel.textColor = SemanticColors.text
        titleLabel.lineBreakMode = .byTruncatingTail
        titleLabel.maximumNumberOfLines = 1
        titleLabel.translatesAutoresizingMaskIntoConstraints = false
        addSubview(titleLabel)

        // Message
        messageLabel.font = NSFont.systemFont(ofSize: 12)
        messageLabel.textColor = SemanticColors.muted
        messageLabel.lineBreakMode = .byTruncatingTail
        messageLabel.maximumNumberOfLines = 3
        messageLabel.cell?.wraps = true
        messageLabel.cell?.truncatesLastVisibleLine = true
        messageLabel.translatesAutoresizingMaskIntoConstraints = false
        messageLabel.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        addSubview(messageLabel)

        // Time
        timeLabel.font = NSFont.systemFont(ofSize: 12)
        timeLabel.textColor = SemanticColors.muted
        timeLabel.lineBreakMode = .byTruncatingTail
        timeLabel.maximumNumberOfLines = 1
        timeLabel.translatesAutoresizingMaskIntoConstraints = false
        addSubview(timeLabel)

        // Approximate 3-line min height for message: 12pt font * 1.2 leading * 3 lines ~ 43
        let messageMinHeight = messageLabel.heightAnchor.constraint(greaterThanOrEqualToConstant: 43)
        messageMinHeight.priority = .defaultHigh

        NSLayoutConstraint.activate([
            // Status dot in row 1
            statusDot.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 10),
            statusDot.topAnchor.constraint(equalTo: topAnchor, constant: 11),
            statusDot.widthAnchor.constraint(equalToConstant: 8),
            statusDot.heightAnchor.constraint(equalToConstant: 8),

            // Title in row 1
            titleLabel.leadingAnchor.constraint(equalTo: statusDot.trailingAnchor, constant: 6),
            titleLabel.centerYAnchor.constraint(equalTo: statusDot.centerYAnchor),
            titleLabel.trailingAnchor.constraint(lessThanOrEqualTo: trailingAnchor, constant: -10),

            // Message in row 2
            messageLabel.topAnchor.constraint(equalTo: statusDot.bottomAnchor, constant: 8),
            messageLabel.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 10),
            messageLabel.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -10),
            messageMinHeight,

            // Time in row 3
            timeLabel.topAnchor.constraint(equalTo: messageLabel.bottomAnchor, constant: 8),
            timeLabel.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 10),
            timeLabel.trailingAnchor.constraint(lessThanOrEqualTo: trailingAnchor, constant: -10),
            timeLabel.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -11),
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

        if isSelected {
            layer.backgroundColor = accent.withAlphaComponent(0.12)
                .blended(withFraction: 0.88, of: SemanticColors.panel2)?.cgColor
                ?? SemanticColors.panel2.cgColor
            layer.borderColor = accent.withAlphaComponent(0.55)
                .blended(withFraction: 0.45, of: SemanticColors.line)?.cgColor
                ?? SemanticColors.line.cgColor
        } else if isHovered {
            layer.backgroundColor = accent.withAlphaComponent(0.06)
                .blended(withFraction: 0.94, of: SemanticColors.panel2)?.cgColor
                ?? SemanticColors.panel2.cgColor
            layer.borderColor = accent.withAlphaComponent(0.35).cgColor
        } else {
            layer.backgroundColor = SemanticColors.panel2.cgColor
            layer.borderColor = SemanticColors.line.withAlphaComponent(0.78).cgColor
        }

        layer.borderWidth = 1
    }
}
