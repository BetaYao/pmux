import AppKit

protocol DashboardDelegate: AnyObject {
    func dashboard(_ dashboard: DashboardViewController, didSelectWorktree info: WorktreeInfo, surface: TerminalSurface)
}

/// Dashboard view controller that manages Grid and Spotlight modes.
/// Grid mode: all worktree cards in a responsive grid.
/// Spotlight mode: one large terminal + sidebar of small cards.
class DashboardViewController: NSViewController {
    weak var dashboardDelegate: DashboardDelegate?

    enum Mode {
        case grid
        case spotlight(focusedIndex: Int)
    }

    private(set) var mode: Mode = .grid
    private var cards: [TerminalCardView] = []
    var cardCount: Int { cards.count }
    private var worktrees: [(info: WorktreeInfo, surface: TerminalSurface)] = []

    // Grid mode views
    private let scrollView = NSScrollView()
    private let gridContainer = NSView()

    // Spotlight mode views
    private let spotlightContainer = NSView()
    private let spotlightMainContainer = NSView()
    private let spotlightSidebar = NSScrollView()
    private let spotlightSidebarStack = NSStackView()
    private let openTabButton = NSButton()

    private let minCardWidth: CGFloat = 300
    private let minCardHeight: CGFloat = 200
    private let gridSpacing: CGFloat = 12

    override func loadView() {
        self.view = NSView()
        view.wantsLayer = true
        view.layer?.backgroundColor = Theme.background.cgColor

        setupGridView()
        setupSpotlightView()
        showGrid()
    }

    // MARK: - Setup

    private func setupGridView() {
        scrollView.translatesAutoresizingMaskIntoConstraints = false
        scrollView.hasVerticalScroller = true
        scrollView.hasHorizontalScroller = false
        scrollView.drawsBackground = false
        scrollView.borderType = .noBorder

        gridContainer.wantsLayer = true
        gridContainer.translatesAutoresizingMaskIntoConstraints = false
        scrollView.documentView = gridContainer

        view.addSubview(scrollView)

        NSLayoutConstraint.activate([
            scrollView.topAnchor.constraint(equalTo: view.topAnchor, constant: gridSpacing),
            scrollView.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: gridSpacing),
            scrollView.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -gridSpacing),
            scrollView.bottomAnchor.constraint(equalTo: view.bottomAnchor, constant: -gridSpacing),
        ])
    }

    private func setupSpotlightView() {
        spotlightContainer.translatesAutoresizingMaskIntoConstraints = false
        spotlightContainer.wantsLayer = true
        spotlightContainer.isHidden = true
        view.addSubview(spotlightContainer)

        // Main terminal area (left, large)
        spotlightMainContainer.wantsLayer = true
        spotlightMainContainer.layer?.cornerRadius = Theme.cardCornerRadius
        spotlightMainContainer.translatesAutoresizingMaskIntoConstraints = false
        spotlightContainer.addSubview(spotlightMainContainer)

        // Sidebar (right, small cards stacked)
        spotlightSidebar.translatesAutoresizingMaskIntoConstraints = false
        spotlightSidebar.hasVerticalScroller = true
        spotlightSidebar.scrollerStyle = .overlay
        spotlightSidebar.drawsBackground = false
        spotlightSidebar.borderType = .noBorder

        spotlightSidebarStack.orientation = .vertical
        spotlightSidebarStack.spacing = 8
        spotlightSidebarStack.alignment = .leading
        spotlightSidebarStack.translatesAutoresizingMaskIntoConstraints = false
        spotlightSidebar.documentView = spotlightSidebarStack

        spotlightContainer.addSubview(spotlightSidebar)

        let sidebarWidth: CGFloat = 200

        NSLayoutConstraint.activate([
            spotlightContainer.topAnchor.constraint(equalTo: view.topAnchor, constant: gridSpacing),
            spotlightContainer.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: gridSpacing),
            spotlightContainer.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -gridSpacing),
            spotlightContainer.bottomAnchor.constraint(equalTo: view.bottomAnchor, constant: -gridSpacing),

            spotlightMainContainer.topAnchor.constraint(equalTo: spotlightContainer.topAnchor),
            spotlightMainContainer.leadingAnchor.constraint(equalTo: spotlightContainer.leadingAnchor),
            spotlightMainContainer.bottomAnchor.constraint(equalTo: spotlightContainer.bottomAnchor),
            spotlightMainContainer.trailingAnchor.constraint(equalTo: spotlightSidebar.leadingAnchor, constant: -gridSpacing),

            spotlightSidebar.topAnchor.constraint(equalTo: spotlightContainer.topAnchor),
            spotlightSidebar.trailingAnchor.constraint(equalTo: spotlightContainer.trailingAnchor),
            spotlightSidebar.bottomAnchor.constraint(equalTo: spotlightContainer.bottomAnchor),
            spotlightSidebar.widthAnchor.constraint(equalToConstant: sidebarWidth),
        ])

        // "Open in Tab" button (top-right of spotlight main area)
        openTabButton.title = "Open in Tab ⌘↵"
        openTabButton.bezelStyle = .recessed
        openTabButton.isBordered = false
        openTabButton.font = NSFont.systemFont(ofSize: 11, weight: .medium)
        openTabButton.contentTintColor = Theme.accent
        openTabButton.target = self
        openTabButton.action = #selector(openTabButtonClicked)
        openTabButton.translatesAutoresizingMaskIntoConstraints = false
        openTabButton.isHidden = true
        spotlightContainer.addSubview(openTabButton)

        NSLayoutConstraint.activate([
            openTabButton.topAnchor.constraint(equalTo: spotlightMainContainer.topAnchor, constant: 4),
            openTabButton.trailingAnchor.constraint(equalTo: spotlightMainContainer.trailingAnchor, constant: -8),
        ])
    }

    // MARK: - Data

    func worktreeAt(index: Int) -> (info: WorktreeInfo, surface: TerminalSurface)? {
        guard index >= 0, index < worktrees.count else { return nil }
        return worktrees[index]
    }

    func updateStatus(for path: String, status: AgentStatus) {
        for card in cards where card.worktreeInfo.path == path {
            card.status = status
        }
    }

    func setWorktrees(_ worktrees: [(info: WorktreeInfo, surface: TerminalSurface)]) {
        self.worktrees = worktrees
        rebuildCards()
    }

    private func rebuildCards() {
        cards.forEach { $0.removeFromSuperview() }
        cards.removeAll()

        for (info, surface) in worktrees {
            let card = TerminalCardView(worktreeInfo: info, surface: surface)
            card.delegate = self
            card.status = .idle
            cards.append(card)
        }

        switch mode {
        case .grid:
            layoutGrid()
        case .spotlight(let index):
            layoutSpotlight(focusedIndex: index)
        }
    }

    // MARK: - Grid Layout

    private func showGrid() {
        mode = .grid
        scrollView.isHidden = false
        spotlightContainer.isHidden = true
        openTabButton.isHidden = true
        layoutGrid()
    }

    private func layoutGrid() {
        // Remove all cards from current parents and clear Auto Layout constraints from Spotlight
        for card in cards {
            card.removeFromSuperview()
            card.removeAllConstraintsFromSuperviews()
            card.translatesAutoresizingMaskIntoConstraints = true  // back to frame-based
        }

        guard !cards.isEmpty else { return }

        let availableWidth = scrollView.contentView.bounds.width
        let columns = max(1, Int(availableWidth / minCardWidth))
        let cardWidth = (availableWidth - gridSpacing * CGFloat(columns - 1)) / CGFloat(columns)
        let cardHeight = cardWidth * 0.6  // aspect ratio

        let rows = Int(ceil(Double(cards.count) / Double(columns)))
        let totalHeight = CGFloat(rows) * cardHeight + CGFloat(rows - 1) * gridSpacing

        gridContainer.frame = NSRect(x: 0, y: 0, width: availableWidth, height: max(totalHeight, scrollView.contentView.bounds.height))

        for (index, card) in cards.enumerated() {
            let col = index % columns
            let row = index / columns
            let x = CGFloat(col) * (cardWidth + gridSpacing)
            let y = totalHeight - CGFloat(row + 1) * cardHeight - CGFloat(row) * gridSpacing

            card.frame = NSRect(x: x, y: y, width: cardWidth, height: cardHeight)
            gridContainer.addSubview(card)
            card.embedTerminal()
        }
    }

    // MARK: - Spotlight Layout

    @objc private func openTabButtonClicked() {
        if case .spotlight(let index) = mode, let worktree = worktreeAt(index: index) {
            dashboardDelegate?.dashboard(self, didSelectWorktree: worktree.info, surface: worktree.surface)
        }
    }

    func enterSpotlight(focusedIndex: Int) {
        guard focusedIndex >= 0, focusedIndex < cards.count else { return }
        mode = .spotlight(focusedIndex: focusedIndex)
        scrollView.isHidden = true
        spotlightContainer.isHidden = false
        openTabButton.isHidden = false
        layoutSpotlight(focusedIndex: focusedIndex)
    }

    private func layoutSpotlight(focusedIndex: Int) {
        // Remove all cards from parents
        cards.forEach { $0.removeFromSuperview() }
        spotlightSidebarStack.arrangedSubviews.forEach { $0.removeFromSuperview() }

        guard focusedIndex < cards.count else { return }

        // Main card — reparent terminal to spotlight main area
        let mainCard = cards[focusedIndex]
        mainCard.surface.reparent(to: spotlightMainContainer)

        // Make the spotlight terminal interactive
        if let window = view.window {
            window.makeFirstResponder(mainCard.surface.view)
        }

        // Sidebar cards — small versions of other terminals
        let sidebarWidth: CGFloat = 200
        for (index, card) in cards.enumerated() {
            guard index != focusedIndex else { continue }
            card.translatesAutoresizingMaskIntoConstraints = false
            spotlightSidebarStack.addArrangedSubview(card)

            NSLayoutConstraint.activate([
                card.widthAnchor.constraint(equalToConstant: sidebarWidth),
                card.heightAnchor.constraint(equalToConstant: 120),
            ])
            card.embedTerminal()
        }
    }

    func exitSpotlight() {
        showGrid()
    }

    /// Re-embed terminals in their grid cards after returning from a repo tab
    func refreshAfterReturn() {
        switch mode {
        case .grid:
            layoutGrid()
        case .spotlight(let index):
            layoutSpotlight(focusedIndex: index)
        }
    }

    // MARK: - Resize

    override func viewDidLayout() {
        super.viewDidLayout()
        switch mode {
        case .grid:
            layoutGrid()
        case .spotlight:
            break  // Auto Layout handles spotlight
        }
    }
}

// MARK: - TerminalCardDelegate

// MARK: - NSView helper

private extension NSView {
    /// Remove all constraints that reference this view from its superview
    func removeAllConstraintsFromSuperviews() {
        // Remove constraints owned by this view that reference only itself (width/height)
        for constraint in constraints {
            if constraint.firstItem === self && constraint.secondItem == nil {
                removeConstraint(constraint)
            }
        }
        // Remove constraints from superview that reference this view
        if let superview = superview {
            for constraint in superview.constraints {
                if constraint.firstItem === self || constraint.secondItem === self {
                    superview.removeConstraint(constraint)
                }
            }
        }
    }
}

// MARK: - TerminalCardDelegate

extension DashboardViewController: TerminalCardDelegate {
    func terminalCardClicked(_ card: TerminalCardView) {
        guard let index = cards.firstIndex(where: { $0 === card }) else { return }

        switch mode {
        case .grid:
            enterSpotlight(focusedIndex: index)
        case .spotlight:
            // Clicked a sidebar card — swap it to spotlight
            enterSpotlight(focusedIndex: index)
        }
    }

    func terminalCardDoubleClicked(_ card: TerminalCardView) {
        guard let index = cards.firstIndex(where: { $0 === card }) else { return }
        let worktree = worktrees[index]
        dashboardDelegate?.dashboard(self, didSelectWorktree: worktree.info, surface: worktree.surface)
    }
}
