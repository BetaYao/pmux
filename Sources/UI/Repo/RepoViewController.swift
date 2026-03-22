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
    private var surfaces: [String: TerminalSurface] = [:]
    private var activeWorktreeIndex: Int = 0
    private var activeSurface: TerminalSurface?
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
            activeSurface?.syncSize()
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

// MARK: - Collection safe subscript

extension Collection {
    subscript(safe index: Index) -> Element? {
        indices.contains(index) ? self[index] : nil
    }
}
