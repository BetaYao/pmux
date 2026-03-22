import AppKit

protocol FocusPanelDelegate: AnyObject {
    func focusPanelDidRequestEnterProject(_ projectName: String)
}

final class FocusPanelView: NSView {
    enum HeaderPosition: Equatable {
        case top
        case bottom
    }

    static let defaultHeaderPosition: HeaderPosition = .bottom
    static let defaultCornerRadius: CGFloat = 10

    enum Typography {
        static let primaryPointSize: CGFloat = 13
        static let bodyPointSize: CGFloat = 12
        static let secondaryPointSize: CGFloat = 11
    }

    weak var delegate: FocusPanelDelegate?
    let terminalContainer: NSView = NSView()

    private let headerView = NSView()
    private let statusDot = NSView()
    private let nameLabel = NSTextField(labelWithString: "")
    private let metaLabel = NSTextField(labelWithString: "")
    private let durationLabel = NSTextField(labelWithString: "")
    private let enterButton = NSButton()
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
        layer?.cornerRadius = Self.defaultCornerRadius
        layer?.cornerCurve = .continuous
        layer?.masksToBounds = true
        layer?.borderWidth = 1
        // Colors set in applyColors() via updateLayer
        setAccessibilityIdentifier("dashboard.focusPanel")

        setupHeader()
        setupTerminalContainer()
    }

    func setCornerMask(_ maskedCorners: CACornerMask, radius: CGFloat = defaultCornerRadius) {
        layer?.cornerRadius = radius
        layer?.maskedCorners = maskedCorners
    }

    private func setupHeader() {
        headerView.wantsLayer = true
        headerView.translatesAutoresizingMaskIntoConstraints = false
        addSubview(headerView)

        // Top border for header (separates from terminal above)
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
        nameLabel.font = NSFont.systemFont(ofSize: Typography.primaryPointSize, weight: .semibold)
        nameLabel.textColor = SemanticColors.text
        nameLabel.lineBreakMode = .byTruncatingTail
        nameLabel.translatesAutoresizingMaskIntoConstraints = false
        headerView.addSubview(nameLabel)

        // Meta (project + thread)
        metaLabel.font = NSFont.systemFont(ofSize: Typography.bodyPointSize)
        metaLabel.textColor = SemanticColors.muted
        metaLabel.lineBreakMode = .byTruncatingTail
        metaLabel.translatesAutoresizingMaskIntoConstraints = false
        headerView.addSubview(metaLabel)

        // Duration
        durationLabel.font = NSFont.systemFont(ofSize: Typography.secondaryPointSize)
        durationLabel.textColor = SemanticColors.muted
        durationLabel.lineBreakMode = .byTruncatingTail
        durationLabel.translatesAutoresizingMaskIntoConstraints = false
        headerView.addSubview(durationLabel)

        // Enter project button
        enterButton.bezelStyle = .texturedRounded
        enterButton.isBordered = true
        enterButton.image = NSImage(systemSymbolName: "arrow.up.right", accessibilityDescription: "Enter project")
        enterButton.symbolConfiguration = NSImage.SymbolConfiguration(pointSize: 12, weight: .regular)
        enterButton.target = self
        enterButton.action = #selector(enterProjectClicked)
        enterButton.setAccessibilityIdentifier("dashboard.focusPanel.enterProject")
        enterButton.translatesAutoresizingMaskIntoConstraints = false
        headerView.addSubview(enterButton)

        NSLayoutConstraint.activate([
            headerView.bottomAnchor.constraint(equalTo: bottomAnchor),
            headerView.leadingAnchor.constraint(equalTo: leadingAnchor),
            headerView.trailingAnchor.constraint(equalTo: trailingAnchor),
            headerView.heightAnchor.constraint(equalToConstant: 42),

            headerBorder.leadingAnchor.constraint(equalTo: headerView.leadingAnchor),
            headerBorder.trailingAnchor.constraint(equalTo: headerView.trailingAnchor),
            headerBorder.topAnchor.constraint(equalTo: headerView.topAnchor),
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

            enterButton.trailingAnchor.constraint(equalTo: headerView.trailingAnchor, constant: -10),
            enterButton.centerYAnchor.constraint(equalTo: headerView.centerYAnchor),
            enterButton.widthAnchor.constraint(equalToConstant: 26),
            enterButton.heightAnchor.constraint(equalToConstant: 24),

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
            terminalContainer.topAnchor.constraint(equalTo: topAnchor),
            terminalContainer.leadingAnchor.constraint(equalTo: leadingAnchor),
            terminalContainer.trailingAnchor.constraint(equalTo: trailingAnchor),
            terminalContainer.bottomAnchor.constraint(equalTo: headerView.topAnchor),
        ])
    }

    @objc private func enterProjectClicked() {
        delegate?.focusPanelDidRequestEnterProject(projectName)
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
        layer?.borderColor = resolvedCGColor(SemanticColors.lineAlpha70)
        layer?.backgroundColor = resolvedCGColor(SemanticColors.tileBg)
    }
}
