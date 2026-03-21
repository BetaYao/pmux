import AppKit

struct ModalConfig {
    let title: String
    let subtitle: String
    var placeholder: String = ""
    var initialValue: String = ""
    var confirmText: String = "确认"
    var isMultiline: Bool = false
    var confirmStyle: ModalButtonStyle = .primary

    enum ModalButtonStyle {
        case primary
        case warn
    }
}

protocol UnifiedModalDelegate: AnyObject {
    func modalDidConfirm(value: String)
    func modalDidCancel()
}

final class UnifiedModalView: NSView, NSTextFieldDelegate, NSTextViewDelegate {
    weak var delegate: UnifiedModalDelegate?

    private let cardView = NSView()
    private let titleLabel = NSTextField(labelWithString: "")
    private let subtitleLabel = NSTextField(labelWithString: "")
    private var singleLineInput: NSTextField?
    private var multilineScrollView: NSScrollView?
    private var multilineTextView: NSTextView?
    private let cancelButton = NSButton()
    private let confirmButton = NSButton()
    private let buttonRow = NSStackView()
    private let contentStack = NSStackView()

    private var currentConfig: ModalConfig?
    private var isMultilineMode = false

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        setupView()
    }

    required init?(coder: NSCoder) {
        super.init(coder: coder)
        setupView()
    }

    private func setupView() {
        wantsLayer = true
        translatesAutoresizingMaskIntoConstraints = false
        setAccessibilityIdentifier("modal.overlay")
        isHidden = true

        // Card
        cardView.wantsLayer = true
        cardView.translatesAutoresizingMaskIntoConstraints = false
        addSubview(cardView)

        // Title
        titleLabel.font = NSFont.systemFont(ofSize: 16, weight: .bold)
        titleLabel.textColor = SemanticColors.text
        titleLabel.setAccessibilityIdentifier("modal.title")
        titleLabel.lineBreakMode = .byTruncatingTail
        titleLabel.maximumNumberOfLines = 1

        // Subtitle
        subtitleLabel.font = NSFont.systemFont(ofSize: 12, weight: .regular)
        subtitleLabel.textColor = SemanticColors.muted
        subtitleLabel.setAccessibilityIdentifier("modal.subtitle")
        subtitleLabel.lineBreakMode = .byWordWrapping
        subtitleLabel.maximumNumberOfLines = 0

        // Cancel button
        cancelButton.title = "取消"
        cancelButton.bezelStyle = .rounded
        cancelButton.setAccessibilityIdentifier("modal.cancel")
        cancelButton.target = self
        cancelButton.action = #selector(cancelClicked)

        // Confirm button
        confirmButton.bezelStyle = .rounded
        confirmButton.setAccessibilityIdentifier("modal.confirm")
        confirmButton.target = self
        confirmButton.action = #selector(confirmClicked)

        // Button row
        buttonRow.orientation = .horizontal
        buttonRow.spacing = 8
        buttonRow.addArrangedSubview(cancelButton)
        buttonRow.addArrangedSubview(confirmButton)

        // Content stack
        contentStack.orientation = .vertical
        contentStack.alignment = .leading
        contentStack.spacing = 12
        contentStack.translatesAutoresizingMaskIntoConstraints = false
        contentStack.addArrangedSubview(titleLabel)
        contentStack.addArrangedSubview(subtitleLabel)
        // Input will be inserted at index 2
        contentStack.addArrangedSubview(buttonRow)
        contentStack.edgeInsets = NSEdgeInsets(top: 16, left: 16, bottom: 16, right: 16)
        cardView.addSubview(contentStack)

        // Card constraints - centered, max width 560
        let cardWidth = cardView.widthAnchor.constraint(equalToConstant: 560)
        cardWidth.priority = .defaultHigh

        NSLayoutConstraint.activate([
            cardView.centerXAnchor.constraint(equalTo: centerXAnchor),
            cardView.centerYAnchor.constraint(equalTo: centerYAnchor),
            cardWidth,
            cardView.widthAnchor.constraint(lessThanOrEqualTo: widthAnchor, constant: -32),

            contentStack.topAnchor.constraint(equalTo: cardView.topAnchor),
            contentStack.leadingAnchor.constraint(equalTo: cardView.leadingAnchor),
            contentStack.trailingAnchor.constraint(equalTo: cardView.trailingAnchor),
            contentStack.bottomAnchor.constraint(equalTo: cardView.bottomAnchor),
        ])

        // Button row alignment: trailing
        let buttonRowTrailing = buttonRow.trailingAnchor.constraint(
            equalTo: contentStack.trailingAnchor, constant: -16)
        buttonRowTrailing.isActive = true

        applyTheme()
    }

    private func applyTheme() {
        layer?.backgroundColor = NSColor.black.withAlphaComponent(0.6).cgColor

        cardView.layer?.backgroundColor = SemanticColors.panel.cgColor
        cardView.layer?.cornerRadius = 10
        cardView.layer?.borderWidth = 1
        cardView.layer?.borderColor = NSColor(hex: 0x333333).cgColor

        titleLabel.textColor = SemanticColors.text
        subtitleLabel.textColor = SemanticColors.muted

        if let input = singleLineInput {
            styleSingleLineInput(input)
        }
        if let tv = multilineTextView, let sv = multilineScrollView {
            styleMultilineInput(scrollView: sv, textView: tv)
        }

        styleConfirmButton()
    }

    private func styleSingleLineInput(_ field: NSTextField) {
        field.backgroundColor = .textBackgroundColor
        field.textColor = SemanticColors.text
        field.focusRingType = .default
        field.isBordered = true
        field.bezelStyle = .roundedBezel
    }

    private func styleMultilineInput(scrollView: NSScrollView, textView: NSTextView) {
        scrollView.wantsLayer = true
        scrollView.layer?.cornerRadius = 6
        scrollView.layer?.borderWidth = 1
        scrollView.layer?.borderColor = SemanticColors.line.cgColor
        scrollView.backgroundColor = SemanticColors.tileBg
        scrollView.drawsBackground = true

        textView.backgroundColor = SemanticColors.tileBg
        textView.textColor = SemanticColors.text
        textView.insertionPointColor = SemanticColors.text
    }

    private func styleConfirmButton() {
        guard let config = currentConfig else { return }
        confirmButton.wantsLayer = false
        confirmButton.isBordered = true
        confirmButton.bezelStyle = .rounded

        let color: NSColor
        switch config.confirmStyle {
        case .primary:
            color = SemanticColors.accent
        case .warn:
            color = SemanticColors.danger
        }
        confirmButton.contentTintColor = color

        // Style cancel button
        cancelButton.wantsLayer = false
        cancelButton.isBordered = true
        cancelButton.bezelStyle = .rounded
        cancelButton.contentTintColor = NSColor(hex: 0xaaaaaa)
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        applyTheme()
    }

    // MARK: - Public API

    func show(config: ModalConfig) {
        currentConfig = config
        isMultilineMode = config.isMultiline

        titleLabel.stringValue = config.title
        subtitleLabel.stringValue = config.subtitle
        confirmButton.title = config.confirmText

        // Remove old input if present
        if let old = singleLineInput {
            contentStack.removeArrangedSubview(old)
            old.removeFromSuperview()
            singleLineInput = nil
        }
        if let old = multilineScrollView {
            contentStack.removeArrangedSubview(old)
            old.removeFromSuperview()
            multilineScrollView = nil
            multilineTextView = nil
        }

        if config.isMultiline {
            let (scrollView, textView) = makeMultilineInput(config: config)
            multilineScrollView = scrollView
            multilineTextView = textView
            // Insert before button row (index 2)
            contentStack.insertArrangedSubview(scrollView, at: 2)
            scrollView.translatesAutoresizingMaskIntoConstraints = false
            NSLayoutConstraint.activate([
                scrollView.widthAnchor.constraint(equalTo: contentStack.widthAnchor, constant: -32),
                scrollView.heightAnchor.constraint(greaterThanOrEqualToConstant: 108),
            ])
        } else {
            let input = makeSingleLineInput(config: config)
            singleLineInput = input
            contentStack.insertArrangedSubview(input, at: 2)
            input.translatesAutoresizingMaskIntoConstraints = false
            NSLayoutConstraint.activate([
                input.widthAnchor.constraint(equalTo: contentStack.widthAnchor, constant: -32),
            ])
        }

        applyTheme()
        isHidden = false

        // Focus input after showing
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            if let input = self.singleLineInput {
                self.window?.makeFirstResponder(input)
            } else if let textView = self.multilineTextView {
                self.window?.makeFirstResponder(textView)
            }
        }
    }

    func dismiss() {
        isHidden = true
    }

    // MARK: - Input factory

    private func makeSingleLineInput(config: ModalConfig) -> NSTextField {
        let field = NSTextField()
        field.placeholderString = config.placeholder
        field.stringValue = config.initialValue
        field.font = NSFont.systemFont(ofSize: 13)
        field.setAccessibilityIdentifier("modal.input")
        field.delegate = self
        field.usesSingleLineMode = true
        field.cell?.wraps = false
        field.cell?.isScrollable = true

        // Padding via custom cell inset
        let paddedCell = PaddedTextFieldCell(textCell: "")
        paddedCell.font = field.font
        paddedCell.placeholderString = config.placeholder
        paddedCell.stringValue = config.initialValue
        paddedCell.isEditable = true
        paddedCell.isSelectable = true
        paddedCell.usesSingleLineMode = true
        paddedCell.wraps = false
        paddedCell.isScrollable = true
        field.cell = paddedCell

        styleSingleLineInput(field)
        field.heightAnchor.constraint(equalToConstant: 36).isActive = true
        return field
    }

    private func makeMultilineInput(config: ModalConfig) -> (NSScrollView, NSTextView) {
        let scrollView = NSScrollView()
        scrollView.hasVerticalScroller = true
        scrollView.hasHorizontalScroller = false
        scrollView.autohidesScrollers = true
        scrollView.setAccessibilityIdentifier("modal.input")

        let textView = NSTextView()
        textView.isRichText = false
        textView.isEditable = true
        textView.isSelectable = true
        textView.allowsUndo = true
        textView.isVerticallyResizable = true
        textView.isHorizontallyResizable = false
        textView.autoresizingMask = [.width]
        textView.textContainerInset = NSSize(width: 10, height: 10)
        textView.setAccessibilityIdentifier("modal.input")

        // Font: try JetBrains Mono, fall back to monospace system font
        let monoFont: NSFont
        if let jb = NSFont(name: "JetBrains Mono", size: 13) {
            monoFont = jb
        } else {
            monoFont = NSFont.monospacedSystemFont(ofSize: 13, weight: .regular)
        }
        textView.font = monoFont

        // Line spacing: 1.5x line height
        let paragraphStyle = NSMutableParagraphStyle()
        let lineHeight = monoFont.ascender + abs(monoFont.descender) + monoFont.leading
        paragraphStyle.lineSpacing = lineHeight * 0.5
        textView.defaultParagraphStyle = paragraphStyle
        textView.typingAttributes = [
            .font: monoFont,
            .paragraphStyle: paragraphStyle,
            .foregroundColor: SemanticColors.text,
        ]

        textView.string = config.initialValue
        textView.delegate = self

        let contentSize = scrollView.contentSize
        textView.minSize = NSSize(width: 0, height: 108)
        textView.maxSize = NSSize(width: CGFloat.greatestFiniteMagnitude, height: CGFloat.greatestFiniteMagnitude)
        textView.textContainer?.containerSize = NSSize(width: contentSize.width, height: CGFloat.greatestFiniteMagnitude)
        textView.textContainer?.widthTracksTextView = true

        scrollView.documentView = textView

        styleMultilineInput(scrollView: scrollView, textView: textView)
        return (scrollView, textView)
    }

    // MARK: - Actions

    @objc private func cancelClicked() {
        dismiss()
        delegate?.modalDidCancel()
    }

    @objc private func confirmClicked() {
        let value: String
        if isMultilineMode, let tv = multilineTextView {
            value = tv.string
        } else if let input = singleLineInput {
            value = input.stringValue
        } else {
            value = ""
        }
        dismiss()
        delegate?.modalDidConfirm(value: value)
    }

    // MARK: - NSTextFieldDelegate (single-line Enter)

    func control(_ control: NSControl, textView: NSTextView, doCommandBy commandSelector: Selector) -> Bool {
        if commandSelector == #selector(insertNewline(_:)) {
            confirmClicked()
            return true
        }
        if commandSelector == #selector(cancelOperation(_:)) {
            cancelClicked()
            return true
        }
        return false
    }

    // MARK: - NSTextDelegate (multiline Cmd+Enter handled via keyDown)

    // MARK: - Key handling

    override func keyDown(with event: NSEvent) {
        // Escape
        if event.keyCode == 53 {
            cancelClicked()
            return
        }
        // Cmd+Enter for multiline
        if isMultilineMode,
           event.modifierFlags.contains(.command),
           event.keyCode == 36 {
            confirmClicked()
            return
        }
        super.keyDown(with: event)
    }

    // MARK: - Mouse handling (click overlay background to cancel)

    override func mouseDown(with event: NSEvent) {
        let locationInSelf = convert(event.locationInWindow, from: nil)
        let locationInCard = cardView.convert(event.locationInWindow, from: nil)
        if !cardView.bounds.contains(locationInCard) && bounds.contains(locationInSelf) {
            cancelClicked()
            return
        }
        super.mouseDown(with: event)
    }

    // MARK: - Layout

    override func resizeSubviews(withOldSize oldSize: NSSize) {
        super.resizeSubviews(withOldSize: oldSize)
        frame = superview?.bounds ?? frame
    }

    override func viewDidMoveToSuperview() {
        super.viewDidMoveToSuperview()
        if let sv = superview {
            frame = sv.bounds
            autoresizingMask = [.width, .height]
        }
    }
}

// MARK: - Padded text field cell

private final class PaddedTextFieldCell: NSTextFieldCell {
    private let padding = NSSize(width: 10, height: 0)

    override func cellSize(forBounds rect: NSRect) -> NSSize {
        var size = super.cellSize(forBounds: rect)
        size.width += padding.width * 2
        return size
    }

    override func titleRect(forBounds rect: NSRect) -> NSRect {
        var insetRect = rect.insetBy(dx: padding.width, dy: 0)
        insetRect.origin.x = rect.origin.x + padding.width
        insetRect.size.width = rect.size.width - padding.width * 2
        return insetRect
    }

    override func edit(withFrame rect: NSRect, in controlView: NSView, editor textObj: NSText, delegate: Any?, event: NSEvent?) {
        super.edit(withFrame: titleRect(forBounds: rect), in: controlView, editor: textObj, delegate: delegate, event: event)
    }

    override func select(withFrame rect: NSRect, in controlView: NSView, editor textObj: NSText, delegate: Any?, start selStart: Int, length selLength: Int) {
        super.select(withFrame: titleRect(forBounds: rect), in: controlView, editor: textObj, delegate: delegate, start: selStart, length: selLength)
    }

    override func drawInterior(withFrame cellFrame: NSRect, in controlView: NSView) {
        super.drawInterior(withFrame: titleRect(forBounds: cellFrame), in: controlView)
    }
}
