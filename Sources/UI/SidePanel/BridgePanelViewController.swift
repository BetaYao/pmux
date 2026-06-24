import AppKit

/// First Mate tab — shows red-zone pending orders (top) and green-zone watch entries (below).
/// Keyboard: j/k move selection, Enter approve/expand, n dismiss, x clear watch, → navigate.
final class BridgePanelViewController: NSViewController {

    var queue: PendingOrdersQueue? {
        didSet { rebind() }
    }
    var onNavigateToWorktree: ((String) -> Void)?
    var onApprove: ((PendingOrder) -> Void)?

    // MARK: - Private state

    private var pendingOrders: [PendingOrder] = []
    private var expandedOrderIds: Set<String> = []

    // MARK: - Views

    private let stackView = NSStackView()

    private let ordersHeader = NSTextField(labelWithString: "Pending Orders · 0")
    private let ordersTableView = NSTableView()
    private let ordersScrollView = NSScrollView()

    private let watchHeader = NSTextField(labelWithString: "Watch")
    private let watchTableView = NSTableView()
    private let watchScrollView = NSScrollView()

    // MARK: - Lifecycle

    override func loadView() {
        let root = NSView()
        root.wantsLayer = true
        root.layer?.backgroundColor = Theme.background.cgColor

        stackView.orientation = .vertical
        stackView.alignment = .leading
        stackView.spacing = 0
        stackView.distribution = .fill
        stackView.translatesAutoresizingMaskIntoConstraints = false

        root.addSubview(stackView)
        NSLayoutConstraint.activate([
            stackView.topAnchor.constraint(equalTo: root.topAnchor),
            stackView.leadingAnchor.constraint(equalTo: root.leadingAnchor),
            stackView.trailingAnchor.constraint(equalTo: root.trailingAnchor),
            stackView.bottomAnchor.constraint(equalTo: root.bottomAnchor),
        ])

        setupOrdersSection()
        setupWatchSection()

        view = root
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        reload()
    }

    // MARK: - Setup

    private func setupOrdersSection() {
        ordersHeader.font = NSFont.systemFont(ofSize: 11, weight: .semibold)
        ordersHeader.textColor = Theme.textSecondary
        ordersHeader.translatesAutoresizingMaskIntoConstraints = false

        let col = NSTableColumn(identifier: NSUserInterfaceItemIdentifier("OrderCol"))
        col.title = ""
        ordersTableView.addTableColumn(col)
        ordersTableView.headerView = nil
        ordersTableView.rowHeight = 28
        ordersTableView.dataSource = self
        ordersTableView.delegate = self
        ordersTableView.tag = 1
        ordersTableView.setAccessibilityIdentifier("bridge.ordersTable")
        ordersTableView.allowsEmptySelection = true

        ordersScrollView.documentView = ordersTableView
        ordersScrollView.hasVerticalScroller = true
        ordersScrollView.autohidesScrollers = true
        ordersScrollView.translatesAutoresizingMaskIntoConstraints = false

        let section = makeSectionContainer(header: ordersHeader, scroll: ordersScrollView, minHeight: 100)
        stackView.addArrangedSubview(section)
        stackView.addArrangedSubview(makeDivider())
    }

    private func setupWatchSection() {
        watchHeader.font = NSFont.systemFont(ofSize: 11, weight: .semibold)
        watchHeader.textColor = Theme.textSecondary
        watchHeader.translatesAutoresizingMaskIntoConstraints = false

        let col = NSTableColumn(identifier: NSUserInterfaceItemIdentifier("WatchCol"))
        col.title = ""
        watchTableView.addTableColumn(col)
        watchTableView.headerView = nil
        watchTableView.rowHeight = 22
        watchTableView.dataSource = self
        watchTableView.delegate = self
        watchTableView.tag = 2
        watchTableView.setAccessibilityIdentifier("bridge.watchTable")
        watchTableView.allowsEmptySelection = true

        watchScrollView.documentView = watchTableView
        watchScrollView.hasVerticalScroller = true
        watchScrollView.autohidesScrollers = true
        watchScrollView.translatesAutoresizingMaskIntoConstraints = false

        let section = makeSectionContainer(header: watchHeader, scroll: watchScrollView, minHeight: 60)
        stackView.addArrangedSubview(section)
    }

    private func makeSectionContainer(header: NSTextField, scroll: NSScrollView, minHeight: CGFloat) -> NSView {
        let container = NSView()
        container.translatesAutoresizingMaskIntoConstraints = false
        container.addSubview(header)
        container.addSubview(scroll)
        NSLayoutConstraint.activate([
            header.topAnchor.constraint(equalTo: container.topAnchor, constant: 6),
            header.leadingAnchor.constraint(equalTo: container.leadingAnchor, constant: 8),
            header.trailingAnchor.constraint(equalTo: container.trailingAnchor, constant: -8),

            scroll.topAnchor.constraint(equalTo: header.bottomAnchor, constant: 4),
            scroll.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            scroll.trailingAnchor.constraint(equalTo: container.trailingAnchor),
            scroll.bottomAnchor.constraint(equalTo: container.bottomAnchor),
            scroll.heightAnchor.constraint(greaterThanOrEqualToConstant: minHeight),
        ])
        return container
    }

    private func makeDivider() -> NSView {
        let line = NSView()
        line.wantsLayer = true
        line.layer?.backgroundColor = NSColor.separatorColor.cgColor
        line.translatesAutoresizingMaskIntoConstraints = false
        line.heightAnchor.constraint(equalToConstant: 1).isActive = true
        return line
    }

    // MARK: - Data

    private func rebind() {
        queue?.onChange = { [weak self] in
            DispatchQueue.main.async { self?.reload() }
        }
        if isViewLoaded { reload() }
    }

    private func reload() {
        pendingOrders = queue?.all() ?? []
        ordersHeader.stringValue = "Pending Orders · \(pendingOrders.count)"
        ordersTableView.reloadData()
        watchTableView.reloadData()
    }

    // MARK: - Keyboard

    override func keyDown(with event: NSEvent) {
        guard let key = event.characters else { super.keyDown(with: event); return }

        let activeTable = ordersTableView.window?.firstResponder === ordersTableView
            ? ordersTableView
            : (watchTableView.window?.firstResponder === watchTableView ? watchTableView : ordersTableView)

        switch key {
        case "j":
            moveSelection(in: activeTable, by: 1)
        case "k":
            moveSelection(in: activeTable, by: -1)
        case "\r":
            handleEnter(in: activeTable)
        case "n":
            handleDismiss(in: activeTable)
        case "x":
            // clear watch — no explicit watch store, orders are in queue
            break
        default:
            if event.keyCode == 124 { // right arrow
                handleNavigate(in: activeTable)
            } else {
                super.keyDown(with: event)
            }
        }
    }

    private func moveSelection(in tableView: NSTableView, by delta: Int) {
        let count = tableView.numberOfRows
        guard count > 0 else { return }
        let current = tableView.selectedRow
        let next = max(0, min(count - 1, (current == -1 ? (delta > 0 ? 0 : count - 1) : current + delta)))
        tableView.selectRowIndexes(IndexSet(integer: next), byExtendingSelection: false)
        tableView.scrollRowToVisible(next)
    }

    private func handleEnter(in tableView: NSTableView) {
        guard tableView.tag == 1 else { return }
        let row = tableView.selectedRow
        guard row >= 0, row < pendingOrders.count else { return }
        let order = pendingOrders[row]
        let isExpanded = expandedOrderIds.contains(order.id)
        let decision = BridgeConfirmFlow.onEnter(kind: order.action.kind, expanded: isExpanded)
        switch decision {
        case .expand:
            expandedOrderIds.insert(order.id)
            ordersTableView.reloadData(forRowIndexes: IndexSet(integer: row), columnIndexes: IndexSet(integer: 0))
        case .execute:
            expandedOrderIds.remove(order.id)
            onApprove?(order)
            queue?.resolve(id: order.id)
        }
    }

    private func handleDismiss(in tableView: NSTableView) {
        guard tableView.tag == 1 else { return }
        let row = tableView.selectedRow
        guard row >= 0, row < pendingOrders.count else { return }
        let order = pendingOrders[row]
        queue?.resolve(id: order.id)
    }

    private func handleNavigate(in tableView: NSTableView) {
        let row = tableView.selectedRow
        guard tableView.tag == 1, row >= 0, row < pendingOrders.count else { return }
        let order = pendingOrders[row]
        onNavigateToWorktree?(order.action.worktreePath)
    }
}

// MARK: - NSTableViewDataSource / Delegate

extension BridgePanelViewController: NSTableViewDataSource, NSTableViewDelegate {
    func numberOfRows(in tableView: NSTableView) -> Int {
        tableView.tag == 1 ? pendingOrders.count : 0
    }

    func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView? {
        guard tableView.tag == 1, row < pendingOrders.count else { return nil }
        let order = pendingOrders[row]
        let isExpanded = expandedOrderIds.contains(order.id)

        let id = NSUserInterfaceItemIdentifier("OrderCell")
        let cell: OrderCellView
        if let reused = tableView.makeView(withIdentifier: id, owner: self) as? OrderCellView {
            cell = reused
        } else {
            cell = OrderCellView()
            cell.identifier = id
        }
        cell.configure(order: order, expanded: isExpanded,
                       onApprove: { [weak self] in self?.approveOrder(order) },
                       onDismiss: { [weak self] in self?.queue?.resolve(id: order.id) })
        return cell
    }
}

// MARK: - Private helpers

private extension BridgePanelViewController {
    func approveOrder(_ order: PendingOrder) {
        let isExpanded = expandedOrderIds.contains(order.id)
        let decision = BridgeConfirmFlow.onEnter(kind: order.action.kind, expanded: isExpanded)
        switch decision {
        case .expand:
            expandedOrderIds.insert(order.id)
            if let idx = pendingOrders.firstIndex(where: { $0.id == order.id }) {
                ordersTableView.reloadData(forRowIndexes: IndexSet(integer: idx), columnIndexes: IndexSet(integer: 0))
            }
        case .execute:
            expandedOrderIds.remove(order.id)
            onApprove?(order)
            queue?.resolve(id: order.id)
        }
    }
}

// MARK: - OrderCellView

private final class OrderCellView: NSTableCellView {
    private let messageLabel = NSTextField(labelWithString: "")
    private let approveButton = NSButton()
    private let dismissButton = NSButton()

    private var onApprove: (() -> Void)?
    private var onDismiss: (() -> Void)?

    override init(frame: NSRect) {
        super.init(frame: frame)
        setup()
    }
    required init?(coder: NSCoder) { fatalError() }

    private func setup() {
        messageLabel.font = NSFont.systemFont(ofSize: 12)
        messageLabel.lineBreakMode = .byTruncatingTail
        messageLabel.translatesAutoresizingMaskIntoConstraints = false

        approveButton.bezelStyle = .inline
        approveButton.title = "✓"
        approveButton.contentTintColor = .systemGreen
        approveButton.isBordered = false
        approveButton.target = self
        approveButton.action = #selector(tappedApprove)
        approveButton.translatesAutoresizingMaskIntoConstraints = false

        dismissButton.bezelStyle = .inline
        dismissButton.title = "✕"
        dismissButton.contentTintColor = .systemRed
        dismissButton.isBordered = false
        dismissButton.target = self
        dismissButton.action = #selector(tappedDismiss)
        dismissButton.translatesAutoresizingMaskIntoConstraints = false

        addSubview(messageLabel)
        addSubview(approveButton)
        addSubview(dismissButton)

        NSLayoutConstraint.activate([
            dismissButton.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -6),
            dismissButton.centerYAnchor.constraint(equalTo: centerYAnchor),
            dismissButton.widthAnchor.constraint(equalToConstant: 20),

            approveButton.trailingAnchor.constraint(equalTo: dismissButton.leadingAnchor, constant: -4),
            approveButton.centerYAnchor.constraint(equalTo: centerYAnchor),
            approveButton.widthAnchor.constraint(equalToConstant: 20),

            messageLabel.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 8),
            messageLabel.trailingAnchor.constraint(equalTo: approveButton.leadingAnchor, constant: -4),
            messageLabel.centerYAnchor.constraint(equalTo: centerYAnchor),
        ])
    }

    func configure(order: PendingOrder, expanded: Bool, onApprove: @escaping () -> Void, onDismiss: @escaping () -> Void) {
        self.onApprove = onApprove
        self.onDismiss = onDismiss
        let prefix = expanded ? "⚠ " : ""
        messageLabel.stringValue = prefix + order.action.message
        messageLabel.textColor = expanded ? .systemOrange : NSColor(named: "textPrimary") ?? .labelColor
        approveButton.title = expanded ? "!!" : "✓"
    }

    @objc private func tappedApprove() { onApprove?() }
    @objc private func tappedDismiss() { onDismiss?() }
}
