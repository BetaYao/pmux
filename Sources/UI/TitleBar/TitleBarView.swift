import AppKit

protocol TitleBarDelegate: AnyObject {
    func titleBarDidToggleTheme()
    func titleBarDidRequestCollapseSidebar()
    func titleBarDidRequestCleanMergedWorktrees()
}

final class TitleBarView: NSView {
    enum Layout {
        static let barHeight: CGFloat = 45
        static let capsuleHeight: CGFloat = 37
        static let rightCapsuleCompactWidth: CGFloat = 76
        static let rightCapsuleCleanWidth: CGFloat = 112
        static let arcVerticalOffset: CGFloat = 2
        static let dashboardLeadingInset: CGFloat = 16
        static let dashboardHorizontalPadding: CGFloat = 10
        static let tipRotationInterval: TimeInterval = 7.0
        static let usageProgressWidth: CGFloat = 78
    }

    weak var delegate: TitleBarDelegate?

    // MARK: - Arc Blocks

    private let leftArcBlock = NSView()
    private let rightArcBlock = NSView()

    // Left controls — primary capsule
    private let capsuleIconView = NSImageView()
    private let capsuleLeadingLabel = NSTextField(labelWithString: "")
    private let capsuleBodyLabel = NSTextField(labelWithString: "")
    private let capsuleTrailingLabel = NSTextField(labelWithString: "")
    private let capsuleSecondaryLabel = NSTextField(labelWithString: "")
    private let capsuleSecondaryTrailingLabel = NSTextField(labelWithString: "")
    private let capsuleSep1Label = NSTextField(labelWithString: "\u{00B7}")
    private let capsuleSep2Label = NSTextField(labelWithString: "\u{00B7}")
    private let capsuleSep3Label = NSTextField(labelWithString: "\u{00B7}")
    private let primaryCapsuleStack = NSStackView()
    private let usageProgressTrack = NSView()
    private let usageProgressFill = NSView()
    private var usageProgressWidthConstraint: NSLayoutConstraint?
    private let secondaryUsageProgressTrack = NSView()
    private let secondaryUsageProgressFill = NSView()
    private var secondaryUsageProgressWidthConstraint: NSLayoutConstraint?

    // Right controls — action group
    private let cleanWorktreeButton = NSButton()
    private let themeButton = NSButton()
    private let collapseSidebarButton = NSButton()
    private var rightArcWidthConstraint: NSLayoutConstraint?

    // State
    private var isWindowHovered = false
    private var highlightedNotificationStatus: AgentStatus?
    private var primaryCapsuleFrames: [PrimaryCapsuleFrame] = UsageSummaryFormatter.rotationFrames(
        claude: UsageSnapshot(provider: .claude, rateLimit: nil, todayTokens: nil, updatedAt: nil, isStale: true),
        codex: UsageSnapshot(provider: .codex, rateLimit: nil, todayTokens: nil, updatedAt: nil, isStale: true)
    )
    private var currentPrimaryCapsuleIndex = 0
    private var tipRotationTimer: Timer?
    private var isPrimaryCapsuleHovered = false
    private var usesFocusedWorktreeMode = false
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

    func updateChromeState(isGridLayout: Bool, hasWorkspaces: Bool = true, canCleanWorktrees: Bool = false) {
        cleanWorktreeButton.isHidden = !canCleanWorktrees
        cleanWorktreeButton.isEnabled = canCleanWorktrees
        rightArcWidthConstraint?.constant = canCleanWorktrees
            ? Layout.rightCapsuleCleanWidth
            : Layout.rightCapsuleCompactWidth
        collapseSidebarButton.isHidden = !hasWorkspaces
        collapseSidebarButton.isEnabled = !isGridLayout
        collapseSidebarButton.alphaValue = isGridLayout ? 0.3 : 1.0
        layoutSubtreeIfNeeded()
    }

    func updateNotificationSummary(entry: NotificationEntry?, unreadCount: Int) {
        highlightedNotificationStatus = nil
        updateArcBlockColors()
        startTipRotationIfNeeded()
    }

    /// Collapse whitespace/newlines and cap a resolved worktree title to a
    /// label-length string (with an ellipsis) so it fits the title-bar capsule.
    static func clampTitle(_ title: String, limit: Int = 64) -> String {
        let collapsed = title
            .components(separatedBy: .whitespacesAndNewlines)
            .filter { !$0.isEmpty }
            .joined(separator: " ")
        guard collapsed.count > limit else { return collapsed }
        return collapsed.prefix(limit).trimmingCharacters(in: .whitespaces) + "\u{2026}"
    }

    /// Show the focused worktree's title on the left and a token placeholder on the right.
    func updateFocusedWorktree(title: String, tokenText: String = "\u{2014}") {
        tipRotationTimer?.invalidate()
        tipRotationTimer = nil
        usesFocusedWorktreeMode = true
        capsuleIconView.isHidden = true
        // A resolved title can be a whole task-description paragraph; clamp it to
        // a label-length string so the full-width capsule reads as a title and
        // doesn't run text across the entire (possibly ultrawide) title bar.
        capsuleLeadingLabel.stringValue = Self.clampTitle(title)
        capsuleLeadingLabel.toolTip = title
        capsuleLeadingLabel.lineBreakMode = .byTruncatingTail
        capsuleLeadingLabel.maximumNumberOfLines = 1
        capsuleLeadingLabel.cell?.wraps = false
        capsuleLeadingLabel.cell?.usesSingleLineMode = true
        capsuleLeadingLabel.cell?.lineBreakMode = .byTruncatingTail
        capsuleBodyLabel.isHidden = true
        capsuleTrailingLabel.stringValue = tokenText
        capsuleTrailingLabel.isHidden = false
        capsuleSecondaryLabel.isHidden = true
        capsuleSecondaryTrailingLabel.isHidden = true
        capsuleSep1Label.isHidden = true
        capsuleSep2Label.isHidden = true
        capsuleSep3Label.isHidden = true
        usageProgressTrack.isHidden = true
        secondaryUsageProgressTrack.isHidden = true
    }

    func updatePrimaryCapsuleFrames(_ frames: [PrimaryCapsuleFrame]) {
        guard !usesFocusedWorktreeMode else { return }
        guard !frames.isEmpty else { return }
        primaryCapsuleFrames = frames
        currentPrimaryCapsuleIndex = min(currentPrimaryCapsuleIndex, frames.count - 1)
        showCurrentPrimaryCapsuleFrame()
        startTipRotationIfNeeded()
    }

    // MARK: - Setup

    private func setup() {
        wantsLayer = true
        layer?.backgroundColor = NSColor.clear.cgColor
        translatesAutoresizingMaskIntoConstraints = false
        setAccessibilityIdentifier("titlebar")

        setupLeftArcBlock()
        setupRightArcBlock()

        let rightWidth = rightArcBlock.widthAnchor.constraint(equalToConstant: Layout.rightCapsuleCompactWidth)
        rightArcWidthConstraint = rightWidth

        NSLayoutConstraint.activate([
            leftArcBlock.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 8),
            leftArcBlock.centerYAnchor.constraint(equalTo: centerYAnchor, constant: Layout.arcVerticalOffset),
            leftArcBlock.heightAnchor.constraint(equalToConstant: Layout.capsuleHeight),

            rightArcBlock.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -8),
            rightArcBlock.centerYAnchor.constraint(equalTo: centerYAnchor, constant: Layout.arcVerticalOffset),
            rightArcBlock.heightAnchor.constraint(equalToConstant: Layout.capsuleHeight),
            rightWidth,

            leftArcBlock.trailingAnchor.constraint(equalTo: rightArcBlock.leadingAnchor, constant: -8),
        ])

        showCurrentPrimaryCapsuleFrame()
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

        capsuleSecondaryLabel.font = NSFont.systemFont(ofSize: 11, weight: .regular)
        capsuleSecondaryLabel.textColor = SemanticColors.muted
        capsuleSecondaryLabel.lineBreakMode = .byTruncatingTail
        capsuleSecondaryLabel.setContentHuggingPriority(.required, for: .horizontal)
        capsuleSecondaryLabel.setContentCompressionResistancePriority(.required, for: .horizontal)

        capsuleSecondaryTrailingLabel.font = NSFont.systemFont(ofSize: 11, weight: .regular)
        capsuleSecondaryTrailingLabel.textColor = SemanticColors.muted
        capsuleSecondaryTrailingLabel.lineBreakMode = .byTruncatingTail
        capsuleSecondaryTrailingLabel.setContentHuggingPriority(.required, for: .horizontal)
        capsuleSecondaryTrailingLabel.setContentCompressionResistancePriority(.required, for: .horizontal)

        capsuleSep1Label.font = NSFont.systemFont(ofSize: 11, weight: .medium)
        capsuleSep1Label.textColor = SemanticColors.muted
        capsuleSep1Label.setContentHuggingPriority(.required, for: .horizontal)

        capsuleSep2Label.font = NSFont.systemFont(ofSize: 11, weight: .medium)
        capsuleSep2Label.textColor = SemanticColors.muted
        capsuleSep2Label.setContentHuggingPriority(.required, for: .horizontal)

        capsuleSep3Label.font = NSFont.systemFont(ofSize: 11, weight: .medium)
        capsuleSep3Label.textColor = SemanticColors.muted
        capsuleSep3Label.setContentHuggingPriority(.required, for: .horizontal)

        usageProgressTrack.wantsLayer = true
        usageProgressTrack.layer?.cornerRadius = 3
        usageProgressTrack.layer?.backgroundColor = NSColor(hex: 0x5E6A75).withAlphaComponent(0.35).cgColor
        usageProgressTrack.translatesAutoresizingMaskIntoConstraints = false
        usageProgressTrack.isHidden = true

        usageProgressFill.wantsLayer = true
        usageProgressFill.layer?.cornerRadius = 3
        usageProgressFill.layer?.backgroundColor = SemanticColors.accent.cgColor
        usageProgressFill.translatesAutoresizingMaskIntoConstraints = false
        usageProgressTrack.addSubview(usageProgressFill)

        NSLayoutConstraint.activate([
            usageProgressTrack.widthAnchor.constraint(equalToConstant: Layout.usageProgressWidth),
            usageProgressTrack.heightAnchor.constraint(equalToConstant: 10),
            usageProgressFill.leadingAnchor.constraint(equalTo: usageProgressTrack.leadingAnchor),
            usageProgressFill.topAnchor.constraint(equalTo: usageProgressTrack.topAnchor),
            usageProgressFill.bottomAnchor.constraint(equalTo: usageProgressTrack.bottomAnchor),
        ])
        usageProgressWidthConstraint = usageProgressFill.widthAnchor.constraint(equalToConstant: 0)
        usageProgressWidthConstraint?.isActive = true

        secondaryUsageProgressTrack.wantsLayer = true
        secondaryUsageProgressTrack.layer?.cornerRadius = 3
        secondaryUsageProgressTrack.layer?.backgroundColor = NSColor(hex: 0x5E6A75).withAlphaComponent(0.35).cgColor
        secondaryUsageProgressTrack.translatesAutoresizingMaskIntoConstraints = false
        secondaryUsageProgressTrack.isHidden = true

        secondaryUsageProgressFill.wantsLayer = true
        secondaryUsageProgressFill.layer?.cornerRadius = 3
        secondaryUsageProgressFill.layer?.backgroundColor = SemanticColors.accent.cgColor
        secondaryUsageProgressFill.translatesAutoresizingMaskIntoConstraints = false
        secondaryUsageProgressTrack.addSubview(secondaryUsageProgressFill)

        NSLayoutConstraint.activate([
            secondaryUsageProgressTrack.widthAnchor.constraint(equalToConstant: Layout.usageProgressWidth),
            secondaryUsageProgressTrack.heightAnchor.constraint(equalToConstant: 10),
            secondaryUsageProgressFill.leadingAnchor.constraint(equalTo: secondaryUsageProgressTrack.leadingAnchor),
            secondaryUsageProgressFill.topAnchor.constraint(equalTo: secondaryUsageProgressTrack.topAnchor),
            secondaryUsageProgressFill.bottomAnchor.constraint(equalTo: secondaryUsageProgressTrack.bottomAnchor),
        ])
        secondaryUsageProgressWidthConstraint = secondaryUsageProgressFill.widthAnchor.constraint(equalToConstant: 0)
        secondaryUsageProgressWidthConstraint?.isActive = true

        primaryCapsuleStack.orientation = .horizontal
        primaryCapsuleStack.spacing = 5
        primaryCapsuleStack.alignment = .centerY
        primaryCapsuleStack.translatesAutoresizingMaskIntoConstraints = false
        primaryCapsuleStack.addArrangedSubview(capsuleIconView)
        primaryCapsuleStack.addArrangedSubview(capsuleLeadingLabel)
        primaryCapsuleStack.addArrangedSubview(capsuleSep1Label)
        primaryCapsuleStack.addArrangedSubview(capsuleBodyLabel)
        primaryCapsuleStack.addArrangedSubview(usageProgressTrack)
        primaryCapsuleStack.addArrangedSubview(capsuleSep2Label)
        primaryCapsuleStack.addArrangedSubview(capsuleTrailingLabel)
        primaryCapsuleStack.addArrangedSubview(capsuleSep3Label)
        primaryCapsuleStack.addArrangedSubview(capsuleSecondaryLabel)
        primaryCapsuleStack.addArrangedSubview(secondaryUsageProgressTrack)
        primaryCapsuleStack.addArrangedSubview(capsuleSecondaryTrailingLabel)
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

        let actionStack = NSStackView()
        actionStack.orientation = .horizontal
        actionStack.spacing = 2
        actionStack.alignment = .centerY

        configureArcIconButton(themeButton, symbol: "circle.lefthalf.filled",
                               identifier: "titlebar.themeToggle", label: "Toggle Theme",
                               action: #selector(themeClicked))
        actionStack.addArrangedSubview(themeButton)

        configureArcIconButton(collapseSidebarButton, symbol: "sidebar.right",
                               identifier: "titlebar.collapseSidebar", label: "Toggle Sidebar",
                               action: #selector(collapseSidebarClicked))
        actionStack.addArrangedSubview(collapseSidebarButton)

        configureCleanWorktreeButton()

        let rightStack = NSStackView()
        rightStack.orientation = .horizontal
        rightStack.spacing = 6
        rightStack.alignment = .centerY
        rightStack.translatesAutoresizingMaskIntoConstraints = false
        rightStack.addArrangedSubview(cleanWorktreeButton)
        rightStack.addArrangedSubview(actionStack)
        rightArcBlock.addSubview(rightStack)

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

    private func configureCleanWorktreeButton() {
        cleanWorktreeButton.title = ""
        cleanWorktreeButton.bezelStyle = .recessed
        cleanWorktreeButton.isBordered = false
        cleanWorktreeButton.image = NSImage(systemSymbolName: "trash", accessibilityDescription: "Clean worktrees")?
            .withSymbolConfiguration(NSImage.SymbolConfiguration(pointSize: 13, weight: .medium))
        cleanWorktreeButton.imagePosition = .imageOnly
        cleanWorktreeButton.contentTintColor = SemanticColors.muted
        cleanWorktreeButton.isHidden = true
        cleanWorktreeButton.isEnabled = false
        cleanWorktreeButton.target = self
        cleanWorktreeButton.action = #selector(cleanWorktreeClicked)
        cleanWorktreeButton.translatesAutoresizingMaskIntoConstraints = false
        cleanWorktreeButton.setAccessibilityIdentifier("titlebar.cleanWorktree")
        cleanWorktreeButton.setAccessibilityLabel("Clean merged worktrees")
        cleanWorktreeButton.wantsLayer = true
        cleanWorktreeButton.layer?.cornerRadius = 7
        cleanWorktreeButton.layer?.backgroundColor = NSColor.clear.cgColor
        NSLayoutConstraint.activate([
            cleanWorktreeButton.widthAnchor.constraint(equalToConstant: 30),
            cleanWorktreeButton.heightAnchor.constraint(equalToConstant: 30),
        ])
        setupHoverTracking(for: cleanWorktreeButton)
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

    @objc private func themeClicked() {
        delegate?.titleBarDidToggleTheme()
    }

    @objc private func collapseSidebarClicked() {
        delegate?.titleBarDidRequestCollapseSidebar()
    }

    @objc private func cleanWorktreeClicked() {
        delegate?.titleBarDidRequestCleanMergedWorktrees()
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
        let hasSecondaryText = (
            !capsuleSecondaryLabel.stringValue.isEmpty
                || !capsuleSecondaryTrailingLabel.stringValue.isEmpty
                || !secondaryUsageProgressTrack.isHidden
        )
        capsuleSep1Label.isHidden = !hasBodyText
        capsuleBodyLabel.isHidden = !hasBodyText
        capsuleSep2Label.isHidden = !hasBodyText || !hasTrailingText
        capsuleTrailingLabel.isHidden = !hasTrailingText
        capsuleSep3Label.isHidden = !hasSecondaryText || (!hasBodyText && !hasTrailingText)
        capsuleSecondaryLabel.isHidden = capsuleSecondaryLabel.stringValue.isEmpty
        capsuleSecondaryTrailingLabel.isHidden = capsuleSecondaryTrailingLabel.stringValue.isEmpty
    }

    private func showCurrentPrimaryCapsuleFrame() {
        guard !primaryCapsuleFrames.isEmpty else { return }
        let frame = primaryCapsuleFrames[currentPrimaryCapsuleIndex]
        capsuleIconView.image = NSImage(systemSymbolName: frame.iconName, accessibilityDescription: frame.leadingText)
        capsuleIconView.contentTintColor = frame.kind == .usage ? SemanticColors.accent : SemanticColors.muted
        capsuleLeadingLabel.lineBreakMode = .byTruncatingTail
        capsuleLeadingLabel.maximumNumberOfLines = 1
        capsuleLeadingLabel.cell?.wraps = false
        capsuleLeadingLabel.cell?.usesSingleLineMode = true
        capsuleLeadingLabel.cell?.lineBreakMode = .byTruncatingTail
        capsuleLeadingLabel.attributedStringValue = NSAttributedString(
            string: frame.leadingText,
            attributes: [
                .font: NSFont.systemFont(ofSize: 11, weight: .semibold),
                .foregroundColor: SemanticColors.text
            ]
        )
        capsuleBodyLabel.stringValue = frame.bodyText
        capsuleTrailingLabel.stringValue = [frame.resetText, frame.trailingText]
            .compactMap { $0 }
            .filter { !$0.isEmpty }
            .joined(separator: "  ")
        capsuleSecondaryLabel.stringValue = frame.secondaryText
        capsuleSecondaryTrailingLabel.stringValue = frame.secondaryTrailingText
        usageProgressTrack.isHidden = frame.usageProgress == nil
        usageProgressWidthConstraint?.constant = Layout.usageProgressWidth * CGFloat(frame.usageProgress ?? 0)
        secondaryUsageProgressTrack.isHidden = frame.secondaryUsageProgress == nil
        secondaryUsageProgressWidthConstraint?.constant = Layout.usageProgressWidth * CGFloat(frame.secondaryUsageProgress ?? 0)
        updatePrimaryCapsuleSeparators()
        updateArcBlockColors()
    }

    private func startTipRotationIfNeeded() {
        // Rotation disabled: the primary capsule now shows the focused worktree
        // title + token placeholder via updateFocusedWorktree(title:tokenText:).
        // Global usage/tips will live in a bottom status bar.
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
        capsuleSecondaryLabel.textColor = SemanticColors.muted
        capsuleSecondaryTrailingLabel.textColor = SemanticColors.muted
        capsuleSep1Label.textColor = SemanticColors.muted
        capsuleSep2Label.textColor = SemanticColors.muted
        capsuleSep3Label.textColor = SemanticColors.muted
        usageProgressTrack.layer?.backgroundColor = NSColor(hex: 0x5E6A75).withAlphaComponent(0.35).cgColor
        usageProgressFill.layer?.backgroundColor = SemanticColors.accent.cgColor
        secondaryUsageProgressTrack.layer?.backgroundColor = NSColor(hex: 0x5E6A75).withAlphaComponent(0.35).cgColor
        secondaryUsageProgressFill.layer?.backgroundColor = SemanticColors.accent.cgColor
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
