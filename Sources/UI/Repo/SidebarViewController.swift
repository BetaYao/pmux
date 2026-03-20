import AppKit

protocol SidebarDelegate: AnyObject {
    func sidebar(_ sidebar: SidebarViewController, didSelectWorktreeAt index: Int)
    func sidebar(_ sidebar: SidebarViewController, didRequestDeleteWorktreeAt index: Int)
}

/// Left sidebar showing thread list with status dots
class SidebarViewController: NSViewController {
    weak var sidebarDelegate: SidebarDelegate?

    private let scrollView = NSScrollView()
    private let tableView = NSTableView()
    private let emptyStateLabel = NSTextField(labelWithString: "No thread yet. Click New Thread in titlebar.")
    private var worktrees: [WorktreeInfo] = []
    private var statuses: [String: AgentStatus] = [:]
    private var lastMessages: [String: String] = [:]
    private var selectedIndex: Int = 0
    private var suppressSelectionNotification = false

    override func loadView() {
        self.view = NSView()
        view.wantsLayer = true
        view.layer?.backgroundColor = SemanticColors.panel.cgColor

        scrollView.translatesAutoresizingMaskIntoConstraints = false
        scrollView.hasVerticalScroller = true
        scrollView.scrollerStyle = .overlay
        scrollView.drawsBackground = false
        scrollView.borderType = .noBorder
        scrollView.contentInsets = NSEdgeInsets(top: 6, left: 6, bottom: 6, right: 6)
        view.addSubview(scrollView)

        tableView.backgroundColor = .clear
        tableView.headerView = nil
        tableView.selectionHighlightStyle = .none
        tableView.rowHeight = 60
        tableView.intercellSpacing = NSSize(width: 0, height: 4)
        tableView.delegate = self
        tableView.dataSource = self

        let column = NSTableColumn(identifier: NSUserInterfaceItemIdentifier("worktree"))
        column.resizingMask = .autoresizingMask
        tableView.addTableColumn(column)

        tableView.setAccessibilityIdentifier("sidebar.worktreeList")
        let contextMenu = NSMenu()
        contextMenu.delegate = self
        tableView.menu = contextMenu

        scrollView.documentView = tableView

        // Empty state label
        emptyStateLabel.translatesAutoresizingMaskIntoConstraints = false
        emptyStateLabel.font = NSFont.systemFont(ofSize: 12)
        emptyStateLabel.textColor = SemanticColors.muted
        emptyStateLabel.alignment = .center
        emptyStateLabel.lineBreakMode = .byWordWrapping
        emptyStateLabel.maximumNumberOfLines = 3
        emptyStateLabel.preferredMaxLayoutWidth = 240
        emptyStateLabel.isHidden = true
        emptyStateLabel.setAccessibilityIdentifier("project.emptyState")
        view.addSubview(emptyStateLabel)

        NSLayoutConstraint.activate([
            scrollView.topAnchor.constraint(equalTo: view.topAnchor),
            scrollView.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            scrollView.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            scrollView.bottomAnchor.constraint(equalTo: view.bottomAnchor),

            emptyStateLabel.centerXAnchor.constraint(equalTo: view.centerXAnchor),
            emptyStateLabel.centerYAnchor.constraint(equalTo: view.centerYAnchor),
            emptyStateLabel.leadingAnchor.constraint(greaterThanOrEqualTo: view.leadingAnchor, constant: 16),
            emptyStateLabel.trailingAnchor.constraint(lessThanOrEqualTo: view.trailingAnchor, constant: -16),
        ])
    }

    func setWorktrees(_ worktrees: [WorktreeInfo]) {
        self.worktrees = worktrees
        emptyStateLabel.isHidden = !worktrees.isEmpty
        scrollView.isHidden = worktrees.isEmpty
        tableView.reloadData()
        if !worktrees.isEmpty {
            suppressSelectionNotification = true
            tableView.selectRowIndexes(IndexSet(integer: selectedIndex), byExtendingSelection: false)
            suppressSelectionNotification = false
        }
    }

    func updateStatus(for path: String, status: AgentStatus, lastMessage: String = "") {
        statuses[path] = status
        if !lastMessage.isEmpty {
            lastMessages[path] = lastMessage
        }
        if let rowIndex = worktrees.firstIndex(where: { $0.path == path }) {
            tableView.reloadData(forRowIndexes: IndexSet(integer: rowIndex),
                                 columnIndexes: IndexSet(integer: 0))
        }
    }

    func selectWorktree(at index: Int) {
        guard index >= 0, index < worktrees.count else { return }
        selectedIndex = index
        suppressSelectionNotification = true
        tableView.selectRowIndexes(IndexSet(integer: index), byExtendingSelection: false)
        suppressSelectionNotification = false
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

    func tableView(_ tableView: NSTableView, rowViewForRow row: Int) -> NSTableRowView? {
        let info = worktrees[row]
        let isSelected = (row == selectedIndex)
        let status = statuses[info.path] ?? .unknown
        let rowView = ThreadRowView(isActive: isSelected, status: status)
        return rowView
    }

    func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView? {
        let info = worktrees[row]
        let status = statuses[info.path] ?? .unknown
        let message = lastMessages[info.path] ?? ""
        let isSelected = (row == selectedIndex)

        let cell: SidebarCellView
        if let reused = tableView.makeView(withIdentifier: SidebarCellView.identifier, owner: nil) as? SidebarCellView {
            cell = reused
        } else {
            cell = SidebarCellView()
            cell.identifier = SidebarCellView.identifier
        }

        cell.configure(info: info, status: status, message: message, isSelected: isSelected)
        return cell
    }

    func tableViewSelectionDidChange(_ notification: Notification) {
        guard !suppressSelectionNotification else { return }
        let row = tableView.selectedRow
        guard row >= 0 else { return }
        selectedIndex = row
        tableView.reloadData()  // refresh row styles
        sidebarDelegate?.sidebar(self, didSelectWorktreeAt: row)
    }
}

// MARK: - Reusable Cell View

private class SidebarCellView: NSTableCellView {
    static let identifier = NSUserInterfaceItemIdentifier("SidebarCell")

    let nameLabel = NSTextField(labelWithString: "")
    let dotView = NSView()
    let messageLabel = NSTextField(labelWithString: "")

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        setup()
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) not supported")
    }

    private func setup() {
        nameLabel.font = NSFont.systemFont(ofSize: 12, weight: .bold)
        nameLabel.lineBreakMode = .byTruncatingTail
        nameLabel.translatesAutoresizingMaskIntoConstraints = false
        nameLabel.drawsBackground = false
        nameLabel.isBezeled = false
        nameLabel.isEditable = false
        addSubview(nameLabel)

        dotView.wantsLayer = true
        dotView.layer?.cornerRadius = 4
        dotView.translatesAutoresizingMaskIntoConstraints = false
        addSubview(dotView)

        messageLabel.font = NSFont.systemFont(ofSize: 12)
        messageLabel.lineBreakMode = .byTruncatingTail
        messageLabel.maximumNumberOfLines = 2
        messageLabel.translatesAutoresizingMaskIntoConstraints = false
        messageLabel.drawsBackground = false
        messageLabel.isBezeled = false
        messageLabel.isEditable = false
        messageLabel.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        addSubview(messageLabel)

        NSLayoutConstraint.activate([
            nameLabel.topAnchor.constraint(equalTo: topAnchor, constant: 9),
            nameLabel.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 8),
            nameLabel.trailingAnchor.constraint(lessThanOrEqualTo: dotView.leadingAnchor, constant: -6),

            dotView.widthAnchor.constraint(equalToConstant: 8),
            dotView.heightAnchor.constraint(equalToConstant: 8),
            dotView.centerYAnchor.constraint(equalTo: nameLabel.centerYAnchor),
            dotView.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -8),

            messageLabel.topAnchor.constraint(equalTo: nameLabel.bottomAnchor, constant: 2),
            messageLabel.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 8),
            messageLabel.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -8),
            messageLabel.bottomAnchor.constraint(lessThanOrEqualTo: bottomAnchor, constant: -9),
        ])
    }

    func configure(info: WorktreeInfo, status: AgentStatus, message: String, isSelected: Bool) {
        nameLabel.stringValue = info.displayName
        nameLabel.textColor = SemanticColors.text
        dotView.layer?.backgroundColor = status.color.cgColor
        messageLabel.stringValue = message.isEmpty ? status.rawValue : message
        messageLabel.textColor = SemanticColors.muted
        setAccessibilityIdentifier("sidebar.row.\(info.branch.isEmpty ? info.displayName : info.branch)")
    }
}

// MARK: - Custom Row View

/// Thread row with accent-tinted selection style.
private class ThreadRowView: NSTableRowView {
    private let isActive: Bool

    init(isActive: Bool, status: AgentStatus) {
        self.isActive = isActive
        super.init(frame: .zero)
        wantsLayer = true
        layer?.cornerRadius = 6
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) not supported")
    }

    override func drawSelection(in dirtyRect: NSRect) {}

    override var wantsUpdateLayer: Bool { true }

    override func updateLayer() {
        if isActive {
            effectiveAppearance.performAsCurrentDrawingAppearance {
                layer?.backgroundColor = SemanticColors.accent.withAlphaComponent(0.07).cgColor
                layer?.borderColor = SemanticColors.accent.withAlphaComponent(0.38).cgColor
                layer?.borderWidth = 1
            }
        } else {
            layer?.backgroundColor = nil
            layer?.borderColor = nil
            layer?.borderWidth = 0
        }
    }

    override var interiorBackgroundStyle: NSView.BackgroundStyle { .normal }
}

// MARK: - NSMenuDelegate

extension SidebarViewController: NSMenuDelegate {
    func menuNeedsUpdate(_ menu: NSMenu) {
        menu.removeAllItems()
        let clickedRow = tableView.clickedRow
        guard clickedRow >= 0, clickedRow < worktrees.count else { return }
        let info = worktrees[clickedRow]
        guard !info.isMainWorktree else { return }

        let deleteItem = NSMenuItem(title: "Delete Worktree...", action: #selector(deleteClicked(_:)), keyEquivalent: "")
        deleteItem.target = self
        deleteItem.tag = clickedRow
        menu.addItem(deleteItem)
    }

    @objc private func deleteClicked(_ sender: NSMenuItem) {
        sidebarDelegate?.sidebar(self, didRequestDeleteWorktreeAt: sender.tag)
    }
}
