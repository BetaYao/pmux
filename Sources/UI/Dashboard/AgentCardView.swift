import AppKit

protocol AgentCardDelegate: AnyObject {
    func agentCardClicked(agentId: String)
}

final class AgentCardView: NSView {
    weak var delegate: AgentCardDelegate?
    private(set) var agentId: String = ""
    var isSelected: Bool = false { didSet { updateBorder() } }

    /// Container where the Ghostty terminal surface will be embedded.
    let terminalContainer = NSView()

    /// Fixed-height bottom bar showing status dot, branch name, and status text.
    let bottomBar = NSView()

    private let separatorLine = NSView()
    private let statusDot = NSView()
    private let branchLabel = NSTextField(labelWithString: "")
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

        branchLabel.stringValue = thread
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

        // Status dot (6px circle)
        statusDot.wantsLayer = true
        statusDot.layer?.cornerRadius = 3
        statusDot.translatesAutoresizingMaskIntoConstraints = false
        bottomBar.addSubview(statusDot)

        // Branch label
        branchLabel.font = NSFont.systemFont(ofSize: 9, weight: .medium)
        branchLabel.textColor = .white
        branchLabel.lineBreakMode = .byTruncatingTail
        branchLabel.maximumNumberOfLines = 1
        branchLabel.translatesAutoresizingMaskIntoConstraints = false
        bottomBar.addSubview(branchLabel)

        // Status text label (right-aligned, dim)
        statusLabel.font = NSFont.systemFont(ofSize: 8, weight: .regular)
        statusLabel.textColor = NSColor(calibratedRed: 0.333, green: 0.333, blue: 0.333, alpha: 1.0) // #555
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

            // Bottom bar — fixed 24px height
            bottomBar.leadingAnchor.constraint(equalTo: leadingAnchor),
            bottomBar.trailingAnchor.constraint(equalTo: trailingAnchor),
            bottomBar.bottomAnchor.constraint(equalTo: bottomAnchor),
            bottomBar.heightAnchor.constraint(equalToConstant: 24),

            // Status dot inside bottom bar
            statusDot.leadingAnchor.constraint(equalTo: bottomBar.leadingAnchor, constant: 8),
            statusDot.centerYAnchor.constraint(equalTo: bottomBar.centerYAnchor),
            statusDot.widthAnchor.constraint(equalToConstant: 6),
            statusDot.heightAnchor.constraint(equalToConstant: 6),

            // Branch label
            branchLabel.leadingAnchor.constraint(equalTo: statusDot.trailingAnchor, constant: 5),
            branchLabel.centerYAnchor.constraint(equalTo: bottomBar.centerYAnchor),
            branchLabel.trailingAnchor.constraint(lessThanOrEqualTo: statusLabel.leadingAnchor, constant: -6),

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
        layer?.backgroundColor = SemanticColors.tileBg.cgColor
        bottomBar.layer?.backgroundColor = SemanticColors.tileBarBg.cgColor
        separatorLine.layer?.backgroundColor = SemanticColors.line.cgColor
        statusDot.layer?.backgroundColor = AgentDisplayHelpers.statusColor(currentStatus).cgColor
        branchLabel.textColor = SemanticColors.text
        statusLabel.textColor = SemanticColors.muted
        updateBorder()
    }

    private func updateBorder() {
        guard let layer = layer else { return }
        if isHovered || isSelected {
            layer.borderColor = SemanticColors.accent.cgColor
            layer.borderWidth = 1.5
        } else {
            layer.borderColor = SemanticColors.line.cgColor
            layer.borderWidth = 1
        }
    }
}
