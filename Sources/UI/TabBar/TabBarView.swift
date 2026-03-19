import AppKit

protocol TabBarDelegate: AnyObject {
    func tabBar(_ tabBar: TabBarView, didSelectTabAt index: Int)
    func tabBar(_ tabBar: TabBarView, didCloseTabAt index: Int)
}

struct TabItem {
    let title: String
    let isClosable: Bool
}

class TabBarView: NSView {
    weak var delegate: TabBarDelegate?

    private var tabs: [TabItem] = []
    private var selectedIndex: Int = 0
    private var tabViews: [TabButtonView] = []
    private let stackView = NSStackView()
    private let statusLabel = NSTextField(labelWithString: "")

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        setup()
    }

    required init?(coder: NSCoder) {
        super.init(coder: coder)
        setup()
    }

    private func setup() {
        wantsLayer = true
        layer?.backgroundColor = Theme.background.cgColor

        stackView.orientation = .horizontal
        stackView.spacing = 2
        stackView.alignment = .centerY
        stackView.translatesAutoresizingMaskIntoConstraints = false
        addSubview(stackView)

        // Status counts (right side)
        statusLabel.font = NSFont.monospacedSystemFont(ofSize: 11, weight: .medium)
        statusLabel.textColor = Theme.textSecondary
        statusLabel.translatesAutoresizingMaskIntoConstraints = false
        addSubview(statusLabel)

        NSLayoutConstraint.activate([
            stackView.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 78),
            stackView.topAnchor.constraint(equalTo: topAnchor),
            stackView.bottomAnchor.constraint(equalTo: bottomAnchor),

            statusLabel.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -12),
            statusLabel.centerYAnchor.constraint(equalTo: centerYAnchor),
        ])
    }

    func setTabs(_ newTabs: [TabItem], selected: Int) {
        tabs = newTabs
        selectedIndex = selected
        rebuildButtons()
    }

    func selectTab(at index: Int) {
        guard index >= 0, index < tabs.count else { return }
        selectedIndex = index
        updateAppearance()
    }

    private func rebuildButtons() {
        tabViews.forEach { $0.removeFromSuperview() }
        tabViews.removeAll()

        for (index, tab) in tabs.enumerated() {
            let tabView = TabButtonView(title: tab.title, isClosable: tab.isClosable, index: index)
            tabView.onSelect = { [weak self] idx in
                self?.selectAndNotify(idx)
            }
            tabView.onClose = { [weak self] idx in
                self?.delegate?.tabBar(self!, didCloseTabAt: idx)
            }
            stackView.addArrangedSubview(tabView)
            tabViews.append(tabView)
        }
        updateAppearance()
    }

    private func selectAndNotify(_ index: Int) {
        guard index >= 0, index < tabs.count else { return }
        selectedIndex = index
        updateAppearance()
        delegate?.tabBar(self, didSelectTabAt: index)
    }

    private func updateAppearance() {
        for (index, tabView) in tabViews.enumerated() {
            tabView.setSelected(index == selectedIndex)
        }
    }

    func updateStatusCounts(running: Int, waiting: Int, error: Int) {
        var parts: [String] = []
        if running > 0 { parts.append("● \(running)") }
        if waiting > 0 { parts.append("◐ \(waiting)") }
        if error > 0   { parts.append("✕ \(error)") }
        statusLabel.stringValue = parts.joined(separator: "  ")

        // Color the label based on most urgent status
        if error > 0 {
            statusLabel.textColor = AgentStatus.error.color
        } else if waiting > 0 {
            statusLabel.textColor = AgentStatus.waiting.color
        } else if running > 0 {
            statusLabel.textColor = AgentStatus.running.color
        } else {
            statusLabel.textColor = Theme.textSecondary
        }
    }

    override var intrinsicContentSize: NSSize {
        return NSSize(width: NSView.noIntrinsicMetric, height: Theme.tabBarHeight)
    }
}

// MARK: - TabButtonView

private class TabButtonView: NSView {
    var onSelect: ((Int) -> Void)?
    var onClose: ((Int) -> Void)?

    private let titleLabel = NSTextField(labelWithString: "")
    private let closeButton = NSButton()
    private let index: Int
    private let isClosable: Bool
    private var isSelected = false

    init(title: String, isClosable: Bool, index: Int) {
        self.index = index
        self.isClosable = isClosable
        super.init(frame: .zero)

        wantsLayer = true
        layer?.cornerRadius = 6

        titleLabel.stringValue = title
        titleLabel.font = NSFont.systemFont(ofSize: 12, weight: .medium)
        titleLabel.lineBreakMode = .byTruncatingTail
        titleLabel.translatesAutoresizingMaskIntoConstraints = false
        addSubview(titleLabel)

        if isClosable {
            closeButton.title = "×"
            closeButton.bezelStyle = .recessed
            closeButton.isBordered = false
            closeButton.font = NSFont.systemFont(ofSize: 14, weight: .medium)
            closeButton.target = self
            closeButton.action = #selector(closeTapped)
            closeButton.translatesAutoresizingMaskIntoConstraints = false
            closeButton.contentTintColor = Theme.textSecondary
            addSubview(closeButton)

            NSLayoutConstraint.activate([
                titleLabel.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 10),
                titleLabel.centerYAnchor.constraint(equalTo: centerYAnchor),

                closeButton.leadingAnchor.constraint(equalTo: titleLabel.trailingAnchor, constant: 4),
                closeButton.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -4),
                closeButton.centerYAnchor.constraint(equalTo: centerYAnchor),
                closeButton.widthAnchor.constraint(equalToConstant: 18),
                closeButton.heightAnchor.constraint(equalToConstant: 18),
            ])
        } else {
            NSLayoutConstraint.activate([
                titleLabel.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 10),
                titleLabel.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -10),
                titleLabel.centerYAnchor.constraint(equalTo: centerYAnchor),
            ])
        }

        translatesAutoresizingMaskIntoConstraints = false
        NSLayoutConstraint.activate([
            heightAnchor.constraint(equalToConstant: 28),
            widthAnchor.constraint(greaterThanOrEqualToConstant: 80),
            widthAnchor.constraint(lessThanOrEqualToConstant: 180),
        ])

        // Click to select
        let click = NSClickGestureRecognizer(target: self, action: #selector(selectTapped))
        addGestureRecognizer(click)
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) not supported")
    }

    @objc private func selectTapped() {
        onSelect?(index)
    }

    @objc private func closeTapped() {
        onClose?(index)
    }

    func setSelected(_ selected: Bool) {
        isSelected = selected
        layer?.backgroundColor = selected ? Theme.surface.cgColor : NSColor.clear.cgColor
        titleLabel.textColor = selected ? Theme.textPrimary : Theme.textSecondary
        if isClosable {
            closeButton.contentTintColor = selected ? Theme.textSecondary : Theme.textSecondary.withAlphaComponent(0.5)
        }
    }
}
