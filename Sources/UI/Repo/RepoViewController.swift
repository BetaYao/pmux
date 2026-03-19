import AppKit

protocol RepoViewDelegate: AnyObject {
    func repoView(_ repoVC: RepoViewController, didRequestDeleteWorktree info: WorktreeInfo)
}

/// Full repo view: sidebar (worktree list) + terminal area with split panes
class RepoViewController: NSViewController {
    weak var repoDelegate: RepoViewDelegate?
    private let splitView = NSSplitView()
    private let sidebarVC = SidebarViewController()
    private let terminalSplitView = TerminalSplitView()

    private var worktrees: [WorktreeInfo] = []
    private var surfaces: [String: TerminalSurface] = [:]
    private var activeWorktreeIndex: Int = 0
    private var needsTerminalOnLayout = false
    private var isInLayout = false
    private var didSetInitialDivider = false

    /// Extra surfaces created by splits (not in the main surfaces dict)
    private var splitSurfaces: [TerminalSurface] = []

    override func loadView() {
        self.view = NSView()
        view.wantsLayer = true
        view.layer?.backgroundColor = Theme.background.cgColor

        // Split view: sidebar | terminal area
        splitView.isVertical = true
        splitView.dividerStyle = .thin
        splitView.delegate = self
        splitView.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(splitView)

        // Sidebar
        sidebarVC.sidebarDelegate = self
        let sidebarView = sidebarVC.view
        sidebarView.translatesAutoresizingMaskIntoConstraints = true

        // Terminal split area
        terminalSplitView.translatesAutoresizingMaskIntoConstraints = true

        splitView.addSubview(sidebarView)
        splitView.addSubview(terminalSplitView)
        splitView.adjustSubviews()

        NSLayoutConstraint.activate([
            splitView.topAnchor.constraint(equalTo: view.topAnchor),
            splitView.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            splitView.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            splitView.bottomAnchor.constraint(equalTo: view.bottomAnchor),
        ])
    }

    override func viewDidLayout() {
        super.viewDidLayout()
        guard !isInLayout else { return }
        isInLayout = true
        defer { isInLayout = false }

        let totalWidth = splitView.bounds.width
        if totalWidth > 0 && !didSetInitialDivider {
            didSetInitialDivider = true
            splitView.setPosition(200, ofDividerAt: 0)
        }

        if needsTerminalOnLayout && terminalSplitView.bounds.width > 0 {
            needsTerminalOnLayout = false
            showTerminal(at: activeWorktreeIndex)
        } else {
            terminalSplitView.syncAllSurfaceSizes()
        }
    }

    func configure(worktrees: [WorktreeInfo], surfaces: [String: TerminalSurface]) {
        self.worktrees = worktrees
        self.surfaces = surfaces
        sidebarVC.setWorktrees(worktrees)

        if !worktrees.isEmpty {
            if terminalSplitView.bounds.width > 0 {
                showTerminal(at: 0)
            } else {
                activeWorktreeIndex = 0
                needsTerminalOnLayout = true
            }
        }
    }

    private func showTerminal(at index: Int) {
        guard index >= 0, index < worktrees.count else { return }
        activeWorktreeIndex = index

        let info = worktrees[index]
        guard let surface = surfaces[info.path] else { return }

        // Clean up split surfaces from previous worktree
        for s in splitSurfaces {
            s.destroy()
        }
        splitSurfaces.removeAll()

        terminalSplitView.setSingleTerminal(surface: surface, workingDirectory: info.path)
        sidebarVC.selectWorktree(at: index)
    }

    func selectWorktree(byPath path: String) {
        guard let index = worktrees.firstIndex(where: { $0.path == path }) else { return }
        showTerminal(at: index)
    }

    // MARK: - Split Pane Operations

    func splitVertical() {
        guard let info = worktrees[safe: activeWorktreeIndex] else { return }
        if let newSurface = terminalSplitView.splitFocused(
            orientation: .vertical,
            worktreePath: info.path,
            sessionName: nil  // Split panes don't need tmux sessions
        ) {
            splitSurfaces.append(newSurface)
        }
    }

    func splitHorizontal() {
        guard let info = worktrees[safe: activeWorktreeIndex] else { return }
        if let newSurface = terminalSplitView.splitFocused(
            orientation: .horizontal,
            worktreePath: info.path,
            sessionName: nil
        ) {
            splitSurfaces.append(newSurface)
        }
    }

    func closePane() {
        guard terminalSplitView.paneCount > 1 else { return }
        _ = terminalSplitView.closeFocused()
    }

    func updateStatus(for path: String, status: AgentStatus, lastMessage: String = "") {
        sidebarVC.updateStatus(for: path, status: status, lastMessage: lastMessage)
    }

    /// Detach all terminals so they can be reparented elsewhere
    func detachActiveTerminal() {
        terminalSplitView.detachAll()
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
}

// MARK: - NSSplitViewDelegate

extension RepoViewController: NSSplitViewDelegate {
    func splitView(_ splitView: NSSplitView, constrainMinCoordinate proposedMinimumPosition: CGFloat, ofSubviewAt dividerIndex: Int) -> CGFloat {
        return 140
    }

    func splitView(_ splitView: NSSplitView, constrainMaxCoordinate proposedMaximumPosition: CGFloat, ofSubviewAt dividerIndex: Int) -> CGFloat {
        return 350
    }

    func splitView(_ splitView: NSSplitView, canCollapseSubview subview: NSView) -> Bool {
        return subview === sidebarVC.view
    }

    func splitViewDidResizeSubviews(_ notification: Notification) {
        terminalSplitView.syncAllSurfaceSizes()
    }
}

// MARK: - Collection safe subscript

extension Collection {
    subscript(safe index: Index) -> Element? {
        indices.contains(index) ? self[index] : nil
    }
}
