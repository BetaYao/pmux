import AppKit

/// Full repo view: sidebar (worktree list) + terminal area
class RepoViewController: NSViewController {
    private let splitView = NSSplitView()
    private let sidebarVC = SidebarViewController()
    private let terminalContainer = NSView()

    private var worktrees: [WorktreeInfo] = []
    private var surfaces: [String: TerminalSurface] = [:]
    private var activeWorktreeIndex: Int = 0
    private var needsTerminalOnLayout = false

    override func loadView() {
        self.view = NSView()
        view.wantsLayer = true
        view.layer?.backgroundColor = Theme.background.cgColor

        // Split view: sidebar | terminal
        splitView.isVertical = true
        splitView.dividerStyle = .thin
        splitView.delegate = self
        splitView.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(splitView)

        // Sidebar — let NSSplitView manage its frame
        sidebarVC.sidebarDelegate = self
        let sidebarView = sidebarVC.view
        sidebarView.translatesAutoresizingMaskIntoConstraints = true

        // Terminal area — let NSSplitView manage its frame
        terminalContainer.wantsLayer = true
        terminalContainer.layer?.backgroundColor = Theme.background.cgColor
        terminalContainer.translatesAutoresizingMaskIntoConstraints = true

        splitView.addSubview(sidebarView)
        splitView.addSubview(terminalContainer)
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

        // Set sidebar width after layout
        let totalWidth = splitView.bounds.width
        if totalWidth > 0 {
            let sidebarWidth: CGFloat = 200
            splitView.setPosition(sidebarWidth, ofDividerAt: 0)
        }

        // Show terminal once we have a real frame
        if needsTerminalOnLayout && terminalContainer.bounds.width > 0 {
            needsTerminalOnLayout = false
            showTerminal(at: activeWorktreeIndex)
        }
    }

    func configure(worktrees: [WorktreeInfo], surfaces: [String: TerminalSurface]) {
        self.worktrees = worktrees
        self.surfaces = surfaces
        sidebarVC.setWorktrees(worktrees)

        if !worktrees.isEmpty {
            if terminalContainer.bounds.width > 0 {
                showTerminal(at: 0)
            } else {
                // Defer until layout gives us a real frame
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

        // Remove any existing terminal view from container
        for sub in terminalContainer.subviews {
            sub.removeFromSuperview()
        }

        if surface.surface == nil {
            _ = surface.create(in: terminalContainer, workingDirectory: info.path, sessionName: surface.sessionName)
        } else {
            surface.reparent(to: terminalContainer)
        }

        // Ensure correct size after reparent
        surface.syncSize()
        surface.syncContentScale()

        // Give it keyboard focus
        DispatchQueue.main.async { [weak self] in
            self?.view.window?.makeFirstResponder(surface.view)
        }

        sidebarVC.selectWorktree(at: index)
    }

    func updateStatus(for path: String, status: AgentStatus) {
        sidebarVC.updateStatus(for: path, status: status)
    }

    /// Detach terminal so it can be reparented elsewhere
    func detachActiveTerminal() {
        if let info = worktrees[safe: activeWorktreeIndex],
           let surface = surfaces[info.path] {
            surface.view?.removeFromSuperview()
        }
    }
}

// MARK: - SidebarDelegate

extension RepoViewController: SidebarDelegate {
    func sidebar(_ sidebar: SidebarViewController, didSelectWorktreeAt index: Int) {
        showTerminal(at: index)
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
}

// MARK: - Collection safe subscript

extension Collection {
    subscript(safe index: Index) -> Element? {
        indices.contains(index) ? self[index] : nil
    }
}
