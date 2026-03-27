import AppKit

protocol AIPanelDelegate: AnyObject {
    func aiPanelDidRequestClose()
}

final class AIPanelView: NSView, NSTextViewDelegate {

    weak var delegate: AIPanelDelegate?
    private(set) var isOpen: Bool = false

    private enum Tab {
        case todo
        case ideas
    }

    private var currentTab: Tab = .todo

    // MARK: - Data

    private var todoItems: [TodoDisplayItem] = []
    private var ideaItems: [IdeaDisplayItem] = []

    struct TodoDisplayItem {
        let id: Int
        let task: String
        let status: String      // pending_approval, approved, running, completed, failed, skipped
        let issue: String?
        let worktree: String?
        let progress: String?
    }

    struct IdeaDisplayItem {
        let timestamp: String
        let text: String
        let source: String
        let tags: [String]
    }

    // MARK: - Header Subviews

    private let headerLabel: NSTextField = {
        let label = NSTextField(labelWithString: "TODO")
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

    private let leftBorder: NSView = {
        let v = NSView()
        v.wantsLayer = true
        v.translatesAutoresizingMaskIntoConstraints = false
        return v
    }()

    // MARK: - Tab Bar

    private let tabBar: NSView = {
        let v = NSView()
        v.wantsLayer = true
        v.translatesAutoresizingMaskIntoConstraints = false
        return v
    }()

    private let todoTabButton: NSButton = {
        let button = NSButton(title: "TODO", target: nil, action: nil)
        button.identifier = NSUserInterfaceItemIdentifier("panel.ai.tab.todo")
        button.isBordered = false
        button.font = NSFont.systemFont(ofSize: 12, weight: .medium)
        button.translatesAutoresizingMaskIntoConstraints = false
        return button
    }()

    private let ideasTabButton: NSButton = {
        let button = NSButton(title: "Ideas", target: nil, action: nil)
        button.identifier = NSUserInterfaceItemIdentifier("panel.ai.tab.ideas")
        button.isBordered = false
        button.font = NSFont.systemFont(ofSize: 12, weight: .medium)
        button.translatesAutoresizingMaskIntoConstraints = false
        return button
    }()

    private let tabIndicator: NSView = {
        let v = NSView()
        v.wantsLayer = true
        v.translatesAutoresizingMaskIntoConstraints = false
        return v
    }()

    private let tabBarBorder: NSView = {
        let v = NSView()
        v.wantsLayer = true
        v.translatesAutoresizingMaskIntoConstraints = false
        return v
    }()

    private var tabIndicatorLeading: NSLayoutConstraint!
    private var tabIndicatorWidth: NSLayoutConstraint!

    // MARK: - Content Area (shared scroll view, swapped content)

    private let contentScrollView: NSScrollView = {
        let sv = NSScrollView()
        sv.identifier = NSUserInterfaceItemIdentifier("panel.ai.content")
        sv.hasVerticalScroller = true
        sv.hasHorizontalScroller = false
        sv.drawsBackground = false
        sv.automaticallyAdjustsContentInsets = false
        sv.translatesAutoresizingMaskIntoConstraints = false
        return sv
    }()

    private let todoStack: NSStackView = {
        let stack = NSStackView()
        stack.orientation = .vertical
        stack.spacing = 2
        stack.edgeInsets = NSEdgeInsets(top: 8, left: 12, bottom: 8, right: 12)
        stack.translatesAutoresizingMaskIntoConstraints = false
        return stack
    }()

    private let ideasStack: NSStackView = {
        let stack = NSStackView()
        stack.orientation = .vertical
        stack.spacing = 6
        stack.edgeInsets = NSEdgeInsets(top: 8, left: 12, bottom: 8, right: 12)
        stack.translatesAutoresizingMaskIntoConstraints = false
        return stack
    }()

    // MARK: - Input (Ideas tab only)

    private let inputContainer: NSView = {
        let v = NSView()
        v.wantsLayer = true
        v.translatesAutoresizingMaskIntoConstraints = false
        return v
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
        tv.font = NSFont.systemFont(ofSize: 13)
        tv.textColor = SemanticColors.text
        tv.insertionPointColor = SemanticColors.text
        tv.isRichText = false
        tv.isAutomaticQuoteSubstitutionEnabled = false
        tv.isAutomaticDashSubstitutionEnabled = false
        tv.isAutomaticTextReplacementEnabled = false
        tv.textContainerInset = NSSize(width: 4, height: 6)
        tv.isVerticallyResizable = true
        tv.isHorizontallyResizable = false
        tv.autoresizingMask = [.width]
        return tv
    }()

    private let sendButton: NSButton = {
        let button = NSButton(image: NSImage(systemSymbolName: "arrow.up.circle.fill", accessibilityDescription: "Add idea")!, target: nil, action: nil)
        button.identifier = NSUserInterfaceItemIdentifier("panel.ai.send")
        button.isBordered = false
        button.wantsLayer = true
        button.translatesAutoresizingMaskIntoConstraints = false
        button.symbolConfiguration = NSImage.SymbolConfiguration(pointSize: 20, weight: .medium)
        return button
    }()

    private let placeholderLabel: NSTextField = {
        let label = NSTextField(labelWithString: "Write an idea...")
        label.font = NSFont.systemFont(ofSize: 13)
        label.textColor = SemanticColors.muted
        label.translatesAutoresizingMaskIntoConstraints = false
        return label
    }()

    private var inputHeightConstraint: NSLayoutConstraint!
    private var contentBottomToInput: NSLayoutConstraint!
    private var contentBottomToPanel: NSLayoutConstraint!
    private var contentStackWidth: NSLayoutConstraint?

    // MARK: - Empty State

    private let emptyLabel: NSTextField = {
        let label = NSTextField(labelWithString: "")
        label.font = NSFont.systemFont(ofSize: 12)
        label.textColor = SemanticColors.muted
        label.alignment = .center
        label.translatesAutoresizingMaskIntoConstraints = false
        return label
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

    func updateTodoItems(_ items: [TodoDisplayItem]) {
        todoItems = items
        if currentTab == .todo {
            rebuildTodoList()
        }
    }

    func updateIdeaItems(_ items: [IdeaDisplayItem]) {
        ideaItems = items
        if currentTab == .ideas {
            rebuildIdeasList()
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
        todoTabButton.target = self
        todoTabButton.action = #selector(todoTabClicked)
        ideasTabButton.target = self
        ideasTabButton.action = #selector(ideasTabClicked)

        inputTextView.delegate = self

        // Add subviews
        addSubview(leftBorder)
        addSubview(headerLabel)
        addSubview(closeButton)
        addSubview(headerBorder)
        addSubview(tabBar)
        tabBar.addSubview(todoTabButton)
        tabBar.addSubview(ideasTabButton)
        tabBar.addSubview(tabIndicator)
        addSubview(tabBarBorder)
        addSubview(contentScrollView)
        addSubview(emptyLabel)
        addSubview(inputContainer)
        inputContainer.addSubview(inputBorder)
        inputContainer.addSubview(inputScrollView)
        inputContainer.addSubview(placeholderLabel)
        inputContainer.addSubview(sendButton)

        contentScrollView.documentView = todoStack
        inputScrollView.documentView = inputTextView

        inputHeightConstraint = inputScrollView.heightAnchor.constraint(equalToConstant: 36)

        tabIndicatorLeading = tabIndicator.leadingAnchor.constraint(equalTo: todoTabButton.leadingAnchor)
        tabIndicatorWidth = tabIndicator.widthAnchor.constraint(equalTo: todoTabButton.widthAnchor)

        contentBottomToInput = contentScrollView.bottomAnchor.constraint(equalTo: inputContainer.topAnchor)
        contentBottomToPanel = contentScrollView.bottomAnchor.constraint(equalTo: bottomAnchor)

        NSLayoutConstraint.activate([
            // Left border
            leftBorder.leadingAnchor.constraint(equalTo: leadingAnchor),
            leftBorder.topAnchor.constraint(equalTo: topAnchor),
            leftBorder.bottomAnchor.constraint(equalTo: bottomAnchor),
            leftBorder.widthAnchor.constraint(equalToConstant: 1),

            // Header
            headerLabel.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 12),
            headerLabel.centerYAnchor.constraint(equalTo: topAnchor, constant: 20),

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

            // Tab bar
            tabBar.topAnchor.constraint(equalTo: headerBorder.bottomAnchor),
            tabBar.leadingAnchor.constraint(equalTo: leadingAnchor),
            tabBar.trailingAnchor.constraint(equalTo: trailingAnchor),
            tabBar.heightAnchor.constraint(equalToConstant: 36),

            todoTabButton.leadingAnchor.constraint(equalTo: tabBar.leadingAnchor, constant: 12),
            todoTabButton.centerYAnchor.constraint(equalTo: tabBar.centerYAnchor),

            ideasTabButton.leadingAnchor.constraint(equalTo: todoTabButton.trailingAnchor, constant: 16),
            ideasTabButton.centerYAnchor.constraint(equalTo: tabBar.centerYAnchor),

            tabIndicator.bottomAnchor.constraint(equalTo: tabBar.bottomAnchor),
            tabIndicator.heightAnchor.constraint(equalToConstant: 2),
            tabIndicatorLeading,
            tabIndicatorWidth,

            // Tab bar border
            tabBarBorder.leadingAnchor.constraint(equalTo: leadingAnchor),
            tabBarBorder.trailingAnchor.constraint(equalTo: trailingAnchor),
            tabBarBorder.topAnchor.constraint(equalTo: tabBar.bottomAnchor),
            tabBarBorder.heightAnchor.constraint(equalToConstant: 1),

            // Content scroll
            contentScrollView.topAnchor.constraint(equalTo: tabBarBorder.bottomAnchor),
            contentScrollView.leadingAnchor.constraint(equalTo: leadingAnchor),
            contentScrollView.trailingAnchor.constraint(equalTo: trailingAnchor),
            contentBottomToPanel,

            // Content stack width (set dynamically in switchToTab)

            // Empty label
            emptyLabel.centerXAnchor.constraint(equalTo: contentScrollView.centerXAnchor),
            emptyLabel.centerYAnchor.constraint(equalTo: contentScrollView.centerYAnchor),

            // Input container
            inputContainer.leadingAnchor.constraint(equalTo: leadingAnchor),
            inputContainer.trailingAnchor.constraint(equalTo: trailingAnchor),
            inputContainer.bottomAnchor.constraint(equalTo: bottomAnchor),

            inputBorder.leadingAnchor.constraint(equalTo: inputContainer.leadingAnchor),
            inputBorder.trailingAnchor.constraint(equalTo: inputContainer.trailingAnchor),
            inputBorder.topAnchor.constraint(equalTo: inputContainer.topAnchor),
            inputBorder.heightAnchor.constraint(equalToConstant: 1),

            inputScrollView.leadingAnchor.constraint(equalTo: inputContainer.leadingAnchor, constant: 12),
            inputScrollView.trailingAnchor.constraint(equalTo: sendButton.leadingAnchor, constant: -8),
            inputScrollView.topAnchor.constraint(equalTo: inputBorder.bottomAnchor, constant: 8),
            inputScrollView.bottomAnchor.constraint(equalTo: inputContainer.bottomAnchor, constant: -8),
            inputHeightConstraint,

            sendButton.trailingAnchor.constraint(equalTo: inputContainer.trailingAnchor, constant: -12),
            sendButton.bottomAnchor.constraint(equalTo: inputContainer.bottomAnchor, constant: -10),
            sendButton.widthAnchor.constraint(equalToConstant: 24),
            sendButton.heightAnchor.constraint(equalToConstant: 24),

            placeholderLabel.leadingAnchor.constraint(equalTo: inputScrollView.leadingAnchor, constant: 9),
            placeholderLabel.centerYAnchor.constraint(equalTo: inputScrollView.centerYAnchor),
        ])

        applyShadow()
        applyColors()
        switchToTab(.todo)

        // Load sample data for demo
        loadSampleData()
    }

    // MARK: - Tab Switching

    private func switchToTab(_ tab: Tab) {
        currentTab = tab

        // Update tab button styling
        switch tab {
        case .todo:
            headerLabel.stringValue = "TODO"
            todoTabButton.contentTintColor = SemanticColors.accent
            ideasTabButton.contentTintColor = SemanticColors.muted
            tabIndicatorLeading.isActive = false
            tabIndicatorWidth.isActive = false
            tabIndicatorLeading = tabIndicator.leadingAnchor.constraint(equalTo: todoTabButton.leadingAnchor)
            tabIndicatorWidth = tabIndicator.widthAnchor.constraint(equalTo: todoTabButton.widthAnchor)
            tabIndicatorLeading.isActive = true
            tabIndicatorWidth.isActive = true

            contentScrollView.documentView = todoStack
            contentStackWidth?.isActive = false
            contentStackWidth = todoStack.widthAnchor.constraint(equalTo: contentScrollView.widthAnchor)
            contentStackWidth?.isActive = true
            inputContainer.isHidden = true
            contentBottomToInput.isActive = false
            contentBottomToPanel.isActive = true
            rebuildTodoList()

        case .ideas:
            headerLabel.stringValue = "Ideas"
            todoTabButton.contentTintColor = SemanticColors.muted
            ideasTabButton.contentTintColor = SemanticColors.accent
            tabIndicatorLeading.isActive = false
            tabIndicatorWidth.isActive = false
            tabIndicatorLeading = tabIndicator.leadingAnchor.constraint(equalTo: ideasTabButton.leadingAnchor)
            tabIndicatorWidth = tabIndicator.widthAnchor.constraint(equalTo: ideasTabButton.widthAnchor)
            tabIndicatorLeading.isActive = true
            tabIndicatorWidth.isActive = true

            contentScrollView.documentView = ideasStack
            contentStackWidth?.isActive = false
            contentStackWidth = ideasStack.widthAnchor.constraint(equalTo: contentScrollView.widthAnchor)
            contentStackWidth?.isActive = true
            inputContainer.isHidden = false
            contentBottomToPanel.isActive = false
            contentBottomToInput.isActive = true
            rebuildIdeasList()
        }

        needsLayout = true
    }

    // MARK: - Build TODO List

    private func rebuildTodoList() {
        todoStack.arrangedSubviews.forEach { $0.removeFromSuperview() }

        if todoItems.isEmpty {
            emptyLabel.stringValue = "No tasks yet"
            emptyLabel.isHidden = false
            return
        }
        emptyLabel.isHidden = true

        for item in todoItems {
            let row = makeTodoRow(item)
            todoStack.addArrangedSubview(row)
            row.widthAnchor.constraint(equalTo: todoStack.widthAnchor, constant: -24).isActive = true
        }
    }

    private func makeTodoRow(_ item: TodoDisplayItem) -> NSView {
        let container = NSView()
        container.wantsLayer = true
        container.layer?.cornerRadius = 6
        container.translatesAutoresizingMaskIntoConstraints = false

        // Status icon
        let statusIcon = NSTextField(labelWithString: statusEmoji(item.status))
        statusIcon.font = NSFont.systemFont(ofSize: 12)
        statusIcon.translatesAutoresizingMaskIntoConstraints = false

        // Task label
        let taskLabel = NSTextField(wrappingLabelWithString: item.task)
        taskLabel.font = NSFont.systemFont(ofSize: 12)
        taskLabel.textColor = SemanticColors.text
        taskLabel.isEditable = false
        taskLabel.isSelectable = false
        taskLabel.drawsBackground = false
        taskLabel.isBordered = false
        taskLabel.translatesAutoresizingMaskIntoConstraints = false
        taskLabel.maximumNumberOfLines = 2
        taskLabel.preferredMaxLayoutWidth = 260

        // Issue badge
        let issueLabel = NSTextField(labelWithString: item.issue ?? "")
        issueLabel.font = NSFont.monospacedSystemFont(ofSize: 10, weight: .regular)
        issueLabel.textColor = SemanticColors.muted
        issueLabel.translatesAutoresizingMaskIntoConstraints = false
        issueLabel.isHidden = item.issue == nil

        // Status badge
        let statusLabel = NSTextField(labelWithString: item.status)
        statusLabel.font = NSFont.systemFont(ofSize: 10, weight: .medium)
        statusLabel.textColor = statusColor(item.status)
        statusLabel.translatesAutoresizingMaskIntoConstraints = false

        // Progress line
        let progressLabel = NSTextField(wrappingLabelWithString: item.progress ?? "")
        progressLabel.font = NSFont.systemFont(ofSize: 10)
        progressLabel.textColor = SemanticColors.muted
        progressLabel.isEditable = false
        progressLabel.isSelectable = false
        progressLabel.drawsBackground = false
        progressLabel.isBordered = false
        progressLabel.translatesAutoresizingMaskIntoConstraints = false
        progressLabel.maximumNumberOfLines = 1
        progressLabel.isHidden = item.progress == nil

        container.addSubview(statusIcon)
        container.addSubview(taskLabel)
        container.addSubview(issueLabel)
        container.addSubview(statusLabel)
        container.addSubview(progressLabel)

        let bgColor: NSColor = item.status == "running"
            ? SemanticColors.tileBg
            : .clear

        container.layer?.backgroundColor = bgColor.cgColor

        NSLayoutConstraint.activate([
            statusIcon.leadingAnchor.constraint(equalTo: container.leadingAnchor, constant: 8),
            statusIcon.topAnchor.constraint(equalTo: container.topAnchor, constant: 8),
            statusIcon.widthAnchor.constraint(equalToConstant: 16),

            taskLabel.leadingAnchor.constraint(equalTo: statusIcon.trailingAnchor, constant: 6),
            taskLabel.topAnchor.constraint(equalTo: container.topAnchor, constant: 8),
            taskLabel.trailingAnchor.constraint(lessThanOrEqualTo: container.trailingAnchor, constant: -8),

            issueLabel.leadingAnchor.constraint(equalTo: taskLabel.leadingAnchor),
            issueLabel.topAnchor.constraint(equalTo: taskLabel.bottomAnchor, constant: 2),

            statusLabel.leadingAnchor.constraint(equalTo: issueLabel.trailingAnchor, constant: 8),
            statusLabel.centerYAnchor.constraint(equalTo: issueLabel.centerYAnchor),

            progressLabel.leadingAnchor.constraint(equalTo: taskLabel.leadingAnchor),
            progressLabel.topAnchor.constraint(equalTo: issueLabel.bottomAnchor, constant: 2),
            progressLabel.trailingAnchor.constraint(lessThanOrEqualTo: container.trailingAnchor, constant: -8),
            progressLabel.bottomAnchor.constraint(equalTo: container.bottomAnchor, constant: -8),
        ])

        return container
    }

    private func statusEmoji(_ status: String) -> String {
        switch status {
        case "running": return "\u{25B6}"       // play
        case "completed": return "\u{2714}"     // check
        case "failed": return "\u{2718}"        // cross
        case "approved": return "\u{25CB}"      // circle
        case "pending_approval": return "\u{23F3}" // hourglass
        case "rejected", "skipped": return "\u{2013}" // dash
        default: return "\u{25CB}"
        }
    }

    private func statusColor(_ status: String) -> NSColor {
        switch status {
        case "running": return NSColor.systemBlue
        case "completed": return NSColor.systemGreen
        case "failed": return NSColor.systemRed
        case "approved": return SemanticColors.text
        case "pending_approval": return NSColor.systemOrange
        case "rejected", "skipped": return SemanticColors.muted
        default: return SemanticColors.muted
        }
    }

    // MARK: - Build Ideas List

    private func rebuildIdeasList() {
        ideasStack.arrangedSubviews.forEach { $0.removeFromSuperview() }

        if ideaItems.isEmpty {
            emptyLabel.stringValue = "No ideas yet. Type one below!"
            emptyLabel.isHidden = false
            return
        }
        emptyLabel.isHidden = true

        for item in ideaItems {
            let row = makeIdeaRow(item)
            ideasStack.addArrangedSubview(row)
            row.widthAnchor.constraint(equalTo: ideasStack.widthAnchor, constant: -24).isActive = true
        }
    }

    private func makeIdeaRow(_ item: IdeaDisplayItem) -> NSView {
        let container = NSView()
        container.wantsLayer = true
        container.layer?.backgroundColor = resolvedCGColor(SemanticColors.tileBg)
        container.layer?.cornerRadius = 6
        container.translatesAutoresizingMaskIntoConstraints = false

        let textLabel = NSTextField(wrappingLabelWithString: item.text)
        textLabel.font = NSFont.systemFont(ofSize: 12)
        textLabel.textColor = SemanticColors.text
        textLabel.isEditable = false
        textLabel.isSelectable = true
        textLabel.drawsBackground = false
        textLabel.isBordered = false
        textLabel.translatesAutoresizingMaskIntoConstraints = false
        textLabel.maximumNumberOfLines = 3
        textLabel.preferredMaxLayoutWidth = 280

        let metaLabel = NSTextField(labelWithString: "\(item.source) \u{00B7} \(item.timestamp)")
        metaLabel.font = NSFont.systemFont(ofSize: 10)
        metaLabel.textColor = SemanticColors.muted
        metaLabel.translatesAutoresizingMaskIntoConstraints = false

        container.addSubview(textLabel)
        container.addSubview(metaLabel)

        // Tags
        let tagsStr = item.tags.map { "#\($0)" }.joined(separator: " ")
        if !tagsStr.isEmpty {
            let tagsLabel = NSTextField(labelWithString: tagsStr)
            tagsLabel.font = NSFont.systemFont(ofSize: 10)
            tagsLabel.textColor = SemanticColors.accent
            tagsLabel.translatesAutoresizingMaskIntoConstraints = false
            container.addSubview(tagsLabel)

            NSLayoutConstraint.activate([
                tagsLabel.leadingAnchor.constraint(equalTo: container.leadingAnchor, constant: 10),
                tagsLabel.topAnchor.constraint(equalTo: metaLabel.bottomAnchor, constant: 2),
                tagsLabel.bottomAnchor.constraint(equalTo: container.bottomAnchor, constant: -8),
            ])
        } else {
            metaLabel.bottomAnchor.constraint(equalTo: container.bottomAnchor, constant: -8).isActive = true
        }

        NSLayoutConstraint.activate([
            textLabel.topAnchor.constraint(equalTo: container.topAnchor, constant: 8),
            textLabel.leadingAnchor.constraint(equalTo: container.leadingAnchor, constant: 10),
            textLabel.trailingAnchor.constraint(equalTo: container.trailingAnchor, constant: -10),

            metaLabel.leadingAnchor.constraint(equalTo: container.leadingAnchor, constant: 10),
            metaLabel.topAnchor.constraint(equalTo: textLabel.bottomAnchor, constant: 4),
        ])

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

    private func applyShadow() {
        shadow = NSShadow()
        layer?.shadowColor = NSColor.black.withAlphaComponent(0.3).cgColor
        layer?.shadowOffset = CGSize(width: -8, height: 0)
        layer?.shadowRadius = 24
        layer?.shadowOpacity = 1.0
    }

    private func applyColors() {
        layer?.backgroundColor = SemanticColors.panel.cgColor
        leftBorder.layer?.backgroundColor = SemanticColors.line.cgColor
        headerBorder.layer?.backgroundColor = SemanticColors.line.cgColor
        tabBarBorder.layer?.backgroundColor = SemanticColors.line.cgColor
        tabIndicator.layer?.backgroundColor = SemanticColors.accent.cgColor
        inputBorder.layer?.backgroundColor = SemanticColors.line.cgColor
        inputTextView.backgroundColor = .clear

        sendButton.contentTintColor = SemanticColors.muted
        sendButton.layer?.backgroundColor = .clear
        sendButton.layer?.cornerRadius = 12

        inputScrollView.wantsLayer = true
        inputScrollView.layer?.backgroundColor = resolvedCGColor(
            NSColor(name: nil) { a in
                a.isDark
                    ? NSColor(white: 1, alpha: 0.06)
                    : NSColor(white: 0, alpha: 0.04)
            })
        inputScrollView.layer?.borderWidth = 1
        inputScrollView.layer?.borderColor = resolvedCGColor(
            NSColor(name: nil) { a in
                a.isDark
                    ? NSColor(white: 1, alpha: 0.12)
                    : NSColor(white: 0, alpha: 0.12)
            })
        inputScrollView.layer?.cornerRadius = 8

        // Re-apply tab colors
        switchToTab(currentTab)
    }

    // MARK: - Actions

    @objc private func closeClicked() {
        delegate?.aiPanelDidRequestClose()
    }

    @objc private func todoTabClicked() {
        switchToTab(.todo)
    }

    @objc private func ideasTabClicked() {
        switchToTab(.ideas)
    }

    @objc private func sendClicked() {
        sendCurrentInput()
    }

    private func sendCurrentInput() {
        let text = inputTextView.string.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else { return }

        // Add to ideas list
        let formatter = DateFormatter()
        formatter.dateFormat = "HH:mm"
        let timestamp = formatter.string(from: Date())

        let newIdea = IdeaDisplayItem(
            timestamp: timestamp,
            text: text,
            source: "amux",
            tags: []
        )
        ideaItems.insert(newIdea, at: 0)

        inputTextView.string = ""
        placeholderLabel.isHidden = false
        sendButton.contentTintColor = SemanticColors.muted
        updateInputHeight()
        rebuildIdeasList()

        // TODO: persist to ideas.jsonl via AgentHead/MemoryStore
    }

    // MARK: - NSTextViewDelegate

    func textView(_ textView: NSTextView, doCommandBy commandSelector: Selector) -> Bool {
        if commandSelector == #selector(insertNewline(_:)) {
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
        placeholderLabel.isHidden = !inputTextView.string.isEmpty
        sendButton.contentTintColor = inputTextView.string.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            ? SemanticColors.muted
            : SemanticColors.accent
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

    // MARK: - Sample Data

    private func loadSampleData() {
        todoItems = [
            TodoDisplayItem(id: 1, task: "支付模块重构", status: "running", issue: "#42", worktree: "feat-payment", progress: "Claude Code running, 3 files created"),
            TodoDisplayItem(id: 2, task: "修复登录 bug", status: "approved", issue: "#38", worktree: nil, progress: nil),
            TodoDisplayItem(id: 3, task: "升级 Swift 依赖到 5.11", status: "rejected", issue: nil, worktree: nil, progress: nil),
            TodoDisplayItem(id: 4, task: "补充 API 文档", status: "pending_approval", issue: nil, worktree: nil, progress: nil),
            TodoDisplayItem(id: 5, task: "清理废弃代码", status: "completed", issue: "#30", worktree: nil, progress: nil),
        ]

        ideaItems = [
            IdeaDisplayItem(timestamp: "08:30", text: "登录页能不能加个记住密码", source: "wechat", tags: ["ui", "login"]),
            IdeaDisplayItem(timestamp: "12:15", text: "性能好像变差了，首屏加载要3秒", source: "amux", tags: ["perf"]),
            IdeaDisplayItem(timestamp: "22:00", text: "考虑支持 dark mode 的自动切换", source: "mqtt", tags: []),
        ]
    }
}
