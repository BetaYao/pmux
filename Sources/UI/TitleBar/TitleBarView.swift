import AppKit

protocol TitleBarDelegate: AnyObject {
    func titleBarDidToggleTheme()
    func titleBarDidRequestCollapseLeftColumn()
    func titleBarDidRequestCollapseRightColumn()
    func titleBarDidRequestCleanMergedWorktrees()
}

final class TitleBarView: NSView {
    enum Layout {
        static let barHeight: CGFloat = 45
        static let capsuleHeight: CGFloat = 37
        static let rightCapsuleCompactWidth: CGFloat = 76
        static let rightCapsuleCleanWidth: CGFloat = 112
        static let arcVerticalOffset: CGFloat = 2
    }

    weak var delegate: TitleBarDelegate?

    // MARK: - Subviews

    private let rightArcBlock = NSView()
    private let titleLabel = NSTextField(labelWithString: "")

    // Right controls — action group
    private let cleanWorktreeButton = NSButton()
    private let themeButton = NSButton()
    private let collapseLeftButton = NSButton()
    private let collapseRightButton = NSButton()
    private var rightArcWidthConstraint: NSLayoutConstraint?

    // State
    private var isWindowHovered = false
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

    func updateChromeState(isGridLayout: Bool, hasWorkspaces: Bool = true, canCleanWorktrees: Bool = false) {
        cleanWorktreeButton.isHidden = !canCleanWorktrees
        cleanWorktreeButton.isEnabled = canCleanWorktrees
        rightArcWidthConstraint?.constant = canCleanWorktrees
            ? Layout.rightCapsuleCleanWidth
            : Layout.rightCapsuleCompactWidth
        layoutSubtreeIfNeeded()
    }

    func updateNotificationSummary(entry: NotificationEntry?, unreadCount: Int) {}

    /// Collapse whitespace/newlines and cap a resolved worktree title to a
    /// label-length string (with an ellipsis) so it fits the title-bar label.
    static func clampTitle(_ title: String, limit: Int = 64) -> String {
        let collapsed = title
            .components(separatedBy: .whitespacesAndNewlines)
            .filter { !$0.isEmpty }
            .joined(separator: " ")
        guard collapsed.count > limit else { return collapsed }
        return collapsed.prefix(limit).trimmingCharacters(in: .whitespaces) + "\u{2026}"
    }

    func updateFocusedWorktree(title: String, tokenText: String = "\u{2014}") {
        titleLabel.stringValue = Self.clampTitle(title)
        titleLabel.toolTip = title
    }

    // MARK: - Setup

    private func setup() {
        wantsLayer = true
        layer?.backgroundColor = NSColor.clear.cgColor
        translatesAutoresizingMaskIntoConstraints = false
        setAccessibilityIdentifier("titlebar")

        setupTitleLabel()
        setupRightArcBlock()

        let rightWidth = rightArcBlock.widthAnchor.constraint(equalToConstant: Layout.rightCapsuleCompactWidth)
        rightArcWidthConstraint = rightWidth

        NSLayoutConstraint.activate([
            rightArcBlock.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -8),
            rightArcBlock.centerYAnchor.constraint(equalTo: centerYAnchor, constant: Layout.arcVerticalOffset),
            rightArcBlock.heightAnchor.constraint(equalToConstant: Layout.capsuleHeight),
            rightWidth,
        ])

        updateArcBlockColors()
    }

    private func setupTitleLabel() {
        titleLabel.font = NSFont.systemFont(ofSize: 12, weight: .semibold)
        titleLabel.textColor = SemanticColors.text
        titleLabel.lineBreakMode = .byTruncatingTail
        titleLabel.maximumNumberOfLines = 1
        titleLabel.cell?.usesSingleLineMode = true
        titleLabel.cell?.lineBreakMode = .byTruncatingTail
        titleLabel.alignment = .center
        titleLabel.translatesAutoresizingMaskIntoConstraints = false
        addSubview(titleLabel)

        NSLayoutConstraint.activate([
            titleLabel.centerXAnchor.constraint(equalTo: centerXAnchor),
            titleLabel.centerYAnchor.constraint(equalTo: centerYAnchor),
            titleLabel.leadingAnchor.constraint(greaterThanOrEqualTo: leadingAnchor, constant: 80),
            titleLabel.trailingAnchor.constraint(lessThanOrEqualTo: rightArcBlock.leadingAnchor, constant: -8),
        ])
    }

    private func setupRightArcBlock() {
        rightArcBlock.wantsLayer = true
        rightArcBlock.layer?.cornerRadius = 10
        rightArcBlock.layer?.backgroundColor = NSColor.clear.cgColor
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

        configureArcIconButton(collapseLeftButton, symbol: "sidebar.left",
                               identifier: "titlebar.collapseLeft", label: "Toggle Worktrees",
                               action: #selector(collapseLeftClicked))
        actionStack.addArrangedSubview(collapseLeftButton)

        configureArcIconButton(collapseRightButton, symbol: "sidebar.right",
                               identifier: "titlebar.collapseRight", label: "Toggle Side Panel",
                               action: #selector(collapseRightClicked))
        actionStack.addArrangedSubview(collapseRightButton)

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

    @objc private func collapseLeftClicked() { delegate?.titleBarDidRequestCollapseLeftColumn() }
    @objc private func collapseRightClicked() { delegate?.titleBarDidRequestCollapseRightColumn() }

    @objc private func cleanWorktreeClicked() {
        delegate?.titleBarDidRequestCleanMergedWorktrees()
    }

    // MARK: - State

    private func updateArcBlockColors() {
        rightArcBlock.layer?.backgroundColor = NSColor.clear.cgColor
    }

    // MARK: - Theme

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        applyColors()
    }

    private func applyColors() {
        let saved = NSAppearance.current
        NSAppearance.current = window?.effectiveAppearance ?? NSApp.effectiveAppearance
        titleLabel.textColor = SemanticColors.text
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
