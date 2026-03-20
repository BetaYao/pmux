import AppKit

protocol AIPanelDelegate: AnyObject {
    func aiPanelDidRequestClose()
}

final class AIPanelView: NSView, NSTextViewDelegate {

    weak var delegate: AIPanelDelegate?
    private(set) var isOpen: Bool = false

    enum BubbleRole {
        case user
        case assistant
    }

    // MARK: - Subviews

    private let headerLabel: NSTextField = {
        let label = NSTextField(labelWithString: "AI 助手")
        label.font = NSFont.boldSystemFont(ofSize: 13)
        label.textColor = SemanticColors.text
        label.translatesAutoresizingMaskIntoConstraints = false
        return label
    }()

    private let closeButton: NSButton = {
        let button = NSButton(title: "×", target: nil, action: nil)
        button.identifier = NSUserInterfaceItemIdentifier("panel.ai.close")
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

    private let messagesScrollView: NSScrollView = {
        let sv = NSScrollView()
        sv.identifier = NSUserInterfaceItemIdentifier("panel.ai.messages")
        sv.hasVerticalScroller = true
        sv.hasHorizontalScroller = false
        sv.drawsBackground = false
        sv.automaticallyAdjustsContentInsets = false
        sv.translatesAutoresizingMaskIntoConstraints = false
        return sv
    }()

    private let messagesStack: NSStackView = {
        let stack = NSStackView()
        stack.orientation = .vertical
        stack.spacing = 8
        stack.edgeInsets = NSEdgeInsets(top: 12, left: 12, bottom: 12, right: 12)
        stack.translatesAutoresizingMaskIntoConstraints = false
        return stack
    }()

    private let inputBorder: NSView = {
        let v = NSView()
        v.wantsLayer = true
        v.translatesAutoresizingMaskIntoConstraints = false
        return v
    }()

    private let inputScrollView: NSScrollView = {
        let sv = NSScrollView()
        sv.hasVerticalScroller = false
        sv.hasHorizontalScroller = false
        sv.drawsBackground = false
        sv.translatesAutoresizingMaskIntoConstraints = false
        return sv
    }()

    private let inputTextView: NSTextView = {
        let tv = NSTextView()
        tv.identifier = NSUserInterfaceItemIdentifier("panel.ai.input")
        tv.font = NSFont.systemFont(ofSize: 12)
        tv.textColor = SemanticColors.text
        tv.isRichText = false
        tv.isAutomaticQuoteSubstitutionEnabled = false
        tv.isAutomaticDashSubstitutionEnabled = false
        tv.isAutomaticTextReplacementEnabled = false
        tv.textContainerInset = NSSize(width: 6, height: 6)
        tv.isVerticallyResizable = true
        tv.isHorizontallyResizable = false
        tv.autoresizingMask = [.width]
        return tv
    }()

    private let sendButton: NSButton = {
        let button = NSButton(title: "发送", target: nil, action: nil)
        button.identifier = NSUserInterfaceItemIdentifier("panel.ai.send")
        button.isBordered = false
        button.font = NSFont.systemFont(ofSize: 11, weight: .semibold)
        button.contentTintColor = SemanticColors.text
        button.wantsLayer = true
        button.translatesAutoresizingMaskIntoConstraints = false
        return button
    }()

    private let leftBorder: NSView = {
        let v = NSView()
        v.wantsLayer = true
        v.translatesAutoresizingMaskIntoConstraints = false
        return v
    }()

    private var inputHeightConstraint: NSLayoutConstraint!

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

    func setOpen(_ open: Bool) {
        guard open != isOpen else { return }
        isOpen = open
        isHidden = false

        NSAnimationContext.runAnimationGroup({ context in
            context.duration = 0.22
            context.timingFunction = CAMediaTimingFunction(name: .easeInEaseOut)
            self.animator().alphaValue = open ? 1.0 : 0.0
        }, completionHandler: {
            if !open { self.isHidden = true }
        })
    }

    func addBubble(role: BubbleRole, text: String) {
        let bubble = makeBubble(role: role, text: text)
        messagesStack.addArrangedSubview(bubble)

        // Alignment
        switch role {
        case .user:
            bubble.setContentHuggingPriority(.defaultLow, for: .horizontal)
            if let container = bubble.superview {
                NSLayoutConstraint.activate([
                    bubble.trailingAnchor.constraint(equalTo: container.trailingAnchor, constant: -12),
                    bubble.widthAnchor.constraint(lessThanOrEqualTo: container.widthAnchor, multiplier: 0.92),
                ])
            }
        case .assistant:
            bubble.setContentHuggingPriority(.defaultLow, for: .horizontal)
            if let container = bubble.superview {
                NSLayoutConstraint.activate([
                    bubble.leadingAnchor.constraint(equalTo: container.leadingAnchor, constant: 12),
                    bubble.widthAnchor.constraint(lessThanOrEqualTo: container.widthAnchor, multiplier: 0.92),
                ])
            }
        }

        // Scroll to bottom
        DispatchQueue.main.async {
            let clipView = self.messagesScrollView.contentView
            let docHeight = self.messagesStack.fittingSize.height
            let scrollHeight = self.messagesScrollView.bounds.height
            if docHeight > scrollHeight {
                clipView.scroll(to: NSPoint(x: 0, y: docHeight - scrollHeight))
            }
        }
    }

    // MARK: - Setup

    private func setup() {
        identifier = NSUserInterfaceItemIdentifier("panel.ai")
        wantsLayer = true
        translatesAutoresizingMaskIntoConstraints = false
        isHidden = true
        alphaValue = 0

        closeButton.target = self
        closeButton.action = #selector(closeClicked)
        sendButton.target = self
        sendButton.action = #selector(sendClicked)

        inputTextView.delegate = self

        addSubview(leftBorder)
        addSubview(headerLabel)
        addSubview(closeButton)
        addSubview(headerBorder)
        addSubview(messagesScrollView)
        addSubview(inputBorder)
        addSubview(inputScrollView)
        addSubview(sendButton)

        messagesScrollView.documentView = messagesStack

        inputScrollView.documentView = inputTextView

        inputHeightConstraint = inputScrollView.heightAnchor.constraint(equalToConstant: 36)

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

            // Messages scroll
            messagesScrollView.topAnchor.constraint(equalTo: headerBorder.bottomAnchor),
            messagesScrollView.leadingAnchor.constraint(equalTo: leadingAnchor),
            messagesScrollView.trailingAnchor.constraint(equalTo: trailingAnchor),
            messagesScrollView.bottomAnchor.constraint(equalTo: inputBorder.topAnchor),

            // Messages stack width
            messagesStack.widthAnchor.constraint(equalTo: messagesScrollView.widthAnchor),

            // Input border
            inputBorder.leadingAnchor.constraint(equalTo: leadingAnchor),
            inputBorder.trailingAnchor.constraint(equalTo: trailingAnchor),
            inputBorder.bottomAnchor.constraint(equalTo: inputScrollView.topAnchor, constant: -8),
            inputBorder.heightAnchor.constraint(equalToConstant: 1),

            // Input text view
            inputScrollView.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 12),
            inputScrollView.trailingAnchor.constraint(equalTo: sendButton.leadingAnchor, constant: -8),
            inputScrollView.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -8),
            inputHeightConstraint,

            // Send button
            sendButton.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -12),
            sendButton.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -12),
            sendButton.widthAnchor.constraint(greaterThanOrEqualToConstant: 40),
        ])

        applyShadow()

        // Add welcome message
        addBubble(role: .assistant, text: "你好，我是工作区助手。可以问我关于当前 project、thread 或命令的问题。（原型演示）")
    }

    private func applyShadow() {
        shadow = NSShadow()
        layer?.shadowColor = NSColor.black.withAlphaComponent(0.12).cgColor
        layer?.shadowOffset = CGSize(width: -8, height: 0)
        layer?.shadowRadius = 16
        layer?.shadowOpacity = 1.0
        layer?.shadowPath = CGPath(rect: bounds, transform: nil)
    }

    override func layout() {
        super.layout()
        layer?.shadowPath = CGPath(rect: bounds, transform: nil)
    }

    private func makeBubble(role: BubbleRole, text: String) -> NSView {
        let container = NSView()
        container.wantsLayer = true
        container.translatesAutoresizingMaskIntoConstraints = false

        let label = NSTextField(wrappingLabelWithString: text)
        label.font = NSFont.systemFont(ofSize: 12)
        label.textColor = SemanticColors.text
        label.isEditable = false
        label.isSelectable = true
        label.drawsBackground = false
        label.isBordered = false
        label.translatesAutoresizingMaskIntoConstraints = false
        label.maximumNumberOfLines = 0
        label.preferredMaxLayoutWidth = 280

        container.addSubview(label)

        NSLayoutConstraint.activate([
            label.topAnchor.constraint(equalTo: container.topAnchor, constant: 8),
            label.bottomAnchor.constraint(equalTo: container.bottomAnchor, constant: -8),
            label.leadingAnchor.constraint(equalTo: container.leadingAnchor, constant: 10),
            label.trailingAnchor.constraint(equalTo: container.trailingAnchor, constant: -10),
        ])

        container.layer?.cornerRadius = 10

        // Mark the role via identifier for updateLayer
        container.identifier = NSUserInterfaceItemIdentifier(role == .user ? "bubble.user" : "bubble.assistant")

        return container
    }

    // MARK: - Drawing

    override var wantsUpdateLayer: Bool { true }

    override func updateLayer() {
        layer?.backgroundColor = SemanticColors.panel.cgColor
        leftBorder.layer?.backgroundColor = SemanticColors.line.withAlphaComponent(0.45).cgColor
        headerBorder.layer?.backgroundColor = SemanticColors.line.cgColor
        inputBorder.layer?.backgroundColor = SemanticColors.line.cgColor
        inputTextView.backgroundColor = SemanticColors.panel2

        // Send button tinted background: 22% accent + 78% panel2
        let accent = SemanticColors.accent
        let panel2 = SemanticColors.panel2
        sendButton.layer?.backgroundColor = accent.blended(withFraction: 0.78, of: panel2)?.cgColor

        sendButton.layer?.cornerRadius = 4

        // Update bubble backgrounds
        for view in messagesStack.arrangedSubviews {
            if view.identifier?.rawValue == "bubble.user" {
                // User bubble: 18% accent + 82% panel2
                view.layer?.backgroundColor = accent.blended(withFraction: 0.82, of: panel2)?.cgColor
            } else {
                // Assistant bubble: panel2
                view.layer?.backgroundColor = panel2.cgColor
            }
        }

        // Input scroll view background
        inputScrollView.wantsLayer = true
        inputScrollView.layer?.backgroundColor = SemanticColors.panel2.cgColor
        inputScrollView.layer?.cornerRadius = 6
    }

    // MARK: - Actions

    @objc private func closeClicked() {
        delegate?.aiPanelDidRequestClose()
    }

    @objc private func sendClicked() {
        sendCurrentInput()
    }

    private func sendCurrentInput() {
        let text = inputTextView.string.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else { return }

        addBubble(role: .user, text: text)
        inputTextView.string = ""
        updateInputHeight()

        // Placeholder response after ~450ms
        let truncated = String(text.prefix(80))
        let response = "收到：\(truncated)"
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.45) { [weak self] in
            self?.addBubble(role: .assistant, text: response)
        }
    }

    // MARK: - NSTextViewDelegate

    func textView(_ textView: NSTextView, doCommandBy commandSelector: Selector) -> Bool {
        if commandSelector == #selector(insertNewline(_:)) {
            // Enter without Shift → send
            let flags = NSApp.currentEvent?.modifierFlags ?? []
            if !flags.contains(.shift) {
                sendCurrentInput()
                return true
            }
        }
        return false
    }

    func textDidChange(_ notification: Notification) {
        updateInputHeight()
    }

    private func updateInputHeight() {
        guard let layoutManager = inputTextView.layoutManager,
              let textContainer = inputTextView.textContainer else { return }
        layoutManager.ensureLayout(for: textContainer)
        let usedRect = layoutManager.usedRect(for: textContainer)
        let inset = inputTextView.textContainerInset
        let height = usedRect.height + inset.height * 2
        let clamped = min(max(height, 36), 96)
        inputHeightConstraint.constant = clamped
    }
}
