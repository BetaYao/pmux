import AppKit

protocol TitleBarDelegate: AnyObject {
    func titleBarDidToggleTheme()
    func titleBarDidRequestCollapseLeftColumn()
    func titleBarDidRequestCollapseRightColumn()
    func titleBarDidRequestCleanMergedWorktrees()
    func titleBarDidRequestShowFiles()
    func titleBarDidRequestShowChanges()
    func titleBarDidSelectWorktree(_ path: String)
}

final class TitleBarView: NSView {
    enum Layout {
        static let barHeight: CGFloat = 38
        static let capsuleHeight: CGFloat = 24
        static let arcVerticalOffset: CGFloat = 1
        /// Right edge of the dashboard's first (worktree) column — keeps the
        /// collapse-left icon aligned with that column. = edge(8) + leftColumnWidth(260).
        static let firstColumnRightEdge: CGFloat = 268
        /// With a unified toolbar, the title-bar accessory's content origin is
        /// inset past the traffic-light region. Subtract it so window-x math lines up.
        static let toolbarLeadingInset: CGFloat = 76
    }

    weak var delegate: TitleBarDelegate?

    // MARK: - Subviews

    private let rightArcBlock = NSView()
    private let titleLabel = NSTextField(labelWithString: "")
    private let pathLabel = NSTextField(labelWithString: "")
    private let titleStack = NSStackView()

    // Left control — collapse the worktrees sidebar. Aligned to the first
    // column's right edge when expanded; tucked next to the traffic lights when collapsed.
    private let collapseLeftButton = NSButton()
    private var collapseLeftExpandedConstraint: NSLayoutConstraint?
    private var collapseLeftCollapsedConstraint: NSLayoutConstraint?

    // Right controls — action group: file, changes, clean, theme, collapse-right.
    private let filesButton = NSButton()
    private let changesButton = NSButton()
    private let cleanWorktreeButton = NSButton()
    private let themeButton = NSButton()
    private let collapseRightButton = NSButton()

    // Worktree tab strip
    private let tabStripClipView = NSView()
    private let tabStripStack = NSStackView()
    private var worktreeTabPaths: [String] = []

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

    func updateFocusedWorktree(title: String, path: String = "", tokenText: String = "\u{2014}") {
        titleLabel.stringValue = Self.clampTitle(title)
        titleLabel.toolTip = title
        // Abbreviate the home directory so the second line stays readable.
        let display = path.isEmpty ? "" : (path as NSString).abbreviatingWithTildeInPath
        pathLabel.stringValue = display
        pathLabel.toolTip = path
        pathLabel.isHidden = display.isEmpty
    }

    // MARK: - Setup

    private func setup() {
        wantsLayer = true
        layer?.backgroundColor = NSColor.clear.cgColor
        translatesAutoresizingMaskIntoConstraints = false
        setAccessibilityIdentifier("titlebar")

        // rightArcBlock must be added to the view hierarchy before
        // setupTitleLabel(), which activates a constraint between titleLabel
        // and rightArcBlock — they need a common ancestor to be active.
        setupLeftButton()
        setupRightArcBlock()
        setupTitleLabel()

        NSLayoutConstraint.activate([
            rightArcBlock.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -8),
            rightArcBlock.centerYAnchor.constraint(equalTo: centerYAnchor, constant: Layout.arcVerticalOffset),
            rightArcBlock.heightAnchor.constraint(equalToConstant: Layout.capsuleHeight),
        ])

        setupTabStrip()
        updateArcBlockColors()
    }

    private func setupTabStrip() {
        tabStripStack.orientation = .horizontal
        tabStripStack.spacing = 2
        tabStripStack.alignment = .centerY
        tabStripStack.translatesAutoresizingMaskIntoConstraints = false

        tabStripClipView.wantsLayer = true
        tabStripClipView.layer?.masksToBounds = true
        tabStripClipView.translatesAutoresizingMaskIntoConstraints = false
        tabStripClipView.isHidden = true
        addSubview(tabStripClipView)
        tabStripClipView.addSubview(tabStripStack)

        NSLayoutConstraint.activate([
            tabStripClipView.leadingAnchor.constraint(equalTo: collapseLeftButton.trailingAnchor, constant: 8),
            tabStripClipView.trailingAnchor.constraint(lessThanOrEqualTo: rightArcBlock.leadingAnchor, constant: -8),
            tabStripClipView.centerYAnchor.constraint(equalTo: centerYAnchor, constant: Layout.arcVerticalOffset),
            tabStripClipView.heightAnchor.constraint(equalToConstant: 22),
            tabStripStack.leadingAnchor.constraint(equalTo: tabStripClipView.leadingAnchor),
            tabStripStack.centerYAnchor.constraint(equalTo: tabStripClipView.centerYAnchor),
        ])
    }

    func setWorktreeTabs(_ tabs: [(path: String, title: String, statusColor: NSColor, isSelected: Bool)]) {
        worktreeTabPaths = tabs.map(\.path)
        tabStripStack.arrangedSubviews.forEach { $0.removeFromSuperview() }

        for tab in tabs {
            let btn = WorktreeTabButton(path: tab.path, title: tab.title, statusColor: tab.statusColor, isSelected: tab.isSelected)
            btn.onTap = { [weak self] path in
                self?.delegate?.titleBarDidSelectWorktree(path)
            }
            tabStripStack.addArrangedSubview(btn)
        }

        let hasTabs = !tabs.isEmpty
        tabStripClipView.isHidden = !hasTabs
        titleStack.isHidden = hasTabs
    }

    private func setupLeftButton() {
        configureArcIconButton(collapseLeftButton, symbol: "sidebar.left",
                               identifier: "titlebar.collapseLeft", label: "Toggle Worktrees",
                               action: #selector(collapseLeftClicked))
        addSubview(collapseLeftButton)
        // Expanded: trailing aligns to the first column's right edge.
        // Collapsed: leading tucks next to the traffic lights (titleBar x≈0).
        collapseLeftExpandedConstraint = collapseLeftButton.trailingAnchor.constraint(
            equalTo: leadingAnchor, constant: Layout.firstColumnRightEdge - Layout.toolbarLeadingInset)
        collapseLeftCollapsedConstraint = collapseLeftButton.leadingAnchor.constraint(
            equalTo: leadingAnchor, constant: 4)
        collapseLeftExpandedConstraint?.isActive = true
        NSLayoutConstraint.activate([
            collapseLeftButton.centerYAnchor.constraint(equalTo: centerYAnchor, constant: Layout.arcVerticalOffset),
        ])
    }

    /// Reposition the collapse-left icon: tucked by the traffic lights when the
    /// worktree column is collapsed, aligned to that column's right edge otherwise.
    func setLeftColumnCollapsed(_ collapsed: Bool) {
        collapseLeftExpandedConstraint?.isActive = !collapsed
        collapseLeftCollapsedConstraint?.isActive = collapsed
        layoutSubtreeIfNeeded()
    }

    private func setupTitleLabel() {
        titleLabel.font = NSFont.systemFont(ofSize: 12, weight: .semibold)
        titleLabel.textColor = SemanticColors.text
        titleLabel.lineBreakMode = .byTruncatingTail
        titleLabel.maximumNumberOfLines = 1
        titleLabel.cell?.usesSingleLineMode = true
        titleLabel.cell?.lineBreakMode = .byTruncatingTail
        titleLabel.alignment = .center

        // Second line: full path of the current worktree, dimmer and smaller.
        pathLabel.font = NSFont.systemFont(ofSize: 10, weight: .regular)
        pathLabel.textColor = SemanticColors.muted
        pathLabel.lineBreakMode = .byTruncatingMiddle
        pathLabel.maximumNumberOfLines = 1
        pathLabel.cell?.usesSingleLineMode = true
        pathLabel.cell?.lineBreakMode = .byTruncatingMiddle
        pathLabel.alignment = .center

        titleStack.orientation = .vertical
        titleStack.alignment = .centerX
        titleStack.spacing = 1
        titleStack.translatesAutoresizingMaskIntoConstraints = false
        titleStack.addArrangedSubview(titleLabel)
        titleStack.addArrangedSubview(pathLabel)
        addSubview(titleStack)

        NSLayoutConstraint.activate([
            titleStack.centerXAnchor.constraint(equalTo: centerXAnchor),
            titleStack.centerYAnchor.constraint(equalTo: centerYAnchor, constant: Layout.arcVerticalOffset),
            titleStack.leadingAnchor.constraint(greaterThanOrEqualTo: collapseLeftButton.trailingAnchor, constant: 8),
            titleStack.trailingAnchor.constraint(lessThanOrEqualTo: rightArcBlock.leadingAnchor, constant: -8),
        ])
    }

    private func setupRightArcBlock() {
        rightArcBlock.wantsLayer = true
        rightArcBlock.layer?.cornerRadius = 10
        rightArcBlock.layer?.backgroundColor = NSColor.clear.cgColor
        rightArcBlock.translatesAutoresizingMaskIntoConstraints = false
        addSubview(rightArcBlock)

        // Order: file, changes, clean, theme, collapse-right.
        configureArcIconButton(filesButton, symbol: "doc.text",
                               identifier: "titlebar.files", label: "Browse Files",
                               action: #selector(filesClicked))

        configureArcIconButton(changesButton, symbol: "plusminus",
                               identifier: "titlebar.changes", label: "Show Changes",
                               action: #selector(changesClicked))

        configureCleanWorktreeButton()

        configureArcIconButton(themeButton, symbol: "circle.lefthalf.filled",
                               identifier: "titlebar.themeToggle", label: "Toggle Theme",
                               action: #selector(themeClicked))

        configureArcIconButton(collapseRightButton, symbol: "sidebar.right",
                               identifier: "titlebar.collapseRight", label: "Toggle Side Panel",
                               action: #selector(collapseRightClicked))

        let rightStack = NSStackView()
        rightStack.orientation = .horizontal
        rightStack.spacing = 2
        rightStack.alignment = .centerY
        rightStack.translatesAutoresizingMaskIntoConstraints = false
        rightStack.addArrangedSubview(filesButton)
        rightStack.addArrangedSubview(changesButton)
        rightStack.addArrangedSubview(cleanWorktreeButton)
        rightStack.addArrangedSubview(themeButton)
        rightStack.addArrangedSubview(collapseRightButton)
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
        let config = NSImage.SymbolConfiguration(pointSize: 12, weight: .medium)
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
            button.widthAnchor.constraint(equalToConstant: 24),
            button.heightAnchor.constraint(equalToConstant: 24),
        ])
        setupHoverTracking(for: button)
    }

    private func configureCleanWorktreeButton() {
        cleanWorktreeButton.title = ""
        cleanWorktreeButton.bezelStyle = .recessed
        cleanWorktreeButton.isBordered = false
        cleanWorktreeButton.image = NSImage(systemSymbolName: "trash", accessibilityDescription: "Clean worktrees")?
            .withSymbolConfiguration(NSImage.SymbolConfiguration(pointSize: 12, weight: .medium))
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
            cleanWorktreeButton.widthAnchor.constraint(equalToConstant: 24),
            cleanWorktreeButton.heightAnchor.constraint(equalToConstant: 24),
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
    @objc private func filesClicked() { delegate?.titleBarDidRequestShowFiles() }
    @objc private func changesClicked() { delegate?.titleBarDidRequestShowChanges() }

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
        pathLabel.textColor = SemanticColors.muted
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

// MARK: - WorktreeTabButton

private final class WorktreeTabButton: NSView {
    var onTap: ((String) -> Void)?
    private let path: String
    private let dotView = NSView()
    private let label = NSTextField(labelWithString: "")
    private var hovered = false

    init(path: String, title: String, statusColor: NSColor, isSelected: Bool) {
        self.path = path
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false
        wantsLayer = true
        layer?.cornerRadius = 5

        dotView.wantsLayer = true
        dotView.layer?.cornerRadius = 3
        dotView.layer?.backgroundColor = statusColor.cgColor
        dotView.translatesAutoresizingMaskIntoConstraints = false

        label.font = NSFont.systemFont(ofSize: 11, weight: .medium)
        label.textColor = isSelected ? SemanticColors.text : SemanticColors.muted
        label.lineBreakMode = .byTruncatingTail
        label.maximumNumberOfLines = 1
        label.cell?.usesSingleLineMode = true
        label.translatesAutoresizingMaskIntoConstraints = false
        label.stringValue = title

        addSubview(dotView)
        addSubview(label)

        NSLayoutConstraint.activate([
            dotView.widthAnchor.constraint(equalToConstant: 6),
            dotView.heightAnchor.constraint(equalToConstant: 6),
            dotView.centerYAnchor.constraint(equalTo: centerYAnchor),
            dotView.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 6),
            label.leadingAnchor.constraint(equalTo: dotView.trailingAnchor, constant: 4),
            label.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -6),
            label.centerYAnchor.constraint(equalTo: centerYAnchor),
            heightAnchor.constraint(equalToConstant: 22),
        ])

        applySelectedStyle(isSelected)

        let click = NSClickGestureRecognizer(target: self, action: #selector(handleClick))
        addGestureRecognizer(click)

        updateTrackingAreas()
    }

    required init?(coder: NSCoder) { fatalError() }

    private func applySelectedStyle(_ selected: Bool) {
        layer?.backgroundColor = selected
            ? NSColor.white.withAlphaComponent(0.12).cgColor
            : NSColor.clear.cgColor
    }

    @objc private func handleClick() {
        onTap?(path)
    }

    private var trackingArea: NSTrackingArea?

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        if let existing = trackingArea { removeTrackingArea(existing) }
        let area = NSTrackingArea(rect: bounds, options: [.mouseEnteredAndExited, .activeInActiveApp], owner: self, userInfo: nil)
        addTrackingArea(area)
        trackingArea = area
    }

    override func mouseEntered(with event: NSEvent) {
        hovered = true
        layer?.backgroundColor = NSColor.white.withAlphaComponent(0.08).cgColor
    }

    override func mouseExited(with event: NSEvent) {
        hovered = false
        applySelectedStyle(label.textColor == SemanticColors.text)
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
