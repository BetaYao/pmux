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
    func titleBarDidRequestCloseWindow()
    func titleBarDidRequestMiniaturizeWindow()
    func titleBarDidRequestZoomWindow()
}

final class TitleBarView: NSView, LayoutPopoverDelegate {
    weak var delegate: TitleBarDelegate?

    var currentView: String = "dashboard" {
        didSet { updateViewState() }
    }
    var currentProject: String = ""
    var projects: [String] = []
    var projectStatusProvider: ((String) -> String)?

    // MARK: - Arc Blocks

    private let leftArcBlock = NSView()
    private let rightArcBlock = NSView()

    // Traffic lights
    private let trafficRed = TrafficDot(activeColor: NSColor(hex: 0xff5f57))
    private let trafficYellow = TrafficDot(activeColor: NSColor(hex: 0xfebb2e))
    private let trafficGreen = TrafficDot(activeColor: NSColor(hex: 0x28c840))

    // Left controls
    private let dashboardTab = NSButton()
    private let leftSeparator1 = NSView()
    private let leftSeparator2 = NSView()
    private let tabsScrollView = NSScrollView()
    private let tabsStack = NSStackView()
    private let addButton = NSButton()
    private var projectTabViews: [ProjectTabView] = []

    // Right controls
    private let viewSwitcherButton = NSButton()
    private let notifButton = NSButton()
    private let notifBadge = NSView()
    private let aiButton = NSButton()
    private let themeButton = NSButton()

    // Popover
    private let layoutPopover = LayoutPopoverView()

    // State
    private var isWindowHovered = false
    private var notifCount = 0

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

    func setWindowHovered(_ hovered: Bool) {
        isWindowHovered = hovered
        updateArcBlockColors()
        trafficRed.setWindowHovered(hovered)
        trafficYellow.setWindowHovered(hovered)
        trafficGreen.setWindowHovered(hovered)
    }

    func renderTabs() {
        // Remove old project tabs
        for tab in projectTabViews {
            tab.removeFromSuperview()
        }
        projectTabViews.removeAll()

        // Remove dynamic items from tabs stack
        leftSeparator2.removeFromSuperview()
        addButton.removeFromSuperview()

        // Add separator before project tabs if needed
        if !projects.isEmpty {
            tabsStack.addArrangedSubview(leftSeparator2)
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

        updateDashboardTabAppearance()
    }

    func updateNotifBadge(_ count: Int) {
        notifCount = count
        notifBadge.isHidden = count <= 0
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
        layer?.backgroundColor = NSColor.clear.cgColor
        translatesAutoresizingMaskIntoConstraints = false

        setupLeftArcBlock()
        setupRightArcBlock()
        setupLayoutPopover()

        NSLayoutConstraint.activate([
            heightAnchor.constraint(equalToConstant: 48),

            leftArcBlock.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 8),
            leftArcBlock.centerYAnchor.constraint(equalTo: centerYAnchor),
            leftArcBlock.heightAnchor.constraint(equalToConstant: 36),

            rightArcBlock.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -8),
            rightArcBlock.centerYAnchor.constraint(equalTo: centerYAnchor),
            rightArcBlock.heightAnchor.constraint(equalToConstant: 36),

            // 8px gap between blocks
            rightArcBlock.leadingAnchor.constraint(
                greaterThanOrEqualTo: leftArcBlock.trailingAnchor, constant: 8),
        ])

        updateViewState()
        updateArcBlockColors()
    }

    private func setupLeftArcBlock() {
        leftArcBlock.wantsLayer = true
        leftArcBlock.layer?.cornerRadius = 10
        leftArcBlock.translatesAutoresizingMaskIntoConstraints = false
        addSubview(leftArcBlock)

        // Traffic lights
        let trafficStack = NSStackView(views: [trafficRed, trafficYellow, trafficGreen])
        trafficStack.orientation = .horizontal
        trafficStack.spacing = 6
        trafficStack.translatesAutoresizingMaskIntoConstraints = false
        leftArcBlock.addSubview(trafficStack)

        // Traffic light click actions + accessibility
        trafficRed.target = self
        trafficRed.action = #selector(trafficRedClicked)
        trafficRed.setAccessibilityLabel("Close")
        trafficYellow.target = self
        trafficYellow.action = #selector(trafficYellowClicked)
        trafficYellow.setAccessibilityLabel("Minimize")
        trafficGreen.target = self
        trafficGreen.action = #selector(trafficGreenClicked)
        trafficGreen.setAccessibilityLabel("Zoom")

        // Separator 1 (after traffic lights)
        leftSeparator1.wantsLayer = true
        leftSeparator1.layer?.backgroundColor = NSColor(hex: 0x3a3a3a).cgColor
        leftSeparator1.translatesAutoresizingMaskIntoConstraints = false
        leftArcBlock.addSubview(leftSeparator1)

        // Dashboard tab
        configureDashboardTab()
        leftArcBlock.addSubview(dashboardTab)

        // Separator 2 (after dashboard, before project tabs) — created but added dynamically
        leftSeparator2.wantsLayer = true
        leftSeparator2.layer?.backgroundColor = NSColor(hex: 0x3a3a3a).cgColor
        leftSeparator2.translatesAutoresizingMaskIntoConstraints = false
        NSLayoutConstraint.activate([
            leftSeparator2.widthAnchor.constraint(equalToConstant: 1),
            leftSeparator2.heightAnchor.constraint(equalToConstant: 18),
        ])

        // Tabs scroll area
        tabsScrollView.translatesAutoresizingMaskIntoConstraints = false
        tabsScrollView.hasHorizontalScroller = false
        tabsScrollView.hasVerticalScroller = false
        tabsScrollView.drawsBackground = false
        tabsScrollView.horizontalScrollElasticity = .allowed

        tabsStack.orientation = .horizontal
        tabsStack.spacing = 4
        tabsStack.alignment = .centerY
        tabsStack.translatesAutoresizingMaskIntoConstraints = false
        tabsScrollView.documentView = tabsStack
        leftArcBlock.addSubview(tabsScrollView)

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
        addButton.wantsLayer = true
        addButton.layer?.cornerRadius = 7
        NSLayoutConstraint.activate([
            addButton.widthAnchor.constraint(equalToConstant: 30),
            addButton.heightAnchor.constraint(equalToConstant: 28),
        ])
        tabsStack.addArrangedSubview(addButton)

        NSLayoutConstraint.activate([
            trafficStack.leadingAnchor.constraint(equalTo: leftArcBlock.leadingAnchor, constant: 12),
            trafficStack.centerYAnchor.constraint(equalTo: leftArcBlock.centerYAnchor),

            leftSeparator1.leadingAnchor.constraint(equalTo: trafficStack.trailingAnchor, constant: 10),
            leftSeparator1.widthAnchor.constraint(equalToConstant: 1),
            leftSeparator1.heightAnchor.constraint(equalToConstant: 18),
            leftSeparator1.centerYAnchor.constraint(equalTo: leftArcBlock.centerYAnchor),

            dashboardTab.leadingAnchor.constraint(equalTo: leftSeparator1.trailingAnchor, constant: 6),
            dashboardTab.centerYAnchor.constraint(equalTo: leftArcBlock.centerYAnchor),

            tabsScrollView.leadingAnchor.constraint(equalTo: dashboardTab.trailingAnchor, constant: 4),
            tabsScrollView.trailingAnchor.constraint(equalTo: leftArcBlock.trailingAnchor, constant: -4),
            tabsScrollView.topAnchor.constraint(equalTo: leftArcBlock.topAnchor),
            tabsScrollView.bottomAnchor.constraint(equalTo: leftArcBlock.bottomAnchor),

            tabsStack.topAnchor.constraint(equalTo: tabsScrollView.topAnchor),
            tabsStack.bottomAnchor.constraint(equalTo: tabsScrollView.bottomAnchor),
            tabsStack.leadingAnchor.constraint(equalTo: tabsScrollView.contentView.leadingAnchor),
        ])
    }

    private func configureDashboardTab() {
        dashboardTab.bezelStyle = .recessed
        dashboardTab.isBordered = false
        dashboardTab.translatesAutoresizingMaskIntoConstraints = false
        dashboardTab.setAccessibilityIdentifier("titlebar.dashboardTab")
        dashboardTab.target = self
        dashboardTab.action = #selector(dashboardTabClicked)
        dashboardTab.wantsLayer = true
        dashboardTab.layer?.cornerRadius = 14

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

    private func setupRightArcBlock() {
        rightArcBlock.wantsLayer = true
        rightArcBlock.layer?.cornerRadius = 10
        rightArcBlock.translatesAutoresizingMaskIntoConstraints = false
        addSubview(rightArcBlock)

        let rightStack = NSStackView()
        rightStack.orientation = .horizontal
        rightStack.spacing = 2
        rightStack.alignment = .centerY
        rightStack.translatesAutoresizingMaskIntoConstraints = false
        rightArcBlock.addSubview(rightStack)

        // View switcher
        configureArcIconButton(viewSwitcherButton, symbol: "square.grid.2x2",
                               identifier: "titlebar.viewMenu", label: "Layout",
                               action: #selector(viewMenuClicked))
        rightStack.addArrangedSubview(viewSwitcherButton)

        // Notification button (with badge)
        configureArcIconButton(notifButton, symbol: "bell",
                               identifier: "titlebar.notifButton", label: "Notifications",
                               action: #selector(notifClicked))
        // Badge dot
        notifBadge.wantsLayer = true
        notifBadge.layer?.backgroundColor = SemanticColors.danger.cgColor
        notifBadge.layer?.cornerRadius = 4
        notifBadge.translatesAutoresizingMaskIntoConstraints = false
        notifBadge.isHidden = true
        notifButton.addSubview(notifBadge)
        NSLayoutConstraint.activate([
            notifBadge.widthAnchor.constraint(equalToConstant: 8),
            notifBadge.heightAnchor.constraint(equalToConstant: 8),
            notifBadge.topAnchor.constraint(equalTo: notifButton.topAnchor, constant: 4),
            notifBadge.trailingAnchor.constraint(equalTo: notifButton.trailingAnchor, constant: -4),
        ])
        rightStack.addArrangedSubview(notifButton)

        // AI button
        configureArcIconButton(aiButton, symbol: "sparkles",
                               identifier: "titlebar.aiButton", label: "AI Assistant",
                               action: #selector(aiClicked))
        rightStack.addArrangedSubview(aiButton)

        // Theme button
        configureArcIconButton(themeButton, symbol: "circle.lefthalf.filled",
                               identifier: "titlebar.themeToggle", label: "Toggle Theme",
                               action: #selector(themeClicked))
        rightStack.addArrangedSubview(themeButton)

        NSLayoutConstraint.activate([
            rightStack.leadingAnchor.constraint(equalTo: rightArcBlock.leadingAnchor, constant: 4),
            rightStack.trailingAnchor.constraint(equalTo: rightArcBlock.trailingAnchor, constant: -4),
            rightStack.centerYAnchor.constraint(equalTo: rightArcBlock.centerYAnchor),
        ])
    }

    private func setupLayoutPopover() {
        layoutPopover.delegate = self
        // Popover must be added to a parent that won't clip it.
        // It will be reparented to window contentView by MainWindowController.
    }

    /// Call from MainWindowController to add popover to contentView so it's not clipped by titlebar bounds
    func installPopover(in parentView: NSView) {
        parentView.addSubview(layoutPopover)
        NSLayoutConstraint.activate([
            layoutPopover.topAnchor.constraint(equalTo: bottomAnchor, constant: 4),
            layoutPopover.trailingAnchor.constraint(equalTo: viewSwitcherButton.trailingAnchor),
        ])
    }

    // MARK: - Arc Icon Button Helper

    private func configureArcIconButton(_ button: NSButton, symbol: String,
                                         identifier: String, label: String? = nil, action: Selector) {
        let desc = label ?? identifier
        let config = NSImage.SymbolConfiguration(pointSize: 13, weight: .medium)
        if let image = NSImage(systemSymbolName: symbol, accessibilityDescription: desc) {
            button.image = image.withSymbolConfiguration(config)
        }
        button.bezelStyle = .recessed
        button.isBordered = false
        button.imagePosition = .imageOnly
        button.contentTintColor = NSColor(hex: 0x888888)
        button.target = self
        button.action = action
        button.translatesAutoresizingMaskIntoConstraints = false
        button.setAccessibilityIdentifier(identifier)
        button.setAccessibilityLabel(desc)
        button.wantsLayer = true
        button.layer?.cornerRadius = 7
        NSLayoutConstraint.activate([
            button.widthAnchor.constraint(equalToConstant: 30),
            button.heightAnchor.constraint(equalToConstant: 30),
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
                ? NSColor.white.withAlphaComponent(0.07).cgColor
                : NSColor.clear.cgColor
            button?.contentTintColor = hovered
                ? NSColor.white
                : NSColor(hex: 0x888888)
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

    @objc private func trafficRedClicked() {
        delegate?.titleBarDidRequestCloseWindow()
    }

    @objc private func trafficYellowClicked() {
        delegate?.titleBarDidRequestMiniaturizeWindow()
    }

    @objc private func trafficGreenClicked() {
        delegate?.titleBarDidRequestZoomWindow()
    }

    // MARK: - State

    private func updateViewState() {
        viewSwitcherButton.alphaValue = currentView == "dashboard" ? 1.0 : 0.3
        viewSwitcherButton.isEnabled = currentView == "dashboard"
        updateDashboardTabAppearance()
    }

    private func updateDashboardTabAppearance() {
        let isActive = currentView == "dashboard"
        if isActive {
            dashboardTab.layer?.backgroundColor = SemanticColors.accentAlpha15.cgColor
            dashboardTab.contentTintColor = SemanticColors.accent
        } else {
            dashboardTab.layer?.backgroundColor = NSColor.clear.cgColor
            dashboardTab.contentTintColor = SemanticColors.muted
        }
    }

    private func updateArcBlockColors() {
        let bg = isWindowHovered
            ? SemanticColors.arcBlockHover
            : SemanticColors.arcBlockInactive
        leftArcBlock.layer?.backgroundColor = bg.cgColor
        rightArcBlock.layer?.backgroundColor = bg.cgColor
    }

    // MARK: - Theme

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        applyColors()
    }

    private func applyColors() {
        updateArcBlockColors()
        leftSeparator1.layer?.backgroundColor = NSColor(hex: 0x3a3a3a).cgColor
        leftSeparator2.layer?.backgroundColor = NSColor(hex: 0x3a3a3a).cgColor
        addButton.contentTintColor = SemanticColors.muted
        updateDashboardTabAppearance()
        notifBadge.layer?.backgroundColor = SemanticColors.danger.cgColor
    }
}

// MARK: - TrafficDot

private final class TrafficDot: NSView {
    var target: AnyObject?
    var action: Selector?

    private let activeColor: NSColor
    private let inactiveColor = NSColor(hex: 0x555555)
    private var isWindowHovered = false

    override var acceptsFirstResponder: Bool { true }
    override var canBecomeKeyView: Bool { true }
    override var focusRingType: NSFocusRingType {
        get { .exterior }
        set { /* always exterior */ }
    }

    init(activeColor: NSColor) {
        self.activeColor = activeColor
        super.init(frame: .zero)
        wantsLayer = true
        layer?.cornerRadius = 6
        layer?.backgroundColor = inactiveColor.cgColor
        translatesAutoresizingMaskIntoConstraints = false
        NSLayoutConstraint.activate([
            widthAnchor.constraint(equalToConstant: 12),
            heightAnchor.constraint(equalToConstant: 12),
        ])
        setAccessibilityRole(.button)
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) not supported")
    }

    func setAccessibilityName(_ name: String) {
        setAccessibilityLabel(name)
    }

    func setWindowHovered(_ hovered: Bool) {
        isWindowHovered = hovered
        layer?.backgroundColor = hovered ? activeColor.cgColor : inactiveColor.cgColor
    }

    override func mouseDown(with event: NSEvent) {
        if let target = target, let action = action {
            NSApp.sendAction(action, to: target, from: self)
        }
    }

    override func keyDown(with event: NSEvent) {
        // Space or Return activates the button (standard keyboard activation)
        if event.keyCode == 49 || event.keyCode == 36 {
            if let target = target, let action = action {
                NSApp.sendAction(action, to: target, from: self)
            }
        } else {
            super.keyDown(with: event)
        }
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
        statusDot.layer?.cornerRadius = 3
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
        closeButton.title = "\u{00D7}"
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

            statusDot.widthAnchor.constraint(equalToConstant: 6),
            statusDot.heightAnchor.constraint(equalToConstant: 6),
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
            layer?.backgroundColor = NSColor(hex: 0x1a2a1a).cgColor
            layer?.borderWidth = 1.5
            layer?.borderColor = NSColor(hex: 0x33c17b).cgColor
            nameLabel.textColor = SemanticColors.text
        } else if isHovered {
            layer?.backgroundColor = NSColor(hex: 0x222222).cgColor
            layer?.borderWidth = 1.5
            layer?.borderColor = NSColor.white.withAlphaComponent(0.08).cgColor
            nameLabel.textColor = SemanticColors.text
        } else {
            layer?.backgroundColor = NSColor.clear.cgColor
            layer?.borderWidth = 0
            layer?.borderColor = nil
            nameLabel.textColor = SemanticColors.muted
        }
        closeButton.contentTintColor = isSelected
            ? SemanticColors.muted
            : SemanticColors.mutedAlpha50
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

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
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
