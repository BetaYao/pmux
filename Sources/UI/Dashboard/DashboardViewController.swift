import AppKit
import QuartzCore

// MARK: - DashboardDelegate

protocol DashboardDelegate: AnyObject {
    func dashboardDidSelectProject(_ project: String, thread: String)
    func dashboardDidRequestEnterProject(_ project: String)
    func dashboardDidReorderCards(order: [String])
    func dashboardDidRequestDelete(_ terminalID: String)
    func dashboardDidRequestCloseRepo(_ project: String)
    func dashboardDidRequestAddProject()
    func dashboardDidChangeSelection(_ dashboard: DashboardViewController)
    func dashboardDidRequestBrowseFiles(worktreePath: String)
    func dashboardDidRequestShowChanges(worktreePath: String)
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
        static let leftColumnWidth: CGFloat = 260
        static let rightColumnWidth: CGFloat = 320
        static let columnSpacing: CGFloat = 8

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

    /// Set by MainWindowController. Called when the user drills into a terminal so the
    /// keyboard mode can switch to .insert.
    var onEnterTerminal: (() -> Void)?
    /// Set by MainWindowController. Called when the user requests the new-worktree creator.
    var onRequestNewWorktree: (() -> Void)?

    /// Set by TabCoordinator during setup
    weak var surfaceManager: TerminalSurfaceManager?

    /// Set by MainWindowController — forwards split events to TerminalCoordinator
    weak var splitContainerDelegate: SplitContainerDelegate?

    var currentLayout: DashboardLayout = .leftRight
    /// The most recently used non-grid focus layout. Grid's Return drill-in uses this as the target.
    /// Seeded to .leftRight so first-launch behavior is predictable.
    private(set) var lastFocusLayout: DashboardLayout = .leftRight
    var selectedAgentId: String = ""
    let focusController = DashboardFocusController()
    private var isInDState: Bool { focusController.mode != .idle }

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
    private let inlineCreateView = InlineWorktreeCreateView()
    private var inlineCreateHeightConstraint: NSLayoutConstraint?

    // Right column (3-column layout)
    private let rightColumnContainer = NSView()
    private(set) lazy var sidePanelVC = WorktreeSidePanelViewController(worktreePath: nil)
    private var leftColumnWidthExpanded: NSLayoutConstraint?
    private var leftColumnWidthCollapsed: NSLayoutConstraint?
    private var rightColumnWidthExpanded: NSLayoutConstraint?
    private var rightColumnWidthCollapsed: NSLayoutConstraint?
    private var isLeftColumnCollapsed = false
    private var isRightColumnCollapsed = false

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

    // MARK: - First responder

    override var acceptsFirstResponder: Bool { isInDState }

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

        if isInDState {
            focusController.refreshCards(agents.map { $0.id })
            applyKeyboardFocusVisuals()
        }

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
        syncSidePanelToSelection()
    }

    private func syncSidePanelToSelection() {
        let path = agents.first(where: { $0.id == selectedAgentId })?.worktreePath
        sidePanelVC.setWorktree(path)
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
            WorktreeTitleCache.shared.title(worktreePath: agent.worktreePath, lastUserPrompt: agent.lastUserPrompt, branch: agent.thread) { _ in }
            miniCards[index].miniCardView.configure(
                id: agent.id,
                project: agent.project,
                thread: agent.thread,
                status: agent.status,
                lastMessage: agent.lastMessage,
                lastUserPrompt: WorktreeTitleCache.shared.cachedTitle(worktreePath: agent.worktreePath) ?? agent.lastUserPrompt,
                totalDuration: agent.totalDuration,
                roundDuration: agent.roundDuration,
                paneStatuses: agent.paneStatuses,
                isMainWorktree: agent.isMainWorktree,
                tasks: agent.tasks,
                activityEvents: agent.activityEvents,
                agentType: WorktreeAgentTypeStore.shared.agentType(forWorktree: agent.worktreePath)
                    ?? AgentHead.shared.agent(forWorktree: agent.worktreePath)?.agentType ?? .unknown
            )
            miniCards[index].isSelected = (agent.id == selectedAgentId)
        }
    }

    func setLayout(_ layout: DashboardLayout) {
        guard layout != currentLayout else { return }
        detachTerminals()
        resetSidebarConstraints()
        isLeftColumnCollapsed = false; isRightColumnCollapsed = false
        // Remember the focus layout we are LEAVING, so grid Return can restore it.
        if currentLayout != .grid {
            lastFocusLayout = currentLayout
        }
        currentLayout = layout
        showLayout(layout)
        rebuildCurrentLayout()

        if layout == .grid {
            DispatchQueue.main.async { [weak self] in
                self?.enterDashboardNavigation()
            }
        }
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
        syncSidePanelToSelection()
    }

    func toggleLeftColumnCollapse() {
        isLeftColumnCollapsed.toggle()
        leftColumnWidthExpanded?.isActive = !isLeftColumnCollapsed
        leftColumnWidthCollapsed?.isActive = isLeftColumnCollapsed
        animateColumnLayout {
            self.leftRightSidebarScroll.animator().alphaValue = self.isLeftColumnCollapsed ? 0 : 1
            self.inlineCreateView.animator().alphaValue = self.isLeftColumnCollapsed ? 0 : 1
        }
    }

    func toggleRightColumnCollapse() {
        isRightColumnCollapsed.toggle()
        rightColumnWidthExpanded?.isActive = !isRightColumnCollapsed
        rightColumnWidthCollapsed?.isActive = isRightColumnCollapsed
        animateColumnLayout {
            self.rightColumnContainer.animator().alphaValue = self.isRightColumnCollapsed ? 0 : 1
        }
    }

    private func animateColumnLayout(_ extra: @escaping () -> Void) {
        NSAnimationContext.runAnimationGroup { context in
            context.duration = 0.2
            context.allowsImplicitAnimation = true
            extra()
            self.view.layoutSubtreeIfNeeded()
        }
    }

    // MARK: - Inline worktree creation

    func setupInlineCreate(repoPaths: [String],
                           repoPathsProvider: @escaping () -> [String],
                           onAddRepo: @escaping () -> Void,
                           onCreate: @escaping (String, String, AgentType, Bool) -> Void) {
        inlineCreateView.configure(repoPaths: repoPaths)
        inlineCreateView.repoPathsProvider = repoPathsProvider
        inlineCreateView.onAddRepo = onAddRepo
        inlineCreateView.onCreate = onCreate
    }

    func focusInlineCreate() { inlineCreateView.focusNameField() }

    /// Called when the inline create form ends (submit or cancel) so the owner
    /// can exit `.createForm` and restore the nav ring.
    var onInlineCreateFormEnd: (() -> Void)? {
        didSet { inlineCreateView.onFormEnd = onInlineCreateFormEnd }
    }

    func inlineCreateReportSuccess() { inlineCreateView.reportCreateSuccess() }
    func inlineCreateReportFailure(_ message: String) { inlineCreateView.reportCreateFailure(message) }

    private func resetSidebarConstraints() {
        leftColumnWidthExpanded?.isActive = true
        leftColumnWidthCollapsed?.isActive = false
        rightColumnWidthExpanded?.isActive = true
        rightColumnWidthCollapsed?.isActive = false
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
            container.reorderDelegate = self
            container.configure(paneCount: agent.paneCount)
            WorktreeTitleCache.shared.title(worktreePath: agent.worktreePath, lastUserPrompt: agent.lastUserPrompt, branch: agent.thread) { _ in }
            container.miniCardView.configure(
                id: agent.id, project: agent.project, thread: agent.thread,
                status: agent.status, lastMessage: agent.lastMessage,
                lastUserPrompt: WorktreeTitleCache.shared.cachedTitle(worktreePath: agent.worktreePath) ?? agent.lastUserPrompt,
                totalDuration: agent.totalDuration, roundDuration: agent.roundDuration,
                paneStatuses: agent.paneStatuses,
                isMainWorktree: agent.isMainWorktree,
                tasks: agent.tasks,
                activityEvents: agent.activityEvents,
                agentType: WorktreeAgentTypeStore.shared.agentType(forWorktree: agent.worktreePath)
                    ?? AgentHead.shared.agent(forWorktree: agent.worktreePath)?.agentType ?? .unknown
            )
            container.isSelected = (agent.id == selectedAgentId)
            container.translatesAutoresizingMaskIntoConstraints = false
            refs.miniCards.append(container)
            refs.stack.addArrangedSubview(container)

            switch refs.widthStyle {
            case .fixed:
                NSLayoutConstraint.activate([
                    container.widthAnchor.constraint(equalToConstant: fixedWidth),
                    // Compact card: hug the 3–4 lines of content instead of a 16:9 box.
                    container.heightAnchor.constraint(equalToConstant: 84),
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

        // --- Left column: worktree sidebar scroll + inline create ---
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

        inlineCreateView.translatesAutoresizingMaskIntoConstraints = false
        leftRightContainer.addSubview(inlineCreateView)
        inlineCreateView.onPreferredHeightChange = { [weak self] height, animated in
            self?.setInlineCreateHeight(height, animated: animated)
        }
        let inlineHeight = inlineCreateView.heightAnchor.constraint(equalToConstant: inlineCreateView.preferredHeight)
        inlineCreateHeightConstraint = inlineHeight

        // --- Center column: focus panel ---
        leftRightFocusPanel.translatesAutoresizingMaskIntoConstraints = false
        leftRightFocusPanel.setCornerMask(
            [.layerMinXMinYCorner, .layerMaxXMinYCorner, .layerMinXMaxYCorner, .layerMaxXMaxYCorner],
            radius: LayoutMetrics.focusPanelCornerRadius
        )
        leftRightContainer.addSubview(leftRightFocusPanel)

        // --- Right column: side panel host ---
        rightColumnContainer.translatesAutoresizingMaskIntoConstraints = false
        rightColumnContainer.wantsLayer = true
        rightColumnContainer.setAccessibilityIdentifier("dashboard.rightColumn")
        leftRightContainer.addSubview(rightColumnContainer)

        addChild(sidePanelVC)
        sidePanelVC.view.translatesAutoresizingMaskIntoConstraints = false
        rightColumnContainer.addSubview(sidePanelVC.view)

        let spacing = LayoutMetrics.columnSpacing
        let edge: CGFloat = 8

        // Fixed widths for side columns; centre fills the gap.
        leftColumnWidthExpanded = leftRightSidebarScroll.widthAnchor.constraint(equalToConstant: LayoutMetrics.leftColumnWidth)
        leftColumnWidthCollapsed = leftRightSidebarScroll.widthAnchor.constraint(equalToConstant: 0)
        rightColumnWidthExpanded = rightColumnContainer.widthAnchor.constraint(equalToConstant: LayoutMetrics.rightColumnWidth)
        rightColumnWidthCollapsed = rightColumnContainer.widthAnchor.constraint(equalToConstant: 0)
        leftColumnWidthExpanded?.isActive = true
        rightColumnWidthExpanded?.isActive = true

        NSLayoutConstraint.activate([
            leftRightContainer.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor, constant: layoutTopInset),
            leftRightContainer.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: LayoutMetrics.containerHorizontalInset),
            leftRightContainer.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -LayoutMetrics.containerHorizontalInset),
            leftRightContainer.bottomAnchor.constraint(equalTo: view.bottomAnchor, constant: -LayoutMetrics.containerBottomInset),

            // Left column
            leftRightSidebarScroll.topAnchor.constraint(equalTo: leftRightContainer.topAnchor),
            leftRightSidebarScroll.leadingAnchor.constraint(equalTo: leftRightContainer.leadingAnchor, constant: edge),
            leftRightSidebarScroll.bottomAnchor.constraint(equalTo: inlineCreateView.topAnchor, constant: -10),

            inlineCreateView.leadingAnchor.constraint(equalTo: leftRightSidebarScroll.leadingAnchor),
            inlineCreateView.trailingAnchor.constraint(equalTo: leftRightSidebarScroll.trailingAnchor),
            inlineCreateView.bottomAnchor.constraint(equalTo: leftRightContainer.bottomAnchor, constant: -8),
            inlineHeight,

            // Centre column
            leftRightFocusPanel.topAnchor.constraint(equalTo: leftRightContainer.topAnchor),
            leftRightFocusPanel.leadingAnchor.constraint(equalTo: leftRightSidebarScroll.trailingAnchor, constant: spacing),
            leftRightFocusPanel.bottomAnchor.constraint(equalTo: leftRightContainer.bottomAnchor),
            leftRightFocusPanel.trailingAnchor.constraint(equalTo: rightColumnContainer.leadingAnchor, constant: -spacing),

            // Right column
            rightColumnContainer.topAnchor.constraint(equalTo: leftRightContainer.topAnchor),
            rightColumnContainer.trailingAnchor.constraint(equalTo: leftRightContainer.trailingAnchor, constant: -edge),
            rightColumnContainer.bottomAnchor.constraint(equalTo: leftRightContainer.bottomAnchor),

            sidePanelVC.view.topAnchor.constraint(equalTo: rightColumnContainer.topAnchor),
            sidePanelVC.view.leadingAnchor.constraint(equalTo: rightColumnContainer.leadingAnchor),
            sidePanelVC.view.trailingAnchor.constraint(equalTo: rightColumnContainer.trailingAnchor),
            sidePanelVC.view.bottomAnchor.constraint(equalTo: rightColumnContainer.bottomAnchor),
        ])
    }

    private func setInlineCreateHeight(_ height: CGFloat, animated: Bool) {
        guard let constraint = inlineCreateHeightConstraint else { return }
        guard abs(constraint.constant - height) > 0.5 else { return }

        let layout = {
            constraint.constant = height
            self.view.layoutSubtreeIfNeeded()
        }

        guard animated, view.window != nil else {
            layout()
            return
        }

        NSAnimationContext.runAnimationGroup { context in
            context.duration = 0.22
            context.timingFunction = CAMediaTimingFunction(name: .easeInEaseOut)
            context.allowsImplicitAnimation = true
            layout()
        }
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
            container.reorderDelegate = self
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

    /// Embed the selected agent's split container into the focus panel.
    /// `focusTerminal: false` is used for live nav preview — it keeps the dashboard
    /// VC as first responder so arrow keys keep driving the nav ring.
    func embedSplitContainerForSelectedAgent(focusTerminal: Bool = true) {
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

        // Hold reference to previous view for crossfade
        let previousSplitView = activeSplitContainer
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

        previousSplitView?.removeFromSuperview()
        previousSplitView?.alphaValue = 1
        syncSidePanelToSelection()

        // Focus the active leaf — defer to let the view hierarchy settle.
        // Skipped during nav preview so the dashboard VC keeps first responder.
        guard focusTerminal else { return }

        // A terminal is becoming the active surface outside the nav ring (e.g. initial
        // launch embed): the controller must be in .insert so Cmd+Esc can switch back
        // to NORMAL. In-nav commits leave this to exitDashboardNavigation().
        if !isInDState {
            windowKeyboardMode?.enterInsert()
        }

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

    // MARK: - Dashboard Navigation (D-state)

    func enterDashboardNavigation() {
        guard !isInDState else { return }

        // Nav ring active ⇔ keyboardMode .normal. Idempotent; also clears any stale substate.
        windowKeyboardMode?.enterNormal()

        let snapshot = DashboardFocusController.Snapshot(
            firstResponder: view.window?.firstResponder,
            focusedWorktreePath: agents.first(where: { $0.id == selectedAgentId })?.worktreePath,
            layout: currentLayout
        )
        focusController.captureSnapshot(snapshot)

        let cardIds = agents.map { $0.id }
        let initial = snapshot.focusedWorktreePath
            .flatMap { path in agents.first(where: { $0.worktreePath == path })?.id }
            ?? (selectedAgentId.isEmpty ? nil : selectedAgentId)
        if currentLayout == .grid {
            focusController.enterGrid(cardIds: cardIds, initialId: initial)
        } else {
            focusController.enterFocusLayout(cardIds: cardIds, initialId: initial)
        }

        // Clear mouse-selection visuals so only the keyboard focus ring is visible.
        if currentLayout == .grid {
            gridCards.forEach { $0.isSelected = false }
        }

        view.window?.makeFirstResponder(self)
        applyKeyboardFocusVisuals()
        applyDimOverlayIfNeeded()
    }

    func exitDashboardNavigation(restoreSnapshot: Bool) {
        guard isInDState else { return }

        // Ring is going away: clear any pending delete (guarded no-op otherwise), then
        // assert insert since focus lands on a terminal. Do NOT clear .createForm here.
        windowKeyboardMode?.cancelDelete()
        windowKeyboardMode?.enterInsert()

        let snapshot = focusController.snapshot
        tearDownNavVisuals()

        // A cancelling exit (Esc) undoes any live preview by restoring the pre-nav
        // selection. A committing exit (Return) keeps whatever is currently previewed.
        if restoreSnapshot, currentLayout != .grid,
           let path = snapshot?.focusedWorktreePath,
           let original = agents.first(where: { $0.worktreePath == path }),
           original.id != selectedAgentId {
            selectAgent(byWorktreePath: path)
        }

        if restoreSnapshot, let snap = snapshot, let responder = snap.firstResponder,
           (responder as? NSView)?.window != nil {
            view.window?.makeFirstResponder(responder)
        } else {
            DispatchQueue.main.async { [weak self] in
                guard let self, let container = self.activeSplitContainer, let tree = container.tree else { return }
                let focusedId = tree.focusedId
                if let leaf = tree.allLeaves.first(where: { $0.id == focusedId }),
                   let termView = container.surfaceViews[leaf.surfaceId] {
                    self.view.window?.makeFirstResponder(termView)
                }
            }
        }
    }

    /// Leave the nav focus ring WITHOUT touching `windowKeyboardMode`: opens the inline
    /// create form. `beginCreateForm()` has already set `.normal` + `.createForm`; we only
    /// drop the D-state focus ring so a stray key in the form can't be read as a nav chord
    /// (e.g. `d` starting a delete). On form end, `enterDashboardNavigation()` re-enters.
    func exitNavForCreateForm() {
        guard isInDState else { return }
        tearDownNavVisuals()
    }

    /// Visual/state teardown shared by `exitDashboardNavigation` and `exitNavForCreateForm`.
    /// Drops the focus ring, dim overlays, restores grid mouse-selection visuals, and exits
    /// the focus controller. Deliberately does NOT touch `windowKeyboardMode` or restore the
    /// first responder — callers own those decisions.
    private func tearDownNavVisuals() {
        focusController.exit()

        clearKeyboardFocusVisuals()
        clearDimOverlay()

        // Restore mouse-selection visual on the selected card.
        if currentLayout == .grid {
            for container in gridCards {
                container.isSelected = (container.agentId == selectedAgentId)
            }
        }
    }

    // MARK: - D-state visual helpers

    private func applyKeyboardFocusVisuals() {
        clearKeyboardFocusVisuals()
        switch focusController.focusedTarget {
        case .none: return
        case .bigPanel:
            if let refs = focusLayoutRefs(for: currentLayout) {
                refs.focusPanel.isKeyboardFocused = true
            }
        case .card(let agentId):
            if currentLayout == .grid {
                gridCards.first(where: { $0.agentId == agentId })?.isKeyboardFocused = true
            } else if let refs = focusLayoutRefs(for: currentLayout) {
                refs.miniCards.first(where: { $0.agentId == agentId })?.miniCardView.isKeyboardFocused = true
            }
        }
    }

    private func clearKeyboardFocusVisuals() {
        gridCards.forEach { $0.isKeyboardFocused = false }
        if let refs = focusLayoutRefs(for: currentLayout) {
            refs.focusPanel.isKeyboardFocused = false
            refs.miniCards.forEach { $0.miniCardView.isKeyboardFocused = false }
        }
    }

    private func applyDimOverlayIfNeeded() {
        guard currentLayout != .grid else { return }
        guard let refs = focusLayoutRefs(for: currentLayout) else { return }
        refs.focusPanel.showDimOverlay(opacity: 0.05)
        refs.miniCards.forEach { $0.miniCardView.showDimOverlay(opacity: 0.05) }
    }

    private func clearDimOverlay() {
        if let refs = focusLayoutRefs(for: currentLayout) {
            refs.focusPanel.hideDimOverlay()
            refs.miniCards.forEach { $0.miniCardView.hideDimOverlay() }
        }
    }

    // MARK: - D-state key handling

    override func keyDown(with event: NSEvent) {
        guard isInDState else { super.keyDown(with: event); return }
        let mode = windowKeyboardMode
        let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)

        // deletePending: d/y confirm, esc/other cancel
        if case .deletePending(let agentId) = mode?.substate {
            if event.keyCode == 53 { mode?.cancelDelete(); applyKeyboardFocusVisuals(); return }
            if let ch = event.charactersIgnoringModifiers, ch == "d" || ch == "y" {
                if mode?.confirmDelete() == agentId { performDelete(agentId: agentId) }
                return
            }
            mode?.cancelDelete(); applyKeyboardFocusVisuals(); return
        }

        // Escape with no pending → exit nav (legacy behavior)
        if event.keyCode == 53 && flags.isEmpty {
            exitDashboardNavigation(restoreSnapshot: true); return
        }

        // Build chord: printable char with no command/control/option, else keyCode.
        let chord: KeyChord
        if flags.isDisjoint(with: [.command, .control, .option]),
           let ch = event.charactersIgnoringModifiers, ch.count == 1,
           ch.rangeOfCharacter(from: .alphanumerics) != nil {
            chord = KeyChord(char: ch)
        } else {
            chord = KeyChord(keyCode: event.keyCode)
        }

        guard let action = Keymap.action(mode: .normal, chord: chord) else {
            super.keyDown(with: event); return
        }
        dispatch(action)
    }

    private var windowKeyboardMode: KeyboardModeController? {
        (view.window?.windowController as? MainWindowController)?.keyboardMode
    }

    /// The agent currently focused by the nav ring, if a card is focused.
    private var focusedAgent: AgentDisplayInfo? {
        guard case .card(let agentId) = focusController.focusedTarget else { return nil }
        return agents.first(where: { $0.id == agentId })
    }

    private func dispatch(_ action: KeyboardAction) {
        switch action {
        case .moveFocus(let dir):
            focusController.move(dir, columns: currentGridColumns)
            applyKeyboardFocusVisuals(); scrollFocusedIntoView()
            previewFocusedCard()
        case .jumpToCard(let idx):
            focusController.jump(toIndex: idx)
            applyKeyboardFocusVisuals(); scrollFocusedIntoView()
            previewFocusedCard()
        case .enterTerminal:
            onEnterTerminal?()
            handleReturnInDState()
        case .deleteFocused:
            guard let agent = focusedAgent else { return }
            guard !agent.isMainWorktree else {
                windowKeyboardMode?.flashHint("main worktree 不可删除")
                return
            }
            windowKeyboardMode?.beginDelete(agentId: agent.id)
        case .showChanges:
            guard let agent = focusedAgent else { return }
            dashboardDelegate?.dashboardDidRequestShowChanges(worktreePath: agent.worktreePath)
        case .browseFiles:
            guard let agent = focusedAgent else { return }
            dashboardDelegate?.dashboardDidRequestBrowseFiles(worktreePath: agent.worktreePath)
        case .newWorktree:
            // Leave the D-state focus ring before opening the form so no stale card
            // visuals remain and a stray key in the form isn't read as a nav chord.
            // keyboardMode stays .normal + .createForm (set by beginCreateForm()).
            exitNavForCreateForm()
            onRequestNewWorktree?()
        }
    }

    private func performDelete(agentId: String) {
        dashboardDelegate?.dashboardDidRequestDelete(agentId)
        focusController.removeCurrentCard()
        applyKeyboardFocusVisuals()
    }

    /// Columns per row for the current grid layout. Focus layouts return 1 (vertical list).
    private var currentGridColumns: Int {
        guard currentLayout == .grid else { return 1 }
        return max(1, currentGridLayout.columns)
    }

    private func handleReturnInDState() {
        switch focusController.focusedTarget {
        case .none:
            exitDashboardNavigation(restoreSnapshot: true)
        case .bigPanel:
            exitDashboardNavigation(restoreSnapshot: false)
        case .card(let agentId):
            guard let agent = agents.first(where: { $0.id == agentId }) else {
                exitDashboardNavigation(restoreSnapshot: true); return
            }
            if currentLayout == .grid {
                setLayout(lastFocusLayout)
                selectAgent(byWorktreePath: agent.worktreePath)
                exitDashboardNavigation(restoreSnapshot: false)
            } else {
                selectAgent(byWorktreePath: agent.worktreePath)
                exitDashboardNavigation(restoreSnapshot: false)
            }
        }
    }

    private func scrollFocusedIntoView() {
        guard case .card(let agentId) = focusController.focusedTarget else { return }
        if currentLayout == .grid {
            if let card = gridCards.first(where: { $0.agentId == agentId }) {
                card.scrollToVisible(card.bounds)
            }
        } else if let refs = focusLayoutRefs(for: currentLayout) {
            if let card = refs.miniCards.first(where: { $0.agentId == agentId }) {
                card.scrollToVisible(card.bounds)
            }
        }
    }

    /// In focus layouts, live-preview the focused mini card in the main panel as the
    /// nav ring moves — the left panel "follows" the selection. The terminal is NOT
    /// focused (the dashboard VC keeps first responder) so arrows keep navigating;
    /// Return then commits via `handleReturnInDState`. No-op in grid or on the big panel.
    private func previewFocusedCard() {
        guard currentLayout != .grid else { return }
        guard case .card(let agentId) = focusController.focusedTarget else { return }
        guard let agent = agents.first(where: { $0.id == agentId }), agent.id != selectedAgentId else { return }

        selectedAgentId = agent.id
        detachTerminals()
        embedSplitContainerForSelectedAgent(focusTerminal: false)
        updateMiniCardSelection()
        // Re-assert nav visuals: embedding mutated the panel's subviews.
        applyKeyboardFocusVisuals()
        applyDimOverlayIfNeeded()
    }

    // MARK: - Resize

    override func viewDidAppear() {
        super.viewDidAppear()
        if currentLayout == .grid && !isInDState {
            enterDashboardNavigation()
        }
    }

    override func viewDidLayout() {
        super.viewDidLayout()
        if case .grid = currentLayout {
            layoutGridFrames()
        }
    }

    // MARK: - AgentCardDelegate

    func agentCardClicked(agentId: String) {
        // Mouse click exits D-state — mouse takes over from keyboard.
        if isInDState {
            exitDashboardNavigation(restoreSnapshot: false)
        }
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
            syncSidePanelToSelection()
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

    func agentCardDidRequestCloseRepo(agentId: String) {
        guard let agent = agents.first(where: { $0.id == agentId }) else { return }
        dashboardDelegate?.dashboardDidRequestCloseRepo(agent.project)
    }

    func agentCardDidRequestBrowseFiles(agentId: String) {
        guard let agent = agents.first(where: { $0.id == agentId }) else { return }
        dashboardDelegate?.dashboardDidRequestBrowseFiles(worktreePath: agent.worktreePath)
    }

    func agentCardDidRequestShowChanges(agentId: String) {
        guard let agent = agents.first(where: { $0.id == agentId }) else { return }
        dashboardDelegate?.dashboardDidRequestShowChanges(worktreePath: agent.worktreePath)
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

        dashboardDelegate?.dashboardDidReorderCards(order: agents.map { $0.worktreePath })
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

extension DashboardViewController: MiniCardReorderDelegate {
    func miniCardReorderBegan(_ card: StackedMiniCardContainerView) {
        // No-op — visual lift handled by the card itself
    }

    func miniCardReorderEnded(_ card: StackedMiniCardContainerView) {
        guard let refs = focusLayoutRefs(for: currentLayout) else { return }
        // Read the new order from the stack's arrangedSubviews
        let newOrder = refs.stack.arrangedSubviews.compactMap { ($0 as? StackedMiniCardContainerView)?.agentId }
        guard newOrder.count == agents.count else { return }

        // Rebuild agents in the new order
        var reordered: [AgentDisplayInfo] = []
        for id in newOrder {
            if let agent = agents.first(where: { $0.id == id }) {
                reordered.append(agent)
            }
        }
        agents = reordered

        // Sync the stored miniCards array to match the new stack order
        let updatedCards = refs.stack.arrangedSubviews.compactMap { $0 as? StackedMiniCardContainerView }
        switch currentLayout {
        case .leftRight: leftRightMiniCards = updatedCards
        case .topSmall: topSmallMiniCards = updatedCards
        case .topLarge: topLargeMiniCards = updatedCards
        case .grid: break
        }

        // Persist — pass worktree paths directly to avoid ID→AgentHead lookup failures
        dashboardDelegate?.dashboardDidReorderCards(order: agents.map { $0.worktreePath })
    }
}

extension DashboardViewController: GridCardReorderDelegate {
    /// Index of the card being dragged in `gridCards`, before dragging started.
    private static var gridDragOriginIndex: Int = 0

    func gridCardReorderBegan(_ card: StackedCardContainerView) {
        guard let idx = gridCards.firstIndex(of: card) else { return }
        Self.gridDragOriginIndex = idx
    }

    func gridCardReorderMoved(_ card: StackedCardContainerView, locationInContainer point: NSPoint) {
        let layout = currentGridLayout
        let targetIndex = layout.gridIndex(for: point)
        guard let currentIndex = gridCards.firstIndex(of: card),
              targetIndex != currentIndex,
              targetIndex >= 0, targetIndex < gridCards.count else { return }

        // Move the card in the array
        let moved = gridCards.remove(at: currentIndex)
        gridCards.insert(moved, at: targetIndex)

        // Animate other cards to their new positions (skip the dragged card)
        NSAnimationContext.runAnimationGroup { ctx in
            ctx.duration = 0.2
            ctx.allowsImplicitAnimation = true
            for (i, c) in gridCards.enumerated() where c !== card {
                c.animator().frame = layout.cardFrame(at: i)
            }
        }
    }

    func gridCardReorderEnded(_ card: StackedCardContainerView) {
        // Rebuild agents array to match gridCards order
        var reordered: [AgentDisplayInfo] = []
        for c in gridCards {
            if let agent = agents.first(where: { $0.id == c.agentId }) {
                reordered.append(agent)
            }
        }
        agents = reordered

        // Snap the dragged card to its final grid position and re-layout
        layoutGridFrames()

        // Persist
        dashboardDelegate?.dashboardDidReorderCards(order: agents.map { $0.worktreePath })
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
