import AppKit

protocol SidebarDelegate: AnyObject {
    func sidebar(_ sidebar: SidebarViewController, didSelectWorktreeAt index: Int)
    func sidebar(_ sidebar: SidebarViewController, didRequestDeleteWorktreeAt index: Int)
    func sidebarDidRequestNewThread(_ sidebar: SidebarViewController)
    func sidebar(_ sidebar: SidebarViewController, didRequestShowDiffAt index: Int)
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
    private let diffButton = NSButton()
    private let addButton = NSButton()
    private let headerBorder = NSView()
    private let scrollView = NSScrollView()
    private let tableView = NSTableView()
    private let emptyStateLabel = NSTextField(labelWithString: "No thread yet. Click + to create one.")
    private var worktrees: [WorktreeInfo] = []
    private var statuses: [String: AgentStatus] = [:]
    private var lastMessages: [String: String] = [:]
    private var selectedIndex: Int = 0
    private var hasExplicitSelection = false
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
        addButton.title = ""
        addButton.bezelStyle = .texturedRounded
        addButton.isBordered = true
        addButton.image = NSImage(named: NSImage.addTemplateName)
        addButton.imagePosition = .imageOnly
        addButton.contentTintColor = SemanticColors.muted
        addButton.target = self
        addButton.action = #selector(addThreadClicked)
        addButton.setAccessibilityIdentifier("sidebar.addThread")
        headerBar.addSubview(addButton)

        diffButton.translatesAutoresizingMaskIntoConstraints = false
        diffButton.title = ""
        diffButton.bezelStyle = .texturedRounded
        diffButton.isBordered = true
        diffButton.image = NSImage(systemSymbolName: "doc.text.magnifyingglass", accessibilityDescription: "Show diff")
        diffButton.imagePosition = .imageOnly
        diffButton.contentTintColor = SemanticColors.muted
        diffButton.isEnabled = false
        diffButton.target = self
        diffButton.action = #selector(showDiffClicked)
        diffButton.setAccessibilityIdentifier("sidebar.showDiff")
        headerBar.addSubview(diffButton)

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
        tableView.selectionHighlightStyle = .regular
        tableView.allowsEmptySelection = false
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

            diffButton.centerYAnchor.constraint(equalTo: headerBar.centerYAnchor),
            diffButton.trailingAnchor.constraint(equalTo: addButton.leadingAnchor, constant: -6),
            diffButton.heightAnchor.constraint(equalToConstant: 24),

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

    @objc private func showDiffClicked() {
        guard hasExplicitSelection,
              selectedIndex >= 0,
              selectedIndex < worktrees.count
        else { return }
        sidebarDelegate?.sidebar(self, didRequestShowDiffAt: selectedIndex)
    }

    private func updateDiffButtonState() {
        diffButton.isEnabled = hasExplicitSelection && selectedIndex >= 0 && selectedIndex < worktrees.count
    }

    func setWorktrees(_ worktrees: [WorktreeInfo]) {
        self.worktrees = worktrees
        hasExplicitSelection = false
        countLabel.stringValue = "\(worktrees.count)"
        emptyStateLabel.isHidden = !worktrees.isEmpty
        scrollView.isHidden = worktrees.isEmpty
        updateDiffButtonState()
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
        updateDiffButtonState()
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
        hasExplicitSelection = true
        updateDiffButtonState()
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

private final class SidebarCellView: NSTableCellView {
    private let nameLabel = NSTextField(labelWithString: "")
    private let dotImageView = NSImageView()
    private let messageLabel = NSTextField(labelWithString: "")

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        setupViews()
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) not supported")
    }

    private func setupViews() {
        dotImageView.translatesAutoresizingMaskIntoConstraints = false
        dotImageView.imageScaling = .scaleProportionallyDown
        addSubview(dotImageView)

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
            dotImageView.widthAnchor.constraint(equalToConstant: 10),
            dotImageView.heightAnchor.constraint(equalToConstant: 10),
            dotImageView.leadingAnchor.constraint(equalTo: leadingAnchor, constant: SidebarViewController.Layout.cellLeadingInset),
            dotImageView.centerYAnchor.constraint(equalTo: nameLabel.centerYAnchor),

            nameLabel.topAnchor.constraint(equalTo: topAnchor, constant: 9),
            nameLabel.leadingAnchor.constraint(equalTo: dotImageView.trailingAnchor, constant: 6),
            nameLabel.trailingAnchor.constraint(lessThanOrEqualTo: trailingAnchor, constant: -SidebarViewController.Layout.cellTrailingInset),

            messageLabel.topAnchor.constraint(equalTo: nameLabel.bottomAnchor, constant: 2),
            messageLabel.leadingAnchor.constraint(equalTo: dotImageView.trailingAnchor, constant: 6),
            messageLabel.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -SidebarViewController.Layout.cellTrailingInset),
            messageLabel.bottomAnchor.constraint(lessThanOrEqualTo: bottomAnchor, constant: -9),
        ])
    }

    func update(name: String, status: AgentStatus, message: String) {
        nameLabel.stringValue = name
        let symbolConfig = NSImage.SymbolConfiguration(pointSize: 8, weight: .bold)
        dotImageView.image = NSImage(systemSymbolName: "circle.fill", accessibilityDescription: nil)?.withSymbolConfiguration(symbolConfig)
        dotImageView.contentTintColor = status.color
        messageLabel.stringValue = message.isEmpty ? status.rawValue : message
        setAccessibilityIdentifier("sidebar.row.\(name)")
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
