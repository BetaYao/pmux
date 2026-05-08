import AppKit

enum WorktreeInspectorInitialTab: Int {
    case files = 0
    case changes = 1
}

final class WorktreeInspectorViewController: NSViewController {
    private let worktreePath: String
    private let yaziAvailability: () -> Bool
    private let segmentedControl = NSSegmentedControl(
        labels: ["Files", "Changes"],
        trackingMode: .selectOne,
        target: nil,
        action: nil
    )
    private let contentView = NSView()
    private var selectedTab: WorktreeInspectorInitialTab

    var selectedTabForTesting: WorktreeInspectorInitialTab { selectedTab }

    init(
        worktreePath: String,
        initialTab: WorktreeInspectorInitialTab,
        yaziAvailability: @escaping () -> Bool = { ProcessRunner.commandExists("yazi") }
    ) {
        self.worktreePath = worktreePath
        self.selectedTab = initialTab
        self.yaziAvailability = yaziAvailability
        super.init(nibName: nil, bundle: nil)
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) not supported")
    }

    override func loadView() {
        let root = NSView(frame: NSRect(x: 0, y: 0, width: 1080, height: 700))
        root.wantsLayer = true
        root.layer?.backgroundColor = Theme.background.cgColor
        root.setAccessibilityIdentifier("worktreeInspector")

        let title = NSTextField(labelWithString: URL(fileURLWithPath: worktreePath).lastPathComponent)
        title.font = NSFont.systemFont(ofSize: 14, weight: .semibold)
        title.textColor = Theme.textPrimary
        title.lineBreakMode = .byTruncatingMiddle
        title.translatesAutoresizingMaskIntoConstraints = false
        root.addSubview(title)

        segmentedControl.target = self
        segmentedControl.action = #selector(tabChanged)
        segmentedControl.selectedSegment = selectedTab.rawValue
        segmentedControl.translatesAutoresizingMaskIntoConstraints = false
        root.addSubview(segmentedControl)

        contentView.translatesAutoresizingMaskIntoConstraints = false
        root.addSubview(contentView)

        NSLayoutConstraint.activate([
            title.topAnchor.constraint(equalTo: root.topAnchor, constant: 12),
            title.leadingAnchor.constraint(equalTo: root.leadingAnchor, constant: 16),
            title.trailingAnchor.constraint(lessThanOrEqualTo: segmentedControl.leadingAnchor, constant: -16),

            segmentedControl.topAnchor.constraint(equalTo: root.topAnchor, constant: 8),
            segmentedControl.trailingAnchor.constraint(equalTo: root.trailingAnchor, constant: -16),
            segmentedControl.widthAnchor.constraint(equalToConstant: 180),

            contentView.topAnchor.constraint(equalTo: title.bottomAnchor, constant: 12),
            contentView.leadingAnchor.constraint(equalTo: root.leadingAnchor),
            contentView.trailingAnchor.constraint(equalTo: root.trailingAnchor),
            contentView.bottomAnchor.constraint(equalTo: root.bottomAnchor),
        ])

        view = root
        showSelectedTab()
    }

    @objc private func tabChanged() {
        selectedTab = WorktreeInspectorInitialTab(rawValue: segmentedControl.selectedSegment) ?? .files
        showSelectedTab()
    }

    private func showSelectedTab() {
        contentView.subviews.forEach { $0.removeFromSuperview() }
        switch selectedTab {
        case .files:
            showFilesTab()
        case .changes:
            showChangesTab()
        }
    }

    private func showFilesTab() {
        guard yaziAvailability() else {
            showMessage(
                "Yazi is not installed. Install yazi to browse files in this tab.",
                identifier: "worktreeInspector.filesMissingYazi"
            )
            return
        }

        showMessage("Yazi file browser will appear here.", identifier: "worktreeInspector.filesPlaceholder")
    }

    private func showChangesTab() {
        showMessage("Changes will appear here.", identifier: "worktreeInspector.changesPlaceholder")
    }

    private func showMessage(_ message: String, identifier: String) {
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
            label.leadingAnchor.constraint(greaterThanOrEqualTo: contentView.leadingAnchor, constant: 24),
            label.trailingAnchor.constraint(lessThanOrEqualTo: contentView.trailingAnchor, constant: -24),
        ])
    }
}
