import AppKit

enum SidePanelTab: Int {
    case files = 0
    case changes = 1
}

final class WorktreeSidePanelViewController: NSViewController {
    private var worktreePath: String?
    private var selectedTab: SidePanelTab
    private let makeDiffReviewView: (String) -> DiffReviewView
    private let makeYaziSurface: (NSView, String) -> Bool

    private let segmentedControl = NSSegmentedControl(
        labels: ["Files", "Changes"],
        trackingMode: .selectOne,
        target: nil,
        action: nil
    )
    private let contentView = NSView()
    private var yaziSurface: TerminalSurface?

    var selectedTabForTesting: SidePanelTab { selectedTab }
    var worktreePathForTesting: String? { worktreePath }

    init(
        worktreePath: String?,
        initialTab: SidePanelTab = .changes,
        makeDiffReviewView: @escaping (String) -> DiffReviewView = { DiffReviewView(worktreePath: $0) },
        makeYaziSurface: @escaping (NSView, String) -> Bool = WorktreeSidePanelViewController.defaultYaziLauncher
    ) {
        self.worktreePath = worktreePath
        self.selectedTab = initialTab
        self.makeDiffReviewView = makeDiffReviewView
        self.makeYaziSurface = makeYaziSurface
        super.init(nibName: nil, bundle: nil)
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) not supported") }

    deinit { yaziSurface?.destroy() }

    override func loadView() {
        let root = NSView()
        root.wantsLayer = true
        root.layer?.backgroundColor = Theme.background.cgColor
        root.setAccessibilityIdentifier("sidePanel.view")

        segmentedControl.target = self
        segmentedControl.action = #selector(tabChanged)
        segmentedControl.selectedSegment = selectedTab.rawValue
        segmentedControl.translatesAutoresizingMaskIntoConstraints = false
        root.addSubview(segmentedControl)

        contentView.translatesAutoresizingMaskIntoConstraints = false
        root.addSubview(contentView)

        NSLayoutConstraint.activate([
            segmentedControl.topAnchor.constraint(equalTo: root.topAnchor, constant: 8),
            segmentedControl.leadingAnchor.constraint(equalTo: root.leadingAnchor, constant: 8),
            segmentedControl.trailingAnchor.constraint(lessThanOrEqualTo: root.trailingAnchor, constant: -8),

            contentView.topAnchor.constraint(equalTo: segmentedControl.bottomAnchor, constant: 8),
            contentView.leadingAnchor.constraint(equalTo: root.leadingAnchor),
            contentView.trailingAnchor.constraint(equalTo: root.trailingAnchor),
            contentView.bottomAnchor.constraint(equalTo: root.bottomAnchor),
        ])

        view = root
        rebuildContent()
    }

    func setWorktree(_ path: String?) {
        guard path != worktreePath else { return }
        worktreePath = path
        if isViewLoaded { rebuildContent() }
    }

    @objc private func tabChanged() {
        selectedTab = SidePanelTab(rawValue: segmentedControl.selectedSegment) ?? .changes
        rebuildContent()
    }

    private func rebuildContent() {
        yaziSurface?.destroy()
        yaziSurface = nil
        contentView.subviews.forEach { $0.removeFromSuperview() }

        guard let path = worktreePath else {
            showPlaceholder("No worktree selected", identifier: "sidePanel.emptyPlaceholder")
            return
        }

        switch selectedTab {
        case .files:
            showFilesTab(path)
        case .changes:
            showChangesTab(path)
        }
    }

    private func showChangesTab(_ path: String) {
        let review = makeDiffReviewView(path)
        review.translatesAutoresizingMaskIntoConstraints = false
        contentView.addSubview(review)
        NSLayoutConstraint.activate([
            review.topAnchor.constraint(equalTo: contentView.topAnchor),
            review.leadingAnchor.constraint(equalTo: contentView.leadingAnchor),
            review.trailingAnchor.constraint(equalTo: contentView.trailingAnchor),
            review.bottomAnchor.constraint(equalTo: contentView.bottomAnchor),
        ])
    }

    private func showFilesTab(_ path: String) {
        showPlaceholder("Files", identifier: "sidePanel.filesPlaceholder")
    }

    private func showPlaceholder(_ message: String, identifier: String) {
        let label = NSTextField(labelWithString: message)
        label.font = NSFont.systemFont(ofSize: 13)
        label.textColor = Theme.textSecondary
        label.alignment = .center
        label.maximumNumberOfLines = 0
        label.lineBreakMode = .byWordWrapping
        label.setAccessibilityIdentifier(identifier)
        label.translatesAutoresizingMaskIntoConstraints = false
        contentView.addSubview(label)
        NSLayoutConstraint.activate([
            label.centerXAnchor.constraint(equalTo: contentView.centerXAnchor),
            label.centerYAnchor.constraint(equalTo: contentView.centerYAnchor),
            label.leadingAnchor.constraint(greaterThanOrEqualTo: contentView.leadingAnchor, constant: 16),
            label.trailingAnchor.constraint(lessThanOrEqualTo: contentView.trailingAnchor, constant: -16),
        ])
    }

    static func defaultYaziLauncher(in container: NSView, worktreePath: String) -> Bool {
        false
    }
}
