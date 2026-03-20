import AppKit

protocol FocusPanelDelegate: AnyObject {
    func focusPanelDidRequestEnterProject(_ projectName: String)
}

final class FocusPanelView: NSView {
    weak var delegate: FocusPanelDelegate?
    let terminalContainer: NSView = NSView()

    private let headerView = NSView()
    private let statusDot = NSView()
    private let nameLabel = NSTextField(labelWithString: "")
    private let metaLabel = NSTextField(labelWithString: "")
    private let durationLabel = NSTextField(labelWithString: "")
    private let enterButton = NSButton()
    private let arrowButton = NSView()
    private let arrowImageView = NSImageView()
    private var isArrowHovered = false
    private var projectName: String = ""

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        setup()
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) not supported")
    }

    func configure(name: String, project: String, thread: String, status: String, total: String, round: String) {
        projectName = project

        nameLabel.stringValue = name
        metaLabel.stringValue = "\(project) \u{00B7} \(thread)"

        let compactTotal = AgentDisplayHelpers.compactDuration(total)
        let compactRound = AgentDisplayHelpers.compactDuration(round)
        durationLabel.stringValue = "Total \(compactTotal) / Round \(compactRound)"

        statusDot.layer?.backgroundColor = AgentDisplayHelpers.statusColor(status).cgColor
    }

    private func setup() {
        wantsLayer = true
        layer?.cornerRadius = 4
        layer?.borderWidth = 1
        layer?.borderColor = SemanticColors.line.cgColor
        layer?.backgroundColor = SemanticColors.tileBg.cgColor
        setAccessibilityIdentifier("dashboard.focusPanel")

        setupHeader()
        setupTerminalContainer()
    }

    private func setupHeader() {
        headerView.wantsLayer = true
        headerView.translatesAutoresizingMaskIntoConstraints = false
        addSubview(headerView)

        // Bottom border for header
        let headerBorder = NSView()
        headerBorder.wantsLayer = true
        headerBorder.layer?.backgroundColor = SemanticColors.lineAlpha55.cgColor
        headerBorder.translatesAutoresizingMaskIntoConstraints = false
        headerView.addSubview(headerBorder)

        // Status dot
        statusDot.wantsLayer = true
        statusDot.layer?.cornerRadius = 4
        statusDot.translatesAutoresizingMaskIntoConstraints = false
        headerView.addSubview(statusDot)

        // Name
        nameLabel.font = NSFont.systemFont(ofSize: 12, weight: .bold)
        nameLabel.textColor = SemanticColors.text
        nameLabel.lineBreakMode = .byTruncatingTail
        nameLabel.translatesAutoresizingMaskIntoConstraints = false
        headerView.addSubview(nameLabel)

        // Meta (project + thread)
        metaLabel.font = NSFont.systemFont(ofSize: 12)
        metaLabel.textColor = SemanticColors.muted
        metaLabel.lineBreakMode = .byTruncatingTail
        metaLabel.translatesAutoresizingMaskIntoConstraints = false
        headerView.addSubview(metaLabel)

        // Duration
        durationLabel.font = NSFont.systemFont(ofSize: 12)
        durationLabel.textColor = SemanticColors.muted
        durationLabel.lineBreakMode = .byTruncatingTail
        durationLabel.translatesAutoresizingMaskIntoConstraints = false
        headerView.addSubview(durationLabel)

        // Enter project button
        enterButton.bezelStyle = .toolbar
        enterButton.isBordered = false
        enterButton.image = NSImage(systemSymbolName: "arrow.up.right", accessibilityDescription: "Enter project")
        enterButton.symbolConfiguration = NSImage.SymbolConfiguration(pointSize: 14, weight: .regular)
        enterButton.wantsLayer = true
        enterButton.layer?.cornerRadius = 8
        enterButton.target = self
        enterButton.action = #selector(enterProjectClicked)
        enterButton.setAccessibilityIdentifier("dashboard.focusPanel.enterProject")
        enterButton.translatesAutoresizingMaskIntoConstraints = false
        headerView.addSubview(enterButton)

        // Enter button hover tracking
        let trackingArea = NSTrackingArea(
            rect: .zero,
            options: [.mouseEnteredAndExited, .activeInActiveApp, .inVisibleRect],
            owner: self,
            userInfo: ["target": "enterButton"]
        )
        enterButton.addTrackingArea(trackingArea)

        // Arrow button (chevron.right) for project detail navigation
        arrowButton.wantsLayer = true
        arrowButton.layer?.cornerRadius = 5
        arrowButton.layer?.backgroundColor = NSColor(white: 1, alpha: 0.04).cgColor
        arrowButton.translatesAutoresizingMaskIntoConstraints = false
        headerView.addSubview(arrowButton)

        if let chevronImage = NSImage(systemSymbolName: "chevron.right", accessibilityDescription: "Enter project") {
            let config = NSImage.SymbolConfiguration(pointSize: 12, weight: .medium)
            arrowImageView.image = chevronImage.withSymbolConfiguration(config)
            arrowImageView.contentTintColor = NSColor(hex: 0x999999)
        }
        arrowImageView.translatesAutoresizingMaskIntoConstraints = false
        arrowButton.addSubview(arrowImageView)

        let arrowClick = NSClickGestureRecognizer(target: self, action: #selector(enterProjectClicked))
        arrowButton.addGestureRecognizer(arrowClick)

        let arrowTracking = NSTrackingArea(
            rect: .zero,
            options: [.mouseEnteredAndExited, .activeInActiveApp, .inVisibleRect],
            owner: self,
            userInfo: ["target": "arrowButton"]
        )
        arrowButton.addTrackingArea(arrowTracking)

        NSLayoutConstraint.activate([
            headerView.topAnchor.constraint(equalTo: topAnchor),
            headerView.leadingAnchor.constraint(equalTo: leadingAnchor),
            headerView.trailingAnchor.constraint(equalTo: trailingAnchor),
            headerView.heightAnchor.constraint(equalToConstant: 42),

            headerBorder.leadingAnchor.constraint(equalTo: headerView.leadingAnchor),
            headerBorder.trailingAnchor.constraint(equalTo: headerView.trailingAnchor),
            headerBorder.bottomAnchor.constraint(equalTo: headerView.bottomAnchor),
            headerBorder.heightAnchor.constraint(equalToConstant: 1),

            statusDot.leadingAnchor.constraint(equalTo: headerView.leadingAnchor, constant: 10),
            statusDot.centerYAnchor.constraint(equalTo: headerView.centerYAnchor),
            statusDot.widthAnchor.constraint(equalToConstant: 8),
            statusDot.heightAnchor.constraint(equalToConstant: 8),

            nameLabel.leadingAnchor.constraint(equalTo: statusDot.trailingAnchor, constant: 6),
            nameLabel.centerYAnchor.constraint(equalTo: headerView.centerYAnchor),

            metaLabel.leadingAnchor.constraint(equalTo: nameLabel.trailingAnchor, constant: 8),
            metaLabel.centerYAnchor.constraint(equalTo: headerView.centerYAnchor),

            durationLabel.leadingAnchor.constraint(equalTo: metaLabel.trailingAnchor, constant: 8),
            durationLabel.centerYAnchor.constraint(equalTo: headerView.centerYAnchor),

            arrowButton.trailingAnchor.constraint(equalTo: headerView.trailingAnchor, constant: -10),
            arrowButton.centerYAnchor.constraint(equalTo: headerView.centerYAnchor),
            arrowButton.widthAnchor.constraint(equalToConstant: 22),
            arrowButton.heightAnchor.constraint(equalToConstant: 22),

            arrowImageView.centerXAnchor.constraint(equalTo: arrowButton.centerXAnchor),
            arrowImageView.centerYAnchor.constraint(equalTo: arrowButton.centerYAnchor),

            enterButton.trailingAnchor.constraint(equalTo: arrowButton.leadingAnchor, constant: -6),
            enterButton.centerYAnchor.constraint(equalTo: headerView.centerYAnchor),
            enterButton.widthAnchor.constraint(equalToConstant: 28),
            enterButton.heightAnchor.constraint(equalToConstant: 28),

            durationLabel.trailingAnchor.constraint(lessThanOrEqualTo: enterButton.leadingAnchor, constant: -8),
        ])

        // Compression resistance so labels don't fight
        nameLabel.setContentCompressionResistancePriority(.defaultHigh, for: .horizontal)
        metaLabel.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        durationLabel.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        nameLabel.setContentHuggingPriority(.defaultHigh, for: .horizontal)
    }

    private func setupTerminalContainer() {
        terminalContainer.wantsLayer = true
        terminalContainer.translatesAutoresizingMaskIntoConstraints = false
        terminalContainer.setAccessibilityIdentifier("dashboard.focusPanel.terminal")
        addSubview(terminalContainer)

        NSLayoutConstraint.activate([
            terminalContainer.topAnchor.constraint(equalTo: headerView.bottomAnchor),
            terminalContainer.leadingAnchor.constraint(equalTo: leadingAnchor),
            terminalContainer.trailingAnchor.constraint(equalTo: trailingAnchor),
            terminalContainer.bottomAnchor.constraint(equalTo: bottomAnchor),
        ])
    }

    @objc private func enterProjectClicked() {
        delegate?.focusPanelDidRequestEnterProject(projectName)
    }

    override func mouseEntered(with event: NSEvent) {
        if let target = event.trackingArea?.userInfo?["target"] as? String {
            if target == "enterButton" {
                enterButton.layer?.backgroundColor = SemanticColors.lineAlpha22.cgColor
            } else if target == "arrowButton" {
                isArrowHovered = true
                arrowButton.layer?.backgroundColor = NSColor(white: 1, alpha: 0.09).cgColor
                arrowImageView.contentTintColor = .white
            }
        }
    }

    override func mouseExited(with event: NSEvent) {
        if let target = event.trackingArea?.userInfo?["target"] as? String {
            if target == "enterButton" {
                enterButton.layer?.backgroundColor = nil
            } else if target == "arrowButton" {
                isArrowHovered = false
                arrowButton.layer?.backgroundColor = NSColor(white: 1, alpha: 0.04).cgColor
                arrowImageView.contentTintColor = NSColor(hex: 0x999999)
            }
        }
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        applyColors()
    }

    private func applyColors() {
        layer?.borderColor = SemanticColors.line.cgColor
        layer?.backgroundColor = SemanticColors.tileBg.cgColor
    }
}
