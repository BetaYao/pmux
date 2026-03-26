import AppKit

protocol RepoViewDelegate: AnyObject {
    func repoView(_ repoVC: RepoViewController, didRequestDeleteWorktree info: WorktreeInfo)
    func repoViewDidRequestNewThread(_ repoVC: RepoViewController)
    func repoView(_ repoVC: RepoViewController, didRequestShowDiffForWorktreePath worktreePath: String)
}

/// Full repo view: sidebar (thread list) + single immersive terminal
class RepoViewController: NSViewController {
    static let layoutTopInset: CGFloat = 8
    static let terminalCornerRadius: CGFloat = 10
    static let sideBySideTerminalMaskedCorners: CACornerMask = [.layerMinXMinYCorner, .layerMinXMaxYCorner]

    weak var repoDelegate: RepoViewDelegate?
    private let sidebarVC = SidebarViewController()

    // Two-column layout views
    private let sidebarContainer = NSView()
    private let terminalContainer = NSView()
    private var sidebarWidthConstraint: NSLayoutConstraint!
    private var stackConstraints: [NSLayoutConstraint] = []
    private var isStacked = false

    private var worktrees: [WorktreeInfo] = []
    private var trees: [String: SplitTree] = [:]
    private var activeWorktreeIndex: Int = 0
    private var splitContainers: [String: SplitContainerView] = [:]
    var activeSplitContainer: SplitContainerView?
    private var needsTerminalOnLayout = false

    private func applyTerminalAppearanceStyle() {
        let isDark = terminalContainer.effectiveAppearance.isDark
        terminalContainer.layer?.backgroundColor = terminalContainer.resolvedCGColor(SemanticColors.tileBg)
        terminalContainer.layer?.borderWidth = isDark ? 0 : 1
        terminalContainer.layer?.borderColor = isDark
            ? NSColor.clear.cgColor
            : terminalContainer.resolvedCGColor(SemanticColors.line)
    }

    override func loadView() {
        let rootView = RepoRootView()
        rootView.wantsLayer = true
        rootView.onAppearanceChange = { [weak self] in
            self?.applyTerminalAppearanceStyle()
        }
        self.view = rootView

        // Sidebar container (left column)
        sidebarContainer.translatesAutoresizingMaskIntoConstraints = false
        sidebarContainer.wantsLayer = true
        view.addSubview(sidebarContainer)

        // Embed sidebar VC
        sidebarVC.sidebarDelegate = self
        addChild(sidebarVC)
        let sidebarView = sidebarVC.view
        sidebarView.translatesAutoresizingMaskIntoConstraints = false
        sidebarContainer.addSubview(sidebarView)
        NSLayoutConstraint.activate([
            sidebarView.topAnchor.constraint(equalTo: sidebarContainer.topAnchor),
            sidebarView.leadingAnchor.constraint(equalTo: sidebarContainer.leadingAnchor),
            sidebarView.trailingAnchor.constraint(equalTo: sidebarContainer.trailingAnchor),
            sidebarView.bottomAnchor.constraint(equalTo: sidebarContainer.bottomAnchor),
        ])

        // Terminal container (right column) with panel styling
        terminalContainer.translatesAutoresizingMaskIntoConstraints = false
        terminalContainer.wantsLayer = true
        terminalContainer.layer?.cornerRadius = Self.terminalCornerRadius
        terminalContainer.layer?.masksToBounds = true
        terminalContainer.layer?.maskedCorners = Self.sideBySideTerminalMaskedCorners
        terminalContainer.setAccessibilityIdentifier("project.terminal")
        terminalContainer.setAccessibilityElement(true)
        terminalContainer.setAccessibilityRole(.group)
        view.addSubview(terminalContainer)
        applyTerminalAppearanceStyle()

        sidebarWidthConstraint = sidebarContainer.widthAnchor.constraint(equalToConstant: 300)

        applyLayout()
    }

    override func viewDidLayout() {
        super.viewDidLayout()

        // Check for responsive stacking
        let shouldStack = view.bounds.width <= 900
        if shouldStack != isStacked {
            isStacked = shouldStack
            applyLayout()
        }

        if needsTerminalOnLayout && terminalContainer.bounds.width > 0 {
            needsTerminalOnLayout = false
            showTerminal(at: activeWorktreeIndex)
        } else {
            activeSplitContainer?.layoutTree()
        }
    }

    private func applyLayout() {
        NSLayoutConstraint.deactivate(stackConstraints)
        stackConstraints.removeAll()

        let gap: CGFloat = 12
        let topAnchor = view.safeAreaLayoutGuide.topAnchor

        if isStacked {
            // Vertical stack: 220px sidebar on top, terminal below
            sidebarWidthConstraint.isActive = false
            let sidebarHeight = sidebarContainer.heightAnchor.constraint(equalToConstant: 220)

            stackConstraints = [
                sidebarHeight,
                sidebarContainer.topAnchor.constraint(equalTo: topAnchor, constant: Self.layoutTopInset),
                sidebarContainer.leadingAnchor.constraint(equalTo: view.leadingAnchor),
                sidebarContainer.trailingAnchor.constraint(equalTo: view.trailingAnchor),

                terminalContainer.topAnchor.constraint(equalTo: sidebarContainer.bottomAnchor, constant: gap),
                terminalContainer.leadingAnchor.constraint(equalTo: view.leadingAnchor),
                terminalContainer.trailingAnchor.constraint(equalTo: view.trailingAnchor),
                terminalContainer.bottomAnchor.constraint(equalTo: view.bottomAnchor),
            ]

            // Full rounded corners when stacked
            terminalContainer.layer?.maskedCorners = [.layerMinXMinYCorner, .layerMinXMaxYCorner, .layerMaxXMinYCorner, .layerMaxXMaxYCorner]
        } else {
            // Side-by-side: 300px sidebar | 1fr terminal
            sidebarWidthConstraint.constant = 300
            sidebarWidthConstraint.isActive = true

            stackConstraints = [
                sidebarContainer.topAnchor.constraint(equalTo: topAnchor, constant: Self.layoutTopInset),
                sidebarContainer.leadingAnchor.constraint(equalTo: view.leadingAnchor),
                sidebarContainer.bottomAnchor.constraint(equalTo: view.bottomAnchor),

                terminalContainer.topAnchor.constraint(equalTo: topAnchor, constant: Self.layoutTopInset),
                terminalContainer.leadingAnchor.constraint(equalTo: sidebarContainer.trailingAnchor, constant: gap),
                terminalContainer.trailingAnchor.constraint(equalTo: view.trailingAnchor),
                terminalContainer.bottomAnchor.constraint(equalTo: view.bottomAnchor),
            ]

            // No rounded corners on right edge (full height)
            terminalContainer.layer?.maskedCorners = Self.sideBySideTerminalMaskedCorners
        }

        NSLayoutConstraint.activate(stackConstraints)
    }

    // MARK: - Configuration

    func configure(worktrees: [WorktreeInfo], trees: [String: SplitTree]) {
        self.worktrees = worktrees
        self.trees = trees
        sidebarVC.setWorktrees(worktrees)

        if !worktrees.isEmpty {
            if terminalContainer.bounds.width > 0 {
                showTerminal(at: 0)
            } else {
                activeWorktreeIndex = 0
                needsTerminalOnLayout = true
            }
        }
    }

    /// Update data without resetting the active worktree selection.
    /// Used when switching back to an already-configured tab.
    func reconfigurePreservingSelection(worktrees: [WorktreeInfo], trees: [String: SplitTree]) {
        self.trees = trees

        // Check if worktree list changed structurally
        let oldPaths = self.worktrees.map(\.path)
        let newPaths = worktrees.map(\.path)
        self.worktrees = worktrees

        if oldPaths != newPaths {
            sidebarVC.setWorktrees(worktrees)
        }

        // Clamp active index if worktrees were removed
        if !worktrees.isEmpty {
            activeWorktreeIndex = min(activeWorktreeIndex, worktrees.count - 1)
            // Always defer to viewDidLayout so the view is in the window hierarchy
            needsTerminalOnLayout = true
        }
    }

    func addWorktree(_ info: WorktreeInfo, tree: SplitTree) {
        worktrees.append(info)
        trees[info.path] = tree
        sidebarVC.setWorktrees(worktrees)
        showTerminal(at: worktrees.count - 1)
    }

    func reconfigure() {
        sidebarVC.setWorktrees(worktrees)
        if !worktrees.isEmpty {
            showTerminal(at: activeWorktreeIndex)
        }
    }

    // MARK: - Terminal Display

    func showTerminal(at index: Int) {
        guard index >= 0, index < worktrees.count else { return }
        activeWorktreeIndex = index

        let info = worktrees[index]
        guard let tree = trees[info.path] else { return }

        // Get or create a SplitContainerView for this worktree
        let container: SplitContainerView
        if let existing = splitContainers[info.path] {
            container = existing
        } else {
            let newContainer = SplitContainerView(frame: terminalContainer.bounds)
            newContainer.delegate = self
            splitContainers[info.path] = newContainer
            container = newContainer
        }

        // Detach previous container only if switching to a different one
        if let prev = activeSplitContainer, prev !== container {
            prev.removeFromSuperview()
        }

        // Populate surfaceViews from SurfaceRegistry
        var surfaceViews: [String: NSView] = [:]
        for leaf in tree.allLeaves {
            if let surface = SurfaceRegistry.shared.surface(forId: leaf.surfaceId) {
                // Ensure surface is created
                if surface.surface == nil {
                    _ = surface.create(in: terminalContainer, workingDirectory: info.path, sessionName: surface.sessionName)
                }
                if let termView = surface.view {
                    surfaceViews[leaf.surfaceId] = termView
                }
            }
        }
        container.surfaceViews = surfaceViews
        container.tree = tree
        activeSplitContainer = container

        // Embed the container only when it has live surface views
        if !surfaceViews.isEmpty {
            container.frame = terminalContainer.bounds
            container.autoresizingMask = [.width, .height]
            if container.superview != terminalContainer {
                terminalContainer.addSubview(container)
            }
        }

        sidebarVC.selectWorktree(at: index)

        // Focus the tree's focused leaf (falls back to first leaf)
        let leafToFocus = tree.allLeaves.first(where: { $0.id == tree.focusedId }) ?? tree.allLeaves.first
        if let leaf = leafToFocus,
           let surface = SurfaceRegistry.shared.surface(forId: leaf.surfaceId),
           let termView = surface.view {
            view.window?.makeFirstResponder(termView)
        }

        NotificationCenter.default.post(
            name: .repoViewDidChangeWorktree,
            object: self,
            userInfo: ["worktreePath": info.path]
        )
    }

    func selectWorktree(byPath path: String) {
        guard let index = worktrees.firstIndex(where: { $0.path == path }) else { return }
        showTerminal(at: index)
    }

    func selectWorktree(branch: String) {
        guard let index = worktrees.firstIndex(where: { $0.branch == branch }) else { return }
        showTerminal(at: index)
    }

    func updateWorktreeInfos(_ newWorktrees: [WorktreeInfo]) {
        self.worktrees = newWorktrees
        sidebarVC.updateWorktreeInfos(newWorktrees)
    }

    func updateStatus(for path: String, status: AgentStatus, lastMessage: String = "") {
        sidebarVC.updateStatus(for: path, status: status, lastMessage: lastMessage)
    }

    /// Focus a specific pane (1-based index) within the current worktree's split tree.
    func focusPane(at paneIndex: Int) {
        guard let container = activeSplitContainer,
              let tree = container.tree else { return }
        let leaves = tree.allLeaves
        let zeroBasedIndex = paneIndex - 1
        guard zeroBasedIndex >= 0, zeroBasedIndex < leaves.count else { return }
        let leaf = leaves[zeroBasedIndex]
        tree.focusedId = leaf.id
        container.updateDimOverlays()
        if let surface = SurfaceRegistry.shared.surface(forId: leaf.surfaceId),
           let termView = surface.view {
            view.window?.makeFirstResponder(termView)
        }
    }

    /// Detach the active split container so it can be reparented elsewhere
    func detachActiveTerminal() {
        activeSplitContainer?.removeFromSuperview()
    }
}

private final class RepoRootView: NSView {
    var onAppearanceChange: (() -> Void)?

    override var wantsUpdateLayer: Bool { true }

    override func updateLayer() {
        layer?.backgroundColor = resolvedCGColor(SemanticColors.bg)
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        needsDisplay = true
        onAppearanceChange?()
    }
}

// MARK: - SidebarDelegate

extension RepoViewController: SidebarDelegate {
    func sidebar(_ sidebar: SidebarViewController, didSelectWorktreeAt index: Int) {
        showTerminal(at: index)
    }

    func sidebar(_ sidebar: SidebarViewController, didRequestDeleteWorktreeAt index: Int) {
        guard index >= 0, index < worktrees.count else { return }
        repoDelegate?.repoView(self, didRequestDeleteWorktree: worktrees[index])
    }

    func sidebarDidRequestNewThread(_ sidebar: SidebarViewController) {
        repoDelegate?.repoViewDidRequestNewThread(self)
    }

    func sidebar(_ sidebar: SidebarViewController, didRequestShowDiffAt index: Int) {
        guard index >= 0, index < worktrees.count else { return }
        repoDelegate?.repoView(self, didRequestShowDiffForWorktreePath: worktrees[index].path)
    }
}

// MARK: - TerminalSurfaceDelegate

extension RepoViewController: TerminalSurfaceDelegate {
    func terminalSurfaceDidRecover(_ surface: TerminalSurface) {
        // Find the worktree that owns this surface and re-embed it
        guard let container = activeSplitContainer,
              let tree = container.tree,
              tree.allSurfaceIds.contains(surface.id),
              let termView = surface.view else { return }
        container.surfaceViews[surface.id] = termView
        container.layoutTree()
    }
}

// MARK: - SplitContainerDelegate

extension RepoViewController: SplitContainerDelegate {
    func splitContainer(_ view: SplitContainerView, didChangeFocus leafId: String) {
        // Task 8 will wire focus management
    }

    func splitContainer(_ view: SplitContainerView, didRequestSplit axis: SplitAxis) {
        // Task 8 will wire split creation
    }

    func splitContainer(_ view: SplitContainerView, didRequestClosePane leafId: String) {
        // Task 8 will wire pane closing
    }

    func splitContainerDidChangeLayout(_ view: SplitContainerView) {
        // Task 8 will persist layout changes
    }
}

// MARK: - Collection safe subscript

extension Collection {
    subscript(safe index: Index) -> Element? {
        indices.contains(index) ? self[index] : nil
    }
}

extension Notification.Name {
    static let repoViewDidChangeWorktree = Notification.Name("repoViewDidChangeWorktree")
}
