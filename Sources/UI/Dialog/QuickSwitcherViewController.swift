import AppKit

protocol QuickSwitcherDelegate: AnyObject {
    func quickSwitcher(_ vc: QuickSwitcherViewController, didSelect worktree: WorktreeInfo)
}

/// Spotlight-style quick switcher for jumping to worktrees by fuzzy search.
class QuickSwitcherViewController: NSViewController {
    weak var quickSwitcherDelegate: QuickSwitcherDelegate?

    private let searchField = NSTextField()
    private let resultsTableView = NSTableView()
    private let resultsScrollView = NSScrollView()

    private var allWorktrees: [WorktreeInfo] = []
    private var filteredWorktrees: [WorktreeInfo] = []
    private var statuses: [String: AgentStatus] = [:]

    init(worktrees: [WorktreeInfo], statuses: [String: AgentStatus]) {
        self.allWorktrees = worktrees
        self.filteredWorktrees = worktrees
        self.statuses = statuses
        super.init(nibName: nil, bundle: nil)
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) not supported")
    }

    override func loadView() {
        let container = NSView(frame: NSRect(x: 0, y: 0, width: 450, height: 340))
        container.wantsLayer = true
        container.layer?.backgroundColor = SemanticColors.panel.cgColor
        container.layer?.cornerRadius = 10
        container.setAccessibilityIdentifier("dialog.quickSwitcher")
        container.setAccessibilityElement(true)
        container.setAccessibilityRole(.group)
        self.view = container

        // Search field
        searchField.placeholderString = "Search worktrees..."
        searchField.font = NSFont.systemFont(ofSize: 16)
        searchField.isBordered = true
        searchField.bezelStyle = .roundedBezel
        searchField.focusRingType = .none
        searchField.delegate = self
        searchField.setAccessibilityIdentifier("dialog.quickSwitcher.searchField")
        searchField.translatesAutoresizingMaskIntoConstraints = false
        container.addSubview(searchField)

        // Results table
        resultsScrollView.hasVerticalScroller = true
        resultsScrollView.drawsBackground = false
        resultsScrollView.borderType = .noBorder
        resultsScrollView.translatesAutoresizingMaskIntoConstraints = false
        container.addSubview(resultsScrollView)

        resultsTableView.backgroundColor = .clear
        resultsTableView.headerView = nil
        resultsTableView.rowHeight = 36
        resultsTableView.intercellSpacing = NSSize(width: 0, height: 1)
        resultsTableView.delegate = self
        resultsTableView.dataSource = self
        resultsTableView.doubleAction = #selector(confirmSelection)
        resultsTableView.target = self

        let col = NSTableColumn(identifier: NSUserInterfaceItemIdentifier("worktree"))
        col.resizingMask = .autoresizingMask
        resultsTableView.addTableColumn(col)
        resultsTableView.setAccessibilityIdentifier("dialog.quickSwitcher.resultsList")
        resultsScrollView.documentView = resultsTableView

        // Hint label
        let hintLabel = NSTextField(labelWithString: "↑↓ navigate  ↵ select  ⎋ cancel")
        hintLabel.font = NSFont.systemFont(ofSize: 10)
        hintLabel.textColor = SemanticColors.muted
        hintLabel.translatesAutoresizingMaskIntoConstraints = false
        container.addSubview(hintLabel)

        NSLayoutConstraint.activate([
            searchField.topAnchor.constraint(equalTo: container.topAnchor, constant: 12),
            searchField.leadingAnchor.constraint(equalTo: container.leadingAnchor, constant: 12),
            searchField.trailingAnchor.constraint(equalTo: container.trailingAnchor, constant: -12),
            searchField.heightAnchor.constraint(equalToConstant: 28),

            resultsScrollView.topAnchor.constraint(equalTo: searchField.bottomAnchor, constant: 8),
            resultsScrollView.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            resultsScrollView.trailingAnchor.constraint(equalTo: container.trailingAnchor),
            resultsScrollView.bottomAnchor.constraint(equalTo: hintLabel.topAnchor, constant: -4),

            hintLabel.leadingAnchor.constraint(equalTo: container.leadingAnchor, constant: 12),
            hintLabel.bottomAnchor.constraint(equalTo: container.bottomAnchor, constant: -8),
        ])
    }

    override func viewDidAppear() {
        super.viewDidAppear()
        view.window?.makeFirstResponder(searchField)
        if !filteredWorktrees.isEmpty {
            resultsTableView.selectRowIndexes(IndexSet(integer: 0), byExtendingSelection: false)
        }
    }

    // MARK: - Keyboard

    override func keyDown(with event: NSEvent) {
        switch event.keyCode {
        case 125: // Down arrow
            moveSelection(by: 1)
        case 126: // Up arrow
            moveSelection(by: -1)
        case 36: // Return
            confirmSelection()
        case 53: // Esc
            dismiss(nil)
        default:
            super.keyDown(with: event)
        }
    }

    private func moveSelection(by delta: Int) {
        guard !filteredWorktrees.isEmpty else { return }
        let current = resultsTableView.selectedRow
        let next = max(0, min(filteredWorktrees.count - 1, current + delta))
        resultsTableView.selectRowIndexes(IndexSet(integer: next), byExtendingSelection: false)
        resultsTableView.scrollRowToVisible(next)
    }

    @objc private func confirmSelection() {
        let row = resultsTableView.selectedRow
        guard row >= 0, row < filteredWorktrees.count else { return }
        let selected = filteredWorktrees[row]
        dismiss(nil)
        quickSwitcherDelegate?.quickSwitcher(self, didSelect: selected)
    }

    private func updateFilter() {
        let query = searchField.stringValue
        filteredWorktrees = FuzzyMatch.filter(allWorktrees, query: query) { info in
            info.branch.isEmpty ? info.displayName : info.branch
        }
        resultsTableView.reloadData()
        if !filteredWorktrees.isEmpty {
            resultsTableView.selectRowIndexes(IndexSet(integer: 0), byExtendingSelection: false)
        }
    }
}

// MARK: - NSTextFieldDelegate

extension QuickSwitcherViewController: NSTextFieldDelegate {
    func controlTextDidChange(_ obj: Notification) {
        updateFilter()
    }

    func control(_ control: NSControl, textView: NSTextView, doCommandBy commandSelector: Selector) -> Bool {
        if commandSelector == #selector(moveDown(_:)) {
            moveSelection(by: 1)
            return true
        }
        if commandSelector == #selector(moveUp(_:)) {
            moveSelection(by: -1)
            return true
        }
        if commandSelector == #selector(insertNewline(_:)) {
            confirmSelection()
            return true
        }
        if commandSelector == #selector(cancelOperation(_:)) {
            dismiss(nil)
            return true
        }
        return false
    }
}

// MARK: - NSTableViewDataSource

extension QuickSwitcherViewController: NSTableViewDataSource {
    func numberOfRows(in tableView: NSTableView) -> Int {
        return filteredWorktrees.count
    }
}

// MARK: - NSTableViewDelegate

extension QuickSwitcherViewController: NSTableViewDelegate {
    func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView? {
        let info = filteredWorktrees[row]
        let status = statuses[info.path] ?? .unknown

        let cell = NSView()
        cell.wantsLayer = true

        // Status dot
        let dot = NSView(frame: NSRect(x: 12, y: 13, width: 10, height: 10))
        dot.wantsLayer = true
        dot.layer?.cornerRadius = 5
        dot.layer?.backgroundColor = status.color.cgColor
        cell.addSubview(dot)

        // Branch name
        let branchLabel = NSTextField(labelWithString: info.displayName)
        branchLabel.font = NSFont.systemFont(ofSize: 13, weight: .medium)
        branchLabel.textColor = SemanticColors.text
        branchLabel.frame = NSRect(x: 30, y: 10, width: 250, height: 18)
        branchLabel.lineBreakMode = .byTruncatingTail
        cell.addSubview(branchLabel)

        // Path (secondary)
        let pathLabel = NSTextField(labelWithString: shortenPath(info.path))
        pathLabel.font = NSFont.systemFont(ofSize: 10)
        pathLabel.textColor = SemanticColors.muted
        pathLabel.frame = NSRect(x: 290, y: 12, width: 150, height: 14)
        pathLabel.lineBreakMode = .byTruncatingHead
        pathLabel.alignment = .right
        cell.addSubview(pathLabel)

        return cell
    }

    private func shortenPath(_ path: String) -> String {
        let url = URL(fileURLWithPath: path)
        let parent = url.deletingLastPathComponent().lastPathComponent
        let name = url.lastPathComponent
        return "\(parent)/\(name)"
    }
}
