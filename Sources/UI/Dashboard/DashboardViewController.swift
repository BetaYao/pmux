import AppKit

// MARK: - DashboardDelegate

protocol DashboardDelegate: AnyObject {
    func dashboardDidSelectProject(_ project: String, thread: String)
    func dashboardDidRequestEnterProject(_ project: String)
    func dashboardDidReorderCards(order: [String])
    func dashboardDidRequestDeleteWorktree(_ path: String)
}

// MARK: - AgentDisplayInfo

struct AgentDisplayInfo {
    let id: String          // worktree path
    let name: String        // display name like "Agent-Alpha"
    let project: String     // repo display name
    let thread: String      // branch name
    let status: String      // "running", "waiting", "idle", "error"
    let lastMessage: String
    let totalDuration: String   // "HH:MM:SS" format
    let roundDuration: String   // "HH:MM:SS" format
    let surface: TerminalSurface
}

// MARK: - Pasteboard type (used by DraggableGridView)

extension NSPasteboard.PasteboardType {
    static let terminalCard = NSPasteboard.PasteboardType("com.pmux.terminalCard")
}

// MARK: - DashboardViewController

class DashboardViewController: NSViewController, AgentCardDelegate, FocusPanelDelegate, DraggableGridDelegate {
    weak var dashboardDelegate: DashboardDelegate?

    var currentLayout: DashboardLayout = .leftRight
    var selectedAgentId: String = ""
    private(set) var zoomIndex: Int = GridLayout.defaultZoomIndex

    // Data
    private var agents: [AgentDisplayInfo] = []

    // Grid layout
    private let gridScrollView = NSScrollView()
    private let gridContainer = DraggableGridView()
    private var gridCards: [AgentCardView] = []

    private let gridSpacing: CGFloat = 3
    private let aspectRatio: CGFloat = 0.5625

    // Left-Right layout
    private let leftRightContainer = NSView()
    private let leftRightFocusPanel = FocusPanelView()
    private let leftRightSidebarScroll = NSScrollView()
    private let leftRightSidebarStack = NSStackView()
    private var leftRightMiniCards: [MiniCardView] = []

    // Top-Small layout
    private let topSmallContainer = NSView()
    private let topSmallFocusPanel = FocusPanelView()
    private let topSmallTopScroll = NSScrollView()
    private let topSmallTopStack = NSStackView()
    private var topSmallMiniCards: [MiniCardView] = []

    // Top-Large layout
    private let topLargeContainer = NSView()
    private let topLargeFocusPanel = FocusPanelView()
    private let topLargeBottomScroll = NSScrollView()
    private let topLargeBottomStack = NSStackView()
    private var topLargeMiniCards: [MiniCardView] = []

    private var currentMinCardWidth: CGFloat {
        GridLayout.zoomLevels[zoomIndex]
    }

    // MARK: - View lifecycle

    override func loadView() {
        let root = NSView()
        root.wantsLayer = true
        root.layer?.backgroundColor = SemanticColors.panel.cgColor
        root.setAccessibilityIdentifier("dashboard.view")
        self.view = root

        setupGridLayout()
        setupLeftRightLayout()
        setupTopSmallLayout()
        setupTopLargeLayout()

        showLayout(currentLayout)
    }

    // MARK: - Public API

    func updateAgents(_ newAgents: [AgentDisplayInfo]) {
        let oldIds = Set(agents.map { $0.id })
        let newIds = Set(newAgents.map { $0.id })
        let structureChanged = oldIds != newIds

        agents = newAgents

        // Validate selectedAgentId
        if !agents.contains(where: { $0.id == selectedAgentId }) {
            selectedAgentId = sortedAgents().first?.id ?? ""
        }

        if structureChanged {
            rebuildCurrentLayout()
        } else {
            updateCurrentLayoutInPlace()
        }
    }

    /// Update existing views in-place without rebuilding the view hierarchy
    private func updateCurrentLayoutInPlace() {
        let sorted = sortedAgents()
        switch currentLayout {
        case .grid:
            updateGridInPlace(sorted)
        case .leftRight:
            updateFocusLayoutInPlace(sorted, miniCards: leftRightMiniCards, focusPanel: leftRightFocusPanel)
        case .topSmall:
            updateFocusLayoutInPlace(sorted, miniCards: topSmallMiniCards, focusPanel: topSmallFocusPanel)
        case .topLarge:
            updateFocusLayoutInPlace(sorted, miniCards: topLargeMiniCards, focusPanel: topLargeFocusPanel)
        }
    }

    private func updateGridInPlace(_ sorted: [AgentDisplayInfo]) {
        guard sorted.count == gridCards.count else {
            rebuildGrid()
            return
        }
        for (index, agent) in sorted.enumerated() {
            gridCards[index].configure(
                id: agent.id,
                project: agent.project,
                thread: agent.thread,
                status: agent.status,
                lastMessage: agent.lastMessage,
                totalDuration: agent.totalDuration,
                roundDuration: agent.roundDuration
            )
        }
    }

    private func updateFocusLayoutInPlace(_ sorted: [AgentDisplayInfo], miniCards: [MiniCardView], focusPanel: FocusPanelView) {
        guard sorted.count == miniCards.count else {
            rebuildCurrentLayout()
            return
        }
        // Update focus panel
        if let selected = sorted.first(where: { $0.id == selectedAgentId }) ?? sorted.first {
            configureFocusPanel(focusPanel, with: selected)
        }
        // Update mini cards
        for (index, agent) in sorted.enumerated() {
            miniCards[index].configure(
                id: agent.id,
                project: agent.project,
                thread: agent.thread,
                status: agent.status,
                lastMessage: agent.lastMessage,
                totalDuration: agent.totalDuration,
                roundDuration: agent.roundDuration
            )
            miniCards[index].isSelected = (agent.id == selectedAgentId)
        }
    }

    func setLayout(_ layout: DashboardLayout) {
        guard layout != currentLayout else { return }
        detachTerminals()
        currentLayout = layout
        showLayout(layout)
        rebuildCurrentLayout()
    }

    func zoomIn() {
        setZoomIndex(zoomIndex - 1)
    }

    func zoomOut() {
        setZoomIndex(zoomIndex + 1)
    }

    func detachTerminals() {
        // Detach all terminal surfaces from focus panels
        leftRightFocusPanel.terminalContainer.subviews.forEach { $0.removeFromSuperview() }
        topSmallFocusPanel.terminalContainer.subviews.forEach { $0.removeFromSuperview() }
        topLargeFocusPanel.terminalContainer.subviews.forEach { $0.removeFromSuperview() }
    }

    // MARK: - Sorting

    private func sortedAgents() -> [AgentDisplayInfo] {
        agents.sorted { a, b in
            statusOrder(a.status) < statusOrder(b.status)
        }
    }

    private func statusOrder(_ status: String) -> Int {
        switch status.lowercased() {
        case "waiting": return 0
        case "running": return 1
        default: return 2
        }
    }

    // MARK: - Layout visibility

    private func showLayout(_ layout: DashboardLayout) {
        gridScrollView.isHidden = true
        leftRightContainer.isHidden = true
        topSmallContainer.isHidden = true
        topLargeContainer.isHidden = true

        switch layout {
        case .grid:
            gridScrollView.isHidden = false
        case .leftRight:
            leftRightContainer.isHidden = false
        case .topSmall:
            topSmallContainer.isHidden = false
        case .topLarge:
            topLargeContainer.isHidden = false
        }
    }

    private func rebuildCurrentLayout() {
        switch currentLayout {
        case .grid:
            rebuildGrid()
        case .leftRight:
            rebuildLeftRight()
        case .topSmall:
            rebuildTopSmall()
        case .topLarge:
            rebuildTopLarge()
        }
    }

    // MARK: - Zoom

    func setZoomIndex(_ index: Int) {
        zoomIndex = GridLayout.clampZoomIndex(index)
        if case .grid = currentLayout {
            rebuildGrid()
        }
    }

    // MARK: - Setup: Grid

    private func setupGridLayout() {
        gridScrollView.translatesAutoresizingMaskIntoConstraints = false
        gridScrollView.hasVerticalScroller = true
        gridScrollView.hasHorizontalScroller = false
        gridScrollView.drawsBackground = false
        gridScrollView.borderType = .noBorder

        gridContainer.wantsLayer = true
        gridContainer.translatesAutoresizingMaskIntoConstraints = false
        gridContainer.setAccessibilityIdentifier("dashboard.layout.grid")
        gridContainer.dragDelegate = self
        gridScrollView.documentView = gridContainer

        view.addSubview(gridScrollView)

        NSLayoutConstraint.activate([
            gridScrollView.topAnchor.constraint(equalTo: view.topAnchor, constant: gridSpacing),
            gridScrollView.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: gridSpacing),
            gridScrollView.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -gridSpacing),
            gridScrollView.bottomAnchor.constraint(equalTo: view.bottomAnchor, constant: -gridSpacing),
        ])
    }

    // MARK: - Setup: Left-Right

    private func setupLeftRightLayout() {
        leftRightContainer.translatesAutoresizingMaskIntoConstraints = false
        leftRightContainer.wantsLayer = true
        leftRightContainer.isHidden = true
        leftRightContainer.setAccessibilityIdentifier("dashboard.layout.left-right")
        view.addSubview(leftRightContainer)

        // Focus panel (left, 78%)
        leftRightFocusPanel.translatesAutoresizingMaskIntoConstraints = false
        leftRightFocusPanel.delegate = self
        leftRightContainer.addSubview(leftRightFocusPanel)

        // Sidebar scroll (right, 22%)
        leftRightSidebarScroll.translatesAutoresizingMaskIntoConstraints = false
        leftRightSidebarScroll.hasVerticalScroller = true
        leftRightSidebarScroll.scrollerStyle = .overlay
        leftRightSidebarScroll.drawsBackground = false
        leftRightSidebarScroll.borderType = .noBorder

        leftRightSidebarStack.orientation = .vertical
        leftRightSidebarStack.spacing = 8
        leftRightSidebarStack.alignment = .leading
        leftRightSidebarStack.translatesAutoresizingMaskIntoConstraints = false
        leftRightSidebarScroll.documentView = leftRightSidebarStack

        leftRightContainer.addSubview(leftRightSidebarScroll)

        let spacing: CGFloat = 8

        NSLayoutConstraint.activate([
            leftRightContainer.topAnchor.constraint(equalTo: view.topAnchor, constant: spacing),
            leftRightContainer.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: spacing),
            leftRightContainer.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -spacing),
            leftRightContainer.bottomAnchor.constraint(equalTo: view.bottomAnchor, constant: -spacing),

            leftRightFocusPanel.topAnchor.constraint(equalTo: leftRightContainer.topAnchor),
            leftRightFocusPanel.leadingAnchor.constraint(equalTo: leftRightContainer.leadingAnchor),
            leftRightFocusPanel.bottomAnchor.constraint(equalTo: leftRightContainer.bottomAnchor),
            leftRightFocusPanel.widthAnchor.constraint(equalTo: leftRightContainer.widthAnchor, multiplier: 0.78, constant: -spacing / 2),

            leftRightSidebarScroll.topAnchor.constraint(equalTo: leftRightContainer.topAnchor),
            leftRightSidebarScroll.trailingAnchor.constraint(equalTo: leftRightContainer.trailingAnchor),
            leftRightSidebarScroll.bottomAnchor.constraint(equalTo: leftRightContainer.bottomAnchor),
            leftRightSidebarScroll.leadingAnchor.constraint(equalTo: leftRightFocusPanel.trailingAnchor, constant: spacing),
        ])
    }

    // MARK: - Setup: Top-Small

    private func setupTopSmallLayout() {
        topSmallContainer.translatesAutoresizingMaskIntoConstraints = false
        topSmallContainer.wantsLayer = true
        topSmallContainer.isHidden = true
        topSmallContainer.setAccessibilityIdentifier("dashboard.layout.top-small")
        view.addSubview(topSmallContainer)

        // Top: horizontal scrolling row of mini cards
        topSmallTopScroll.translatesAutoresizingMaskIntoConstraints = false
        topSmallTopScroll.hasVerticalScroller = false
        topSmallTopScroll.hasHorizontalScroller = true
        topSmallTopScroll.scrollerStyle = .overlay
        topSmallTopScroll.drawsBackground = false
        topSmallTopScroll.borderType = .noBorder

        topSmallTopStack.orientation = .horizontal
        topSmallTopStack.spacing = 8
        topSmallTopStack.alignment = .centerY
        topSmallTopStack.translatesAutoresizingMaskIntoConstraints = false
        topSmallTopScroll.documentView = topSmallTopStack

        topSmallContainer.addSubview(topSmallTopScroll)

        // Bottom: focus panel
        topSmallFocusPanel.translatesAutoresizingMaskIntoConstraints = false
        topSmallFocusPanel.delegate = self
        topSmallContainer.addSubview(topSmallFocusPanel)

        let spacing: CGFloat = 8
        // Mini card height in top-small: derive from clamped width range 180-260 at 16:9
        let miniCardHeight: CGFloat = 120

        NSLayoutConstraint.activate([
            topSmallContainer.topAnchor.constraint(equalTo: view.topAnchor, constant: spacing),
            topSmallContainer.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: spacing),
            topSmallContainer.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -spacing),
            topSmallContainer.bottomAnchor.constraint(equalTo: view.bottomAnchor, constant: -spacing),

            topSmallTopScroll.topAnchor.constraint(equalTo: topSmallContainer.topAnchor),
            topSmallTopScroll.leadingAnchor.constraint(equalTo: topSmallContainer.leadingAnchor),
            topSmallTopScroll.trailingAnchor.constraint(equalTo: topSmallContainer.trailingAnchor),
            topSmallTopScroll.heightAnchor.constraint(equalToConstant: miniCardHeight),

            topSmallFocusPanel.topAnchor.constraint(equalTo: topSmallTopScroll.bottomAnchor, constant: spacing),
            topSmallFocusPanel.leadingAnchor.constraint(equalTo: topSmallContainer.leadingAnchor),
            topSmallFocusPanel.trailingAnchor.constraint(equalTo: topSmallContainer.trailingAnchor),
            topSmallFocusPanel.bottomAnchor.constraint(equalTo: topSmallContainer.bottomAnchor),
        ])
    }

    // MARK: - Setup: Top-Large

    private func setupTopLargeLayout() {
        topLargeContainer.translatesAutoresizingMaskIntoConstraints = false
        topLargeContainer.wantsLayer = true
        topLargeContainer.isHidden = true
        topLargeContainer.setAccessibilityIdentifier("dashboard.layout.top-large")
        view.addSubview(topLargeContainer)

        // Top: focus panel
        topLargeFocusPanel.translatesAutoresizingMaskIntoConstraints = false
        topLargeFocusPanel.delegate = self
        topLargeContainer.addSubview(topLargeFocusPanel)

        // Bottom: horizontal scrolling row of mini cards
        topLargeBottomScroll.translatesAutoresizingMaskIntoConstraints = false
        topLargeBottomScroll.hasVerticalScroller = false
        topLargeBottomScroll.hasHorizontalScroller = true
        topLargeBottomScroll.scrollerStyle = .overlay
        topLargeBottomScroll.drawsBackground = false
        topLargeBottomScroll.borderType = .noBorder

        topLargeBottomStack.orientation = .horizontal
        topLargeBottomStack.spacing = 8
        topLargeBottomStack.alignment = .centerY
        topLargeBottomStack.translatesAutoresizingMaskIntoConstraints = false
        topLargeBottomScroll.documentView = topLargeBottomStack

        topLargeContainer.addSubview(topLargeBottomScroll)

        let spacing: CGFloat = 8
        let miniCardHeight: CGFloat = 120

        NSLayoutConstraint.activate([
            topLargeContainer.topAnchor.constraint(equalTo: view.topAnchor, constant: spacing),
            topLargeContainer.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: spacing),
            topLargeContainer.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -spacing),
            topLargeContainer.bottomAnchor.constraint(equalTo: view.bottomAnchor, constant: -spacing),

            topLargeFocusPanel.topAnchor.constraint(equalTo: topLargeContainer.topAnchor),
            topLargeFocusPanel.leadingAnchor.constraint(equalTo: topLargeContainer.leadingAnchor),
            topLargeFocusPanel.trailingAnchor.constraint(equalTo: topLargeContainer.trailingAnchor),
            topLargeFocusPanel.bottomAnchor.constraint(equalTo: topLargeBottomScroll.topAnchor, constant: -spacing),

            topLargeBottomScroll.leadingAnchor.constraint(equalTo: topLargeContainer.leadingAnchor),
            topLargeBottomScroll.trailingAnchor.constraint(equalTo: topLargeContainer.trailingAnchor),
            topLargeBottomScroll.bottomAnchor.constraint(equalTo: topLargeContainer.bottomAnchor),
            topLargeBottomScroll.heightAnchor.constraint(equalToConstant: miniCardHeight),
        ])
    }

    // MARK: - Rebuild: Grid

    private func rebuildGrid() {
        gridCards.forEach { $0.removeFromSuperview() }
        gridCards.removeAll()

        let sorted = sortedAgents()
        guard !sorted.isEmpty else { return }

        for agent in sorted {
            let card = AgentCardView()
            card.delegate = self
            card.configure(
                id: agent.id,
                project: agent.project,
                thread: agent.thread,
                status: agent.status,
                lastMessage: agent.lastMessage,
                totalDuration: agent.totalDuration,
                roundDuration: agent.roundDuration
            )
            card.translatesAutoresizingMaskIntoConstraints = true
            gridCards.append(card)
            gridContainer.addSubview(card)
        }

        layoutGridFrames()
    }

    private var currentGridLayout: GridLayout {
        let availableWidth = gridScrollView.contentView.bounds.width
        let availableHeight = gridScrollView.contentView.bounds.height
        return GridLayout(
            availableWidth: availableWidth,
            availableHeight: availableHeight,
            cardCount: gridCards.count,
            minCardWidth: currentMinCardWidth,
            spacing: gridSpacing,
            aspectRatio: aspectRatio
        )
    }

    private func layoutGridFrames() {
        guard !gridCards.isEmpty else { return }
        let layout = currentGridLayout
        let availableWidth = gridScrollView.contentView.bounds.width
        gridContainer.frame = NSRect(x: 0, y: 0, width: availableWidth, height: layout.scrollContentHeight)

        for (index, card) in gridCards.enumerated() {
            card.frame = layout.cardFrame(at: index)
        }
    }

    // MARK: - Rebuild: Left-Right

    private func rebuildLeftRight() {
        // Clear old mini cards
        leftRightMiniCards.forEach { $0.removeFromSuperview() }
        leftRightMiniCards.removeAll()
        leftRightSidebarStack.arrangedSubviews.forEach { $0.removeFromSuperview() }

        let sorted = sortedAgents()
        guard !sorted.isEmpty else { return }

        // Configure focus panel with selected agent
        if let selected = sorted.first(where: { $0.id == selectedAgentId }) ?? sorted.first {
            selectedAgentId = selected.id
            configureFocusPanel(leftRightFocusPanel, with: selected)
            selected.surface.reparent(to: leftRightFocusPanel.terminalContainer)
        }

        // Build sidebar mini cards for non-selected agents
        let sidebarWidth = leftRightSidebarScroll.bounds.width > 0 ? leftRightSidebarScroll.bounds.width : 240
        for agent in sorted {
            let card = MiniCardView()
            card.delegate = self
            card.configure(
                id: agent.id,
                project: agent.project,
                thread: agent.thread,
                status: agent.status,
                lastMessage: agent.lastMessage,
                totalDuration: agent.totalDuration,
                roundDuration: agent.roundDuration
            )
            card.isSelected = (agent.id == selectedAgentId)
            card.translatesAutoresizingMaskIntoConstraints = false
            leftRightMiniCards.append(card)
            leftRightSidebarStack.addArrangedSubview(card)

            NSLayoutConstraint.activate([
                card.widthAnchor.constraint(equalToConstant: sidebarWidth),
            ])
        }
    }

    // MARK: - Rebuild: Top-Small

    private func rebuildTopSmall() {
        topSmallMiniCards.forEach { $0.removeFromSuperview() }
        topSmallMiniCards.removeAll()
        topSmallTopStack.arrangedSubviews.forEach { $0.removeFromSuperview() }

        let sorted = sortedAgents()
        guard !sorted.isEmpty else { return }

        // Configure focus panel
        if let selected = sorted.first(where: { $0.id == selectedAgentId }) ?? sorted.first {
            selectedAgentId = selected.id
            configureFocusPanel(topSmallFocusPanel, with: selected)
            selected.surface.reparent(to: topSmallFocusPanel.terminalContainer)
        }

        // Build horizontal mini cards
        for agent in sorted {
            let card = MiniCardView()
            card.delegate = self
            card.configure(
                id: agent.id,
                project: agent.project,
                thread: agent.thread,
                status: agent.status,
                lastMessage: agent.lastMessage,
                totalDuration: agent.totalDuration,
                roundDuration: agent.roundDuration
            )
            card.isSelected = (agent.id == selectedAgentId)
            card.translatesAutoresizingMaskIntoConstraints = false
            topSmallMiniCards.append(card)
            topSmallTopStack.addArrangedSubview(card)

            // Clamp width 180-260
            let widthConstraint = card.widthAnchor.constraint(equalToConstant: 220)
            widthConstraint.priority = .defaultHigh
            let minWidth = card.widthAnchor.constraint(greaterThanOrEqualToConstant: 180)
            let maxWidth = card.widthAnchor.constraint(lessThanOrEqualToConstant: 260)
            NSLayoutConstraint.activate([widthConstraint, minWidth, maxWidth])
        }
    }

    // MARK: - Rebuild: Top-Large

    private func rebuildTopLarge() {
        topLargeMiniCards.forEach { $0.removeFromSuperview() }
        topLargeMiniCards.removeAll()
        topLargeBottomStack.arrangedSubviews.forEach { $0.removeFromSuperview() }

        let sorted = sortedAgents()
        guard !sorted.isEmpty else { return }

        // Configure focus panel
        if let selected = sorted.first(where: { $0.id == selectedAgentId }) ?? sorted.first {
            selectedAgentId = selected.id
            configureFocusPanel(topLargeFocusPanel, with: selected)
            selected.surface.reparent(to: topLargeFocusPanel.terminalContainer)
        }

        // Build horizontal mini cards at bottom
        for agent in sorted {
            let card = MiniCardView()
            card.delegate = self
            card.configure(
                id: agent.id,
                project: agent.project,
                thread: agent.thread,
                status: agent.status,
                lastMessage: agent.lastMessage,
                totalDuration: agent.totalDuration,
                roundDuration: agent.roundDuration
            )
            card.isSelected = (agent.id == selectedAgentId)
            card.translatesAutoresizingMaskIntoConstraints = false
            topLargeMiniCards.append(card)
            topLargeBottomStack.addArrangedSubview(card)

            // Clamp width 180-260
            let widthConstraint = card.widthAnchor.constraint(equalToConstant: 220)
            widthConstraint.priority = .defaultHigh
            let minWidth = card.widthAnchor.constraint(greaterThanOrEqualToConstant: 180)
            let maxWidth = card.widthAnchor.constraint(lessThanOrEqualToConstant: 260)
            NSLayoutConstraint.activate([widthConstraint, minWidth, maxWidth])
        }
    }

    // MARK: - Focus panel helper

    private func configureFocusPanel(_ panel: FocusPanelView, with agent: AgentDisplayInfo) {
        panel.configure(
            name: agent.name,
            project: agent.project,
            thread: agent.thread,
            status: agent.status,
            total: agent.totalDuration,
            round: agent.roundDuration
        )
    }

    // MARK: - Resize

    override func viewDidLayout() {
        super.viewDidLayout()
        if case .grid = currentLayout {
            layoutGridFrames()
        }
    }

    // MARK: - AgentCardDelegate

    func agentCardClicked(agentId: String) {
        switch currentLayout {
        case .grid:
            // Single click → enter Speaker View with clicked agent focused
            detachTerminals()
            selectedAgentId = agentId
            setLayout(.leftRight)
        default:
            // In other layouts, change selection and refresh focus panel
            detachTerminals()
            selectedAgentId = agentId
            rebuildCurrentLayout()
        }
    }

    // MARK: - FocusPanelDelegate

    func focusPanelDidRequestEnterProject(_ projectName: String) {
        dashboardDelegate?.dashboardDidRequestEnterProject(projectName)
    }

    // MARK: - DraggableGridDelegate

    func draggableGrid(_ grid: DraggableGridView, dropIndexFor point: NSPoint) -> Int {
        currentGridLayout.gridIndex(for: point)
    }

    func draggableGrid(_ grid: DraggableGridView, dropIndicatorFrameAt index: Int) -> NSRect {
        guard !gridCards.isEmpty else { return .zero }
        return currentGridLayout.dropIndicatorFrame(at: index)
    }

    func draggableGrid(_ grid: DraggableGridView, didDropItemWithPath path: String, atIndex toIndex: Int) {
        guard let fromIndex = agents.firstIndex(where: { $0.id == path }) else { return }
        guard fromIndex != toIndex, toIndex >= 0, toIndex <= agents.count else { return }

        var mutableAgents = agents
        let item = mutableAgents.remove(at: fromIndex)
        let adjustedIndex = toIndex > fromIndex ? toIndex - 1 : toIndex
        mutableAgents.insert(item, at: min(adjustedIndex, mutableAgents.count))
        agents = mutableAgents

        rebuildGrid()

        dashboardDelegate?.dashboardDidReorderCards(order: agents.map { $0.id })
    }
}

// MARK: - NSView helper

private extension NSView {
    /// Remove all constraints that reference this view from its superview
    func removeAllConstraintsFromSuperviews() {
        for constraint in constraints {
            if constraint.firstItem === self && constraint.secondItem == nil {
                removeConstraint(constraint)
            }
        }
        if let superview = superview {
            for constraint in superview.constraints {
                if constraint.firstItem === self || constraint.secondItem === self {
                    superview.removeConstraint(constraint)
                }
            }
        }
    }
}
