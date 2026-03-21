import AppKit

protocol SidebarDelegate: AnyObject {
    func sidebar(_ sidebar: SidebarViewController, didSelectWorktreeAt index: Int)
    func sidebar(_ sidebar: SidebarViewController, didRequestDeleteWorktreeAt index: Int)
    func sidebarDidRequestNewThread(_ sidebar: SidebarViewController)
}

/// Left sidebar showing thread list with status dots
class SidebarViewController: NSViewController {
    enum Layout {
        static let listHorizontalInset: CGFloat = 0
        static let rowBackgroundHorizontalInset: CGFloat = 8
        static let cellLeadingInset: CGFloat = 8
        static let cellTrailingInset: CGFloat = 6
        static let usesNativeSelectionStyle = true
        static let showsHeaderSeparator = false
    }

    weak var sidebarDelegate: SidebarDelegate?

    private let headerBar = NSView()
    private let threadsLabel = NSTextField(labelWithString: "Threads")
    private let countLabel = NSTextField(labelWithString: "")
    private let addButton = SidebarAddButton()
    private let headerBorder = NSView()
    private let scrollView = NSScrollView()
    private let tableView = NSTableView()
    private let emptyStateLabel = NSTextField(labelWithString: "No thread yet. Click + to create one.")
    private var worktrees: [WorktreeInfo] = []
    private var statuses: [String: AgentStatus] = [:]
    private var lastMessages: [String: String] = [:]
    private var selectedIndex: Int = 0
    private var suppressSelectionNotification = false

    override func loadView() {
        self.view = NSView()
        view.wantsLayer = true
        view.layer?.backgroundColor = NSColor.clear.cgColor

        // MARK: Header bar
        headerBar.translatesAutoresizingMaskIntoConstraints = false
        headerBar.wantsLayer = true
        view.addSubview(headerBar)

        threadsLabel.translatesAutoresizingMaskIntoConstraints = false
        threadsLabel.font = NSFont.systemFont(ofSize: 12, weight: .medium)
        threadsLabel.textColor = NSColor(white: 0.667, alpha: 1) // #aaa
        threadsLabel.drawsBackground = false
        threadsLabel.isBezeled = false
        threadsLabel.isEditable = false
        headerBar.addSubview(threadsLabel)

        countLabel.translatesAutoresizingMaskIntoConstraints = false
        countLabel.font = NSFont.systemFont(ofSize: 11)
        countLabel.textColor = NSColor(white: 0.333, alpha: 1) // #555
        countLabel.drawsBackground = false
        countLabel.isBezeled = false
        countLabel.isEditable = false
        headerBar.addSubview(countLabel)

        addButton.translatesAutoresizingMaskIntoConstraints = false
        addButton.target = self
        addButton.action = #selector(addThreadClicked)
        addButton.setAccessibilityIdentifier("sidebar.addThread")
        headerBar.addSubview(addButton)

        headerBorder.translatesAutoresizingMaskIntoConstraints = false
        headerBorder.wantsLayer = true
        headerBorder.layer?.backgroundColor = SemanticColors.line.cgColor
        headerBorder.isHidden = !Layout.showsHeaderSeparator
        headerBar.addSubview(headerBorder)

        // MARK: Scroll view + table
        scrollView.translatesAutoresizingMaskIntoConstraints = false
        scrollView.hasVerticalScroller = true
        scrollView.scrollerStyle = .overlay
        scrollView.drawsBackground = false
        scrollView.borderType = .noBorder
        scrollView.contentInsets = NSEdgeInsets(top: 6, left: Layout.listHorizontalInset, bottom: 6, right: Layout.listHorizontalInset)
        view.addSubview(scrollView)

        tableView.backgroundColor = .clear
        tableView.headerView = nil
        if Layout.usesNativeSelectionStyle {
            tableView.selectionHighlightStyle = .regular
        } else {
            tableView.selectionHighlightStyle = .none
        }
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
            // Header bar
            headerBar.topAnchor.constraint(equalTo: view.topAnchor),
            headerBar.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            headerBar.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            headerBar.heightAnchor.constraint(equalToConstant: 36),

            threadsLabel.centerYAnchor.constraint(equalTo: headerBar.centerYAnchor),
            threadsLabel.leadingAnchor.constraint(equalTo: headerBar.leadingAnchor, constant: 12),

            countLabel.centerYAnchor.constraint(equalTo: headerBar.centerYAnchor),
            countLabel.leadingAnchor.constraint(equalTo: threadsLabel.trailingAnchor, constant: 6),

            addButton.centerYAnchor.constraint(equalTo: headerBar.centerYAnchor),
            addButton.trailingAnchor.constraint(equalTo: headerBar.trailingAnchor, constant: -8),
            addButton.widthAnchor.constraint(equalToConstant: 24),
            addButton.heightAnchor.constraint(equalToConstant: 24),

            headerBorder.leadingAnchor.constraint(equalTo: headerBar.leadingAnchor),
            headerBorder.trailingAnchor.constraint(equalTo: headerBar.trailingAnchor),
            headerBorder.bottomAnchor.constraint(equalTo: headerBar.bottomAnchor),
            headerBorder.heightAnchor.constraint(equalToConstant: Layout.showsHeaderSeparator ? 1 : 0),

            // Scroll view below header
            scrollView.topAnchor.constraint(equalTo: headerBar.bottomAnchor),
            scrollView.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            scrollView.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            scrollView.bottomAnchor.constraint(equalTo: view.bottomAnchor),

            emptyStateLabel.centerXAnchor.constraint(equalTo: view.centerXAnchor),
            emptyStateLabel.centerYAnchor.constraint(equalTo: view.centerYAnchor),
            emptyStateLabel.leadingAnchor.constraint(greaterThanOrEqualTo: view.leadingAnchor, constant: 16),
            emptyStateLabel.trailingAnchor.constraint(lessThanOrEqualTo: view.trailingAnchor, constant: -16),
        ])
    }

    @objc private func addThreadClicked() {
        sidebarDelegate?.sidebarDidRequestNewThread(self)
    }

    func setWorktrees(_ worktrees: [WorktreeInfo]) {
        self.worktrees = worktrees
        countLabel.stringValue = "\(worktrees.count)"
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
        guard !Layout.usesNativeSelectionStyle else { return nil }

        let isSelected = (row == selectedIndex)
        let rowView = ThreadRowView(isActive: isSelected)
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
        dotView.wantsLayer = true
        dotView.layer?.cornerRadius = 3.5
        dotView.translatesAutoresizingMaskIntoConstraints = false
        addSubview(dotView)

        nameLabel.font = NSFont.systemFont(ofSize: 11, weight: .medium)
        nameLabel.textColor = SemanticColors.text
        nameLabel.lineBreakMode = .byTruncatingTail
        nameLabel.translatesAutoresizingMaskIntoConstraints = false
        nameLabel.drawsBackground = false
        nameLabel.isBezeled = false
        nameLabel.isEditable = false
        addSubview(nameLabel)

        messageLabel.font = NSFont.monospacedSystemFont(ofSize: 9, weight: .regular)
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
            dotView.widthAnchor.constraint(equalToConstant: 7),
            dotView.heightAnchor.constraint(equalToConstant: 7),
            dotView.leadingAnchor.constraint(equalTo: leadingAnchor, constant: SidebarViewController.Layout.cellLeadingInset),
            dotView.centerYAnchor.constraint(equalTo: nameLabel.centerYAnchor),

            nameLabel.topAnchor.constraint(equalTo: topAnchor, constant: 9),
            nameLabel.leadingAnchor.constraint(equalTo: dotView.trailingAnchor, constant: 6),
            nameLabel.trailingAnchor.constraint(lessThanOrEqualTo: trailingAnchor, constant: -SidebarViewController.Layout.cellTrailingInset),

            messageLabel.topAnchor.constraint(equalTo: nameLabel.bottomAnchor, constant: 2),
            messageLabel.leadingAnchor.constraint(equalTo: dotView.trailingAnchor, constant: 6),
            messageLabel.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -SidebarViewController.Layout.cellTrailingInset),
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

/// Thread row with green-tinted selection and hover styles.
private class ThreadRowView: NSTableRowView {
    private let isActive: Bool
    private var isHovered = false
    private var trackingArea: NSTrackingArea?

    init(isActive: Bool) {
        self.isActive = isActive
        super.init(frame: .zero)
        wantsLayer = true
        layer?.cornerRadius = 6
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) not supported")
    }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        if let existing = trackingArea {
            removeTrackingArea(existing)
        }
        let area = NSTrackingArea(
            rect: bounds,
            options: [.mouseEnteredAndExited, .activeInActiveApp],
            owner: self,
            userInfo: nil
        )
        addTrackingArea(area)
        trackingArea = area
    }

    override func mouseEntered(with event: NSEvent) {
        if !isActive {
            isHovered = true
            needsDisplay = true
        }
    }

    override func mouseExited(with event: NSEvent) {
        if isHovered {
            isHovered = false
            needsDisplay = true
        }
    }

    override func drawSelection(in dirtyRect: NSRect) {
        // Selection handled in draw(dirtyRect:)
    }

    override func draw(_ dirtyRect: NSRect) {
        super.draw(dirtyRect)

        if isActive {
            SemanticColors.threadRowBg.setFill()
            let bgRect = bounds.insetBy(dx: SidebarViewController.Layout.rowBackgroundHorizontalInset, dy: 0)
            let bgPath = NSBezierPath(roundedRect: bgRect, xRadius: 6, yRadius: 6)
            bgPath.fill()

            SemanticColors.threadRowBorder.setStroke()
            let borderRect = bgRect.insetBy(dx: 0.5, dy: 0.5)
            let borderPath = NSBezierPath(roundedRect: borderRect, xRadius: 6, yRadius: 6)
            borderPath.lineWidth = 1
            borderPath.stroke()
        } else if isHovered {
            SemanticColors.threadRowHoverBg.setFill()
            let bgRect = bounds.insetBy(dx: SidebarViewController.Layout.rowBackgroundHorizontalInset, dy: 0)
            let bgPath = NSBezierPath(roundedRect: bgRect, xRadius: 6, yRadius: 6)
            bgPath.fill()

            SemanticColors.threadRowHoverBorder.setStroke()
            let borderRect = bgRect.insetBy(dx: 0.5, dy: 0.5)
            let borderPath = NSBezierPath(roundedRect: borderRect, xRadius: 6, yRadius: 6)
            borderPath.lineWidth = 1
            borderPath.stroke()
        }
    }

    override var interiorBackgroundStyle: NSView.BackgroundStyle { .normal }
}

// MARK: - Sidebar Add Button

/// "+" button with hover effect for sidebar header.
private class SidebarAddButton: NSButton {
    private var isHovered = false
    private var hoverTrackingArea: NSTrackingArea?

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        setup()
    }

    required init?(coder: NSCoder) {
        super.init(coder: coder)
        setup()
    }

    private func setup() {
        title = ""
        isBordered = false
        wantsLayer = true
        layer?.cornerRadius = 6
        layer?.backgroundColor = NSColor(white: 1, alpha: 0.04).cgColor

        let plusLabel = NSTextField(labelWithString: "+")
        plusLabel.font = NSFont.systemFont(ofSize: 14, weight: .medium)
        plusLabel.textColor = NSColor(white: 0.667, alpha: 1) // #aaa
        plusLabel.translatesAutoresizingMaskIntoConstraints = false
        plusLabel.drawsBackground = false
        plusLabel.isBezeled = false
        plusLabel.isEditable = false
        plusLabel.tag = 100
        addSubview(plusLabel)
        NSLayoutConstraint.activate([
            plusLabel.centerXAnchor.constraint(equalTo: centerXAnchor),
            plusLabel.centerYAnchor.constraint(equalTo: centerYAnchor),
        ])
    }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        if let existing = hoverTrackingArea {
            removeTrackingArea(existing)
        }
        let area = NSTrackingArea(
            rect: bounds,
            options: [.mouseEnteredAndExited, .activeInActiveApp],
            owner: self,
            userInfo: nil
        )
        addTrackingArea(area)
        hoverTrackingArea = area
    }

    override func mouseEntered(with event: NSEvent) {
        isHovered = true
        layer?.backgroundColor = NSColor(white: 1, alpha: 0.09).cgColor
        if let label = viewWithTag(100) as? NSTextField {
            label.textColor = .white
        }
    }

    override func mouseExited(with event: NSEvent) {
        isHovered = false
        layer?.backgroundColor = NSColor(white: 1, alpha: 0.04).cgColor
        if let label = viewWithTag(100) as? NSTextField {
            label.textColor = NSColor(white: 0.667, alpha: 1)
        }
    }
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
