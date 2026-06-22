import AppKit

// MARK: - CenterOverlayView

/// Full-cover overlay rendered on top of the center terminal panel.
/// Contains a header bar (title + close button) and a content area below.
/// The terminal keeps running underneath; dismiss via Esc or the close button.
final class CenterOverlayView: NSView {

    // MARK: Private

    private let titleLabel = NSTextField(labelWithString: "")
    private let closeButton = NSButton()
    private let headerBar = NSView()
    private let contentContainer = NSView()
    private let onClose: () -> Void

    // MARK: Init

    init(title: String, content: NSView, onClose: @escaping () -> Void) {
        self.onClose = onClose
        super.init(frame: .zero)

        wantsLayer = true
        layer?.backgroundColor = SemanticColors.bg.cgColor

        setupHeader(title: title)
        setupContent(content)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError() }

    // MARK: Setup

    private func setupHeader(title: String) {
        headerBar.translatesAutoresizingMaskIntoConstraints = false
        addSubview(headerBar)

        titleLabel.translatesAutoresizingMaskIntoConstraints = false
        titleLabel.stringValue = title
        titleLabel.textColor = SemanticColors.text
        titleLabel.font = NSFont.systemFont(ofSize: 13, weight: .medium)
        titleLabel.lineBreakMode = .byTruncatingTail
        headerBar.addSubview(titleLabel)

        closeButton.translatesAutoresizingMaskIntoConstraints = false
        closeButton.isBordered = false
        closeButton.image = NSImage(systemSymbolName: "xmark", accessibilityDescription: "Close")
        closeButton.contentTintColor = SemanticColors.text
        closeButton.target = self
        closeButton.action = #selector(closeButtonTapped)
        headerBar.addSubview(closeButton)

        NSLayoutConstraint.activate([
            headerBar.topAnchor.constraint(equalTo: topAnchor),
            headerBar.leadingAnchor.constraint(equalTo: leadingAnchor),
            headerBar.trailingAnchor.constraint(equalTo: trailingAnchor),
            headerBar.heightAnchor.constraint(equalToConstant: 32),

            titleLabel.leadingAnchor.constraint(equalTo: headerBar.leadingAnchor, constant: 12),
            titleLabel.centerYAnchor.constraint(equalTo: headerBar.centerYAnchor),
            titleLabel.trailingAnchor.constraint(lessThanOrEqualTo: closeButton.leadingAnchor, constant: -8),

            closeButton.trailingAnchor.constraint(equalTo: headerBar.trailingAnchor, constant: -8),
            closeButton.centerYAnchor.constraint(equalTo: headerBar.centerYAnchor),
            closeButton.widthAnchor.constraint(equalToConstant: 20),
            closeButton.heightAnchor.constraint(equalToConstant: 20),
        ])
    }

    private func setupContent(_ content: NSView) {
        contentContainer.translatesAutoresizingMaskIntoConstraints = false
        addSubview(contentContainer)

        content.translatesAutoresizingMaskIntoConstraints = false
        contentContainer.addSubview(content)

        NSLayoutConstraint.activate([
            contentContainer.topAnchor.constraint(equalTo: headerBar.bottomAnchor),
            contentContainer.leadingAnchor.constraint(equalTo: leadingAnchor),
            contentContainer.trailingAnchor.constraint(equalTo: trailingAnchor),
            contentContainer.bottomAnchor.constraint(equalTo: bottomAnchor),

            content.topAnchor.constraint(equalTo: contentContainer.topAnchor),
            content.leadingAnchor.constraint(equalTo: contentContainer.leadingAnchor),
            content.trailingAnchor.constraint(equalTo: contentContainer.trailingAnchor),
            content.bottomAnchor.constraint(equalTo: contentContainer.bottomAnchor),
        ])
    }

    // MARK: Actions

    @objc private func closeButtonTapped() {
        onClose()
    }

    // MARK: Key handling

    override var acceptsFirstResponder: Bool { true }

    override func keyDown(with event: NSEvent) {
        if event.keyCode == 53 { // Esc
            onClose()
        } else {
            super.keyDown(with: event)
        }
    }

    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        if event.keyCode == 53 {
            onClose()
            return true
        }
        return super.performKeyEquivalent(with: event)
    }
}
