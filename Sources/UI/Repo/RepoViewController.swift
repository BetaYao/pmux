import AppKit

protocol RepoViewDelegate: AnyObject {
    func repoView(_ repoVC: RepoViewController, didRequestDeleteWorktree info: WorktreeInfo)
}

/// Full repo view: sidebar (thread list) + single immersive terminal
class RepoViewController: NSViewController {
    weak var repoDelegate: RepoViewDelegate?
    private let sidebarVC = SidebarViewController()

    // Two-column layout views
    private let sidebarContainer = NSView()
    private let terminalContainer = NSView()
    private var sidebarWidthConstraint: NSLayoutConstraint!
    private var stackConstraints: [NSLayoutConstraint] = []
    private var isStacked = false

    private var worktrees: [WorktreeInfo] = []
    private var surfaces: [String: TerminalSurface] = [:]
    private var activeWorktreeIndex: Int = 0
    private var activeSurface: TerminalSurface?
    private var needsTerminalOnLayout = false

    override func loadView() {
        self.view = NSView()
        view.wantsLayer = true
        view.layer?.backgroundColor = SemanticColors.bg.cgColor

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
        terminalContainer.layer?.backgroundColor = SemanticColors.panel2.cgColor
        terminalContainer.layer?.borderWidth = 1
        terminalContainer.layer?.borderColor = SemanticColors.line.withAlphaComponent(0.38).cgColor
        terminalContainer.layer?.cornerRadius = 8
        terminalContainer.layer?.maskedCorners = [.layerMinXMinYCorner, .layerMinXMaxYCorner]
        terminalContainer.setAccessibilityIdentifier("project.terminal")
        terminalContainer.setAccessibilityElement(true)
        terminalContainer.setAccessibilityRole(.group)
        view.addSubview(terminalContainer)

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

        // Update dynamic border color (appearance changes)
        terminalContainer.layer?.borderColor = SemanticColors.line.withAlphaComponent(0.38).cgColor
        terminalContainer.layer?.backgroundColor = SemanticColors.panel2.cgColor

        if needsTerminalOnLayout && terminalContainer.bounds.width > 0 {
            needsTerminalOnLayout = false
            showTerminal(at: activeWorktreeIndex)
        } else {
            activeSurface?.syncSize()
        }
    }

    private func applyLayout() {
        NSLayoutConstraint.deactivate(stackConstraints)
        stackConstraints.removeAll()

        let gap: CGFloat = 12

        if isStacked {
            // Vertical stack: 220px sidebar on top, terminal below
            sidebarWidthConstraint.isActive = false
            let sidebarHeight = sidebarContainer.heightAnchor.constraint(equalToConstant: 220)

            stackConstraints = [
                sidebarHeight,
                sidebarContainer.topAnchor.constraint(equalTo: view.topAnchor),
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
                sidebarContainer.topAnchor.constraint(equalTo: view.topAnchor),
                sidebarContainer.leadingAnchor.constraint(equalTo: view.leadingAnchor),
                sidebarContainer.bottomAnchor.constraint(equalTo: view.bottomAnchor),

                terminalContainer.topAnchor.constraint(equalTo: view.topAnchor),
                terminalContainer.leadingAnchor.constraint(equalTo: sidebarContainer.trailingAnchor, constant: gap),
                terminalContainer.trailingAnchor.constraint(equalTo: view.trailingAnchor),
                terminalContainer.bottomAnchor.constraint(equalTo: view.bottomAnchor),
            ]

            // No rounded corners on right edge (full height)
            terminalContainer.layer?.maskedCorners = [.layerMinXMinYCorner, .layerMinXMaxYCorner]
        }

        NSLayoutConstraint.activate(stackConstraints)
    }

    // MARK: - Configuration

    func configure(worktrees: [WorktreeInfo], surfaces: [String: TerminalSurface]) {
        self.worktrees = worktrees
        self.surfaces = surfaces
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
        guard let surface = surfaces[info.path] else { return }

        // Detach previous surface
        activeSurface?.view?.removeFromSuperview()

        if surface.surface == nil {
            _ = surface.create(in: terminalContainer, workingDirectory: info.path, sessionName: surface.sessionName)
        } else {
            surface.reparent(to: terminalContainer)
        }

        activeSurface = surface
        sidebarVC.selectWorktree(at: index)

        DispatchQueue.main.async { [weak self] in
            self?.view.window?.makeFirstResponder(surface.view)
        }
    }

    func selectWorktree(byPath path: String) {
        guard let index = worktrees.firstIndex(where: { $0.path == path }) else { return }
        showTerminal(at: index)
    }

    func selectWorktree(branch: String) {
        guard let index = worktrees.firstIndex(where: { $0.branch == branch }) else { return }
        showTerminal(at: index)
    }

    func updateStatus(for path: String, status: AgentStatus, lastMessage: String = "") {
        sidebarVC.updateStatus(for: path, status: status, lastMessage: lastMessage)
    }

    /// Detach the active terminal so it can be reparented elsewhere
    func detachActiveTerminal() {
        activeSurface?.view?.removeFromSuperview()
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

// MARK: - Collection safe subscript

extension Collection {
    subscript(safe index: Index) -> Element? {
        indices.contains(index) ? self[index] : nil
    }
}
