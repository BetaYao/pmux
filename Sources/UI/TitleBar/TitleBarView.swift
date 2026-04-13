import AppKit

protocol TitleBarDelegate: AnyObject {
    func titleBarDidRequestNewThread()
    func titleBarDidRequestCollapseSidebar()
    func titleBarDidRequestAddProject()
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

    // MARK: - Arc Blocks

    private let leftArcBlock = NSView()
    private let rightArcBlock = NSView()

    // Left controls — worktree info
    private let worktreeStatusDot = NSView()
    private let worktreeBranchLabel = NSTextField(labelWithString: "")
    private let worktreeRepoLabel = NSTextField(labelWithString: "")
    private let worktreeMetaLabel = NSTextField(labelWithString: "")
    private let addProjectButton = NSButton()
    private let dashboardTitleLabel = NSTextField(labelWithString: "AMUX Dashboard")
    private let worktreeInfoStack = NSStackView()

    // Right controls
    private let newWorktreeButton = NSButton()
    private let collapseSidebarButton = NSButton()
    private let viewSwitcherButton = NSButton()
    private let notifButton = NSButton()
    private let notifBadge = NSView()
    private let aiButton = NSButton()
    private let themeButton = NSButton()

    private var currentLayout: DashboardLayout = .grid

    // State
    private var isWindowHovered = false
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

    func updateWorktreeInfo(branch: String?, repo: String?, status: AgentStatus?, agentName: String?, isGridLayout: Bool, hasWorkspaces: Bool = true) {
        let showWorktreeInfo = !isGridLayout && branch != nil

        dashboardTitleLabel.isHidden = showWorktreeInfo
        worktreeInfoStack.isHidden = !showWorktreeInfo

        // Hide workspace-dependent buttons when no workspaces exist
        collapseSidebarButton.isHidden = !hasWorkspaces
        newWorktreeButton.isHidden = !hasWorkspaces
        viewSwitcherButton.isHidden = !hasWorkspaces
        notifButton.isHidden = !hasWorkspaces

        if showWorktreeInfo {
            worktreeBranchLabel.stringValue = branch ?? ""
            worktreeRepoLabel.stringValue = repo ?? ""

            var metaParts: [String] = []
            if let status = status {
                metaParts.append(status.rawValue)
            }
            if let agentName = agentName, !agentName.isEmpty {
                metaParts.append(agentName)
            }
            worktreeMetaLabel.stringValue = metaParts.joined(separator: " \u{00B7} ")

            let dotColor: NSColor
            switch status {
            case .running: dotColor = SemanticColors.running
            case .waiting: dotColor = SemanticColors.waiting
            case .error: dotColor = SemanticColors.danger
            case .exited: dotColor = SemanticColors.danger
            default: dotColor = SemanticColors.idle
            }
            worktreeStatusDot.layer?.backgroundColor = dotColor.cgColor
        }
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

            // 8px gap between blocks — left block fills remaining space
            leftArcBlock.trailingAnchor.constraint(
                equalTo: rightArcBlock.leadingAnchor, constant: -8),
        ])

        updateArcBlockColors()
    }

    private func setupLeftArcBlock() {
        leftArcBlock.wantsLayer = true
        leftArcBlock.layer?.cornerRadius = 10
        leftArcBlock.translatesAutoresizingMaskIntoConstraints = false
        addSubview(leftArcBlock)

        // Dashboard title (shown in grid mode)
        dashboardTitleLabel.font = NSFont.systemFont(ofSize: 12, weight: .semibold)
        dashboardTitleLabel.textColor = SemanticColors.text
        dashboardTitleLabel.translatesAutoresizingMaskIntoConstraints = false
        dashboardTitleLabel.setContentHuggingPriority(.required, for: .horizontal)
        leftArcBlock.addSubview(dashboardTitleLabel)

        // Status dot (8px circle)
        worktreeStatusDot.wantsLayer = true
        worktreeStatusDot.layer?.cornerRadius = 4
        worktreeStatusDot.layer?.backgroundColor = SemanticColors.idle.cgColor
        worktreeStatusDot.translatesAutoresizingMaskIntoConstraints = false
        NSLayoutConstraint.activate([
            worktreeStatusDot.widthAnchor.constraint(equalToConstant: 8),
            worktreeStatusDot.heightAnchor.constraint(equalToConstant: 8),
        ])

        // Branch label (bold)
        worktreeBranchLabel.font = NSFont.systemFont(ofSize: 11, weight: .semibold)
        worktreeBranchLabel.textColor = SemanticColors.text
        worktreeBranchLabel.lineBreakMode = .byTruncatingTail
        worktreeBranchLabel.setContentHuggingPriority(.defaultHigh, for: .horizontal)
        worktreeBranchLabel.setContentCompressionResistancePriority(.defaultHigh, for: .horizontal)

        // Repo label (dimmed)
        worktreeRepoLabel.font = NSFont.systemFont(ofSize: 11, weight: .regular)
        worktreeRepoLabel.textColor = SemanticColors.muted
        worktreeRepoLabel.lineBreakMode = .byTruncatingTail
        worktreeRepoLabel.setContentHuggingPriority(.defaultLow, for: .horizontal)
        worktreeRepoLabel.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)

        // Meta label — status/agent (dimmed)
        worktreeMetaLabel.font = NSFont.systemFont(ofSize: 11, weight: .regular)
        worktreeMetaLabel.textColor = SemanticColors.muted
        worktreeMetaLabel.lineBreakMode = .byTruncatingTail
        worktreeMetaLabel.setContentHuggingPriority(.defaultLow, for: .horizontal)
        worktreeMetaLabel.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)

        // Dot separator helpers
        let dotSep1 = NSTextField(labelWithString: "\u{00B7}")
        dotSep1.font = NSFont.systemFont(ofSize: 11, weight: .medium)
        dotSep1.textColor = SemanticColors.muted
        dotSep1.setContentHuggingPriority(.required, for: .horizontal)

        let dotSep2 = NSTextField(labelWithString: "\u{00B7}")
        dotSep2.font = NSFont.systemFont(ofSize: 11, weight: .medium)
        dotSep2.textColor = SemanticColors.muted
        dotSep2.setContentHuggingPriority(.required, for: .horizontal)

        // Info stack: [dot] [branch] [·] [repo] [·] [meta]
        worktreeInfoStack.orientation = .horizontal
        worktreeInfoStack.spacing = 5
        worktreeInfoStack.alignment = .centerY
        worktreeInfoStack.translatesAutoresizingMaskIntoConstraints = false
        worktreeInfoStack.addArrangedSubview(worktreeStatusDot)
        worktreeInfoStack.addArrangedSubview(worktreeBranchLabel)
        worktreeInfoStack.addArrangedSubview(dotSep1)
        worktreeInfoStack.addArrangedSubview(worktreeRepoLabel)
        worktreeInfoStack.addArrangedSubview(dotSep2)
        worktreeInfoStack.addArrangedSubview(worktreeMetaLabel)
        worktreeInfoStack.isHidden = true
        leftArcBlock.addSubview(worktreeInfoStack)

        // Right-side button stack: [Add Project] [AI]
        configureArcIconButton(addProjectButton, symbol: "plus.rectangle",
                               identifier: "titlebar.addProject", label: "Add Project",
                               action: #selector(addProjectClicked))
        configureArcIconButton(aiButton, symbol: "sparkles",
                               identifier: "titlebar.aiButton", label: "AI Assistant",
                               action: #selector(aiClicked))

        let leftButtonStack = NSStackView()
        leftButtonStack.orientation = .horizontal
        leftButtonStack.spacing = 2
        leftButtonStack.alignment = .centerY
        leftButtonStack.translatesAutoresizingMaskIntoConstraints = false
        leftButtonStack.addArrangedSubview(addProjectButton)
        leftButtonStack.addArrangedSubview(aiButton)
        leftArcBlock.addSubview(leftButtonStack)

        NSLayoutConstraint.activate([
            dashboardTitleLabel.leadingAnchor.constraint(equalTo: leftArcBlock.leadingAnchor, constant: Layout.dashboardLeadingInset),
            dashboardTitleLabel.centerYAnchor.constraint(equalTo: leftArcBlock.centerYAnchor),

            worktreeInfoStack.leadingAnchor.constraint(equalTo: leftArcBlock.leadingAnchor, constant: Layout.dashboardLeadingInset),
            worktreeInfoStack.centerYAnchor.constraint(equalTo: leftArcBlock.centerYAnchor),

            leftButtonStack.leadingAnchor.constraint(greaterThanOrEqualTo: worktreeInfoStack.trailingAnchor, constant: 8),
            leftButtonStack.leadingAnchor.constraint(greaterThanOrEqualTo: dashboardTitleLabel.trailingAnchor, constant: 8),
            leftButtonStack.trailingAnchor.constraint(equalTo: leftArcBlock.trailingAnchor, constant: -4),
            leftButtonStack.centerYAnchor.constraint(equalTo: leftArcBlock.centerYAnchor),
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

        // Collapse sidebar
        configureArcIconButton(collapseSidebarButton, symbol: "sidebar.right",
                               identifier: "titlebar.collapseSidebar", label: "Toggle Sidebar",
                               action: #selector(collapseSidebarClicked))
        rightStack.addArrangedSubview(collapseSidebarButton)

        // New worktree
        configureArcIconButton(newWorktreeButton, symbol: "plus",
                               identifier: "titlebar.newWorktree", label: "New Worktree",
                               action: #selector(newWorktreeClicked))
        rightStack.addArrangedSubview(newWorktreeButton)

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

    @objc private func newWorktreeClicked() {
        delegate?.titleBarDidRequestNewThread()
    }

    @objc private func collapseSidebarClicked() {
        delegate?.titleBarDidRequestCollapseSidebar()
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
        dashboardTitleLabel.textColor = SemanticColors.text
        worktreeBranchLabel.textColor = SemanticColors.text
        worktreeRepoLabel.textColor = SemanticColors.muted
        worktreeMetaLabel.textColor = SemanticColors.muted
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
