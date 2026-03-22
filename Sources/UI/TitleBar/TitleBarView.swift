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

final class TitleBarView: NSView {
    enum Layout {
        static let barHeight: CGFloat = 45
        static let capsuleHeight: CGFloat = 37
        static let arcVerticalOffset: CGFloat = 2
        static let dashboardLeadingInset: CGFloat = 16
        static let dashboardHorizontalPadding: CGFloat = 10
    }

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

    // Left controls
    private let dashboardTab = NSButton()
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

    private var currentLayout: DashboardLayout = .grid

    // State
    private var isWindowHovered = false
    private var isDashboardTabHovered = false
    private var notifCount = 0
    private var hoverTrackingArea: NSTrackingArea?

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
    }

    func renderTabs() {
        // Fully reset arranged subviews to avoid stale NSStackView state
        for view in tabsStack.arrangedSubviews {
            tabsStack.removeArrangedSubview(view)
            view.removeFromSuperview()
        }
        projectTabViews.removeAll()

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

        if currentView == "dashboard" {
            tabsScrollView.contentView.setBoundsOrigin(.zero)
            tabsScrollView.reflectScrolledClipView(tabsScrollView.contentView)
        }

        updateDashboardTabAppearance()
    }

    func updateNotifBadge(_ count: Int) {
        notifCount = count
        notifBadge.isHidden = count <= 0
    }

    func setCurrentLayout(_ layout: DashboardLayout) {
        currentLayout = layout
        viewSwitcherButton.menu = makeLayoutMenu()
    }

    func notificationsAnchorView() -> NSView {
        notifButton
    }

    func aiAnchorView() -> NSView {
        aiButton
    }

    // MARK: - Setup

    private func setup() {
        wantsLayer = true
        layer?.backgroundColor = NSColor.clear.cgColor
        translatesAutoresizingMaskIntoConstraints = false
        setAccessibilityIdentifier("titlebar")

        setupLeftArcBlock()
        setupRightArcBlock()

        NSLayoutConstraint.activate([
            leftArcBlock.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 8),
            leftArcBlock.centerYAnchor.constraint(equalTo: centerYAnchor, constant: Layout.arcVerticalOffset),
            leftArcBlock.heightAnchor.constraint(equalToConstant: Layout.capsuleHeight),

            rightArcBlock.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -8),
            rightArcBlock.centerYAnchor.constraint(equalTo: centerYAnchor, constant: Layout.arcVerticalOffset),
            rightArcBlock.heightAnchor.constraint(equalToConstant: Layout.capsuleHeight),

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

        // Dashboard tab
        configureDashboardTab()
        leftArcBlock.addSubview(dashboardTab)

        // Separator 2 (after dashboard, before project tabs) — created but added dynamically
        leftSeparator2.wantsLayer = true
        leftSeparator2.layer?.backgroundColor = SemanticColors.line.cgColor
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
        setupHoverTracking(for: addButton, defaultTint: SemanticColors.muted)
        tabsStack.addArrangedSubview(addButton)

        NSLayoutConstraint.activate([
            dashboardTab.leadingAnchor.constraint(equalTo: leftArcBlock.leadingAnchor, constant: Layout.dashboardLeadingInset),
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
        dashboardTab.alignment = .center
        dashboardTab.setContentHuggingPriority(.required, for: .horizontal)

        let contentWidth = ceil(attrString.size().width + (Layout.dashboardHorizontalPadding * 2))
        NSLayoutConstraint.activate([
            dashboardTab.widthAnchor.constraint(equalToConstant: contentWidth),
            dashboardTab.heightAnchor.constraint(equalToConstant: 28),
        ])

        setupDashboardTabHoverTracking()
    }

    private func setupDashboardTabHoverTracking() {
        let hover = HoverTrackingView()
        hover.translatesAutoresizingMaskIntoConstraints = false
        dashboardTab.addSubview(hover)
        NSLayoutConstraint.activate([
            hover.topAnchor.constraint(equalTo: dashboardTab.topAnchor),
            hover.leadingAnchor.constraint(equalTo: dashboardTab.leadingAnchor),
            hover.trailingAnchor.constraint(equalTo: dashboardTab.trailingAnchor),
            hover.bottomAnchor.constraint(equalTo: dashboardTab.bottomAnchor),
        ])
        hover.onHoverChanged = { [weak self] hovered in
            self?.isDashboardTabHovered = hovered
            self?.updateDashboardTabAppearance(animated: true)
        }
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

    private func setupHoverTracking(for button: NSButton, defaultTint: NSColor = NSColor(hex: 0x888888)) {
        let hover = HoverTrackingView()
        hover.translatesAutoresizingMaskIntoConstraints = false
        button.addSubview(hover)
        NSLayoutConstraint.activate([
            hover.topAnchor.constraint(equalTo: button.topAnchor),
            hover.leadingAnchor.constraint(equalTo: button.leadingAnchor),
            hover.trailingAnchor.constraint(equalTo: button.trailingAnchor),
            hover.bottomAnchor.constraint(equalTo: button.bottomAnchor),
        ])
        hover.onHoverChanged = { [weak self, weak button] hovered in
            guard let self, let button else { return }
            self.updateIconButtonAppearance(button, hovered: hovered, defaultTint: defaultTint, animated: true)
        }
    }

    private func updateIconButtonAppearance(_ button: NSButton, hovered: Bool, defaultTint: NSColor, animated: Bool) {
        let apply = {
            button.layer?.backgroundColor = hovered
                ? button.resolvedCGColor(SemanticColors.iconButtonHoverBg)
                : NSColor.clear.cgColor
            if animated {
                button.animator().contentTintColor = hovered
                    ? SemanticColors.iconButtonHoverTint
                    : defaultTint
            } else {
                button.contentTintColor = hovered
                    ? SemanticColors.iconButtonHoverTint
                    : defaultTint
            }
        }

        if animated {
            animateHoverTransition(apply)
        } else {
            apply()
        }
    }

    private func animateHoverTransition(_ changes: @escaping () -> Void) {
        NSAnimationContext.runAnimationGroup { context in
            context.duration = 0.12
            context.allowsImplicitAnimation = true
            changes()
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
        let menu = makeLayoutMenu()
        _ = menu.popUp(positioning: nil, at: NSPoint(x: 0, y: viewSwitcherButton.bounds.height), in: viewSwitcherButton)
    }

    private func makeLayoutMenu() -> NSMenu {
        let menu = NSMenu(title: "Layout")
        addLayoutMenuItem(menu, title: "Grid", symbol: "square.grid.2x2", layout: .grid)
        addLayoutMenuItem(menu, title: "Left Right", symbol: "rectangle.split.2x1", layout: .leftRight)
        addLayoutMenuItem(menu, title: "Top Small", symbol: "rectangle.split.1x2", layout: .topSmall)
        addLayoutMenuItem(menu, title: "Top Large", symbol: "rectangle.tophalf.filled", layout: .topLarge)
        return menu
    }

    private func addLayoutMenuItem(_ menu: NSMenu, title: String, symbol: String, layout: DashboardLayout) {
        let item = NSMenuItem(title: title, action: #selector(layoutMenuItemSelected(_:)), keyEquivalent: "")
        item.target = self
        item.representedObject = layout.rawValue
        item.state = (currentLayout == layout) ? .on : .off
        if let image = NSImage(systemSymbolName: symbol, accessibilityDescription: title) {
            let config = NSImage.SymbolConfiguration(pointSize: 12, weight: .regular)
            item.image = image.withSymbolConfiguration(config)
        }
        menu.addItem(item)
    }

    @objc private func layoutMenuItemSelected(_ sender: NSMenuItem) {
        guard let raw = sender.representedObject as? String,
              let layout = DashboardLayout(rawValue: raw)
        else {
            return
        }
        currentLayout = layout
        delegate?.titleBarDidSelectLayout(layout)
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
        viewSwitcherButton.alphaValue = currentView == "dashboard" ? 1.0 : 0.3
        viewSwitcherButton.isEnabled = currentView == "dashboard"
        updateDashboardTabAppearance()
    }

    private func updateDashboardTabAppearance(animated: Bool = false) {
        let isActive = currentView == "dashboard"
        let bgColor: CGColor
        let tintColor: NSColor

        if isActive {
            bgColor = SemanticColors.accentAlpha15.cgColor
            tintColor = SemanticColors.accent
        } else if isDashboardTabHovered {
            bgColor = SemanticColors.iconButtonHoverBg.cgColor
            tintColor = SemanticColors.iconButtonHoverTint
        } else {
            bgColor = NSColor.clear.cgColor
            tintColor = SemanticColors.muted
        }

        let apply = {
            self.dashboardTab.layer?.backgroundColor = bgColor
            if animated {
                self.dashboardTab.animator().contentTintColor = tintColor
            } else {
                self.dashboardTab.contentTintColor = tintColor
            }
        }

        if animated {
            animateHoverTransition(apply)
        } else {
            apply()
        }
    }

    private func updateArcBlockColors() {
        // Caller (applyColors/setWindowHovered) must set NSAppearance.current first
        let saved = NSAppearance.current
        NSAppearance.current = window?.effectiveAppearance ?? NSApp.effectiveAppearance
        let bg = isWindowHovered
            ? SemanticColors.arcBlockHover
            : SemanticColors.arcBlockInactive
        leftArcBlock.layer?.backgroundColor = bg.cgColor
        rightArcBlock.layer?.backgroundColor = bg.cgColor
        NSAppearance.current = saved
    }

    // MARK: - Theme

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        applyColors()
    }

    private func applyColors() {
        let saved = NSAppearance.current
        NSAppearance.current = window?.effectiveAppearance ?? NSApp.effectiveAppearance
        updateArcBlockColors()
        leftSeparator2.layer?.backgroundColor = SemanticColors.line.cgColor
        addButton.contentTintColor = SemanticColors.muted
        updateDashboardTabAppearance()
        notifBadge.layer?.backgroundColor = SemanticColors.danger.cgColor
        NSAppearance.current = saved
    }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        if let existing = hoverTrackingArea {
            removeTrackingArea(existing)
        }
        let area = NSTrackingArea(
            rect: .zero,
            options: [.mouseEnteredAndExited, .activeAlways, .inVisibleRect],
            owner: self,
            userInfo: nil
        )
        addTrackingArea(area)
        hoverTrackingArea = area
    }

    override func mouseEntered(with event: NSEvent) {
        setWindowHovered(true)
        super.mouseEntered(with: event)
    }

    override func mouseExited(with event: NSEvent) {
        setWindowHovered(false)
        super.mouseExited(with: event)
    }
}

// MARK: - ProjectTabView

private final class ProjectTabView: NSButton {
    var onSelect: (() -> Void)?
    var onClose: (() -> Void)?

    private let statusDot = NSView()
    private let nameLabel = NSTextField(labelWithString: "")
    private let closeButton = NSButton()
    private var isSelected = false
    private var trackingArea: NSTrackingArea?
    private var isHovered = false
    private let projectName: String

    override var mouseDownCanMoveWindow: Bool { false }
    override var acceptsFirstResponder: Bool { true }

    init(name: String) {
        self.projectName = name
        super.init(frame: .zero)
        setup()
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) not supported")
    }

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool {
        true
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
        isBordered = false
        title = ""
        target = self
        action = #selector(selectTapped)
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

    }

    @objc private func selectTapped() {
        onSelect?()
    }

    @objc private func closeTapped() {
        onClose?()
    }

    private func updateAppearance() {
        if isSelected {
            layer?.backgroundColor = resolvedCGColor(SemanticColors.tabSelectedBg)
            layer?.borderWidth = 1.5
            layer?.borderColor = resolvedCGColor(SemanticColors.tabSelectedBorder)
            nameLabel.textColor = SemanticColors.text
        } else if isHovered {
            layer?.backgroundColor = resolvedCGColor(SemanticColors.tabHoverBg)
            layer?.borderWidth = 1.5
            layer?.borderColor = resolvedCGColor(SemanticColors.tabHoverBorder)
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
