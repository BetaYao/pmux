import AppKit

protocol NotificationPanelDelegate: AnyObject {
    func notificationPanelDidRequestClose()
    func notificationPanelDidSelectItem(worktreePath: String)
}

final class NotificationPanelView: NSView {

    weak var delegate: NotificationPanelDelegate?
    private(set) var isOpen: Bool = false

    // MARK: - Data

    private var items: [(title: String, meta: String, worktreePath: String)] = []

    // MARK: - Subviews

    private let headerLabel: NSTextField = {
        let label = NSTextField(labelWithString: "通知")
        label.font = NSFont.boldSystemFont(ofSize: 13)
        label.textColor = SemanticColors.text
        label.translatesAutoresizingMaskIntoConstraints = false
        return label
    }()

    private let closeButton: NSButton = {
        let button = NSButton(title: "×", target: nil, action: nil)
        button.identifier = NSUserInterfaceItemIdentifier("panel.notification.close")
        button.isBordered = false
        button.font = NSFont.systemFont(ofSize: 16, weight: .medium)
        button.contentTintColor = SemanticColors.muted
        button.translatesAutoresizingMaskIntoConstraints = false
        return button
    }()

    private let headerBorder: NSView = {
        let v = NSView()
        v.wantsLayer = true
        v.translatesAutoresizingMaskIntoConstraints = false
        return v
    }()

    private let scrollView: NSScrollView = {
        let sv = NSScrollView()
        sv.hasVerticalScroller = true
        sv.hasHorizontalScroller = false
        sv.drawsBackground = false
        sv.automaticallyAdjustsContentInsets = false
        sv.translatesAutoresizingMaskIntoConstraints = false
        return sv
    }()

    private let contentStack: NSStackView = {
        let stack = NSStackView()
        stack.orientation = .vertical
        stack.spacing = 8
        stack.edgeInsets = NSEdgeInsets(top: 12, left: 12, bottom: 12, right: 12)
        stack.translatesAutoresizingMaskIntoConstraints = false
        return stack
    }()

    private let leftBorder: NSView = {
        let v = NSView()
        v.wantsLayer = true
        v.translatesAutoresizingMaskIntoConstraints = false
        return v
    }()

    // MARK: - Init

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        setup()
    }

    required init?(coder: NSCoder) {
        super.init(coder: coder)
        setup()
    }

    // MARK: - Public API

    func setOpen(_ open: Bool, animated: Bool = true) {
        guard open != isOpen else { return }
        isOpen = open
        isHidden = false

        if animated {
            NSAnimationContext.runAnimationGroup({ context in
                context.duration = 0.22
                context.timingFunction = CAMediaTimingFunction(name: .easeInEaseOut)
                self.animator().alphaValue = open ? 1.0 : 0.0
            }, completionHandler: {
                if !open { self.isHidden = true }
            })
        } else {
            alphaValue = open ? 1.0 : 0.0
            isHidden = !open
        }
    }

    func updateNotifications(_ items: [(title: String, meta: String)]) {
        updateNotifications(items.map { (title: $0.title, meta: $0.meta, worktreePath: "") })
    }

    func updateNotifications(_ items: [(title: String, meta: String, worktreePath: String)]) {
        self.items = items

        // Remove old items
        for view in contentStack.arrangedSubviews {
            contentStack.removeArrangedSubview(view)
            view.removeFromSuperview()
        }

        for (index, item) in items.enumerated() {
            let card = makeItemView(index: index, title: item.title, meta: item.meta)
            contentStack.addArrangedSubview(card)

            NSLayoutConstraint.activate([
                card.leadingAnchor.constraint(equalTo: contentStack.leadingAnchor, constant: 12),
                card.trailingAnchor.constraint(equalTo: contentStack.trailingAnchor, constant: -12),
            ])
        }
    }

    // MARK: - Setup

    private func setup() {
        identifier = NSUserInterfaceItemIdentifier("panel.notification")
        wantsLayer = true
        translatesAutoresizingMaskIntoConstraints = false
        isHidden = true
        alphaValue = 0

        closeButton.target = self
        closeButton.action = #selector(closeClicked)

        addSubview(leftBorder)
        addSubview(headerLabel)
        addSubview(closeButton)
        addSubview(headerBorder)
        addSubview(scrollView)

        scrollView.documentView = contentStack

        NSLayoutConstraint.activate([
            // Left border
            leftBorder.leadingAnchor.constraint(equalTo: leadingAnchor),
            leftBorder.topAnchor.constraint(equalTo: topAnchor),
            leftBorder.bottomAnchor.constraint(equalTo: bottomAnchor),
            leftBorder.widthAnchor.constraint(equalToConstant: 1),

            // Header
            headerLabel.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 16),
            headerLabel.centerYAnchor.constraint(equalTo: topAnchor, constant: 20),

            closeButton.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -12),
            closeButton.centerYAnchor.constraint(equalTo: headerLabel.centerYAnchor),

            // Header border
            headerBorder.leadingAnchor.constraint(equalTo: leadingAnchor),
            headerBorder.trailingAnchor.constraint(equalTo: trailingAnchor),
            headerBorder.topAnchor.constraint(equalTo: topAnchor, constant: 40),
            headerBorder.heightAnchor.constraint(equalToConstant: 1),

            // Scroll view
            scrollView.topAnchor.constraint(equalTo: headerBorder.bottomAnchor),
            scrollView.leadingAnchor.constraint(equalTo: leadingAnchor),
            scrollView.trailingAnchor.constraint(equalTo: trailingAnchor),
            scrollView.bottomAnchor.constraint(equalTo: bottomAnchor),

            // Content stack width
            contentStack.widthAnchor.constraint(equalTo: scrollView.widthAnchor),
        ])

        applyShadow()
    }

    private func applyShadow() {
        shadow = NSShadow()
        layer?.shadowColor = NSColor.black.withAlphaComponent(0.12).cgColor
        layer?.shadowOffset = CGSize(width: -8, height: 0)
        layer?.shadowRadius = 16
        layer?.shadowOpacity = 1.0
    }

    private func makeItemView(index: Int, title: String, meta: String) -> NSView {
        let container = NSView()
        container.wantsLayer = true
        container.identifier = NSUserInterfaceItemIdentifier("panel.notification.item.\(index)")
        container.translatesAutoresizingMaskIntoConstraints = false

        let titleLabel = NSTextField(labelWithString: title)
        titleLabel.font = NSFont.systemFont(ofSize: 12, weight: .semibold)
        titleLabel.textColor = SemanticColors.text
        titleLabel.lineBreakMode = .byTruncatingTail
        titleLabel.translatesAutoresizingMaskIntoConstraints = false

        let metaLabel = NSTextField(labelWithString: meta)
        metaLabel.font = NSFont.systemFont(ofSize: 11)
        metaLabel.textColor = SemanticColors.muted
        metaLabel.lineBreakMode = .byTruncatingTail
        metaLabel.translatesAutoresizingMaskIntoConstraints = false

        container.addSubview(titleLabel)
        container.addSubview(metaLabel)

        NSLayoutConstraint.activate([
            titleLabel.topAnchor.constraint(equalTo: container.topAnchor, constant: 10),
            titleLabel.leadingAnchor.constraint(equalTo: container.leadingAnchor, constant: 10),
            titleLabel.trailingAnchor.constraint(lessThanOrEqualTo: container.trailingAnchor, constant: -10),

            metaLabel.topAnchor.constraint(equalTo: titleLabel.bottomAnchor, constant: 4),
            metaLabel.leadingAnchor.constraint(equalTo: container.leadingAnchor, constant: 10),
            metaLabel.trailingAnchor.constraint(lessThanOrEqualTo: container.trailingAnchor, constant: -10),
            metaLabel.bottomAnchor.constraint(equalTo: container.bottomAnchor, constant: -10),
        ])

        // Click gesture
        let click = NSClickGestureRecognizer(target: self, action: #selector(itemClicked(_:)))
        container.addGestureRecognizer(click)

        return container
    }

    // MARK: - Drawing

    override var wantsUpdateLayer: Bool { true }

    override func updateLayer() {
        layer?.backgroundColor = SemanticColors.panel.cgColor
        leftBorder.layer?.backgroundColor = SemanticColors.line.withAlphaComponent(0.45).cgColor
        headerBorder.layer?.backgroundColor = SemanticColors.line.cgColor

        // Update item backgrounds
        for view in contentStack.arrangedSubviews {
            view.layer?.backgroundColor = SemanticColors.panel2.cgColor
            view.layer?.cornerRadius = 8
        }
    }

    // MARK: - Actions

    @objc private func closeClicked() {
        delegate?.notificationPanelDidRequestClose()
    }

    @objc private func itemClicked(_ gesture: NSClickGestureRecognizer) {
        guard let view = gesture.view,
              let idStr = view.identifier?.rawValue,
              idStr.hasPrefix("panel.notification.item."),
              let index = Int(idStr.replacingOccurrences(of: "panel.notification.item.", with: "")),
              index < items.count else { return }
        delegate?.notificationPanelDidSelectItem(worktreePath: items[index].worktreePath)
    }
}
