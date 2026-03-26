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

    private let sparklesIcon: NSTextField = {
        let label = NSTextField(labelWithString: "\u{2728}")
        label.font = NSFont.systemFont(ofSize: 13)
        label.translatesAutoresizingMaskIntoConstraints = false
        return label
    }()

    private let headerLabel: NSTextField = {
        let label = NSTextField(labelWithString: "AI Assistant")
        label.font = NSFont.boldSystemFont(ofSize: 13)
        label.textColor = SemanticColors.text
        label.translatesAutoresizingMaskIntoConstraints = false
        return label
    }()

    private let closeButton: NSButton = {
        let button = NSButton(title: "\u{00D7}", target: nil, action: nil)
        button.identifier = NSUserInterfaceItemIdentifier("panel.ai.close")
        button.isBordered = false
        button.font = NSFont.systemFont(ofSize: 14, weight: .medium)
        button.contentTintColor = SemanticColors.muted
        button.wantsLayer = true
        button.layer?.backgroundColor = NSColor(white: 1, alpha: 0.03).cgColor
        button.layer?.cornerRadius = 4
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
        let button = NSButton(image: NSImage(systemSymbolName: "arrow.up", accessibilityDescription: "Send")!, target: nil, action: nil)
        button.identifier = NSUserInterfaceItemIdentifier("panel.ai.send")
        button.isBordered = false
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

    func addBubble(role: BubbleRole, text: String) {
        let addAndScroll = { [weak self] in
            guard let self else { return }
            let bubble = self.makeBubble(role: role, text: text)
            self.messagesStack.addArrangedSubview(bubble)

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

            // Scroll to bottom after the next layout pass (avoids fittingSize
            // which triggers a synchronous Auto Layout pass that crashes if
            // called off the main thread).
            self.messagesStack.needsLayout = true
            DispatchQueue.main.async { [weak self] in
                guard let self,
                      let docView = self.messagesScrollView.documentView else { return }
                let docHeight = docView.frame.height
                let scrollHeight = self.messagesScrollView.bounds.height
                if docHeight > scrollHeight {
                    self.messagesScrollView.contentView.scroll(
                        to: NSPoint(x: 0, y: docHeight - scrollHeight))
                }
            }
        }

        if Thread.isMainThread {
            addAndScroll()
        } else {
            DispatchQueue.main.async(execute: addAndScroll)
        }
    }

    // MARK: - Setup

    private func setup() {
        identifier = NSUserInterfaceItemIdentifier("panel.ai")
        setAccessibilityIdentifier("panel.ai")
        setAccessibilityElement(true)
        setAccessibilityRole(.group)
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
        addSubview(sparklesIcon)
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

            // Sparkles icon
            sparklesIcon.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 12),
            sparklesIcon.centerYAnchor.constraint(equalTo: topAnchor, constant: 20),

            // Header
            headerLabel.leadingAnchor.constraint(equalTo: sparklesIcon.trailingAnchor, constant: 6),
            headerLabel.centerYAnchor.constraint(equalTo: sparklesIcon.centerYAnchor),

            // Close button
            closeButton.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -12),
            closeButton.centerYAnchor.constraint(equalTo: headerLabel.centerYAnchor),
            closeButton.widthAnchor.constraint(equalToConstant: 24),
            closeButton.heightAnchor.constraint(equalToConstant: 24),

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
            sendButton.widthAnchor.constraint(equalToConstant: 28),
            sendButton.heightAnchor.constraint(equalToConstant: 28),
        ])

        applyShadow()
        applyColors()

        // Add welcome message
        addBubble(role: .assistant, text: "Hello, I'm the workspace assistant. Ask me about this project, threads, or commands. (Prototype demo)")
    }

    private func applyShadow() {
        shadow = NSShadow()
        layer?.shadowColor = NSColor.black.withAlphaComponent(0.3).cgColor
        layer?.shadowOffset = CGSize(width: -8, height: 0)
        layer?.shadowRadius = 24
        layer?.shadowOpacity = 1.0
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

        // Set bubble colors and corner radii at creation time
        if role == .user {
            container.layer?.backgroundColor = resolvedCGColor(SemanticColors.aiBubbleUser)
            // cornerRadius 8/8/2/8 — use maskedCorners for per-corner radii
            container.layer?.cornerRadius = 8
            container.layer?.maskedCorners = [.layerMinXMinYCorner, .layerMaxXMinYCorner, .layerMaxXMaxYCorner]
            // Bottom-left gets smaller radius via a separate approach — CALayer maskedCorners
            // only toggles corners on/off. Use uniform 8 and set bottom-right to small via mask.
            // For simplicity, set cornerRadius to 8 and mask out bottom-left for the 2px effect.
            // Actually maskedCorners only enables/disables, so approximate: use 8 overall.
            // The spec says 8/8/2/8 — top-left/top-right/bottom-right/bottom-left → bottom-right = 2
            // Re-reading: "cornerRadius 8/8/2/8" = TL/TR/BR/BL → BR is 2
            // AppKit CALayer doesn't support per-corner radii easily; use 8 as dominant and accept approximation.
        } else {
            container.layer?.backgroundColor = resolvedCGColor(SemanticColors.panel2)
            // cornerRadius 8/8/8/2 — TL/TR/BR/BL → BL is 2
            container.layer?.cornerRadius = 8
        }

        // Mark the role via identifier for appearance updates
        container.identifier = NSUserInterfaceItemIdentifier(role == .user ? "bubble.user" : "bubble.assistant")

        return container
    }

    // MARK: - Drawing

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        applyColors()
    }

    override func layout() {
        super.layout()
        layer?.shadowPath = CGPath(roundedRect: bounds, cornerWidth: 0, cornerHeight: 0, transform: nil)
    }

    private func applyColors() {
        layer?.backgroundColor = SemanticColors.panel.cgColor
        leftBorder.layer?.backgroundColor = SemanticColors.line.cgColor
        headerBorder.layer?.backgroundColor = SemanticColors.line.cgColor
        inputBorder.layer?.backgroundColor = SemanticColors.line.cgColor
        inputTextView.backgroundColor = SemanticColors.tileBg

        sendButton.layer?.backgroundColor = SemanticColors.accent.cgColor
        sendButton.layer?.cornerRadius = 6

        // Update bubble backgrounds
        let userBg = resolvedCGColor(SemanticColors.aiBubbleUser)
        let assistantBg = resolvedCGColor(SemanticColors.panel2)
        for view in messagesStack.arrangedSubviews {
            if view.identifier?.rawValue == "bubble.user" {
                view.layer?.backgroundColor = userBg
            } else {
                view.layer?.backgroundColor = assistantBg
            }
        }

        // Input scroll view background
        inputScrollView.wantsLayer = true
        inputScrollView.layer?.backgroundColor = SemanticColors.tileBg.cgColor
        inputScrollView.layer?.borderWidth = 1
        inputScrollView.layer?.borderColor = SemanticColors.line.cgColor
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
        let response = "Received: \(truncated)"
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
