import AppKit

final class StatusBarView: NSView {

    // MARK: - Subviews

    private let statusLabel: NSTextField = {
        let label = NSTextField(labelWithString: "")
        label.font = NSFont.systemFont(ofSize: 11)
        label.textColor = SemanticColors.muted
        label.lineBreakMode = .byTruncatingTail
        label.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        label.identifier = NSUserInterfaceItemIdentifier("statusbar.summary")
        label.translatesAutoresizingMaskIntoConstraints = false
        return label
    }()

    private let hintsStack: NSStackView = {
        let stack = NSStackView()
        stack.orientation = .horizontal
        stack.spacing = 8
        stack.translatesAutoresizingMaskIntoConstraints = false
        return stack
    }()

    private let topBorder: NSView = {
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

    func updateStatus(_ text: String) {
        statusLabel.stringValue = text
    }

    // MARK: - Setup

    private func setup() {
        identifier = NSUserInterfaceItemIdentifier("statusbar")
        wantsLayer = true
        translatesAutoresizingMaskIntoConstraints = false

        addSubview(topBorder)
        addSubview(statusLabel)
        addSubview(hintsStack)

        // Build hint badges
        let hints: [(String, [String])] = [
            ("切换布局", ["V"]),
            ("新建 Thread", ["N"]),
            ("提交弹窗", ["⌘", "Enter"]),
        ]
        for (label, keys) in hints {
            let pill = makeHintPill(label: label, keys: keys)
            hintsStack.addArrangedSubview(pill)
        }

        NSLayoutConstraint.activate([
            // Height
            heightAnchor.constraint(equalToConstant: 32),

            // Top border
            topBorder.topAnchor.constraint(equalTo: topAnchor),
            topBorder.leadingAnchor.constraint(equalTo: leadingAnchor),
            topBorder.trailingAnchor.constraint(equalTo: trailingAnchor),
            topBorder.heightAnchor.constraint(equalToConstant: 1),

            // Status label – left side, vertically centered
            statusLabel.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 10),
            statusLabel.centerYAnchor.constraint(equalTo: centerYAnchor),

            // Hints stack – right side, vertically centered
            hintsStack.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -10),
            hintsStack.centerYAnchor.constraint(equalTo: centerYAnchor),

            // Prevent overlap
            statusLabel.trailingAnchor.constraint(lessThanOrEqualTo: hintsStack.leadingAnchor, constant: -8),
        ])
    }

    // MARK: - Theme

    override func updateLayer() {
        layer?.backgroundColor = SemanticColors.panel.cgColor
        topBorder.layer?.backgroundColor = SemanticColors.line.withAlphaComponent(0.55).cgColor
        statusLabel.textColor = SemanticColors.muted
        updateHintColors()
    }

    private func updateHintColors() {
        for pill in hintsStack.arrangedSubviews {
            pill.layer?.borderColor = SemanticColors.line.withAlphaComponent(0.6).cgColor
            for sub in pill.subviews {
                if let label = sub as? NSTextField {
                    label.textColor = SemanticColors.muted
                } else if sub.identifier?.rawValue == "kbd" {
                    sub.layer?.backgroundColor = SemanticColors.panel2.cgColor
                    sub.layer?.borderColor = SemanticColors.line.cgColor
                    if let kbdLabel = sub.subviews.first as? NSTextField {
                        kbdLabel.textColor = SemanticColors.muted
                    }
                }
            }
        }
    }

    // MARK: - Hint pill builder

    private func makeHintPill(label: String, keys: [String]) -> NSView {
        let pill = NSView()
        pill.wantsLayer = true
        pill.layer?.cornerRadius = 6
        pill.layer?.borderWidth = 1
        pill.layer?.borderColor = SemanticColors.line.withAlphaComponent(0.6).cgColor
        pill.translatesAutoresizingMaskIntoConstraints = false

        let innerStack = NSStackView()
        innerStack.orientation = .horizontal
        innerStack.spacing = 4
        innerStack.edgeInsets = NSEdgeInsets(top: 0, left: 6, bottom: 0, right: 6)
        innerStack.translatesAutoresizingMaskIntoConstraints = false

        let textLabel = NSTextField(labelWithString: label)
        textLabel.font = NSFont.systemFont(ofSize: 10)
        textLabel.textColor = SemanticColors.muted
        textLabel.translatesAutoresizingMaskIntoConstraints = false
        innerStack.addArrangedSubview(textLabel)

        for key in keys {
            let kbd = makeKbd(key)
            innerStack.addArrangedSubview(kbd)
        }

        pill.addSubview(innerStack)

        NSLayoutConstraint.activate([
            innerStack.leadingAnchor.constraint(equalTo: pill.leadingAnchor),
            innerStack.trailingAnchor.constraint(equalTo: pill.trailingAnchor),
            innerStack.topAnchor.constraint(equalTo: pill.topAnchor),
            innerStack.bottomAnchor.constraint(equalTo: pill.bottomAnchor),
            pill.heightAnchor.constraint(equalToConstant: 22),
        ])

        return pill
    }

    private func makeKbd(_ text: String) -> NSView {
        let container = NSView()
        container.wantsLayer = true
        container.identifier = NSUserInterfaceItemIdentifier("kbd")
        container.layer?.cornerRadius = 4
        container.layer?.borderWidth = 1
        container.layer?.borderColor = SemanticColors.line.cgColor
        container.layer?.backgroundColor = SemanticColors.panel2.cgColor
        container.translatesAutoresizingMaskIntoConstraints = false

        let label = NSTextField(labelWithString: text)
        label.font = NSFont.systemFont(ofSize: 10, weight: .medium)
        label.textColor = SemanticColors.muted
        label.alignment = .center
        label.translatesAutoresizingMaskIntoConstraints = false
        container.addSubview(label)

        NSLayoutConstraint.activate([
            container.heightAnchor.constraint(equalToConstant: 17),
            container.widthAnchor.constraint(greaterThanOrEqualToConstant: 17),
            label.centerXAnchor.constraint(equalTo: container.centerXAnchor),
            label.centerYAnchor.constraint(equalTo: container.centerYAnchor),
            label.leadingAnchor.constraint(greaterThanOrEqualTo: container.leadingAnchor, constant: 3),
            label.trailingAnchor.constraint(lessThanOrEqualTo: container.trailingAnchor, constant: -3),
        ])

        return container
    }
}
