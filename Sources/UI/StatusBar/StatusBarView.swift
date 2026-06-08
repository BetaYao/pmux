import AppKit

/// Fixed-height bottom bar: global Claude/Codex usage (left), notification
/// summary (center), high-frequency shortcuts (right).
final class StatusBarView: NSView {
    static let height: CGFloat = 26

    private let usageLabel = NSTextField(labelWithString: "")
    private let notificationLabel = NSTextField(labelWithString: "")
    private let shortcutsLabel = NSTextField(labelWithString: "")

    var usageTextForTesting: String { usageLabel.stringValue }
    var notificationTextForTesting: String { notificationLabel.stringValue }

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        setup()
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) not supported") }

    func updateUsage(text: String) { usageLabel.stringValue = text }
    func updateNotification(text: String) { notificationLabel.stringValue = text }
    func updateShortcuts(text: String) { shortcutsLabel.stringValue = text }

    private func setup() {
        wantsLayer = true
        layer?.backgroundColor = resolvedCGColor(SemanticColors.tileBarBg)

        for label in [usageLabel, notificationLabel, shortcutsLabel] {
            label.font = NSFont.systemFont(ofSize: 10, weight: .regular)
            label.textColor = SemanticColors.muted
            label.lineBreakMode = .byTruncatingTail
            label.maximumNumberOfLines = 1
            label.translatesAutoresizingMaskIntoConstraints = false
            addSubview(label)
        }
        shortcutsLabel.alignment = .right
        notificationLabel.alignment = .center

        shortcutsLabel.stringValue = "\u{2318}N New  \u{00B7}  \u{2318}D Split  \u{00B7}  \u{2318}P Switch"

        NSLayoutConstraint.activate([
            usageLabel.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 12),
            usageLabel.centerYAnchor.constraint(equalTo: centerYAnchor),
            notificationLabel.centerXAnchor.constraint(equalTo: centerXAnchor),
            notificationLabel.centerYAnchor.constraint(equalTo: centerYAnchor),
            shortcutsLabel.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -12),
            shortcutsLabel.centerYAnchor.constraint(equalTo: centerYAnchor),
            usageLabel.trailingAnchor.constraint(lessThanOrEqualTo: notificationLabel.leadingAnchor, constant: -8),
            notificationLabel.trailingAnchor.constraint(lessThanOrEqualTo: shortcutsLabel.leadingAnchor, constant: -8),
        ])
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        layer?.backgroundColor = resolvedCGColor(SemanticColors.tileBarBg)
    }
}
