import AppKit

enum SidePanelTab: Int {
    case files = 0
    case changes = 1
}

protocol WorktreeSidePanelDelegate: AnyObject {
    func sidePanel(_ vc: WorktreeSidePanelViewController, didSelectFile path: String)
    func sidePanel(_ vc: WorktreeSidePanelViewController, didSelectChange path: String)
}

final class WorktreeSidePanelViewController: NSViewController {
    private var worktreePath: String?
    private var selectedTab: SidePanelTab

    weak var delegate: WorktreeSidePanelDelegate?

    private let segmentedControl = NSSegmentedControl(
        labels: ["Files", "Changes"],
        trackingMode: .selectOne,
        target: nil,
        action: nil
    )
    private let contentView = NSView()

    // Files tab
    private var fileTreeController: FileTreeOutlineController?

    // Changes tab
    private var changesTableView: NSTableView?
    private var changesScrollView: NSScrollView?
    private var changedFiles: [GitChangedFile] = []

    var selectedTabForTesting: SidePanelTab { selectedTab }
    var worktreePathForTesting: String? { worktreePath }

    init(worktreePath: String?, initialTab: SidePanelTab = .files) {
        self.worktreePath = worktreePath
        self.selectedTab = initialTab
        super.init(nibName: nil, bundle: nil)
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) not supported") }

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

    // MARK: - Internal selection handlers (called by tree/table, forwarded to delegate)

    func handleFileSelection(_ path: String) {
        delegate?.sidePanel(self, didSelectFile: path)
    }

    func handleChangeSelection(_ path: String) {
        delegate?.sidePanel(self, didSelectChange: path)
    }

    @objc private func tabChanged() {
        selectedTab = SidePanelTab(rawValue: segmentedControl.selectedSegment) ?? .files
        rebuildContent()
    }

    private func rebuildContent() {
        contentView.subviews.forEach { $0.removeFromSuperview() }
        fileTreeController = nil
        changesTableView = nil
        changesScrollView = nil

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

    private func showFilesTab(_ path: String) {
        let controller = FileTreeOutlineController(rootPath: path)
        controller.onSelectFile = { [weak self] filePath in
            self?.handleFileSelection(filePath)
        }
        fileTreeController = controller

        let scrollView = NSScrollView()
        scrollView.documentView = controller.outlineView
        scrollView.hasVerticalScroller = true
        scrollView.autohidesScrollers = true
        scrollView.translatesAutoresizingMaskIntoConstraints = false
        contentView.addSubview(scrollView)

        NSLayoutConstraint.activate([
            scrollView.topAnchor.constraint(equalTo: contentView.topAnchor),
            scrollView.leadingAnchor.constraint(equalTo: contentView.leadingAnchor),
            scrollView.trailingAnchor.constraint(equalTo: contentView.trailingAnchor),
            scrollView.bottomAnchor.constraint(equalTo: contentView.bottomAnchor),
        ])
    }

    private func showChangesTab(_ path: String) {
        changedFiles = GitDiff.changedFileEntries(worktreePath: path)

        if changedFiles.isEmpty {
            showPlaceholder("No changes", identifier: "sidePanel.changesEmpty")
            return
        }

        let column = NSTableColumn(identifier: NSUserInterfaceItemIdentifier("ChangeColumn"))
        column.title = "Changed Files"

        let tableView = NSTableView()
        tableView.addTableColumn(column)
        tableView.headerView = nil
        tableView.rowHeight = 22
        tableView.dataSource = self
        tableView.delegate = self
        tableView.target = self
        tableView.action = #selector(changeRowClicked)
        tableView.setAccessibilityIdentifier("sidePanel.changesTable")

        let scrollView = NSScrollView()
        scrollView.documentView = tableView
        scrollView.hasVerticalScroller = true
        scrollView.autohidesScrollers = true
        scrollView.translatesAutoresizingMaskIntoConstraints = false
        contentView.addSubview(scrollView)

        NSLayoutConstraint.activate([
            scrollView.topAnchor.constraint(equalTo: contentView.topAnchor),
            scrollView.leadingAnchor.constraint(equalTo: contentView.leadingAnchor),
            scrollView.trailingAnchor.constraint(equalTo: contentView.trailingAnchor),
            scrollView.bottomAnchor.constraint(equalTo: contentView.bottomAnchor),
        ])

        changesTableView = tableView
        changesScrollView = scrollView
    }

    @objc private func changeRowClicked() {
        guard let tableView = changesTableView else { return }
        let row = tableView.clickedRow
        guard row >= 0, row < changedFiles.count else { return }
        handleChangeSelection(changedFiles[row].path)
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
}

// MARK: - NSTableViewDataSource / Delegate (Changes tab)

extension WorktreeSidePanelViewController: NSTableViewDataSource, NSTableViewDelegate {
    func numberOfRows(in tableView: NSTableView) -> Int {
        changedFiles.count
    }

    func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView? {
        let entry = changedFiles[row]
        let badge: String
        switch entry.status {
        case .added:    badge = "A"
        case .modified: badge = "M"
        case .deleted:  badge = "D"
        case .renamed:  badge = "R"
        case .unknown:  badge = "?"
        }

        let id = NSUserInterfaceItemIdentifier("ChangeCell")
        let cellView: NSTableCellView
        if let reused = tableView.makeView(withIdentifier: id, owner: self) as? NSTableCellView {
            cellView = reused
        } else {
            cellView = NSTableCellView()
            cellView.identifier = id

            let badgeLabel = NSTextField(labelWithString: "")
            badgeLabel.font = NSFont.monospacedSystemFont(ofSize: 11, weight: .semibold)
            badgeLabel.translatesAutoresizingMaskIntoConstraints = false
            badgeLabel.tag = 100

            let pathLabel = NSTextField(labelWithString: "")
            pathLabel.font = NSFont.systemFont(ofSize: 12)
            pathLabel.lineBreakMode = .byTruncatingMiddle
            pathLabel.translatesAutoresizingMaskIntoConstraints = false
            cellView.textField = pathLabel

            cellView.addSubview(badgeLabel)
            cellView.addSubview(pathLabel)

            NSLayoutConstraint.activate([
                badgeLabel.leadingAnchor.constraint(equalTo: cellView.leadingAnchor, constant: 6),
                badgeLabel.centerYAnchor.constraint(equalTo: cellView.centerYAnchor),
                badgeLabel.widthAnchor.constraint(equalToConstant: 14),

                pathLabel.leadingAnchor.constraint(equalTo: badgeLabel.trailingAnchor, constant: 6),
                pathLabel.trailingAnchor.constraint(equalTo: cellView.trailingAnchor, constant: -6),
                pathLabel.centerYAnchor.constraint(equalTo: cellView.centerYAnchor),
            ])
        }

        (cellView.viewWithTag(100) as? NSTextField)?.stringValue = badge
        cellView.textField?.stringValue = entry.path
        return cellView
    }
}
