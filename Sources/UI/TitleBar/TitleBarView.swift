import AppKit

protocol TitleBarDelegate: AnyObject {
    func titleBarDidSelectDashboard()
    func titleBarDidSelectProject(_ projectName: String)
    func titleBarDidRequestCloseProject(_ projectName: String)
    func titleBarDidRequestAddProject()
    func titleBarDidRequestNewThread()
    func titleBarDidSelectLayout(_ layout: DashboardLayout)
    func titleBarDidToggleNotifications()
    func titleBarDidToggleAI()
    func titleBarDidToggleTheme()
}

final class TitleBarView: NSView, LayoutPopoverDelegate {
    weak var delegate: TitleBarDelegate?

    var currentView: String = "dashboard" {
        didSet { updateViewState() }
    }
    var currentProject: String = ""
    var projects: [String] = []
    var projectStatusProvider: ((String) -> String)?

    // MARK: - Subviews

    private let leftStack = NSStackView()
    private let rightStack = NSStackView()
    private let bottomBorder = NSView()

    // Traffic lights
    private let trafficRed = makeTrafficDot(color: NSColor(hex: 0xff5f57))
    private let trafficYellow = makeTrafficDot(color: NSColor(hex: 0xfebb2e))
    private let trafficGreen = makeTrafficDot(color: NSColor(hex: 0x28c840))

    // Left controls
    private let dashboardTab = NSButton()
    private let tabSeparator = NSView()
    private let addButton = NSButton()
    private var projectTabViews: [ProjectTabView] = []
    private let tabsStack = NSStackView()

    // Right controls
    private let newThreadButton = NSButton()
    private let viewMenuButton = NSButton()
    private let notifButton = NSButton()
    private let notifBadge = NSTextField(labelWithString: "")
    private let aiButton = NSButton()
    private let themeToggle = NSButton()

    // Popover
    private let layoutPopover = LayoutPopoverView()

    // MARK: - Init

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        setup()
    }

    required init?(coder: NSCoder) {
        super.init(coder: coder)
        setup()
    }

    // MARK: - Public API

    func renderTabs() {
        // Remove old project tabs
        for tab in projectTabViews {
            tab.removeFromSuperview()
        }
        projectTabViews.removeAll()

        // Remove separator temporarily
        tabSeparator.removeFromSuperview()
        addButton.removeFromSuperview()

        // Re-add separator after dashboard tab
        if !projects.isEmpty {
            tabsStack.addArrangedSubview(tabSeparator)
        }

        // Add project tabs
        for name in projects {
            let tab = ProjectTabView(name: name)
            let statusString = projectStatusProvider?(name) ?? "idle"
            tab.updateStatus(statusString)
            tab.setSelected(currentView == "project" && currentProject == name)
            tab.onSelect = { [weak self] in
                self?.delegate?.titleBarDidSelectProject(name)
            }
            tab.onClose = { [weak self] in
                self?.delegate?.titleBarDidRequestCloseProject(name)
            }
            tab.identifier = NSUserInterfaceItemIdentifier("titlebar.projectTab.\(name)")
            tabsStack.addArrangedSubview(tab)
            projectTabViews.append(tab)
        }

        // Add "+" button at the end
        tabsStack.addArrangedSubview(addButton)

        // Update dashboard tab state
        updateDashboardTabAppearance()
    }

    func updateNotifBadge(_ count: Int) {
        notifBadge.isHidden = count <= 0
        notifBadge.stringValue = count > 99 ? "99+" : "\(count)"
    }

    func setCurrentLayout(_ layout: DashboardLayout) {
        layoutPopover.setLayout(layout)
    }

    // MARK: - LayoutPopoverDelegate

    func layoutPopover(_ popover: LayoutPopoverView, didSelect layout: DashboardLayout) {
        delegate?.titleBarDidSelectLayout(layout)
    }

    // MARK: - Setup

    private func setup() {
        wantsLayer = true
        translatesAutoresizingMaskIntoConstraints = false

        // Bottom border
        bottomBorder.wantsLayer = true
        bottomBorder.translatesAutoresizingMaskIntoConstraints = false
        addSubview(bottomBorder)

        setupLeftSide()
        setupRightSide()
        setupLayoutPopover()

        NSLayoutConstraint.activate([
            heightAnchor.constraint(equalToConstant: 40),

            bottomBorder.leadingAnchor.constraint(equalTo: leadingAnchor),
            bottomBorder.trailingAnchor.constraint(equalTo: trailingAnchor),
            bottomBorder.bottomAnchor.constraint(equalTo: bottomAnchor),
            bottomBorder.heightAnchor.constraint(equalToConstant: 1),

            leftStack.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 10),
            leftStack.centerYAnchor.constraint(equalTo: centerYAnchor),

            rightStack.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -10),
            rightStack.centerYAnchor.constraint(equalTo: centerYAnchor),

            rightStack.leadingAnchor.constraint(
                greaterThanOrEqualTo: leftStack.trailingAnchor, constant: 12),
        ])

        updateViewState()
    }

    private func setupLeftSide() {
        leftStack.orientation = .horizontal
        leftStack.spacing = 12
        leftStack.alignment = .centerY
        leftStack.translatesAutoresizingMaskIntoConstraints = false
        addSubview(leftStack)

        // Traffic lights container
        let trafficStack = NSStackView(views: [trafficRed, trafficYellow, trafficGreen])
        trafficStack.orientation = .horizontal
        trafficStack.spacing = 6
        leftStack.addArrangedSubview(trafficStack)

        // Tabs stack
        tabsStack.orientation = .horizontal
        tabsStack.spacing = 4
        tabsStack.alignment = .centerY
        tabsStack.translatesAutoresizingMaskIntoConstraints = false
        leftStack.addArrangedSubview(tabsStack)

        // Dashboard tab
        configureDashboardTab()
        tabsStack.addArrangedSubview(dashboardTab)

        // Separator
        tabSeparator.wantsLayer = true
        tabSeparator.translatesAutoresizingMaskIntoConstraints = false
        NSLayoutConstraint.activate([
            tabSeparator.widthAnchor.constraint(equalToConstant: 1),
            tabSeparator.heightAnchor.constraint(equalToConstant: 18),
        ])

        // Add button
        addButton.title = "+"
        addButton.bezelStyle = .recessed
        addButton.isBordered = false
        addButton.font = NSFont.systemFont(ofSize: 16, weight: .medium)
        addButton.contentTintColor = SemanticColors.muted
        addButton.target = self
        addButton.action = #selector(addProjectClicked)
        addButton.translatesAutoresizingMaskIntoConstraints = false
        addButton.setAccessibilityIdentifier("titlebar.addProject")
        NSLayoutConstraint.activate([
            addButton.widthAnchor.constraint(equalToConstant: 30),
            addButton.heightAnchor.constraint(equalToConstant: 28),
        ])
        addButton.wantsLayer = true
        addButton.layer?.cornerRadius = 7
        tabsStack.addArrangedSubview(addButton)
    }

    private func configureDashboardTab() {
        dashboardTab.bezelStyle = .recessed
        dashboardTab.isBordered = false
        dashboardTab.translatesAutoresizingMaskIntoConstraints = false
        dashboardTab.setAccessibilityIdentifier("titlebar.dashboardTab")
        dashboardTab.target = self
        dashboardTab.action = #selector(dashboardTabClicked)
        dashboardTab.wantsLayer = true
        dashboardTab.layer?.cornerRadius = 999

        // Build attributed title with icon + text
        let attachment = NSTextAttachment()
        if let image = NSImage(systemSymbolName: "square.grid.2x2", accessibilityDescription: "Dashboard") {
            let config = NSImage.SymbolConfiguration(pointSize: 11, weight: .semibold)
            attachment.image = image.withSymbolConfiguration(config)
        }
        let attrString = NSMutableAttributedString(attachment: attachment)
        attrString.append(NSAttributedString(string: " Dashboard", attributes: [
            .font: NSFont.systemFont(ofSize: 11, weight: .semibold),
        ]))
        dashboardTab.attributedTitle = attrString

        NSLayoutConstraint.activate([
            dashboardTab.heightAnchor.constraint(equalToConstant: 28),
        ])
    }

    private func setupRightSide() {
        rightStack.orientation = .horizontal
        rightStack.spacing = 4
        rightStack.alignment = .centerY
        rightStack.translatesAutoresizingMaskIntoConstraints = false
        addSubview(rightStack)

        // New Thread button
        newThreadButton.title = "New Thread"
        newThreadButton.bezelStyle = .rounded
        newThreadButton.isBordered = true
        newThreadButton.font = NSFont.systemFont(ofSize: 11)
        newThreadButton.target = self
        newThreadButton.action = #selector(newThreadClicked)
        newThreadButton.setAccessibilityIdentifier("titlebar.newThread")
        newThreadButton.translatesAutoresizingMaskIntoConstraints = false
        rightStack.addArrangedSubview(newThreadButton)

        // View menu button
        configureIconButton(viewMenuButton, symbol: "square.grid.2x2",
                            identifier: "titlebar.viewMenu", action: #selector(viewMenuClicked))
        rightStack.addArrangedSubview(viewMenuButton)

        // Notification button (with badge)
        configureIconButton(notifButton, symbol: "bell",
                            identifier: "titlebar.notifButton", action: #selector(notifClicked))
        // Badge
        notifBadge.font = NSFont.systemFont(ofSize: 9, weight: .bold)
        notifBadge.textColor = .white
        notifBadge.alignment = .center
        notifBadge.backgroundColor = SemanticColors.danger
        notifBadge.drawsBackground = true
        notifBadge.wantsLayer = true
        notifBadge.layer?.cornerRadius = 7
        notifBadge.layer?.masksToBounds = true
        notifBadge.isHidden = true
        notifBadge.translatesAutoresizingMaskIntoConstraints = false
        notifBadge.identifier = NSUserInterfaceItemIdentifier("titlebar.notifBadge")
        notifButton.addSubview(notifBadge)
        NSLayoutConstraint.activate([
            notifBadge.heightAnchor.constraint(equalToConstant: 14),
            notifBadge.widthAnchor.constraint(greaterThanOrEqualToConstant: 14),
            notifBadge.topAnchor.constraint(equalTo: notifButton.topAnchor, constant: 2),
            notifBadge.trailingAnchor.constraint(equalTo: notifButton.trailingAnchor, constant: -2),
        ])
        rightStack.addArrangedSubview(notifButton)

        // AI button
        configureIconButton(aiButton, symbol: "sparkles",
                            identifier: "titlebar.aiButton", action: #selector(aiClicked))
        rightStack.addArrangedSubview(aiButton)

        // Theme toggle
        themeToggle.title = "◐"
        themeToggle.bezelStyle = .recessed
        themeToggle.isBordered = false
        themeToggle.font = NSFont.systemFont(ofSize: 16)
        themeToggle.contentTintColor = SemanticColors.muted
        themeToggle.target = self
        themeToggle.action = #selector(themeClicked)
        themeToggle.translatesAutoresizingMaskIntoConstraints = false
        themeToggle.setAccessibilityIdentifier("titlebar.themeToggle")
        themeToggle.wantsLayer = true
        themeToggle.layer?.cornerRadius = 10
        NSLayoutConstraint.activate([
            themeToggle.widthAnchor.constraint(equalToConstant: 32),
            themeToggle.heightAnchor.constraint(equalToConstant: 32),
        ])
        setupHoverTracking(for: themeToggle)
        rightStack.addArrangedSubview(themeToggle)
    }

    private func setupLayoutPopover() {
        layoutPopover.delegate = self
        addSubview(layoutPopover)

        NSLayoutConstraint.activate([
            layoutPopover.topAnchor.constraint(equalTo: bottomAnchor, constant: 4),
            layoutPopover.trailingAnchor.constraint(equalTo: viewMenuButton.trailingAnchor),
        ])
    }

    // MARK: - Icon Button Helper

    private func configureIconButton(_ button: NSButton, symbol: String,
                                      identifier: String, action: Selector) {
        let config = NSImage.SymbolConfiguration(pointSize: 14, weight: .medium)
        if let image = NSImage(systemSymbolName: symbol, accessibilityDescription: nil) {
            button.image = image.withSymbolConfiguration(config)
        }
        button.bezelStyle = .recessed
        button.isBordered = false
        button.imagePosition = .imageOnly
        button.contentTintColor = SemanticColors.muted
        button.target = self
        button.action = action
        button.translatesAutoresizingMaskIntoConstraints = false
        button.setAccessibilityIdentifier(identifier)
        button.wantsLayer = true
        button.layer?.cornerRadius = 10
        NSLayoutConstraint.activate([
            button.widthAnchor.constraint(equalToConstant: 32),
            button.heightAnchor.constraint(equalToConstant: 32),
        ])
        setupHoverTracking(for: button)
    }

    // MARK: - Hover Tracking

    private func setupHoverTracking(for button: NSButton) {
        let hover = HoverTrackingView()
        hover.translatesAutoresizingMaskIntoConstraints = false
        button.addSubview(hover)
        NSLayoutConstraint.activate([
            hover.topAnchor.constraint(equalTo: button.topAnchor),
            hover.leadingAnchor.constraint(equalTo: button.leadingAnchor),
            hover.trailingAnchor.constraint(equalTo: button.trailingAnchor),
            hover.bottomAnchor.constraint(equalTo: button.bottomAnchor),
        ])
        hover.onHoverChanged = { [weak button] hovered in
            button?.layer?.backgroundColor = hovered
                ? SemanticColors.line.withAlphaComponent(0.22).cgColor
                : NSColor.clear.cgColor
        }
    }

    // MARK: - Actions

    @objc private func dashboardTabClicked() {
        currentView = "dashboard"
        currentProject = ""
        delegate?.titleBarDidSelectDashboard()
        renderTabs()
    }

    @objc private func addProjectClicked() {
        delegate?.titleBarDidRequestAddProject()
    }

    @objc private func newThreadClicked() {
        delegate?.titleBarDidRequestNewThread()
    }

    @objc private func viewMenuClicked() {
        layoutPopover.toggle()
    }

    @objc private func notifClicked() {
        delegate?.titleBarDidToggleNotifications()
    }

    @objc private func aiClicked() {
        delegate?.titleBarDidToggleAI()
    }

    @objc private func themeClicked() {
        delegate?.titleBarDidToggleTheme()
    }

    // MARK: - State

    private func updateViewState() {
        let isDashboard = currentView == "dashboard"
        newThreadButton.isHidden = isDashboard
        viewMenuButton.alphaValue = isDashboard ? 1.0 : 0.3
        viewMenuButton.isEnabled = isDashboard
        updateDashboardTabAppearance()
    }

    private func updateDashboardTabAppearance() {
        let isActive = currentView == "dashboard"
        if isActive {
            dashboardTab.layer?.backgroundColor = SemanticColors.accent.withAlphaComponent(0.15).cgColor
            dashboardTab.contentTintColor = SemanticColors.accent
        } else {
            dashboardTab.layer?.backgroundColor = NSColor.clear.cgColor
            dashboardTab.contentTintColor = SemanticColors.muted
        }
    }

    // MARK: - Theme

    override func updateLayer() {
        layer?.backgroundColor = SemanticColors.panel.withAlphaComponent(0.88).cgColor
        bottomBorder.layer?.backgroundColor = SemanticColors.line.withAlphaComponent(0.7).cgColor
        tabSeparator.layer?.backgroundColor = SemanticColors.line.withAlphaComponent(0.75).cgColor
        addButton.contentTintColor = SemanticColors.muted
        themeToggle.contentTintColor = SemanticColors.muted
        updateDashboardTabAppearance()

        for tab in projectTabViews {
            tab.needsDisplay = true
        }
    }

    // MARK: - Traffic Dot Factory

    private static func makeTrafficDot(color: NSColor) -> NSView {
        let dot = NSView()
        dot.wantsLayer = true
        dot.layer?.cornerRadius = 5.5
        dot.layer?.backgroundColor = color.cgColor
        dot.translatesAutoresizingMaskIntoConstraints = false
        NSLayoutConstraint.activate([
            dot.widthAnchor.constraint(equalToConstant: 11),
            dot.heightAnchor.constraint(equalToConstant: 11),
        ])
        return dot
    }
}

// MARK: - ProjectTabView

private final class ProjectTabView: NSView {
    var onSelect: (() -> Void)?
    var onClose: (() -> Void)?

    private let statusDot = NSView()
    private let nameLabel = NSTextField(labelWithString: "")
    private let closeButton = NSButton()
    private var isSelected = false
    private var trackingArea: NSTrackingArea?
    private var isHovered = false
    private let projectName: String

    init(name: String) {
        self.projectName = name
        super.init(frame: .zero)
        setup()
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) not supported")
    }

    func setSelected(_ selected: Bool) {
        isSelected = selected
        updateAppearance()
    }

    func updateStatus(_ status: String) {
        let color: NSColor
        switch status {
        case "error": color = SemanticColors.danger
        case "waiting": color = SemanticColors.waiting
        case "running": color = SemanticColors.running
        default: color = SemanticColors.idle
        }
        statusDot.layer?.backgroundColor = color.cgColor
    }

    private func setup() {
        wantsLayer = true
        layer?.cornerRadius = 7
        translatesAutoresizingMaskIntoConstraints = false

        // Status dot
        statusDot.wantsLayer = true
        statusDot.layer?.cornerRadius = 4
        statusDot.layer?.backgroundColor = SemanticColors.idle.cgColor
        statusDot.translatesAutoresizingMaskIntoConstraints = false
        addSubview(statusDot)

        // Name label
        nameLabel.stringValue = projectName
        nameLabel.font = NSFont.systemFont(ofSize: 11)
        nameLabel.lineBreakMode = .byTruncatingTail
        nameLabel.translatesAutoresizingMaskIntoConstraints = false
        addSubview(nameLabel)

        // Close button
        closeButton.title = "×"
        closeButton.bezelStyle = .recessed
        closeButton.isBordered = false
        closeButton.font = NSFont.systemFont(ofSize: 14, weight: .medium)
        closeButton.contentTintColor = SemanticColors.muted
        closeButton.target = self
        closeButton.action = #selector(closeTapped)
        closeButton.translatesAutoresizingMaskIntoConstraints = false
        closeButton.setAccessibilityIdentifier("titlebar.projectTab.\(projectName).close")
        addSubview(closeButton)

        NSLayoutConstraint.activate([
            heightAnchor.constraint(equalToConstant: 28),
            widthAnchor.constraint(greaterThanOrEqualToConstant: 60),
            widthAnchor.constraint(lessThanOrEqualToConstant: 180),

            statusDot.widthAnchor.constraint(equalToConstant: 8),
            statusDot.heightAnchor.constraint(equalToConstant: 8),
            statusDot.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 8),
            statusDot.centerYAnchor.constraint(equalTo: centerYAnchor),

            nameLabel.leadingAnchor.constraint(equalTo: statusDot.trailingAnchor, constant: 5),
            nameLabel.centerYAnchor.constraint(equalTo: centerYAnchor),

            closeButton.leadingAnchor.constraint(equalTo: nameLabel.trailingAnchor, constant: 4),
            closeButton.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -4),
            closeButton.centerYAnchor.constraint(equalTo: centerYAnchor),
            closeButton.widthAnchor.constraint(equalToConstant: 18),
            closeButton.heightAnchor.constraint(equalToConstant: 18),
        ])

        let click = NSClickGestureRecognizer(target: self, action: #selector(selectTapped))
        addGestureRecognizer(click)
    }

    @objc private func selectTapped() {
        onSelect?()
    }

    @objc private func closeTapped() {
        onClose?()
    }

    private func updateAppearance() {
        if isSelected {
            layer?.backgroundColor = SemanticColors.accent.withAlphaComponent(0.15).cgColor
            nameLabel.textColor = SemanticColors.text
        } else if isHovered {
            layer?.backgroundColor = SemanticColors.line.withAlphaComponent(0.18).cgColor
            nameLabel.textColor = SemanticColors.text
        } else {
            layer?.backgroundColor = NSColor.clear.cgColor
            nameLabel.textColor = SemanticColors.muted
        }
        closeButton.contentTintColor = isSelected
            ? SemanticColors.muted
            : SemanticColors.muted.withAlphaComponent(0.5)
    }

    // MARK: - Tracking

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        if let existing = trackingArea {
            removeTrackingArea(existing)
        }
        let area = NSTrackingArea(
            rect: bounds,
            options: [.mouseEnteredAndExited, .activeInActiveApp],
            owner: self,
            userInfo: nil
        )
        addTrackingArea(area)
        trackingArea = area
    }

    override func mouseEntered(with event: NSEvent) {
        isHovered = true
        updateAppearance()
    }

    override func mouseExited(with event: NSEvent) {
        isHovered = false
        updateAppearance()
    }

    override func updateLayer() {
        updateAppearance()
    }
}

// MARK: - HoverTrackingView

private final class HoverTrackingView: NSView {
    var onHoverChanged: ((Bool) -> Void)?
    private var trackingArea: NSTrackingArea?

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        if let existing = trackingArea {
            removeTrackingArea(existing)
        }
        let area = NSTrackingArea(
            rect: bounds,
            options: [.mouseEnteredAndExited, .activeInActiveApp],
            owner: self,
            userInfo: nil
        )
        addTrackingArea(area)
        trackingArea = area
    }

    override func mouseEntered(with event: NSEvent) {
        onHoverChanged?(true)
    }

    override func mouseExited(with event: NSEvent) {
        onHoverChanged?(false)
    }
}
