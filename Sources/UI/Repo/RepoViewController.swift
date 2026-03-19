import AppKit

/// Full repo view: sidebar (worktree list) + terminal area
class RepoViewController: NSViewController {
    private let splitView = NSSplitView()
    private let sidebarVC = SidebarViewController()
    private let terminalContainer = NSView()

    private var worktrees: [WorktreeInfo] = []
    private var surfaces: [String: TerminalSurface] = [:]  // shared with MainWindowController
    private var activeWorktreeIndex: Int = 0

    override func loadView() {
        self.view = NSView()
        view.wantsLayer = true
        view.layer?.backgroundColor = Theme.background.cgColor

        // Split view: sidebar | terminal
        splitView.isVertical = true
        splitView.dividerStyle = .thin
        splitView.translatesAutoresizingMaskIntoConstraints = false
        splitView.delegate = self
        view.addSubview(splitView)

        // Sidebar
        sidebarVC.sidebarDelegate = self
        let sidebarView = sidebarVC.view
        sidebarView.translatesAutoresizingMaskIntoConstraints = false

        // Terminal area
        terminalContainer.wantsLayer = true
        terminalContainer.layer?.backgroundColor = Theme.background.cgColor
        terminalContainer.translatesAutoresizingMaskIntoConstraints = false

        splitView.addArrangedSubview(sidebarView)
        splitView.addArrangedSubview(terminalContainer)

        // Set sidebar initial width
        splitView.setPosition(200, ofDividerAt: 0)

        NSLayoutConstraint.activate([
            splitView.topAnchor.constraint(equalTo: view.topAnchor),
            splitView.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            splitView.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            splitView.bottomAnchor.constraint(equalTo: view.bottomAnchor),
        ])
    }

    func configure(worktrees: [WorktreeInfo], surfaces: [String: TerminalSurface]) {
        self.worktrees = worktrees
        self.surfaces = surfaces
        sidebarVC.setWorktrees(worktrees)

        // Show first worktree's terminal
        if !worktrees.isEmpty {
            showTerminal(at: 0)
        }
    }

    private func showTerminal(at index: Int) {
        guard index >= 0, index < worktrees.count else { return }
        activeWorktreeIndex = index

        let info = worktrees[index]
        guard let surface = surfaces[info.path] else { return }

        // Reparent the terminal to our container
        if surface.surface == nil {
            _ = surface.create(in: terminalContainer, workingDirectory: info.path)
        } else {
            surface.reparent(to: terminalContainer)
        }

        // Give it keyboard focus
        view.window?.makeFirstResponder(surface.view)
    }

    /// Return the active terminal surface so it can be reparented elsewhere when leaving this tab
    func detachActiveTerminal() {
        // Just remove from superview; the surface stays alive
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
        return 140  // min sidebar width
    }

    func splitView(_ splitView: NSSplitView, constrainMaxCoordinate proposedMaximumPosition: CGFloat, ofSubviewAt dividerIndex: Int) -> CGFloat {
        return 350  // max sidebar width
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
