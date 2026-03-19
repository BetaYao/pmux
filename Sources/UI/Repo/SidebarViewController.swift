import AppKit

protocol SidebarDelegate: AnyObject {
    func sidebar(_ sidebar: SidebarViewController, didSelectWorktreeAt index: Int)
}

/// Left sidebar showing worktree list with status indicators
class SidebarViewController: NSViewController {
    weak var sidebarDelegate: SidebarDelegate?

    private let scrollView = NSScrollView()
    private let tableView = NSTableView()
    private var worktrees: [WorktreeInfo] = []
    private var statuses: [String: AgentStatus] = [:]  // keyed by worktree path
    private var selectedIndex: Int = 0

    override func loadView() {
        self.view = NSView()
        view.wantsLayer = true
        view.layer?.backgroundColor = Theme.surface.cgColor

        scrollView.translatesAutoresizingMaskIntoConstraints = false
        scrollView.hasVerticalScroller = true
        scrollView.scrollerStyle = .overlay
        scrollView.drawsBackground = false
        scrollView.borderType = .noBorder
        view.addSubview(scrollView)

        tableView.backgroundColor = .clear
        tableView.headerView = nil
        tableView.selectionHighlightStyle = .regular
        tableView.rowHeight = 36
        tableView.intercellSpacing = NSSize(width: 0, height: 2)
        tableView.delegate = self
        tableView.dataSource = self

        let column = NSTableColumn(identifier: NSUserInterfaceItemIdentifier("worktree"))
        column.resizingMask = .autoresizingMask
        tableView.addTableColumn(column)

        scrollView.documentView = tableView

        NSLayoutConstraint.activate([
            scrollView.topAnchor.constraint(equalTo: view.topAnchor, constant: 8),
            scrollView.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            scrollView.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            scrollView.bottomAnchor.constraint(equalTo: view.bottomAnchor),
        ])
    }

    func setWorktrees(_ worktrees: [WorktreeInfo]) {
        self.worktrees = worktrees
        tableView.reloadData()
        if !worktrees.isEmpty {
            tableView.selectRowIndexes(IndexSet(integer: selectedIndex), byExtendingSelection: false)
        }
    }

    func updateStatus(for path: String, status: AgentStatus) {
        statuses[path] = status
        tableView.reloadData()
    }

    func selectWorktree(at index: Int) {
        guard index >= 0, index < worktrees.count else { return }
        selectedIndex = index
        tableView.selectRowIndexes(IndexSet(integer: index), byExtendingSelection: false)
    }
}

// MARK: - NSTableViewDataSource

extension SidebarViewController: NSTableViewDataSource {
    func numberOfRows(in tableView: NSTableView) -> Int {
        return worktrees.count
    }
}

// MARK: - NSTableViewDelegate

extension SidebarViewController: NSTableViewDelegate {
    func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView? {
        let info = worktrees[row]
        let status = statuses[info.path] ?? .unknown

        let cellView = NSView()
        cellView.wantsLayer = true

        // Status dot
        let dot = NSView(frame: NSRect(x: 12, y: 12, width: 10, height: 10))
        dot.wantsLayer = true
        dot.layer?.cornerRadius = 5
        dot.layer?.backgroundColor = status.color.cgColor
        cellView.addSubview(dot)

        // Branch name
        let label = NSTextField(labelWithString: info.displayName)
        label.font = NSFont.systemFont(ofSize: 12, weight: .medium)
        label.textColor = Theme.textPrimary
        label.frame = NSRect(x: 30, y: 8, width: 200, height: 20)
        label.lineBreakMode = .byTruncatingTail
        cellView.addSubview(label)

        return cellView
    }

    func tableViewSelectionDidChange(_ notification: Notification) {
        let row = tableView.selectedRow
        guard row >= 0 else { return }
        selectedIndex = row
        sidebarDelegate?.sidebar(self, didSelectWorktreeAt: row)
    }
}
