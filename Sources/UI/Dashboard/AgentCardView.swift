import AppKit

protocol AgentCardDelegate: AnyObject {
    func agentCardClicked(agentId: String)
}

final class AgentCardView: NSView {
    enum Typography {
        static let primaryPointSize: CGFloat = 13
        static let bodyPointSize: CGFloat = 12
        static let secondaryPointSize: CGFloat = 11
    }

    weak var delegate: AgentCardDelegate?
    private(set) var agentId: String = ""
    var isSelected: Bool = false { didSet { updateBorder() } }

    /// Container where the Ghostty terminal surface will be embedded.
    let terminalContainer = NSView()

    /// Fixed-height bottom bar showing status dot, branch name, and status text.
    let bottomBar = NSView()

    private let separatorLine = NSView()
    private let statusDot = NSView()
    private let projectLabel = NSTextField(labelWithString: "")
    private let statusLabel = NSTextField(labelWithString: "")
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
        setAccessibilityIdentifier("dashboard.card.\(id)")

        projectLabel.stringValue = project
        statusLabel.stringValue = status.capitalized
        statusDot.layer?.backgroundColor = AgentDisplayHelpers.statusColor(status).cgColor

        updateBorder()
    }

    private func setup() {
        wantsLayer = true
        layer?.cornerRadius = 4
        layer?.masksToBounds = true
        // Colors set in applyColors() via viewDidMoveToWindow/viewDidChangeEffectiveAppearance

        // Terminal container — fills top area
        terminalContainer.wantsLayer = true
        terminalContainer.translatesAutoresizingMaskIntoConstraints = false
        addSubview(terminalContainer)

        // Separator line
        separatorLine.wantsLayer = true
        separatorLine.translatesAutoresizingMaskIntoConstraints = false
        addSubview(separatorLine)

        // Bottom bar
        bottomBar.wantsLayer = true
        bottomBar.translatesAutoresizingMaskIntoConstraints = false
        addSubview(bottomBar)

        // Status dot
        statusDot.wantsLayer = true
        statusDot.layer?.cornerRadius = 3.5
        statusDot.translatesAutoresizingMaskIntoConstraints = false
        bottomBar.addSubview(statusDot)

        // Project label
        projectLabel.font = NSFont.systemFont(ofSize: Typography.bodyPointSize, weight: .medium)
        projectLabel.textColor = SemanticColors.text
        projectLabel.lineBreakMode = .byTruncatingTail
        projectLabel.maximumNumberOfLines = 1
        projectLabel.translatesAutoresizingMaskIntoConstraints = false
        bottomBar.addSubview(projectLabel)

        // Status text label (right-aligned, dim)
        statusLabel.font = NSFont.systemFont(ofSize: Typography.secondaryPointSize, weight: .regular)
        statusLabel.textColor = SemanticColors.muted
        statusLabel.lineBreakMode = .byTruncatingTail
        statusLabel.maximumNumberOfLines = 1
        statusLabel.alignment = .right
        statusLabel.translatesAutoresizingMaskIntoConstraints = false
        statusLabel.setContentCompressionResistancePriority(.required, for: .horizontal)
        bottomBar.addSubview(statusLabel)

        NSLayoutConstraint.activate([
            // Terminal container fills top
            terminalContainer.topAnchor.constraint(equalTo: topAnchor),
            terminalContainer.leadingAnchor.constraint(equalTo: leadingAnchor),
            terminalContainer.trailingAnchor.constraint(equalTo: trailingAnchor),
            terminalContainer.bottomAnchor.constraint(equalTo: separatorLine.topAnchor),

            // Separator line
            separatorLine.leadingAnchor.constraint(equalTo: leadingAnchor),
            separatorLine.trailingAnchor.constraint(equalTo: trailingAnchor),
            separatorLine.heightAnchor.constraint(equalToConstant: 1),
            separatorLine.bottomAnchor.constraint(equalTo: bottomBar.topAnchor),

            // Bottom bar
            bottomBar.leadingAnchor.constraint(equalTo: leadingAnchor),
            bottomBar.trailingAnchor.constraint(equalTo: trailingAnchor),
            bottomBar.bottomAnchor.constraint(equalTo: bottomAnchor),
            bottomBar.heightAnchor.constraint(equalToConstant: 30),

            // Status dot inside bottom bar
            statusDot.leadingAnchor.constraint(equalTo: bottomBar.leadingAnchor, constant: 8),
            statusDot.centerYAnchor.constraint(equalTo: bottomBar.centerYAnchor),
            statusDot.widthAnchor.constraint(equalToConstant: 7),
            statusDot.heightAnchor.constraint(equalToConstant: 7),

            // Project label
            projectLabel.leadingAnchor.constraint(equalTo: statusDot.trailingAnchor, constant: 5),
            projectLabel.centerYAnchor.constraint(equalTo: bottomBar.centerYAnchor),
            projectLabel.trailingAnchor.constraint(lessThanOrEqualTo: statusLabel.leadingAnchor, constant: -6),

            // Status text label (right-aligned)
            statusLabel.trailingAnchor.constraint(equalTo: bottomBar.trailingAnchor, constant: -8),
            statusLabel.centerYAnchor.constraint(equalTo: bottomBar.centerYAnchor),
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

        updateBorder()
    }

    @objc private func handleClick() {
        delegate?.agentCardClicked(agentId: agentId)
    }

    override func mouseEntered(with event: NSEvent) {
        isHovered = true
        updateBorder()
    }

    override func mouseExited(with event: NSEvent) {
        isHovered = false
        updateBorder()
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
        layer?.backgroundColor = resolvedCGColor(SemanticColors.tileBg)
        bottomBar.layer?.backgroundColor = resolvedCGColor(SemanticColors.tileBarBg)
        separatorLine.layer?.backgroundColor = resolvedCGColor(SemanticColors.line)
        statusDot.layer?.backgroundColor = resolvedCGColor(AgentDisplayHelpers.statusColor(currentStatus))
        projectLabel.textColor = SemanticColors.text
        statusLabel.textColor = SemanticColors.muted
        updateBorder()
    }

    private func updateBorder() {
        guard let layer = layer else { return }
        if isHovered || isSelected {
            layer.borderColor = resolvedCGColor(SemanticColors.accent)
            layer.borderWidth = 1.5
        } else {
            layer.borderColor = resolvedCGColor(SemanticColors.line)
            layer.borderWidth = 1
        }
    }
}
