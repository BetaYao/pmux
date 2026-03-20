import AppKit

/// Overlay panel showing git diff for a worktree.
/// Shows file list on left, diff content on right.
class DiffOverlayViewController: NSViewController {
    private let splitView = NSSplitView()
    private let fileListView = NSTableView()
    private let fileScrollView = NSScrollView()
    private let diffTextView = NSTextView()
    private let diffScrollView = NSScrollView()
    private let headerLabel = NSTextField(labelWithString: "")
    private let closeButton = NSButton()

    private var files: [DiffFile] = []
    private var changedFiles: [(status: String, path: String)] = []
    private var worktreePath: String = ""

    init(worktreePath: String) {
        self.worktreePath = worktreePath
        super.init(nibName: nil, bundle: nil)
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) not supported")
    }

    override func loadView() {
        let container = NSView(frame: NSRect(x: 0, y: 0, width: 800, height: 500))
        container.wantsLayer = true
        container.layer?.backgroundColor = Theme.background.cgColor
        container.setAccessibilityIdentifier("repo.diffOverlay")
        container.setAccessibilityElement(true)
        container.setAccessibilityRole(.group)
        self.view = container

        // Header
        headerLabel.font = NSFont.systemFont(ofSize: 14, weight: .semibold)
        headerLabel.textColor = Theme.textPrimary
        headerLabel.translatesAutoresizingMaskIntoConstraints = false
        container.addSubview(headerLabel)

        closeButton.title = "Close"
        closeButton.bezelStyle = .rounded
        closeButton.keyEquivalent = "\u{1b}"
        closeButton.target = self
        closeButton.action = #selector(closeClicked)
        closeButton.translatesAutoresizingMaskIntoConstraints = false
        container.addSubview(closeButton)

        // Split view
        splitView.isVertical = true
        splitView.dividerStyle = .thin
        splitView.translatesAutoresizingMaskIntoConstraints = false
        container.addSubview(splitView)

        // File list (left)
        fileScrollView.hasVerticalScroller = true
        fileScrollView.drawsBackground = false
        fileScrollView.borderType = .noBorder
        fileScrollView.translatesAutoresizingMaskIntoConstraints = true

        fileListView.backgroundColor = .clear
        fileListView.headerView = nil
        fileListView.rowHeight = 24
        fileListView.delegate = self
        fileListView.dataSource = self
        let col = NSTableColumn(identifier: NSUserInterfaceItemIdentifier("file"))
        col.resizingMask = .autoresizingMask
        fileListView.addTableColumn(col)
        fileScrollView.documentView = fileListView

        // Diff content (right)
        diffScrollView.hasVerticalScroller = true
        diffScrollView.hasHorizontalScroller = true
        diffScrollView.drawsBackground = false
        diffScrollView.borderType = .noBorder
        diffScrollView.translatesAutoresizingMaskIntoConstraints = true

        diffTextView.isEditable = false
        diffTextView.isSelectable = true
        diffTextView.font = NSFont.monospacedSystemFont(ofSize: 12, weight: .regular)
        diffTextView.backgroundColor = Theme.background
        diffTextView.textColor = Theme.textPrimary
        diffTextView.textContainerInset = NSSize(width: 8, height: 8)
        diffTextView.isHorizontallyResizable = true
        diffTextView.textContainer?.widthTracksTextView = false
        diffTextView.textContainer?.containerSize = NSSize(width: CGFloat.greatestFiniteMagnitude, height: CGFloat.greatestFiniteMagnitude)
        diffScrollView.documentView = diffTextView

        splitView.addSubview(fileScrollView)
        splitView.addSubview(diffScrollView)
        splitView.adjustSubviews()

        NSLayoutConstraint.activate([
            headerLabel.topAnchor.constraint(equalTo: container.topAnchor, constant: 12),
            headerLabel.leadingAnchor.constraint(equalTo: container.leadingAnchor, constant: 16),

            closeButton.topAnchor.constraint(equalTo: container.topAnchor, constant: 8),
            closeButton.trailingAnchor.constraint(equalTo: container.trailingAnchor, constant: -12),

            splitView.topAnchor.constraint(equalTo: headerLabel.bottomAnchor, constant: 8),
            splitView.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            splitView.trailingAnchor.constraint(equalTo: container.trailingAnchor),
            splitView.bottomAnchor.constraint(equalTo: container.bottomAnchor),
        ])

        loadDiff()
    }

    override func viewDidLayout() {
        super.viewDidLayout()
        splitView.setPosition(220, ofDividerAt: 0)
    }

    private func loadDiff() {
        DispatchQueue.global().async { [weak self] in
            guard let self else { return }
            let files = GitDiff.diff(worktreePath: self.worktreePath)
            let changed = GitDiff.changedFiles(worktreePath: self.worktreePath)
            let stat = GitDiff.diffStat(worktreePath: self.worktreePath)

            DispatchQueue.main.async {
                self.files = files
                self.changedFiles = changed
                let totalAdd = files.reduce(0) { $0 + $1.additions }
                let totalDel = files.reduce(0) { $0 + $1.deletions }
                self.headerLabel.stringValue = "Changes: \(changed.count) files  +\(totalAdd) -\(totalDel)"
                self.fileListView.reloadData()

                // Show all diffs by default
                self.showAllDiffs()
            }
        }
    }

    private func showAllDiffs() {
        let attributed = NSMutableAttributedString()
        let monoFont = NSFont.monospacedSystemFont(ofSize: 12, weight: .regular)

        for file in files {
            // File header
            let fileHeader = "\n━━━ \(file.path) (+\(file.additions) -\(file.deletions)) ━━━\n\n"
            attributed.append(NSAttributedString(string: fileHeader, attributes: [
                .font: NSFont.monospacedSystemFont(ofSize: 12, weight: .bold),
                .foregroundColor: Theme.accent,
            ]))

            for hunk in file.hunks {
                // Hunk header
                attributed.append(NSAttributedString(string: hunk.header + "\n", attributes: [
                    .font: monoFont,
                    .foregroundColor: NSColor.systemCyan,
                ]))

                for line in hunk.lines {
                    let prefix: String
                    let color: NSColor
                    switch line.type {
                    case .addition:
                        prefix = "+"
                        color = NSColor.systemGreen
                    case .deletion:
                        prefix = "-"
                        color = NSColor.systemRed
                    case .context:
                        prefix = " "
                        color = Theme.textSecondary
                    }
                    attributed.append(NSAttributedString(string: prefix + line.content + "\n", attributes: [
                        .font: monoFont,
                        .foregroundColor: color,
                    ]))
                }
            }
        }

        if files.isEmpty {
            attributed.append(NSAttributedString(string: "No changes", attributes: [
                .font: monoFont,
                .foregroundColor: Theme.textSecondary,
            ]))
        }

        diffTextView.textStorage?.setAttributedString(attributed)
    }

    private func showDiffForFile(at index: Int) {
        guard index >= 0, index < files.count else {
            showAllDiffs()
            return
        }

        let file = files[index]
        let attributed = NSMutableAttributedString()
        let monoFont = NSFont.monospacedSystemFont(ofSize: 12, weight: .regular)

        for hunk in file.hunks {
            attributed.append(NSAttributedString(string: hunk.header + "\n", attributes: [
                .font: monoFont,
                .foregroundColor: NSColor.systemCyan,
            ]))

            for line in hunk.lines {
                let prefix: String
                let color: NSColor
                switch line.type {
                case .addition: prefix = "+"; color = NSColor.systemGreen
                case .deletion: prefix = "-"; color = NSColor.systemRed
                case .context:  prefix = " "; color = Theme.textSecondary
                }
                attributed.append(NSAttributedString(string: prefix + line.content + "\n", attributes: [
                    .font: monoFont,
                    .foregroundColor: color,
                ]))
            }
        }

        diffTextView.textStorage?.setAttributedString(attributed)
    }

    @objc private func closeClicked() {
        dismiss(nil)
    }
}

// MARK: - NSTableViewDataSource

extension DiffOverlayViewController: NSTableViewDataSource {
    func numberOfRows(in tableView: NSTableView) -> Int {
        return changedFiles.count
    }
}

// MARK: - NSTableViewDelegate

extension DiffOverlayViewController: NSTableViewDelegate {
    func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView? {
        let item = changedFiles[row]

        let cell = NSView()
        cell.wantsLayer = true

        // Status indicator
        let statusLabel = NSTextField(labelWithString: item.status)
        statusLabel.font = NSFont.monospacedSystemFont(ofSize: 11, weight: .bold)
        statusLabel.textColor = statusColor(item.status)
        statusLabel.frame = NSRect(x: 8, y: 2, width: 20, height: 20)
        cell.addSubview(statusLabel)

        // File path
        let pathLabel = NSTextField(labelWithString: item.path)
        pathLabel.font = NSFont.monospacedSystemFont(ofSize: 11, weight: .regular)
        pathLabel.textColor = Theme.textPrimary
        pathLabel.lineBreakMode = .byTruncatingHead
        pathLabel.frame = NSRect(x: 30, y: 2, width: 180, height: 20)
        cell.addSubview(pathLabel)

        return cell
    }

    func tableViewSelectionDidChange(_ notification: Notification) {
        let row = fileListView.selectedRow
        if row >= 0 {
            // Find matching diff file
            let selectedPath = changedFiles[row].path
            if let fileIndex = files.firstIndex(where: { $0.path == selectedPath }) {
                showDiffForFile(at: fileIndex)
            }
        } else {
            showAllDiffs()
        }
    }

    private func statusColor(_ status: String) -> NSColor {
        switch status {
        case "A", "??": return NSColor.systemGreen
        case "M":       return NSColor.systemYellow
        case "D":       return NSColor.systemRed
        case "R":       return NSColor.systemCyan
        default:        return Theme.textSecondary
        }
    }
}
