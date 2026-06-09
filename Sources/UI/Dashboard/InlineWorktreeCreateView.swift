import AppKit

/// ChatGPT-style sticky worktree creator at the bottom of the sidebar: a tall
/// rounded prompt box with the name field on top and a bottom row showing the
/// target repo (tap to switch or add) plus the reuse-environment toggle.
final class InlineWorktreeCreateView: NSView, NSTextFieldDelegate {
    /// (name, repoPath, agentType, reuseEnvironment)
    var onCreate: ((String, String, AgentType, Bool) -> Void)?
    /// Invoked when the user picks "Add repo…" — should open a picker and add a workspace.
    var onAddRepo: (() -> Void)?
    /// Live source of the current repo paths, read fresh whenever the menu opens
    /// so newly-added repos appear without re-configuring.
    var repoPathsProvider: (() -> [String])?

    private static let agentChoices = AgentType.allCases.filter { $0.isAIAgent }
    private var selectedAgentType: AgentType = .claudeCode

    private let nameField = NSTextField()
    private let repoChip = DropdownChip()
    private let agentChip = DropdownChip()
    private let reuseEnvCheckbox = NSButton(checkboxWithTitle: "Reuse env", target: nil, action: nil)
    private let errorLabel = NSTextField(labelWithString: "")
    private var errorHeight: NSLayoutConstraint!

    private var repoPaths: [String] = []
    private var selectedRepoPath: String?

    // Kept for test-target compatibility (no longer a focus-driven mode).
    var isExpandedForTesting = false

    /// A clearly-elevated fill so the input reads as a distinct box, not the
    /// same surface as the cards above it.
    private static let inputBg = NSColor(name: nil) { a in
        a.isDark ? NSColor(hex: 0x2b2e35) : NSColor(srgbRed: 1, green: 1, blue: 1, alpha: 0.99)
    }
    private static let inputBorder = NSColor(name: nil) { a in
        a.isDark ? NSColor(hex: 0x4a4e57) : NSColor(hex: 0xb9c3d1)
    }

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        setup()
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) not supported") }

    func configure(repoPaths: [String]) {
        self.repoPaths = repoPaths
        if selectedRepoPath == nil || !(repoPaths.contains(selectedRepoPath ?? "")) {
            selectedRepoPath = repoPaths.first
        }
        updateRepoButtonTitle()
    }

    func focusNameField() { window?.makeFirstResponder(nameField) }

    // MARK: Test hooks
    func setNameForTesting(_ s: String) { nameField.stringValue = s }
    func setReuseEnvForTesting(_ on: Bool) { reuseEnvCheckbox.state = on ? .on : .off }
    func setExpandedForTesting(_ on: Bool) { isExpandedForTesting = on }
    func submitForTesting() { submit() }

    private func setup() {
        wantsLayer = true
        layer?.cornerRadius = 12
        layer?.borderWidth = 1.5
        layer?.masksToBounds = false
        layer?.shadowColor = NSColor.black.cgColor
        layer?.shadowOpacity = 0.35
        layer?.shadowRadius = 10
        layer?.shadowOffset = NSSize(width: 0, height: -3)
        applyColors()

        nameField.placeholderString = "New worktree name…"
        nameField.font = NSFont.monospacedSystemFont(ofSize: 13, weight: .regular)
        nameField.isBordered = false
        nameField.drawsBackground = false
        nameField.focusRingType = .none
        nameField.delegate = self
        nameField.target = self
        nameField.action = #selector(submit)
        nameField.translatesAutoresizingMaskIntoConstraints = false
        addSubview(nameField)

        errorLabel.maximumNumberOfLines = 2
        errorLabel.font = NSFont.systemFont(ofSize: 10)
        errorLabel.textColor = .systemRed
        errorLabel.isHidden = true
        errorLabel.lineBreakMode = .byWordWrapping
        errorLabel.translatesAutoresizingMaskIntoConstraints = false
        addSubview(errorLabel)

        // Repo chip: shows the repo name, opens a fresh menu (switch + add).
        repoChip.translatesAutoresizingMaskIntoConstraints = false
        repoChip.onClick = { [weak self] in self?.repoButtonClicked() }
        addSubview(repoChip)

        // Agent chip: pick which AI agent to launch in the new worktree.
        agentChip.translatesAutoresizingMaskIntoConstraints = false
        agentChip.onClick = { [weak self] in self?.agentButtonClicked() }
        agentChip.setTitle(selectedAgentType.shortName)
        addSubview(agentChip)

        reuseEnvCheckbox.font = NSFont.systemFont(ofSize: 11)
        reuseEnvCheckbox.translatesAutoresizingMaskIntoConstraints = false
        reuseEnvCheckbox.setContentHuggingPriority(.required, for: .horizontal)
        addSubview(reuseEnvCheckbox)

        errorHeight = errorLabel.heightAnchor.constraint(equalToConstant: 0)

        NSLayoutConstraint.activate([
            nameField.topAnchor.constraint(equalTo: topAnchor, constant: 12),
            nameField.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 14),
            nameField.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -14),
            nameField.heightAnchor.constraint(greaterThanOrEqualToConstant: 22),

            errorLabel.topAnchor.constraint(equalTo: nameField.bottomAnchor, constant: 6),
            errorLabel.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 14),
            errorLabel.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -14),
            errorHeight,

            repoChip.topAnchor.constraint(equalTo: errorLabel.bottomAnchor, constant: 10),
            repoChip.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 12),
            repoChip.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -12),

            agentChip.centerYAnchor.constraint(equalTo: repoChip.centerYAnchor),
            agentChip.leadingAnchor.constraint(equalTo: repoChip.trailingAnchor, constant: 10),

            reuseEnvCheckbox.centerYAnchor.constraint(equalTo: repoChip.centerYAnchor),
            reuseEnvCheckbox.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -14),
            reuseEnvCheckbox.leadingAnchor.constraint(greaterThanOrEqualTo: agentChip.trailingAnchor, constant: 8),
        ])
    }

    private func agentButtonClicked() {
        let menu = NSMenu()
        for type in Self.agentChoices {
            let item = NSMenuItem(title: type.displayName, action: #selector(selectAgent(_:)), keyEquivalent: "")
            item.target = self
            item.representedObject = type.rawValue
            item.state = (type == selectedAgentType) ? .on : .off
            menu.addItem(item)
        }
        menu.popUp(positioning: nil, at: NSPoint(x: 0, y: agentChip.bounds.height + 4), in: agentChip)
    }

    @objc private func selectAgent(_ sender: NSMenuItem) {
        if let raw = sender.representedObject as? String, let type = AgentType(rawValue: raw) {
            selectedAgentType = type
            agentChip.setTitle(type.shortName)
        }
    }

    private func updateRepoButtonTitle() {
        let name = selectedRepoPath.map { URL(fileURLWithPath: $0).lastPathComponent } ?? "Select repo"
        repoChip.setTitle(name)
    }

    private func repoButtonClicked() {
        let menu = NSMenu()
        let paths = repoPathsProvider?() ?? repoPaths
        repoPaths = paths
        for path in paths {
            let item = NSMenuItem(title: URL(fileURLWithPath: path).lastPathComponent,
                                  action: #selector(selectRepo(_:)), keyEquivalent: "")
            item.target = self
            item.representedObject = path
            item.state = (path == selectedRepoPath) ? .on : .off
            menu.addItem(item)
        }
        if !paths.isEmpty { menu.addItem(.separator()) }
        let add = NSMenuItem(title: "Add repo…", action: #selector(addRepoClicked), keyEquivalent: "")
        add.target = self
        menu.addItem(add)
        menu.popUp(positioning: nil, at: NSPoint(x: 0, y: repoChip.bounds.height + 4), in: repoChip)
    }

    @objc private func selectRepo(_ sender: NSMenuItem) {
        selectedRepoPath = sender.representedObject as? String
        updateRepoButtonTitle()
    }

    @objc private func addRepoClicked() { onAddRepo?() }

    func reportCreateSuccess() {
        nameField.stringValue = ""
        errorLabel.isHidden = true
        errorLabel.stringValue = ""
        errorHeight.constant = 0
    }

    func reportCreateFailure(_ message: String) {
        errorLabel.stringValue = message
        errorLabel.isHidden = false
        errorHeight.constant = errorLabel.intrinsicContentSize.height
    }

    @objc private func submit() {
        let name = nameField.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !name.isEmpty, let repo = selectedRepoPath else { return }
        onCreate?(name, repo, selectedAgentType, reuseEnvCheckbox.state == .on)
    }

    func controlTextDidEndEditing(_ obj: Notification) {}

    // MARK: - Appearance
    private func applyColors() {
        layer?.backgroundColor = resolvedCGColor(Self.inputBg)
        layer?.borderColor = resolvedCGColor(Self.inputBorder)
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        applyColors()
    }
}

/// A bordered, rounded dropdown chip: title + down-chevron, opens a menu on click.
final class DropdownChip: NSView {
    var onClick: (() -> Void)?

    private let titleLabel = NSTextField(labelWithString: "")
    private let chevron = NSImageView()

    private static let chipBg = NSColor(name: nil) { a in
        a.isDark ? NSColor(hex: 0x34373e) : NSColor(hex: 0xeef1f6)
    }
    private static let chipBorder = NSColor(name: nil) { a in
        a.isDark ? NSColor(hex: 0x565a63) : NSColor(hex: 0xc6cfdb)
    }

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        wantsLayer = true
        layer?.cornerRadius = 7
        layer?.borderWidth = 1
        applyColors()

        titleLabel.font = .systemFont(ofSize: 11, weight: .medium)
        titleLabel.textColor = SemanticColors.text
        titleLabel.lineBreakMode = .byTruncatingTail
        titleLabel.translatesAutoresizingMaskIntoConstraints = false
        titleLabel.setContentHuggingPriority(.required, for: .horizontal)
        addSubview(titleLabel)

        chevron.image = NSImage(systemSymbolName: "chevron.down", accessibilityDescription: nil)?
            .withSymbolConfiguration(.init(pointSize: 8, weight: .semibold))
        chevron.contentTintColor = SemanticColors.muted
        chevron.translatesAutoresizingMaskIntoConstraints = false
        chevron.setContentHuggingPriority(.required, for: .horizontal)
        addSubview(chevron)

        NSLayoutConstraint.activate([
            titleLabel.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 10),
            titleLabel.topAnchor.constraint(equalTo: topAnchor, constant: 5),
            titleLabel.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -5),
            chevron.leadingAnchor.constraint(equalTo: titleLabel.trailingAnchor, constant: 6),
            chevron.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -9),
            chevron.centerYAnchor.constraint(equalTo: centerYAnchor),
        ])

        addGestureRecognizer(NSClickGestureRecognizer(target: self, action: #selector(clicked)))
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) not supported") }

    @objc private func clicked() { onClick?() }

    func setTitle(_ s: String) { titleLabel.stringValue = s }

    private func applyColors() {
        layer?.backgroundColor = resolvedCGColor(Self.chipBg)
        layer?.borderColor = resolvedCGColor(Self.chipBorder)
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        applyColors()
    }
}
