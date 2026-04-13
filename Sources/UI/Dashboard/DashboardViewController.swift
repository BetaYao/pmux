import AppKit
import QuartzCore

// MARK: - DashboardDelegate

protocol DashboardDelegate: AnyObject {
    func dashboardDidSelectProject(_ project: String, thread: String)
    func dashboardDidRequestEnterProject(_ project: String)
    func dashboardDidReorderCards(order: [String])
    func dashboardDidRequestDelete(_ terminalID: String)
    func dashboardDidRequestAddProject()
    func dashboardDidChangeSelection(_ dashboard: DashboardViewController)
}

// MARK: - AgentDisplayInfo

struct AgentDisplayInfo {
    let id: String          // terminal ID (from TerminalSurface.id)
    let name: String        // display name like "Agent-Alpha"
    let project: String     // repo display name
    let thread: String      // branch name
    let paneStatuses: [AgentStatus]     // per-pane statuses
    let mostRecentMessage: String       // message from most recently updated pane
    let lastUserPrompt: String          // most recent user prompt text
    let mostRecentPaneIndex: Int
    let totalDuration: String   // "HH:MM:SS" format
    let roundDuration: String   // "HH:MM:SS" format
    let surface: TerminalSurface
    let worktreePath: String    // needed to lazily create the terminal
    let paneCount: Int          // number of split panes (1 = no badge)
    let paneSurfaces: [TerminalSurface]  // all pane surfaces in leaf order
    let isMainWorktree: Bool    // true = base repo, false = git worktree
    let tasks: [TaskItem]              // webhook-tracked task items
    let activityEvents: [ActivityEvent]

    /// Convenience: primary status string for display (first pane's status)
    var status: String {
        (paneStatuses.first ?? .unknown).rawValue.lowercased()
    }

    /// Convenience: backward-compatible lastMessage
    var lastMessage: String {
        mostRecentMessage
    }
}

// MARK: - Pasteboard type (used by DraggableGridView)

extension NSPasteboard.PasteboardType {
    static let terminalCard = NSPasteboard.PasteboardType("com.amux.terminalCard")
}

// MARK: - DashboardViewController

class DashboardViewController: NSViewController, AgentCardDelegate, DraggableGridDelegate {
    enum LayoutMetrics {
        static let focusPanelCornerRadius: CGFloat = 10
        static let containerHorizontalInset: CGFloat = 0
        static let containerBottomInset: CGFloat = 0
        static let topSmallFocusJoinSpacing: CGFloat = 8
        static let topLargeFocusJoinSpacing: CGFloat = 0
        static let topSmallMiniRowHorizontalInset: CGFloat = 8
        static let topLargeMiniRowHorizontalInset: CGFloat = 8
        static let topLargeMiniRowBottomInset: CGFloat = 8
        static let leftRightSidebarTrailingInset: CGFloat = 8

        static let topSmallFocusMaskedCorners: CACornerMask = [.layerMinXMinYCorner, .layerMaxXMinYCorner]
        static let topLargeFocusMaskedCorners: CACornerMask = [.layerMinXMaxYCorner, .layerMaxXMaxYCorner]
        static let leftRightFocusMaskedCorners: CACornerMask = [.layerMaxXMinYCorner, .layerMaxXMaxYCorner]
    }

    private struct FocusLayoutRefs {
        let focusPanel: FocusPanelView
        let scrollView: NSScrollView
        let stack: NSStackView
        var miniCards: [StackedMiniCardContainerView]

        enum WidthStyle {
            case fixed          // Uses scroll view's bounds width (leftRight sidebar)
            case flexible       // Uses 220pt nominal with 180-260 range (topSmall, topLarge)
        }
        let widthStyle: WidthStyle
    }

    weak var dashboardDelegate: DashboardDelegate?

    /// Set by TabCoordinator during setup
    weak var surfaceManager: TerminalSurfaceManager?

    /// Set by MainWindowController — forwards split events to TerminalCoordinator
    weak var splitContainerDelegate: SplitContainerDelegate?

    var currentLayout: DashboardLayout = .leftRight
    var selectedAgentId: String = ""
    private var isSidebarCollapsed = false

    // Constraints swapped when sidebar collapses/expands
    private var leftRightFocusWidthExpanded: NSLayoutConstraint?   // 0.78 multiplier
    private var leftRightFocusWidthCollapsed: NSLayoutConstraint?  // trailing = container trailing
    private var topSmallScrollHeight: NSLayoutConstraint?          // 128pt
    private var topSmallScrollHeightCollapsed: NSLayoutConstraint? // 0pt
    private var topLargeScrollHeight: NSLayoutConstraint?          // 128pt
    private var topLargeScrollHeightCollapsed: NSLayoutConstraint? // 0pt

    var selectedAgentIndex: Int {
        agents.firstIndex(where: { $0.id == selectedAgentId }) ?? 0
    }
    private(set) var zoomIndex: Int = GridLayout.defaultZoomIndex

    /// Cached SplitContainerView per worktree path
    private var splitContainers: [String: SplitContainerView] = [:]

    /// Currently visible split container in the focus panel
    private(set) var activeSplitContainer: SplitContainerView?

    // Data
    private(set) var agents: [AgentDisplayInfo] = []

    // Grid layout
    private let gridScrollView = NonFirstResponderScrollView()
    private let gridContainer = DraggableGridView()
    private var gridCards: [StackedCardContainerView] = []

    private let gridSpacing: CGFloat = 3
    private let aspectRatio: CGFloat = 0.5625
    private let layoutTopInset: CGFloat = 8

    // Left-Right layout
    private let leftRightContainer = NSView()
    private let leftRightFocusPanel = FocusPanelView()
    private let leftRightSidebarScroll = NonFirstResponderScrollView()
    private let leftRightSidebarStack = FlippedStackView()
    private var leftRightMiniCards: [StackedMiniCardContainerView] = []

    // Top-Small layout
    private let topSmallContainer = NSView()
    private let topSmallFocusPanel = FocusPanelView()
    private let topSmallTopScroll = NonFirstResponderScrollView()
    private let topSmallTopStack = NonFirstResponderStackView()
    private var topSmallMiniCards: [StackedMiniCardContainerView] = []

    // Top-Large layout
    private let topLargeContainer = NSView()
    private let topLargeFocusPanel = FocusPanelView()
    private let topLargeBottomScroll = NonFirstResponderScrollView()
    private let topLargeBottomStack = NonFirstResponderStackView()
    private var topLargeMiniCards: [StackedMiniCardContainerView] = []

    // Empty state
    private let emptyStateView = NSView()

    private var currentMinCardWidth: CGFloat {
        GridLayout.zoomLevels[zoomIndex]
    }

    // MARK: - View lifecycle

    override func loadView() {
        let root = DashboardRootView()
        root.wantsLayer = true
        root.setAccessibilityIdentifier("dashboard.view")
        self.view = root

        setupEmptyState()
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

        #if DEBUG
        if structureChanged, !oldIds.isEmpty {
            let added = newIds.subtracting(oldIds)
            let removed = oldIds.subtracting(newIds)
            NSLog("DashboardVC.updateAgents: structureChanged — added=%@ removed=%@", "\(added)", "\(removed)")
        }
        #endif

        agents = newAgents

        // Show empty state when no agents
        if agents.isEmpty {
            emptyStateView.isHidden = false
            showLayout(currentLayout) // hides all layout containers
            gridScrollView.isHidden = true
            leftRightContainer.isHidden = true
            topSmallContainer.isHidden = true
            topLargeContainer.isHidden = true
            return
        } else {
            emptyStateView.isHidden = true
            showLayout(currentLayout)
        }

        // Validate selectedAgentId
        if !agents.contains(where: { $0.id == selectedAgentId }) {
            selectedAgentId = agents.first?.id ?? ""
        }

        if structureChanged {
            rebuildCurrentLayout()
        } else {
            updateCurrentLayoutInPlace()
        }
    }

    /// Update existing views in-place without rebuilding the view hierarchy
    private func updateCurrentLayoutInPlace() {
        if currentLayout == .grid {
            updateGridInPlace(sortedAgents())
        } else if let refs = focusLayoutRefs(for: currentLayout) {
            updateFocusLayoutInPlace(agents, miniCards: refs.miniCards, focusPanel: refs.focusPanel)
        }
    }

    private func updateGridInPlace(_ sorted: [AgentDisplayInfo]) {
        guard sorted.count == gridCards.count else {
            rebuildGrid()
            return
        }
        for (index, agent) in sorted.enumerated() {
            gridCards[index].configure(paneCount: agent.paneCount)
            gridCards[index].layoutChildren()
            gridCards[index].cardView.configure(
                id: agent.id,
                project: agent.project,
                thread: agent.thread,
                status: agent.status,
                lastMessage: agent.lastMessage,
                totalDuration: agent.totalDuration,
                roundDuration: agent.roundDuration,
                paneCount: agent.paneCount,
                paneStatuses: agent.paneStatuses,
                tasks: agent.tasks,
                activityEvents: agent.activityEvents
            )
            gridCards[index].isSelected = (agent.id == selectedAgentId)
        }
    }

    private func updateFocusLayoutInPlace(_ sorted: [AgentDisplayInfo], miniCards: [StackedMiniCardContainerView], focusPanel: FocusPanelView) {
        // Count mismatch means structure changed — handled by structureChanged check in updateAgents
        guard sorted.count == miniCards.count else { return }

        // Re-embed split container if it was detached (e.g. after tab switch),
        // but only when the dashboard is actually visible.
        // Skip if a terminal already has focus — avoids stealing focus during
        // periodic updates (branch refresh, status polling).
        if activeSplitContainer == nil, view.window != nil,
           !(view.window?.firstResponder is GhosttyNSView) {
            embedSplitContainerForSelectedAgent()
        }
        for (index, agent) in sorted.enumerated() {
            miniCards[index].configure(paneCount: agent.paneCount)
            miniCards[index].layoutChildren()
            miniCards[index].miniCardView.configure(
                id: agent.id,
                project: agent.project,
                thread: agent.thread,
                status: agent.status,
                lastMessage: agent.lastMessage,
                lastUserPrompt: agent.lastUserPrompt,
                totalDuration: agent.totalDuration,
                roundDuration: agent.roundDuration,
                paneStatuses: agent.paneStatuses,
                isMainWorktree: agent.isMainWorktree,
                tasks: agent.tasks,
                activityEvents: agent.activityEvents
            )
            miniCards[index].isSelected = (agent.id == selectedAgentId)
        }
    }

    func setLayout(_ layout: DashboardLayout) {
        guard layout != currentLayout else { return }
        detachTerminals()
        resetSidebarConstraints()
        isSidebarCollapsed = false
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
        activeSplitContainer?.removeFromSuperview()
        activeSplitContainer = nil
        activeSplitWorktreePath = nil
    }

    func selectAgent(byWorktreePath path: String) {
        guard let agent = agents.first(where: { $0.worktreePath == path }) else { return }
        selectedAgentId = agent.id
        if currentLayout != .grid {
            detachTerminals()
            embedSplitContainerForSelectedAgent()
            updateMiniCardSelection()
        } else {
            for container in gridCards {
                container.isSelected = (container.agentId == selectedAgentId)
            }
        }
    }

    func toggleSidebarCollapse() {
        guard currentLayout != .grid else { return }
        isSidebarCollapsed.toggle()

        guard let refs = focusLayoutRefs(for: currentLayout) else { return }

        // Swap constraints based on layout type
        switch currentLayout {
        case .leftRight:
            leftRightFocusWidthExpanded?.isActive = !isSidebarCollapsed
            leftRightFocusWidthCollapsed?.isActive = isSidebarCollapsed
        case .topSmall:
            topSmallScrollHeight?.isActive = !isSidebarCollapsed
            topSmallScrollHeightCollapsed?.isActive = isSidebarCollapsed
        case .topLarge:
            topLargeScrollHeight?.isActive = !isSidebarCollapsed
            topLargeScrollHeightCollapsed?.isActive = isSidebarCollapsed
        case .grid:
            break
        }

        NSAnimationContext.runAnimationGroup { context in
            context.duration = 0.2
            context.allowsImplicitAnimation = true

            refs.scrollView.animator().isHidden = isSidebarCollapsed
            refs.scrollView.animator().alphaValue = isSidebarCollapsed ? 0 : 1

            self.view.layoutSubtreeIfNeeded()
        }
    }

    private func resetSidebarConstraints() {
        leftRightFocusWidthExpanded?.isActive = true
        leftRightFocusWidthCollapsed?.isActive = false
        topSmallScrollHeight?.isActive = true
        topSmallScrollHeightCollapsed?.isActive = false
        topLargeScrollHeight?.isActive = true
        topLargeScrollHeightCollapsed?.isActive = false
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
        // Only hide containers that are NOT the target layout.
        // Hiding and immediately un-hiding the active container causes AppKit
        // to resign the first responder (terminal loses keyboard focus).
        gridScrollView.isHidden = layout != .grid
        leftRightContainer.isHidden = layout != .leftRight
        topSmallContainer.isHidden = layout != .topSmall
        topLargeContainer.isHidden = layout != .topLarge
    }

    private func rebuildCurrentLayout() {
        switch currentLayout {
        case .grid:
            rebuildGrid()
        case .leftRight, .topSmall, .topLarge:
            rebuildFocusLayout(currentLayout)
        }
    }

    private func focusLayoutRefs(for layout: DashboardLayout) -> FocusLayoutRefs? {
        switch layout {
        case .grid: return nil
        case .leftRight:
            return FocusLayoutRefs(focusPanel: leftRightFocusPanel, scrollView: leftRightSidebarScroll, stack: leftRightSidebarStack, miniCards: leftRightMiniCards, widthStyle: .fixed)
        case .topSmall:
            return FocusLayoutRefs(focusPanel: topSmallFocusPanel, scrollView: topSmallTopScroll, stack: topSmallTopStack, miniCards: topSmallMiniCards, widthStyle: .flexible)
        case .topLarge:
            return FocusLayoutRefs(focusPanel: topLargeFocusPanel, scrollView: topLargeBottomScroll, stack: topLargeBottomStack, miniCards: topLargeMiniCards, widthStyle: .flexible)
        }
    }

    private func rebuildFocusLayout(_ layout: DashboardLayout) {
        guard var refs = focusLayoutRefs(for: layout) else { return }

        refs.miniCards.forEach { $0.removeFromSuperview() }
        refs.miniCards.removeAll()
        refs.stack.arrangedSubviews.forEach { $0.removeFromSuperview() }

        guard !agents.isEmpty else { return }

        if let selected = agents.first(where: { $0.id == selectedAgentId }) ?? agents.first {
            selectedAgentId = selected.id
            // Only embed when the dashboard is visible to avoid stealing
            // surfaces from the active repo tab's split container.
            if view.window != nil {
                embedSplitContainerForSelectedAgent()
            }
        }

        let fixedWidth = refs.scrollView.bounds.width > 0 ? refs.scrollView.bounds.width : 240
        for agent in agents {
            let container = StackedMiniCardContainerView()
            container.delegate = self
            container.configure(paneCount: agent.paneCount)
            container.miniCardView.configure(
                id: agent.id, project: agent.project, thread: agent.thread,
                status: agent.status, lastMessage: agent.lastMessage,
                lastUserPrompt: agent.lastUserPrompt,
                totalDuration: agent.totalDuration, roundDuration: agent.roundDuration,
                paneStatuses: agent.paneStatuses,
                isMainWorktree: agent.isMainWorktree,
                tasks: agent.tasks,
                activityEvents: agent.activityEvents
            )
            container.isSelected = (agent.id == selectedAgentId)
            container.translatesAutoresizingMaskIntoConstraints = false
            refs.miniCards.append(container)
            refs.stack.addArrangedSubview(container)

            switch refs.widthStyle {
            case .fixed:
                NSLayoutConstraint.activate([
                    container.widthAnchor.constraint(equalToConstant: fixedWidth),
                    container.heightAnchor.constraint(equalTo: container.widthAnchor, multiplier: 9.0 / 16.0),
                ])
            case .flexible:
                let w = container.widthAnchor.constraint(equalToConstant: 220)
                w.priority = .defaultHigh
                NSLayoutConstraint.activate([
                    w,
                    container.widthAnchor.constraint(greaterThanOrEqualToConstant: 180),
                    container.widthAnchor.constraint(lessThanOrEqualToConstant: 260),
                    container.heightAnchor.constraint(equalTo: container.widthAnchor, multiplier: 9.0 / 16.0),
                ])
            }
        }

        // Write back the updated miniCards array to the correct property
        switch layout {
        case .leftRight: leftRightMiniCards = refs.miniCards
        case .topSmall: topSmallMiniCards = refs.miniCards
        case .topLarge: topLargeMiniCards = refs.miniCards
        case .grid: break
        }
    }

    // MARK: - Zoom

    func setZoomIndex(_ index: Int) {
        zoomIndex = GridLayout.clampZoomIndex(index)
        if case .grid = currentLayout {
            rebuildGrid()
        }
    }

    // MARK: - Setup: Empty State

    private func setupEmptyState() {
        emptyStateView.translatesAutoresizingMaskIntoConstraints = false
        emptyStateView.isHidden = true
        view.addSubview(emptyStateView)

        // Folder icon button
        let button = NSButton()
        button.translatesAutoresizingMaskIntoConstraints = false
        button.bezelStyle = .regularSquare
        button.isBordered = false
        button.title = ""
        if let folderImage = NSImage(systemSymbolName: "folder.badge.plus", accessibilityDescription: "Open Folder") {
            let config = NSImage.SymbolConfiguration(pointSize: 48, weight: .light)
            button.image = folderImage.withSymbolConfiguration(config)
        }
        button.contentTintColor = .secondaryLabelColor
        button.imagePosition = .imageOnly
        button.target = self
        button.action = #selector(emptyStateAddProjectClicked)
        button.setAccessibilityIdentifier("dashboard.emptyState.addButton")
        emptyStateView.addSubview(button)

        // Subtitle label
        let label = NSTextField(labelWithString: "Add a workspace to get started")
        label.translatesAutoresizingMaskIntoConstraints = false
        label.font = .systemFont(ofSize: 14)
        label.textColor = .secondaryLabelColor
        label.alignment = .center
        emptyStateView.addSubview(label)

        NSLayoutConstraint.activate([
            emptyStateView.topAnchor.constraint(equalTo: view.topAnchor),
            emptyStateView.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            emptyStateView.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            emptyStateView.bottomAnchor.constraint(equalTo: view.bottomAnchor),

            button.centerXAnchor.constraint(equalTo: emptyStateView.centerXAnchor),
            button.centerYAnchor.constraint(equalTo: emptyStateView.centerYAnchor, constant: -16),

            label.topAnchor.constraint(equalTo: button.bottomAnchor, constant: 12),
            label.centerXAnchor.constraint(equalTo: emptyStateView.centerXAnchor),
        ])
    }

    @objc private func emptyStateAddProjectClicked() {
        dashboardDelegate?.dashboardDidRequestAddProject()
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
        gridContainer.setAccessibilityElement(true)
        gridContainer.dragDelegate = self
        gridScrollView.documentView = gridContainer

        view.addSubview(gridScrollView)

        NSLayoutConstraint.activate([
            gridScrollView.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor, constant: layoutTopInset),
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
        leftRightContainer.setAccessibilityElement(true)
        view.addSubview(leftRightContainer)

        // Focus panel (left, 78%)
        leftRightFocusPanel.translatesAutoresizingMaskIntoConstraints = false
        leftRightFocusPanel.setCornerMask(
            LayoutMetrics.leftRightFocusMaskedCorners,
            radius: LayoutMetrics.focusPanelCornerRadius
        )
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
            leftRightContainer.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor, constant: layoutTopInset),
            leftRightContainer.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: LayoutMetrics.containerHorizontalInset),
            leftRightContainer.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -LayoutMetrics.containerHorizontalInset),
            leftRightContainer.bottomAnchor.constraint(equalTo: view.bottomAnchor, constant: -LayoutMetrics.containerBottomInset),

            leftRightFocusPanel.topAnchor.constraint(equalTo: leftRightContainer.topAnchor),
            leftRightFocusPanel.leadingAnchor.constraint(equalTo: leftRightContainer.leadingAnchor),
            leftRightFocusPanel.bottomAnchor.constraint(equalTo: leftRightContainer.bottomAnchor),

            leftRightSidebarScroll.topAnchor.constraint(equalTo: leftRightContainer.topAnchor),
            leftRightSidebarScroll.trailingAnchor.constraint(equalTo: leftRightContainer.trailingAnchor, constant: -LayoutMetrics.leftRightSidebarTrailingInset),
            leftRightSidebarScroll.bottomAnchor.constraint(equalTo: leftRightContainer.bottomAnchor),
            leftRightSidebarScroll.leadingAnchor.constraint(equalTo: leftRightFocusPanel.trailingAnchor, constant: spacing),
        ])

        // Sidebar collapse constraints for left-right
        leftRightFocusWidthExpanded = leftRightFocusPanel.widthAnchor.constraint(equalTo: leftRightContainer.widthAnchor, multiplier: 0.78, constant: -spacing / 2)
        leftRightFocusWidthCollapsed = leftRightFocusPanel.trailingAnchor.constraint(equalTo: leftRightContainer.trailingAnchor)
        leftRightFocusWidthExpanded?.isActive = true
    }

    // MARK: - Setup: Top-Small

    private func setupTopSmallLayout() {
        topSmallContainer.translatesAutoresizingMaskIntoConstraints = false
        topSmallContainer.wantsLayer = true
        topSmallContainer.isHidden = true
        topSmallContainer.setAccessibilityIdentifier("dashboard.layout.top-small")
        topSmallContainer.setAccessibilityElement(true)
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
        topSmallTopStack.alignment = .top
        topSmallTopStack.translatesAutoresizingMaskIntoConstraints = false
        topSmallTopScroll.documentView = topSmallTopStack

        topSmallContainer.addSubview(topSmallTopScroll)

        // Bottom: focus panel
        topSmallFocusPanel.translatesAutoresizingMaskIntoConstraints = false
        topSmallFocusPanel.setCornerMask(
            LayoutMetrics.topSmallFocusMaskedCorners,
            radius: LayoutMetrics.focusPanelCornerRadius
        )
        topSmallContainer.addSubview(topSmallFocusPanel)

        // Mini card height in top-small: derive from clamped width range 180-260 at 16:9
        let miniCardHeight: CGFloat = 128

        NSLayoutConstraint.activate([
            topSmallContainer.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor, constant: layoutTopInset),
            topSmallContainer.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: LayoutMetrics.containerHorizontalInset),
            topSmallContainer.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -LayoutMetrics.containerHorizontalInset),
            topSmallContainer.bottomAnchor.constraint(equalTo: view.bottomAnchor, constant: -LayoutMetrics.containerBottomInset),

            topSmallTopScroll.topAnchor.constraint(equalTo: topSmallContainer.topAnchor),
            topSmallTopScroll.leadingAnchor.constraint(equalTo: topSmallContainer.leadingAnchor, constant: LayoutMetrics.topSmallMiniRowHorizontalInset),
            topSmallTopScroll.trailingAnchor.constraint(equalTo: topSmallContainer.trailingAnchor, constant: -LayoutMetrics.topSmallMiniRowHorizontalInset),

            topSmallFocusPanel.topAnchor.constraint(equalTo: topSmallTopScroll.bottomAnchor, constant: LayoutMetrics.topSmallFocusJoinSpacing),
            topSmallFocusPanel.leadingAnchor.constraint(equalTo: topSmallContainer.leadingAnchor),
            topSmallFocusPanel.trailingAnchor.constraint(equalTo: topSmallContainer.trailingAnchor),
            topSmallFocusPanel.bottomAnchor.constraint(equalTo: topSmallContainer.bottomAnchor),
        ])

        topSmallScrollHeight = topSmallTopScroll.heightAnchor.constraint(equalToConstant: miniCardHeight)
        topSmallScrollHeightCollapsed = topSmallTopScroll.heightAnchor.constraint(equalToConstant: 0)
        topSmallScrollHeight?.isActive = true
    }

    // MARK: - Setup: Top-Large

    private func setupTopLargeLayout() {
        topLargeContainer.translatesAutoresizingMaskIntoConstraints = false
        topLargeContainer.wantsLayer = true
        topLargeContainer.isHidden = true
        topLargeContainer.setAccessibilityIdentifier("dashboard.layout.top-large")
        topLargeContainer.setAccessibilityElement(true)
        view.addSubview(topLargeContainer)

        // Top: focus panel
        topLargeFocusPanel.translatesAutoresizingMaskIntoConstraints = false
        topLargeFocusPanel.setCornerMask(
            LayoutMetrics.topLargeFocusMaskedCorners,
            radius: LayoutMetrics.focusPanelCornerRadius
        )
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
        topLargeBottomStack.alignment = .top
        topLargeBottomStack.translatesAutoresizingMaskIntoConstraints = false
        topLargeBottomScroll.documentView = topLargeBottomStack

        topLargeContainer.addSubview(topLargeBottomScroll)

        let miniCardHeight: CGFloat = 128

        NSLayoutConstraint.activate([
            topLargeContainer.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor, constant: layoutTopInset),
            topLargeContainer.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: LayoutMetrics.containerHorizontalInset),
            topLargeContainer.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -LayoutMetrics.containerHorizontalInset),
            topLargeContainer.bottomAnchor.constraint(equalTo: view.bottomAnchor, constant: -LayoutMetrics.containerBottomInset),

            topLargeFocusPanel.topAnchor.constraint(equalTo: topLargeContainer.topAnchor),
            topLargeFocusPanel.leadingAnchor.constraint(equalTo: topLargeContainer.leadingAnchor),
            topLargeFocusPanel.trailingAnchor.constraint(equalTo: topLargeContainer.trailingAnchor),
            topLargeFocusPanel.bottomAnchor.constraint(equalTo: topLargeBottomScroll.topAnchor, constant: -LayoutMetrics.topLargeFocusJoinSpacing),

            topLargeBottomScroll.leadingAnchor.constraint(equalTo: topLargeContainer.leadingAnchor, constant: LayoutMetrics.topLargeMiniRowHorizontalInset),
            topLargeBottomScroll.trailingAnchor.constraint(equalTo: topLargeContainer.trailingAnchor, constant: -LayoutMetrics.topLargeMiniRowHorizontalInset),
            topLargeBottomScroll.bottomAnchor.constraint(equalTo: topLargeContainer.bottomAnchor, constant: -LayoutMetrics.topLargeMiniRowBottomInset),
        ])

        topLargeScrollHeight = topLargeBottomScroll.heightAnchor.constraint(equalToConstant: miniCardHeight)
        topLargeScrollHeightCollapsed = topLargeBottomScroll.heightAnchor.constraint(equalToConstant: 0)
        topLargeScrollHeight?.isActive = true
    }

    // MARK: - Rebuild: Grid

    private func rebuildGrid() {
        gridCards.forEach { $0.removeFromSuperview() }
        gridCards.removeAll()

        let sorted = sortedAgents()
        guard !sorted.isEmpty else { return }

        for agent in sorted {
            let container = StackedCardContainerView()
            container.delegate = self
            container.configure(paneCount: agent.paneCount)
            container.cardView.configure(
                id: agent.id,
                project: agent.project,
                thread: agent.thread,
                status: agent.status,
                lastMessage: agent.lastMessage,
                totalDuration: agent.totalDuration,
                roundDuration: agent.roundDuration,
                paneCount: agent.paneCount,
                tasks: agent.tasks,
                activityEvents: agent.activityEvents
            )
            container.isSelected = (agent.id == selectedAgentId)
            container.translatesAutoresizingMaskIntoConstraints = true
            gridCards.append(container)
            gridContainer.addSubview(container)
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

        for (index, container) in gridCards.enumerated() {
            container.frame = layout.cardFrame(at: index)
            container.layoutChildren()
        }
    }

    // MARK: - Split container embedding

    private var activeSplitWorktreePath: String?

    func embedSplitContainerForSelectedAgent() {
        guard currentLayout != .grid else { return }
        guard let refs = focusLayoutRefs(for: currentLayout) else { return }
        let container = refs.focusPanel.terminalContainer

        guard let agent = agents.first(where: { $0.id == selectedAgentId }) ?? agents.first else { return }
        let worktreePath = agent.worktreePath

        // Skip re-embed if the same split container is already active for this worktree
        if let active = activeSplitContainer,
           active.superview === container,
           activeSplitWorktreePath == worktreePath {
            return
        }

        // Deactivate previous
        activeSplitContainer?.removeFromSuperview()
        activeSplitContainer = nil
        activeSplitWorktreePath = nil

        // Get or create SplitContainerView
        let splitView: SplitContainerView
        if let cached = splitContainers[worktreePath] {
            splitView = cached
        } else {
            splitView = SplitContainerView(frame: container.bounds)
            splitView.delegate = splitContainerDelegate
            splitContainers[worktreePath] = splitView
        }

        // Populate surface views from SurfaceRegistry
        guard let tree = surfaceManager?.tree(forPath: worktreePath) else { return }
        var surfaceViews: [String: NSView] = [:]
        for leaf in tree.allLeaves {
            if let surface = SurfaceRegistry.shared.surface(forId: leaf.surfaceId) {
                // Ensure surface is created
                if surface.surface == nil {
                    let surfaceId = leaf.surfaceId
                    _ = surface.create(in: container, workingDirectory: worktreePath, sessionName: surface.sessionName) { [weak splitView] in
                        // Async backend (tmux): register the view once creation finishes
                        guard let splitView, let termView = surface.view else { return }
                        splitView.surfaceViews[surfaceId] = termView
                        splitView.layoutTree()
                    }
                }
                if let termView = surface.view {
                    surfaceViews[leaf.surfaceId] = termView
                }
            }
        }
        splitView.surfaceViews = surfaceViews

        // Embed
        splitView.frame = container.bounds
        splitView.autoresizingMask = [.width, .height]
        container.addSubview(splitView)
        splitView.tree = tree
        activeSplitContainer = splitView
        activeSplitWorktreePath = worktreePath

        // Focus the active leaf — defer to let the view hierarchy settle
        let leafToFocus = tree.allLeaves.first(where: { $0.id == tree.focusedId }) ?? tree.allLeaves.first
        if let leaf = leafToFocus,
           let surface = SurfaceRegistry.shared.surface(forId: leaf.surfaceId),
           let termView = surface.view {
            // Immediate attempt (works when hierarchy is stable)
            termView.window?.makeFirstResponder(termView)
            // Deferred attempt (catches cases where the hierarchy hasn't settled yet)
            DispatchQueue.main.async {
                if !(termView.window?.firstResponder is GhosttyNSView) {
                    termView.window?.makeFirstResponder(termView)
                }
            }
        }
    }

    func invalidateSplitContainer(forPath path: String) {
        let container = splitContainers[path]
        container?.removeFromSuperview()
        if activeSplitContainer === container {
            activeSplitContainer = nil
            activeSplitWorktreePath = nil
        }
        splitContainers.removeValue(forKey: path)
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
            // Single click → select in place (no layout switch)
            selectedAgentId = agentId
            for container in gridCards {
                container.isSelected = (container.agentId == agentId)
            }
        default:
            // Click selects agent and embeds its split container
            detachTerminals()
            selectedAgentId = agentId
            embedSplitContainerForSelectedAgent()
            updateMiniCardSelection()
        }
        dashboardDelegate?.dashboardDidChangeSelection(self)
    }

    func agentCardDoubleClicked(agentId: String) {
        guard let agent = agents.first(where: { $0.id == agentId }) else { return }
        dashboardDelegate?.dashboardDidSelectProject(agent.project, thread: agent.thread)
    }

    func agentCardDidRequestDelete(agentId: String) {
        dashboardDelegate?.dashboardDidRequestDelete(agentId)
    }

    private func updateMiniCardSelection() {
        guard let refs = focusLayoutRefs(for: currentLayout) else { return }
        for card in refs.miniCards {
            card.isSelected = (card.agentId == selectedAgentId)
        }
    }

    // MARK: - DraggableGridDelegate

    func draggableGrid(_ grid: DraggableGridView, dropIndexFor point: NSPoint) -> Int {
        currentGridLayout.gridIndex(for: point)
    }

    func draggableGrid(_ grid: DraggableGridView, dropIndicatorFrameAt index: Int) -> NSRect {
        guard !gridCards.isEmpty else { return .zero }
        return currentGridLayout.dropIndicatorFrame(at: index)
    }

    func draggableGrid(_ grid: DraggableGridView, didDropItemWithID id: String, atIndex toIndex: Int) {
        guard let fromIndex = agents.firstIndex(where: { $0.id == id }) else { return }
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

// MARK: - Dashboard Root View (resolves bg color via updateLayer)

private class DashboardRootView: NSView {
    override var wantsUpdateLayer: Bool { true }
    override func updateLayer() {
        layer?.backgroundColor = resolvedCGColor(SemanticColors.bg)
    }
    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        needsDisplay = true
    }
}

extension DashboardViewController: TerminalSurfaceDelegate {
    func terminalSurfaceDidRecover(_ surface: TerminalSurface) {
        // Only re-embed when the dashboard is visible
        guard view.window != nil else { return }
        // Find the agent whose surface recovered
        guard let agent = agents.first(where: { $0.surface === surface }) else { return }
        // Re-embed into grid card if visible
        if let container = gridCards.first(where: { $0.agentId == agent.id }),
           let surface = surfaceManager?.primarySurface(forPath: agent.worktreePath) {
            surface.delegate = self
            if surface.surface == nil {
                _ = surface.create(in: container.cardView.terminalContainer, workingDirectory: agent.worktreePath, sessionName: surface.sessionName)
            } else {
                surface.reparent(to: container.cardView.terminalContainer)
            }
        }
        // Re-embed the split container for the active agent
        if agent.id == selectedAgentId {
            invalidateSplitContainer(forPath: agent.worktreePath)
            embedSplitContainerForSelectedAgent()
        }
    }
}

private final class FlippedStackView: NSStackView {
    override var isFlipped: Bool { true }
    override var acceptsFirstResponder: Bool { false }
}

private final class NonFirstResponderStackView: NSStackView {
    override var acceptsFirstResponder: Bool { false }
}

/// NSScrollView that never steals keyboard focus from the terminal.
private final class NonFirstResponderScrollView: NSScrollView {
    override var acceptsFirstResponder: Bool { false }
}
