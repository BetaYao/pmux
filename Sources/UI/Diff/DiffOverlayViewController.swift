import AppKit

// MARK: - File tree model

/// A node in the file tree: either a directory (with children) or a leaf file.
private class FileTreeNode {
    let name: String
    let fullPath: String        // relative path from repo root
    var status: String?         // nil for directories
    var children: [FileTreeNode] = []
    var isDirectory: Bool { status == nil }

    init(name: String, fullPath: String, status: String? = nil) {
        self.name = name
        self.fullPath = fullPath
        self.status = status
    }
}

/// Build a tree from flat file paths.
private func buildFileTree(from changedFiles: [(status: String, path: String)]) -> [FileTreeNode] {
    let root = FileTreeNode(name: "", fullPath: "")

    for file in changedFiles {
        let components = file.path.components(separatedBy: "/")
        var current = root

        for (i, component) in components.enumerated() {
            let isLast = i == components.count - 1
            let partialPath = components[0...i].joined(separator: "/")

            if isLast {
                // Leaf file
                let leaf = FileTreeNode(name: component, fullPath: partialPath, status: file.status)
                current.children.append(leaf)
            } else {
                // Directory — find or create
                if let existing = current.children.first(where: { $0.isDirectory && $0.name == component }) {
                    current = existing
                } else {
                    let dir = FileTreeNode(name: component, fullPath: partialPath)
                    current.children.append(dir)
                    current = dir
                }
            }
        }
    }

    // Collapse single-child directory chains (e.g. src/core/utils → "src/core/utils")
    collapseChains(root)

    return root.children
}

private func collapseChains(_ node: FileTreeNode) {
    // Recursively collapse children first
    for child in node.children {
        collapseChains(child)
    }

    // Collapse: if a directory has exactly one child and it's also a directory, merge them
    var collapsed = true
    while collapsed {
        collapsed = false
        for (i, child) in node.children.enumerated() {
            if child.isDirectory && child.children.count == 1 && child.children[0].isDirectory {
                let grandchild = child.children[0]
                let merged = FileTreeNode(
                    name: child.name + "/" + grandchild.name,
                    fullPath: grandchild.fullPath
                )
                merged.children = grandchild.children
                node.children[i] = merged
                collapsed = true
                break
            }
        }
    }
}

// MARK: - DiffOverlayViewController

/// Overlay panel showing git diff for a worktree.
/// Shows file tree on left, diff content on right.
class DiffOverlayViewController: NSViewController {
    private let splitView = NSSplitView()
    private let outlineView = NSOutlineView()
    private let fileScrollView = NSScrollView()
    private let diffTextView = NSTextView()
    private let diffScrollView = NSScrollView()
    private let headerLabel = NSTextField(labelWithString: "")
    private let closeButton = NSButton()

    private var files: [DiffFile] = []
    private var changedFiles: [(status: String, path: String)] = []
    private var treeNodes: [FileTreeNode] = []
    private var worktreePath: String = ""

    init(worktreePath: String) {
        self.worktreePath = worktreePath
        super.init(nibName: nil, bundle: nil)
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) not supported")
    }

    override func loadView() {
        let container = NSView(frame: NSRect(x: 0, y: 0, width: 1000, height: 660))
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

        // File tree (left) — NSOutlineView
        fileScrollView.hasVerticalScroller = true
        fileScrollView.drawsBackground = false
        fileScrollView.borderType = .noBorder
        fileScrollView.translatesAutoresizingMaskIntoConstraints = true

        outlineView.backgroundColor = .clear
        outlineView.headerView = nil
        outlineView.rowHeight = 22
        outlineView.indentationPerLevel = 16
        outlineView.delegate = self
        outlineView.dataSource = self
        outlineView.selectionHighlightStyle = .regular
        let col = NSTableColumn(identifier: NSUserInterfaceItemIdentifier("file"))
        col.resizingMask = .autoresizingMask
        outlineView.addTableColumn(col)
        outlineView.outlineTableColumn = col
        fileScrollView.documentView = outlineView

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
        splitView.setPosition(260, ofDividerAt: 0)
    }

    private func loadDiff() {
        DispatchQueue.global().async { [weak self] in
            guard let self else { return }
            let files = GitDiff.diff(worktreePath: self.worktreePath)
            let changed = GitDiff.changedFiles(worktreePath: self.worktreePath)

            DispatchQueue.main.async {
                self.files = files
                self.changedFiles = changed
                self.treeNodes = buildFileTree(from: changed)
                let totalAdd = files.reduce(0) { $0 + $1.additions }
                let totalDel = files.reduce(0) { $0 + $1.deletions }
                self.headerLabel.stringValue = "Changes: \(changed.count) files  +\(totalAdd) -\(totalDel)"
                self.outlineView.reloadData()
                self.outlineView.expandItem(nil, expandChildren: true)
                self.showAllDiffs()
            }
        }
    }

    private func showAllDiffs() {
        let attributed = NSMutableAttributedString()
        let monoFont = NSFont.monospacedSystemFont(ofSize: 12, weight: .regular)

        for file in files {
            let fileHeader = "\n━━━ \(file.path) (+\(file.additions) -\(file.deletions)) ━━━\n\n"
            attributed.append(NSAttributedString(string: fileHeader, attributes: [
                .font: NSFont.monospacedSystemFont(ofSize: 12, weight: .bold),
                .foregroundColor: Theme.accent,
            ]))

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
        }

        if files.isEmpty {
            attributed.append(NSAttributedString(string: "No changes", attributes: [
                .font: monoFont,
                .foregroundColor: Theme.textSecondary,
            ]))
        }

        diffTextView.textStorage?.setAttributedString(attributed)
    }

    private func showDiffForFile(path: String) {
        guard let file = files.first(where: { $0.path == path }) else {
            showAllDiffs()
            return
        }

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

// MARK: - NSOutlineViewDataSource

extension DiffOverlayViewController: NSOutlineViewDataSource {
    func outlineView(_ outlineView: NSOutlineView, numberOfChildrenOfItem item: Any?) -> Int {
        if let node = item as? FileTreeNode {
            return node.children.count
        }
        return treeNodes.count
    }

    func outlineView(_ outlineView: NSOutlineView, child index: Int, ofItem item: Any?) -> Any {
        if let node = item as? FileTreeNode {
            return node.children[index]
        }
        return treeNodes[index]
    }

    func outlineView(_ outlineView: NSOutlineView, isItemExpandable item: Any) -> Bool {
        guard let node = item as? FileTreeNode else { return false }
        return node.isDirectory
    }
}

// MARK: - NSOutlineViewDelegate

extension DiffOverlayViewController: NSOutlineViewDelegate {
    func outlineView(_ outlineView: NSOutlineView, viewFor tableColumn: NSTableColumn?, item: Any) -> NSView? {
        guard let node = item as? FileTreeNode else { return nil }

        let cell = NSTableCellView()
        cell.identifier = NSUserInterfaceItemIdentifier("FileTreeCell")

        let imageView = NSImageView()
        imageView.translatesAutoresizingMaskIntoConstraints = false
        imageView.imageScaling = .scaleProportionallyDown
        cell.addSubview(imageView)

        let textField = NSTextField(labelWithString: "")
        textField.translatesAutoresizingMaskIntoConstraints = false
        textField.font = NSFont.systemFont(ofSize: 12)
        textField.lineBreakMode = .byTruncatingMiddle
        textField.drawsBackground = false
        textField.isBezeled = false
        textField.isEditable = false
        cell.addSubview(textField)
        cell.textField = textField

        if node.isDirectory {
            let symbolConfig = NSImage.SymbolConfiguration(pointSize: 12, weight: .regular)
            imageView.image = NSImage(systemSymbolName: "folder.fill", accessibilityDescription: nil)?
                .withSymbolConfiguration(symbolConfig)
            imageView.contentTintColor = NSColor.systemBlue
            textField.stringValue = node.name
            textField.textColor = Theme.textPrimary
        } else {
            // File leaf — show status dot
            let symbolConfig = NSImage.SymbolConfiguration(pointSize: 8, weight: .bold)
            imageView.image = NSImage(systemSymbolName: "circle.fill", accessibilityDescription: nil)?
                .withSymbolConfiguration(symbolConfig)
            imageView.contentTintColor = statusColor(node.status ?? "")
            textField.stringValue = node.name
            textField.textColor = Theme.textPrimary
        }

        NSLayoutConstraint.activate([
            imageView.leadingAnchor.constraint(equalTo: cell.leadingAnchor, constant: 2),
            imageView.centerYAnchor.constraint(equalTo: cell.centerYAnchor),
            imageView.widthAnchor.constraint(equalToConstant: 16),
            imageView.heightAnchor.constraint(equalToConstant: 16),

            textField.leadingAnchor.constraint(equalTo: imageView.trailingAnchor, constant: 4),
            textField.trailingAnchor.constraint(equalTo: cell.trailingAnchor, constant: -4),
            textField.centerYAnchor.constraint(equalTo: cell.centerYAnchor),
        ])

        return cell
    }

    func outlineViewSelectionDidChange(_ notification: Notification) {
        let row = outlineView.selectedRow
        guard row >= 0, let node = outlineView.item(atRow: row) as? FileTreeNode else {
            showAllDiffs()
            return
        }

        if node.isDirectory {
            // Show diffs for all files under this directory
            let paths = collectFilePaths(under: node)
            showDiffsForPaths(paths)
        } else {
            showDiffForFile(path: node.fullPath)
        }
    }

    private func collectFilePaths(under node: FileTreeNode) -> [String] {
        if !node.isDirectory {
            return [node.fullPath]
        }
        return node.children.flatMap { collectFilePaths(under: $0) }
    }

    private func showDiffsForPaths(_ paths: [String]) {
        let matchingFiles = files.filter { paths.contains($0.path) }
        guard !matchingFiles.isEmpty else {
            showAllDiffs()
            return
        }

        let attributed = NSMutableAttributedString()
        let monoFont = NSFont.monospacedSystemFont(ofSize: 12, weight: .regular)

        for file in matchingFiles {
            let fileHeader = "\n━━━ \(file.path) (+\(file.additions) -\(file.deletions)) ━━━\n\n"
            attributed.append(NSAttributedString(string: fileHeader, attributes: [
                .font: NSFont.monospacedSystemFont(ofSize: 12, weight: .bold),
                .foregroundColor: Theme.accent,
            ]))

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
        }

        diffTextView.textStorage?.setAttributedString(attributed)
    }
}
