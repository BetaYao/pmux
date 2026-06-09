import AppKit

/// Sticky single-line worktree creator at the bottom of the sidebar. Expands to
/// a second row (repo popup + reuse-environment toggle) on focus.
final class InlineWorktreeCreateView: NSView, NSTextFieldDelegate {
    /// (name, repoPath, reuseEnvironment)
    var onCreate: ((String, String, Bool) -> Void)?

    private let nameField = NSTextField()
    private let repoPopup = NSPopUpButton()
    private let reuseEnvCheckbox = NSButton(checkboxWithTitle: "Reuse env", target: nil, action: nil)
    private let errorLabel = NSTextField(labelWithString: "")
    private let secondRow = NSStackView()
    private var repoPaths: [String] = []
    private var secondRowHeight: NSLayoutConstraint!

    var isExpandedForTesting = false

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        setup()
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) not supported") }

    func configure(repoPaths: [String]) {
        self.repoPaths = repoPaths
        repoPopup.removeAllItems()
        repoPopup.addItems(withTitles: repoPaths.map { URL(fileURLWithPath: $0).lastPathComponent })
        if !repoPaths.isEmpty { repoPopup.selectItem(at: 0) }
    }

    func focusNameField() { window?.makeFirstResponder(nameField) }

    // MARK: Test hooks
    func setNameForTesting(_ s: String) { nameField.stringValue = s }
    func setReuseEnvForTesting(_ on: Bool) { reuseEnvCheckbox.state = on ? .on : .off }
    func setExpandedForTesting(_ on: Bool) { setExpanded(on) }
    func submitForTesting() { submit() }

    private func setup() {
        wantsLayer = true

        nameField.placeholderString = "New worktree name…"
        nameField.font = NSFont.monospacedSystemFont(ofSize: 12, weight: .regular)
        nameField.delegate = self
        nameField.target = self
        nameField.action = #selector(submit)
        nameField.translatesAutoresizingMaskIntoConstraints = false
        addSubview(nameField)

        reuseEnvCheckbox.translatesAutoresizingMaskIntoConstraints = false
        repoPopup.translatesAutoresizingMaskIntoConstraints = false
        secondRow.orientation = .horizontal
        secondRow.spacing = 8
        secondRow.translatesAutoresizingMaskIntoConstraints = false
        secondRow.addArrangedSubview(repoPopup)
        secondRow.addArrangedSubview(reuseEnvCheckbox)
        secondRow.alphaValue = 0
        addSubview(secondRow)

        errorLabel.maximumNumberOfLines = 2
        errorLabel.font = NSFont.systemFont(ofSize: 10)
        errorLabel.textColor = .systemRed
        errorLabel.isHidden = true
        errorLabel.lineBreakMode = .byWordWrapping
        errorLabel.translatesAutoresizingMaskIntoConstraints = false
        addSubview(errorLabel)

        secondRowHeight = secondRow.heightAnchor.constraint(equalToConstant: 0)

        NSLayoutConstraint.activate([
            nameField.topAnchor.constraint(equalTo: topAnchor, constant: 6),
            nameField.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 8),
            nameField.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -8),

            secondRow.topAnchor.constraint(equalTo: nameField.bottomAnchor, constant: 6),
            secondRow.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 8),
            secondRow.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -8),
            secondRow.bottomAnchor.constraint(equalTo: errorLabel.topAnchor, constant: -6),
            secondRowHeight,

            errorLabel.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 8),
            errorLabel.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -8),
            errorLabel.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -6),
        ])
    }

    func reportCreateSuccess() {
        nameField.stringValue = ""
        errorLabel.isHidden = true
        errorLabel.stringValue = ""
    }

    func reportCreateFailure(_ message: String) {
        errorLabel.stringValue = message
        errorLabel.isHidden = false
    }

    private func setExpanded(_ expanded: Bool) {
        isExpandedForTesting = expanded
        secondRowHeight.constant = expanded ? 24 : 0
        secondRow.alphaValue = expanded ? 1 : 0
    }

    @objc private func submit() {
        let name = nameField.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !name.isEmpty, !repoPaths.isEmpty else { return }
        let repo = repoPaths[max(0, repoPopup.indexOfSelectedItem)]
        onCreate?(name, repo, reuseEnvCheckbox.state == .on)
    }

    func controlTextDidBeginEditing(_ obj: Notification) { setExpanded(true) }
    func controlTextDidEndEditing(_ obj: Notification) {
        if nameField.stringValue.trimmingCharacters(in: .whitespaces).isEmpty { setExpanded(false) }
    }
}
