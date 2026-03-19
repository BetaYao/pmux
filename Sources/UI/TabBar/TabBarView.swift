import AppKit

protocol TabBarDelegate: AnyObject {
    func tabBar(_ tabBar: TabBarView, didSelectTabAt index: Int)
    func tabBar(_ tabBar: TabBarView, didCloseTabAt index: Int)
}

struct TabItem {
    let title: String
    let isClosable: Bool  // Dashboard tab is not closable
}

class TabBarView: NSView {
    weak var delegate: TabBarDelegate?

    private var tabs: [TabItem] = []
    private var selectedIndex: Int = 0
    private var tabButtons: [NSButton] = []
    private let stackView = NSStackView()

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
        stackView.spacing = 1
        stackView.alignment = .centerY
        stackView.translatesAutoresizingMaskIntoConstraints = false
        addSubview(stackView)

        NSLayoutConstraint.activate([
            stackView.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 8),
            stackView.topAnchor.constraint(equalTo: topAnchor),
            stackView.bottomAnchor.constraint(equalTo: bottomAnchor),
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
        updateButtonAppearance()
    }

    private func rebuildButtons() {
        tabButtons.forEach { $0.removeFromSuperview() }
        tabButtons.removeAll()

        for (index, tab) in tabs.enumerated() {
            let button = createTabButton(tab, index: index)
            stackView.addArrangedSubview(button)
            tabButtons.append(button)
        }
        updateButtonAppearance()
    }

    private func createTabButton(_ tab: TabItem, index: Int) -> NSButton {
        let button = NSButton(frame: .zero)
        button.title = tab.title
        button.bezelStyle = .recessed
        button.isBordered = false
        button.font = NSFont.systemFont(ofSize: 12, weight: .medium)
        button.tag = index
        button.target = self
        button.action = #selector(tabClicked(_:))
        button.translatesAutoresizingMaskIntoConstraints = false

        NSLayoutConstraint.activate([
            button.heightAnchor.constraint(equalToConstant: 28),
            button.widthAnchor.constraint(greaterThanOrEqualToConstant: 80),
        ])

        return button
    }

    @objc private func tabClicked(_ sender: NSButton) {
        let index = sender.tag
        guard index >= 0, index < tabs.count else { return }
        selectedIndex = index
        updateButtonAppearance()
        delegate?.tabBar(self, didSelectTabAt: index)
    }

    private func updateButtonAppearance() {
        for (index, button) in tabButtons.enumerated() {
            let isSelected = index == selectedIndex
            button.wantsLayer = true
            button.layer?.cornerRadius = 6
            button.layer?.backgroundColor = isSelected ? Theme.surface.cgColor : NSColor.clear.cgColor
            button.contentTintColor = isSelected ? Theme.textPrimary : Theme.textSecondary
        }
    }

    override var intrinsicContentSize: NSSize {
        return NSSize(width: NSView.noIntrinsicMetric, height: Theme.tabBarHeight)
    }
}
