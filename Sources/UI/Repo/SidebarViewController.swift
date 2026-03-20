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
        let oldStatus = statuses[path]
        let oldMessage = lastMessages[path]
        statuses[path] = status
        if !lastMessage.isEmpty {
            lastMessages[path] = lastMessage
        }
        // Only reload the specific changed row instead of the entire table
        if let rowIndex = worktrees.firstIndex(where: { $0.path == path }) {
            if oldStatus != status || oldMessage != lastMessage {
                tableView.reloadData(forRowIndexes: IndexSet(integer: rowIndex), columnIndexes: IndexSet(integer: 0))
            }
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

    private static let cellIdentifier = NSUserInterfaceItemIdentifier("SidebarWorktreeCell")

    func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView? {
        let info = worktrees[row]
        let status = statuses[info.path] ?? .unknown
        let message = lastMessages[info.path] ?? ""

        // Reuse existing cell or create a new one
        if let existing = tableView.makeView(withIdentifier: Self.cellIdentifier, owner: nil) as? SidebarCellView {
            existing.update(name: info.displayName, status: status, message: message)
            return existing
        }

        let cell = SidebarCellView()
        cell.identifier = Self.cellIdentifier
        cell.update(name: info.displayName, status: status, message: message)
        cell.setAccessibilityElement(true)
        cell.setAccessibilityRole(.cell)
        return cell
    }

    func tableViewSelectionDidChange(_ notification: Notification) {
        guard !suppressSelectionNotification else { return }
        let row = tableView.selectedRow
        guard row >= 0 else { return }
        let oldIndex = selectedIndex
        selectedIndex = row
        // Only reload the old and new selected rows instead of the entire table
        var indexSet = IndexSet(integer: row)
        if oldIndex != row, oldIndex >= 0, oldIndex < worktrees.count {
            indexSet.insert(oldIndex)
        }
        tableView.reloadData(forRowIndexes: indexSet, columnIndexes: IndexSet(integer: 0))
        sidebarDelegate?.sidebar(self, didSelectWorktreeAt: row)
    }
}

// MARK: - Reusable Cell View

private class SidebarCellView: NSView {
    private let nameLabel = NSTextField(labelWithString: "")
    private let dotView = NSView()
    private let messageLabel = NSTextField(labelWithString: "")

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        setupViews()
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) not supported")
    }

    private func setupViews() {
        nameLabel.font = NSFont.systemFont(ofSize: 12, weight: .bold)
        nameLabel.textColor = SemanticColors.text
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
        messageLabel.textColor = SemanticColors.muted
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

    func update(name: String, status: AgentStatus, message: String) {
        nameLabel.stringValue = name
        dotView.layer?.backgroundColor = status.color.cgColor
        messageLabel.stringValue = message.isEmpty ? status.rawValue : message
        setAccessibilityIdentifier("sidebar.row.\(name)")
    }
}

// MARK: - Custom Row View

/// Thread row with accent-tinted selection style.
private class ThreadRowView: NSTableRowView {
    private let isActive: Bool
    private let status: AgentStatus

    init(isActive: Bool, status: AgentStatus) {
        self.isActive = isActive
        self.status = status
        super.init(frame: .zero)
        wantsLayer = true
        layer?.cornerRadius = 6
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) not supported")
    }

    override func drawSelection(in dirtyRect: NSRect) {
        // Selection handled in draw(dirtyRect:)
    }

    override func draw(_ dirtyRect: NSRect) {
        super.draw(dirtyRect)

        if isActive {
            // Background: accent at 7% blended with panel
            let accentBg = SemanticColors.accent.withAlphaComponent(0.07)
            accentBg.setFill()
            let bgPath = NSBezierPath(roundedRect: bounds, xRadius: 6, yRadius: 6)
            bgPath.fill()

            // Border: accent at 38% blended with line
            let borderColor = SemanticColors.accent.blended(withFraction: 0.62, of: SemanticColors.line) ?? SemanticColors.accent.withAlphaComponent(0.38)
            borderColor.setStroke()
            let borderPath = NSBezierPath(roundedRect: bounds.insetBy(dx: 0.5, dy: 0.5), xRadius: 6, yRadius: 6)
            borderPath.lineWidth = 1
            borderPath.stroke()
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
