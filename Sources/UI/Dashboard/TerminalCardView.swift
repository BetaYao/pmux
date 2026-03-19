import AppKit

protocol TerminalCardDelegate: AnyObject {
    func terminalCardClicked(_ card: TerminalCardView)
    func terminalCardDoubleClicked(_ card: TerminalCardView)
}

/// A dashboard card showing a mini terminal preview + status + branch name.
/// The terminal NSView is embedded directly (libghostty renders at whatever size).
class TerminalCardView: NSView {
    weak var delegate: TerminalCardDelegate?

    let worktreeInfo: WorktreeInfo
    let surface: TerminalSurface

    private let branchLabel = NSTextField(labelWithString: "")
    private let statusBadge = StatusBadge()
    private let statusLabel = NSTextField(labelWithString: "")
    private let overlayBar = NSView()
    private let terminalContainer = NSView()

    var status: AgentStatus = .unknown {
        didSet {
            statusBadge.status = status
            statusLabel.stringValue = status.rawValue
            statusLabel.textColor = status.color
        }
    }

    init(worktreeInfo: WorktreeInfo, surface: TerminalSurface) {
        self.worktreeInfo = worktreeInfo
        self.surface = surface
        super.init(frame: .zero)
        setup()
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) not supported")
    }

    private func setup() {
        wantsLayer = true
        layer?.backgroundColor = Theme.surface.cgColor
        layer?.cornerRadius = Theme.cardCornerRadius
        layer?.borderWidth = 1
        layer?.borderColor = Theme.border.cgColor

        // Terminal container (fills most of the card)
        terminalContainer.wantsLayer = true
        terminalContainer.layer?.cornerRadius = Theme.cardCornerRadius
        terminalContainer.layer?.maskedCorners = [.layerMinXMaxYCorner, .layerMaxXMaxYCorner]
        terminalContainer.layer?.masksToBounds = true
        terminalContainer.translatesAutoresizingMaskIntoConstraints = false
        addSubview(terminalContainer)

        // Bottom overlay bar with status + branch
        overlayBar.wantsLayer = true
        overlayBar.layer?.backgroundColor = NSColor(white: 0, alpha: 0.7).cgColor
        overlayBar.translatesAutoresizingMaskIntoConstraints = false
        addSubview(overlayBar)

        // Status badge
        statusBadge.translatesAutoresizingMaskIntoConstraints = false
        overlayBar.addSubview(statusBadge)

        // Status label
        statusLabel.font = NSFont.monospacedSystemFont(ofSize: 10, weight: .medium)
        statusLabel.textColor = Theme.textSecondary
        statusLabel.translatesAutoresizingMaskIntoConstraints = false
        overlayBar.addSubview(statusLabel)

        // Branch label
        branchLabel.stringValue = worktreeInfo.displayName
        branchLabel.font = NSFont.monospacedSystemFont(ofSize: 10, weight: .regular)
        branchLabel.textColor = Theme.textPrimary
        branchLabel.lineBreakMode = .byTruncatingMiddle
        branchLabel.translatesAutoresizingMaskIntoConstraints = false
        overlayBar.addSubview(branchLabel)

        NSLayoutConstraint.activate([
            // Terminal fills the card
            terminalContainer.topAnchor.constraint(equalTo: topAnchor),
            terminalContainer.leadingAnchor.constraint(equalTo: leadingAnchor),
            terminalContainer.trailingAnchor.constraint(equalTo: trailingAnchor),
            terminalContainer.bottomAnchor.constraint(equalTo: bottomAnchor),

            // Overlay bar at bottom
            overlayBar.leadingAnchor.constraint(equalTo: leadingAnchor),
            overlayBar.trailingAnchor.constraint(equalTo: trailingAnchor),
            overlayBar.bottomAnchor.constraint(equalTo: bottomAnchor),
            overlayBar.heightAnchor.constraint(equalToConstant: 28),

            // Status badge
            statusBadge.leadingAnchor.constraint(equalTo: overlayBar.leadingAnchor, constant: 8),
            statusBadge.centerYAnchor.constraint(equalTo: overlayBar.centerYAnchor),
            statusBadge.widthAnchor.constraint(equalToConstant: 10),
            statusBadge.heightAnchor.constraint(equalToConstant: 10),

            // Status text
            statusLabel.leadingAnchor.constraint(equalTo: statusBadge.trailingAnchor, constant: 4),
            statusLabel.centerYAnchor.constraint(equalTo: overlayBar.centerYAnchor),

            // Branch name (right-aligned)
            branchLabel.trailingAnchor.constraint(equalTo: overlayBar.trailingAnchor, constant: -8),
            branchLabel.centerYAnchor.constraint(equalTo: overlayBar.centerYAnchor),
            branchLabel.leadingAnchor.constraint(greaterThanOrEqualTo: statusLabel.trailingAnchor, constant: 8),
        ])

        // Click handlers
        let click = NSClickGestureRecognizer(target: self, action: #selector(cardClicked))
        click.numberOfClicksRequired = 1
        addGestureRecognizer(click)

        let doubleClick = NSClickGestureRecognizer(target: self, action: #selector(cardDoubleClicked))
        doubleClick.numberOfClicksRequired = 2
        addGestureRecognizer(doubleClick)

        // Single click should wait to confirm it's not a double click
        click.shouldRequireFailure(of: doubleClick)

        // Hover tracking
        let trackingArea = NSTrackingArea(
            rect: .zero,
            options: [.mouseEnteredAndExited, .activeInActiveApp, .inVisibleRect],
            owner: self,
            userInfo: nil
        )
        addTrackingArea(trackingArea)
    }

    /// Embed the terminal surface view into this card
    func embedTerminal() {
        if surface.surface == nil {
            // First time — create the surface in this container
            _ = surface.create(in: terminalContainer, workingDirectory: worktreeInfo.path, sessionName: surface.sessionName)
        } else {
            // Already created — reparent to this container
            surface.reparent(to: terminalContainer)
        }
    }

    @objc private func cardClicked() {
        delegate?.terminalCardClicked(self)
    }

    @objc private func cardDoubleClicked() {
        delegate?.terminalCardDoubleClicked(self)
    }

    override func mouseEntered(with event: NSEvent) {
        layer?.borderColor = Theme.accent.cgColor
        layer?.borderWidth = 2
    }

    override func mouseExited(with event: NSEvent) {
        layer?.borderColor = Theme.border.cgColor
        layer?.borderWidth = 1
    }
}
