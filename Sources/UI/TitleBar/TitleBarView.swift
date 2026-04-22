import AppKit

protocol TitleBarDelegate: AnyObject {
    func titleBarDidRequestNewThread()
    func titleBarDidRequestAddProject()
    func titleBarDidSelectLayout(_ layout: DashboardLayout)
    func titleBarDidActivatePrimaryCapsule()
    func titleBarDidToggleTheme()
    func titleBarDidRequestCollapseSidebar()
}

final class TitleBarView: NSView {
    enum Layout {
        static let barHeight: CGFloat = 45
        static let capsuleHeight: CGFloat = 37
        static let arcVerticalOffset: CGFloat = 2
        static let dashboardLeadingInset: CGFloat = 16
        static let dashboardHorizontalPadding: CGFloat = 10
        static let tipRotationInterval: TimeInterval = 7.0
    }

    private enum PrimaryCapsuleMode {
        case tips
        case notification(NotificationEntry)
    }

    private static let tips: [(leading: String, body: String)] = [
        ("Tip", "Cmd+1..4 switch layout"),
        ("Tip", "Cmd+J toggle dashboard focus"),
        ("Tip", "Cmd+B toggle sidebar"),
        ("Tip", "Cmd+D split horizontally"),
        ("Tip", "Cmd+Shift+D split vertically"),
        ("Tip", "Cmd+Option+Arrow move focus"),
        ("Tip", "Cmd+Ctrl+Arrow resize split"),
        ("Tip", "Cmd+Shift+F show diff"),
    ]

    weak var delegate: TitleBarDelegate?

    // MARK: - Arc Blocks

    private let leftArcBlock = NSView()
    private let rightArcBlock = NSView()

    // Left controls — primary capsule
    private let capsuleIconView = NSImageView()
    private let capsuleLeadingLabel = NSTextField(labelWithString: "")
    private let capsuleBodyLabel = NSTextField(labelWithString: "")
    private let capsuleTrailingLabel = NSTextField(labelWithString: "")
    private let capsuleSep1Label = NSTextField(labelWithString: "\u{00B7}")
    private let capsuleSep2Label = NSTextField(labelWithString: "\u{00B7}")
    private let primaryCapsuleStack = NSStackView()

    // Right controls — layout group
    private let gridLayoutButton = NSButton()
    private let leftLayoutButton = NSButton()
    private let topSmallLayoutButton = NSButton()
    private let topLargeLayoutButton = NSButton()
    private var layoutButtons: [DashboardLayout: NSButton] = [:]

    // Right controls — action group
    private let addProjectButton = NSButton()
    private let newWorktreeButton = NSButton()
    private let themeButton = NSButton()
    private let collapseSidebarButton = NSButton()

    private var currentLayout: DashboardLayout = .grid

    // State
    private var isWindowHovered = false
    private var highlightedNotificationStatus: AgentStatus?
    private var primaryCapsuleMode: PrimaryCapsuleMode = .tips
    private var currentTipIndex = 0
    private var tipRotationTimer: Timer?
    private var isPrimaryCapsuleHovered = false
    private var hoverTrackingArea: NSTrackingArea?
    private var primaryCapsuleTrackingArea: NSTrackingArea?

    // MARK: - Init

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        setup()
    }

    required init?(coder: NSCoder) {
        super.init(coder: coder)
        setup()
    }

    deinit {
        tipRotationTimer?.invalidate()
    }

    // MARK: - Public API

    func setWindowHovered(_ hovered: Bool) {
        isWindowHovered = hovered
        updateArcBlockColors()
    }

    func updateChromeState(isGridLayout: Bool, hasWorkspaces: Bool = true) {
        newWorktreeButton.isHidden = !hasWorkspaces
        collapseSidebarButton.isHidden = !hasWorkspaces
        collapseSidebarButton.isEnabled = !isGridLayout
        collapseSidebarButton.alphaValue = isGridLayout ? 0.3 : 1.0
    }

    func updateNotificationSummary(entry: NotificationEntry?, unreadCount: Int) {
        highlightedNotificationStatus = entry?.status

        if let entry {
            primaryCapsuleMode = .notification(entry)
            showNotification(entry, unreadCount: unreadCount)
            stopTipRotation()
        } else {
            primaryCapsuleMode = .tips
            showCurrentTip()
            startTipRotationIfNeeded()
        }
    }

    func setCurrentLayout(_ layout: DashboardLayout) {
        currentLayout = layout
        updateLayoutButtonHighlight()
    }

    func aiAnchorView() -> NSView {
        themeButton
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

            leftArcBlock.trailingAnchor.constraint(equalTo: rightArcBlock.leadingAnchor, constant: -8),
        ])

        showCurrentTip()
        startTipRotationIfNeeded()
        updateArcBlockColors()
    }

    private func setupLeftArcBlock() {
        leftArcBlock.wantsLayer = true
        leftArcBlock.layer?.cornerRadius = 10
        leftArcBlock.layer?.borderWidth = 1
        leftArcBlock.translatesAutoresizingMaskIntoConstraints = false
        leftArcBlock.setAccessibilityIdentifier("titlebar.primaryCapsule")
        addSubview(leftArcBlock)

        let click = NSClickGestureRecognizer(target: self, action: #selector(primaryCapsuleClicked))
        leftArcBlock.addGestureRecognizer(click)

        capsuleIconView.translatesAutoresizingMaskIntoConstraints = false
        capsuleIconView.symbolConfiguration = NSImage.SymbolConfiguration(pointSize: 12, weight: .medium)
        capsuleIconView.contentTintColor = SemanticColors.muted
        capsuleIconView.setContentHuggingPriority(.required, for: .horizontal)
        NSLayoutConstraint.activate([
            capsuleIconView.widthAnchor.constraint(equalToConstant: 12),
            capsuleIconView.heightAnchor.constraint(equalToConstant: 12),
        ])

        capsuleLeadingLabel.font = NSFont.systemFont(ofSize: 11, weight: .semibold)
        capsuleLeadingLabel.textColor = SemanticColors.text
        capsuleLeadingLabel.lineBreakMode = .byTruncatingTail
        capsuleLeadingLabel.setContentHuggingPriority(.defaultHigh, for: .horizontal)
        capsuleLeadingLabel.setContentCompressionResistancePriority(.defaultHigh, for: .horizontal)

        capsuleBodyLabel.font = NSFont.systemFont(ofSize: 11, weight: .regular)
        capsuleBodyLabel.textColor = SemanticColors.muted
        capsuleBodyLabel.lineBreakMode = .byTruncatingTail
        capsuleBodyLabel.setContentHuggingPriority(.defaultLow, for: .horizontal)
        capsuleBodyLabel.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)

        capsuleTrailingLabel.font = NSFont.systemFont(ofSize: 11, weight: .regular)
        capsuleTrailingLabel.textColor = SemanticColors.muted
        capsuleTrailingLabel.lineBreakMode = .byTruncatingTail
        capsuleTrailingLabel.setContentHuggingPriority(.required, for: .horizontal)
        capsuleTrailingLabel.setContentCompressionResistancePriority(.required, for: .horizontal)

        capsuleSep1Label.font = NSFont.systemFont(ofSize: 11, weight: .medium)
        capsuleSep1Label.textColor = SemanticColors.muted
        capsuleSep1Label.setContentHuggingPriority(.required, for: .horizontal)

        capsuleSep2Label.font = NSFont.systemFont(ofSize: 11, weight: .medium)
        capsuleSep2Label.textColor = SemanticColors.muted
        capsuleSep2Label.setContentHuggingPriority(.required, for: .horizontal)

        primaryCapsuleStack.orientation = .horizontal
        primaryCapsuleStack.spacing = 5
        primaryCapsuleStack.alignment = .centerY
        primaryCapsuleStack.translatesAutoresizingMaskIntoConstraints = false
        primaryCapsuleStack.addArrangedSubview(capsuleIconView)
        primaryCapsuleStack.addArrangedSubview(capsuleLeadingLabel)
        primaryCapsuleStack.addArrangedSubview(capsuleSep1Label)
        primaryCapsuleStack.addArrangedSubview(capsuleBodyLabel)
        primaryCapsuleStack.addArrangedSubview(capsuleSep2Label)
        primaryCapsuleStack.addArrangedSubview(capsuleTrailingLabel)
        leftArcBlock.addSubview(primaryCapsuleStack)

        NSLayoutConstraint.activate([
            primaryCapsuleStack.leadingAnchor.constraint(equalTo: leftArcBlock.leadingAnchor, constant: Layout.dashboardLeadingInset),
            primaryCapsuleStack.trailingAnchor.constraint(lessThanOrEqualTo: leftArcBlock.trailingAnchor, constant: -Layout.dashboardLeadingInset),
            primaryCapsuleStack.centerYAnchor.constraint(equalTo: leftArcBlock.centerYAnchor),
        ])
    }

    private func setupRightArcBlock() {
        rightArcBlock.wantsLayer = true
        rightArcBlock.layer?.cornerRadius = 10
        rightArcBlock.translatesAutoresizingMaskIntoConstraints = false
        addSubview(rightArcBlock)

        let layoutStack = NSStackView()
        layoutStack.orientation = .horizontal
        layoutStack.spacing = 2
        layoutStack.alignment = .centerY

        configureArcIconButton(gridLayoutButton, symbol: "square.grid.2x2",
                               identifier: "titlebar.layout.grid", label: "Grid",
                               action: #selector(layoutButtonClicked(_:)))
        gridLayoutButton.tag = 0
        layoutStack.addArrangedSubview(gridLayoutButton)

        configureArcIconButton(leftLayoutButton, symbol: "rectangle.split.2x1",
                               identifier: "titlebar.layout.left", label: "Left Right",
                               action: #selector(layoutButtonClicked(_:)))
        leftLayoutButton.tag = 1
        layoutStack.addArrangedSubview(leftLayoutButton)

        configureArcIconButton(topSmallLayoutButton, symbol: "rectangle.split.1x2",
                               identifier: "titlebar.layout.topSmall", label: "Top Small",
                               action: #selector(layoutButtonClicked(_:)))
        topSmallLayoutButton.tag = 2
        layoutStack.addArrangedSubview(topSmallLayoutButton)

        configureArcIconButton(topLargeLayoutButton, symbol: "rectangle.tophalf.filled",
                               identifier: "titlebar.layout.topLarge", label: "Top Large",
                               action: #selector(layoutButtonClicked(_:)))
        topLargeLayoutButton.tag = 3
        layoutStack.addArrangedSubview(topLargeLayoutButton)

        layoutButtons = [
            .grid: gridLayoutButton,
            .leftRight: leftLayoutButton,
            .topSmall: topSmallLayoutButton,
            .topLarge: topLargeLayoutButton,
        ]

        let divider = NSView()
        divider.wantsLayer = true
        divider.layer?.backgroundColor = NSColor(hex: 0x888888).withAlphaComponent(0.3).cgColor
        divider.translatesAutoresizingMaskIntoConstraints = false
        NSLayoutConstraint.activate([
            divider.widthAnchor.constraint(equalToConstant: 1),
            divider.heightAnchor.constraint(equalToConstant: 16),
        ])

        let actionStack = NSStackView()
        actionStack.orientation = .horizontal
        actionStack.spacing = 2
        actionStack.alignment = .centerY

        configureArcIconButton(addProjectButton, symbol: "plus.rectangle",
                               identifier: "titlebar.addProject", label: "Add Project",
                               action: #selector(addProjectClicked))
        actionStack.addArrangedSubview(addProjectButton)

        configureArcIconButton(newWorktreeButton, symbol: "plus",
                               identifier: "titlebar.newWorktree", label: "New Worktree",
                               action: #selector(newWorktreeClicked))
        actionStack.addArrangedSubview(newWorktreeButton)

        configureArcIconButton(themeButton, symbol: "circle.lefthalf.filled",
                               identifier: "titlebar.themeToggle", label: "Toggle Theme",
                               action: #selector(themeClicked))
        actionStack.addArrangedSubview(themeButton)

        configureArcIconButton(collapseSidebarButton, symbol: "sidebar.right",
                               identifier: "titlebar.collapseSidebar", label: "Toggle Sidebar",
                               action: #selector(collapseSidebarClicked))
        actionStack.addArrangedSubview(collapseSidebarButton)

        let rightStack = NSStackView()
        rightStack.orientation = .horizontal
        rightStack.spacing = 6
        rightStack.alignment = .centerY
        rightStack.translatesAutoresizingMaskIntoConstraints = false
        rightStack.addArrangedSubview(layoutStack)
        rightStack.addArrangedSubview(divider)
        rightStack.addArrangedSubview(actionStack)
        rightArcBlock.addSubview(rightStack)

        NSLayoutConstraint.activate([
            rightStack.leadingAnchor.constraint(equalTo: rightArcBlock.leadingAnchor, constant: 4),
            rightStack.trailingAnchor.constraint(equalTo: rightArcBlock.trailingAnchor, constant: -4),
            rightStack.centerYAnchor.constraint(equalTo: rightArcBlock.centerYAnchor),
        ])

        updateLayoutButtonHighlight()
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

    @objc private func addProjectClicked() {
        delegate?.titleBarDidRequestAddProject()
    }

    private static let tagToLayout: [Int: DashboardLayout] = [
        0: .grid, 1: .leftRight, 2: .topSmall, 3: .topLarge,
    ]

    @objc private func layoutButtonClicked(_ sender: NSButton) {
        guard let layout = Self.tagToLayout[sender.tag] else { return }
        currentLayout = layout
        updateLayoutButtonHighlight()
        delegate?.titleBarDidSelectLayout(layout)
    }

    private func updateLayoutButtonHighlight() {
        let activeTint = SemanticColors.accent
        let inactiveTint = NSColor(hex: 0x888888)
        for (layout, button) in layoutButtons {
            button.contentTintColor = (layout == currentLayout) ? activeTint : inactiveTint
        }
    }

    @objc private func primaryCapsuleClicked() {
        if case .notification = primaryCapsuleMode {
            delegate?.titleBarDidActivatePrimaryCapsule()
        }
    }

    @objc private func themeClicked() {
        delegate?.titleBarDidToggleTheme()
    }

    @objc private func collapseSidebarClicked() {
        delegate?.titleBarDidRequestCollapseSidebar()
    }

    // MARK: - State

    private func updateArcBlockColors() {
        let saved = NSAppearance.current
        NSAppearance.current = window?.effectiveAppearance ?? NSApp.effectiveAppearance
        let bg = isWindowHovered
            ? SemanticColors.arcBlockHover
            : SemanticColors.arcBlockInactive
        leftArcBlock.layer?.backgroundColor = bg.cgColor
        leftArcBlock.layer?.borderColor = capsuleBorderColor().cgColor
        rightArcBlock.layer?.backgroundColor = bg.cgColor
        NSAppearance.current = saved
    }

    private func capsuleBorderColor() -> NSColor {
        guard let status = highlightedNotificationStatus else {
            return isWindowHovered ? SemanticColors.lineAlpha45 : SemanticColors.lineAlpha22
        }

        switch status {
        case .error, .exited:
            return SemanticColors.danger.withAlphaComponent(isWindowHovered ? 0.45 : 0.30)
        case .waiting:
            return SemanticColors.waiting.withAlphaComponent(isWindowHovered ? 0.45 : 0.30)
        case .idle:
            return SemanticColors.idle.withAlphaComponent(isWindowHovered ? 0.35 : 0.24)
        case .running:
            return SemanticColors.running.withAlphaComponent(isWindowHovered ? 0.35 : 0.24)
        default:
            return isWindowHovered ? SemanticColors.lineAlpha45 : SemanticColors.lineAlpha22
        }
    }

    private func updatePrimaryCapsuleSeparators() {
        let hasBodyText = !capsuleBodyLabel.stringValue.isEmpty
        let hasTrailingText = !capsuleTrailingLabel.stringValue.isEmpty
        capsuleSep1Label.isHidden = !hasBodyText
        capsuleBodyLabel.isHidden = !hasBodyText
        capsuleSep2Label.isHidden = !hasBodyText || !hasTrailingText
        capsuleTrailingLabel.isHidden = !hasTrailingText
    }

    private func notificationMetaText(for entry: NotificationEntry, unreadCount: Int) -> String {
        var parts: [String] = [entry.status.rawValue]
        if let paneIndex = entry.paneIndex {
            parts.append("Pane \(paneIndex)")
        }
        if unreadCount > 1 {
            parts.append("\(unreadCount) unread")
        } else if unreadCount == 1 {
            parts.append("1 unread")
        }
        return parts.joined(separator: " \u{00B7} ")
    }

    private func showNotification(_ entry: NotificationEntry, unreadCount: Int) {
        capsuleIconView.image = NSImage(systemSymbolName: "bell.fill", accessibilityDescription: "Notification")
        capsuleIconView.contentTintColor = statusColor(for: entry.status)
        capsuleLeadingLabel.attributedStringValue = notificationTargetText(for: entry)
        capsuleBodyLabel.stringValue = Self.sanitizedNotificationMessage(entry.message)
        capsuleTrailingLabel.stringValue = notificationMetaText(for: entry, unreadCount: unreadCount)
        updatePrimaryCapsuleSeparators()
        updateArcBlockColors()
    }

    private func showCurrentTip() {
        let tip = Self.tips[currentTipIndex]
        capsuleIconView.image = NSImage(systemSymbolName: "command", accessibilityDescription: "Tip")
        capsuleIconView.contentTintColor = SemanticColors.muted
        capsuleLeadingLabel.attributedStringValue = NSAttributedString(
            string: tip.leading,
            attributes: [
                .font: NSFont.systemFont(ofSize: 11, weight: .semibold),
                .foregroundColor: SemanticColors.text
            ]
        )
        capsuleBodyLabel.stringValue = tip.body
        capsuleTrailingLabel.stringValue = "Shortcuts"
        updatePrimaryCapsuleSeparators()
        updateArcBlockColors()
    }

    private func notificationTargetText(for entry: NotificationEntry) -> NSAttributedString {
        let workspaceFont = NSFont.systemFont(ofSize: 11, weight: .semibold)
        let branchFont = NSFont.systemFont(ofSize: 11, weight: .regular)

        if entry.workspaceName.isEmpty {
            return NSAttributedString(
                string: entry.branch,
                attributes: [
                    .font: workspaceFont,
                    .foregroundColor: SemanticColors.text
                ]
            )
        }

        let result = NSMutableAttributedString(
            string: entry.workspaceName,
            attributes: [
                .font: workspaceFont,
                .foregroundColor: SemanticColors.text
            ]
        )
        result.append(NSAttributedString(
            string: " / \(entry.branch)",
            attributes: [
                .font: branchFont,
                .foregroundColor: SemanticColors.subtle
            ]
        ))
        return result
    }

    private func startTipRotationIfNeeded() {
        guard tipRotationTimer == nil, case .tips = primaryCapsuleMode else { return }
        tipRotationTimer = Timer.scheduledTimer(withTimeInterval: Layout.tipRotationInterval, repeats: true) { [weak self] _ in
            self?.advanceTipIfNeeded()
        }
    }

    private func stopTipRotation() {
        tipRotationTimer?.invalidate()
        tipRotationTimer = nil
    }

    private func advanceTipIfNeeded() {
        guard case .tips = primaryCapsuleMode, !isPrimaryCapsuleHovered else { return }
        currentTipIndex = (currentTipIndex + 1) % Self.tips.count
        showCurrentTip()
    }

    private func statusColor(for status: AgentStatus) -> NSColor {
        switch status {
        case .running:
            return SemanticColors.running
        case .waiting:
            return SemanticColors.waiting
        case .error, .exited:
            return SemanticColors.danger
        default:
            return SemanticColors.idle
        }
    }

    private static func sanitizedNotificationMessage(_ message: String) -> String {
        let trimmed = message.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return "Open workspace" }
        let firstLine = trimmed.components(separatedBy: .newlines).first ?? trimmed
        return firstLine
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
        capsuleLeadingLabel.textColor = SemanticColors.text
        capsuleBodyLabel.textColor = SemanticColors.muted
        capsuleTrailingLabel.textColor = SemanticColors.muted
        capsuleSep1Label.textColor = SemanticColors.muted
        capsuleSep2Label.textColor = SemanticColors.muted
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

        if let existing = primaryCapsuleTrackingArea {
            leftArcBlock.removeTrackingArea(existing)
        }
        let capsuleArea = NSTrackingArea(
            rect: .zero,
            options: [.mouseEnteredAndExited, .activeAlways, .inVisibleRect],
            owner: self,
            userInfo: ["target": "primaryCapsule"]
        )
        leftArcBlock.addTrackingArea(capsuleArea)
        primaryCapsuleTrackingArea = capsuleArea
    }

    override func mouseEntered(with event: NSEvent) {
        if event.trackingArea?.userInfo?["target"] as? String == "primaryCapsule" {
            isPrimaryCapsuleHovered = true
            return
        }
        setWindowHovered(true)
        super.mouseEntered(with: event)
    }

    override func mouseExited(with event: NSEvent) {
        if event.trackingArea?.userInfo?["target"] as? String == "primaryCapsule" {
            isPrimaryCapsuleHovered = false
            return
        }
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
