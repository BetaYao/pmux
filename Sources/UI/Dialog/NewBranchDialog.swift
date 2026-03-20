import AppKit

protocol NewBranchDialogDelegate: AnyObject {
    func newBranchDialog(_ dialog: NewBranchDialog, didCreateWorktree info: WorktreeInfo, inRepo repoPath: String)
}

/// Modal sheet for creating a new git worktree/branch.
class NewBranchDialog: NSViewController {
    weak var dialogDelegate: NewBranchDialogDelegate?

    private let repoPopup = NSPopUpButton()
    private let branchField = NSTextField()
    private let baseBranchPopup = NSPopUpButton()
    private let createButton = NSButton()
    private let cancelButton = NSButton()
    private let errorLabel = NSTextField(labelWithString: "")

    private var repoPaths: [String] = []

    init(repoPaths: [String]) {
        self.repoPaths = repoPaths
        super.init(nibName: nil, bundle: nil)
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) not supported")
    }

    override func loadView() {
        let container = NSView(frame: NSRect(x: 0, y: 0, width: 420, height: 220))
        container.wantsLayer = true
        container.setAccessibilityIdentifier("dialog.newBranch")
        container.setAccessibilityElement(true)
        container.setAccessibilityRole(.group)
        self.view = container

        let titleLabel = NSTextField(labelWithString: "New Branch")
        titleLabel.font = NSFont.systemFont(ofSize: 16, weight: .semibold)
        titleLabel.textColor = Theme.textPrimary

        // Repo selector
        let repoLabel = NSTextField(labelWithString: "Repository:")
        repoLabel.font = NSFont.systemFont(ofSize: 12, weight: .medium)
        repoLabel.textColor = Theme.textSecondary

        repoPopup.removeAllItems()
        for path in repoPaths {
            repoPopup.addItem(withTitle: URL(fileURLWithPath: path).lastPathComponent)
        }
        repoPopup.target = self
        repoPopup.action = #selector(repoChanged)

        // Branch name
        let branchLabel = NSTextField(labelWithString: "Branch name:")
        branchLabel.font = NSFont.systemFont(ofSize: 12, weight: .medium)
        branchLabel.textColor = Theme.textSecondary

        branchField.placeholderString = "feature/my-feature"
        branchField.font = NSFont.monospacedSystemFont(ofSize: 13, weight: .regular)
        branchField.setAccessibilityIdentifier("dialog.newBranch.nameField")

        // Base branch
        let baseLabel = NSTextField(labelWithString: "Based on:")
        baseLabel.font = NSFont.systemFont(ofSize: 12, weight: .medium)
        baseLabel.textColor = Theme.textSecondary

        baseBranchPopup.removeAllItems()

        // Error label
        errorLabel.textColor = NSColor.systemRed
        errorLabel.font = NSFont.systemFont(ofSize: 11)
        errorLabel.isHidden = true

        // Buttons
        createButton.title = "Create"
        createButton.bezelStyle = .rounded
        createButton.keyEquivalent = "\r"
        createButton.target = self
        createButton.action = #selector(createClicked)
        createButton.setAccessibilityIdentifier("dialog.newBranch.createButton")

        cancelButton.title = "Cancel"
        cancelButton.bezelStyle = .rounded
        cancelButton.keyEquivalent = "\u{1b}"
        cancelButton.target = self
        cancelButton.action = #selector(cancelClicked)

        // Layout with stack views
        let formStack = NSStackView(views: [
            titleLabel,
            makeLabeledRow(repoLabel, repoPopup),
            makeLabeledRow(branchLabel, branchField),
            makeLabeledRow(baseLabel, baseBranchPopup),
            errorLabel,
        ])
        formStack.orientation = .vertical
        formStack.alignment = .leading
        formStack.spacing = 10

        let buttonStack = NSStackView(views: [cancelButton, createButton])
        buttonStack.orientation = .horizontal
        buttonStack.spacing = 8

        let mainStack = NSStackView(views: [formStack, buttonStack])
        mainStack.orientation = .vertical
        mainStack.alignment = .trailing
        mainStack.spacing = 16
        mainStack.translatesAutoresizingMaskIntoConstraints = false
        mainStack.edgeInsets = NSEdgeInsets(top: 20, left: 20, bottom: 20, right: 20)
        container.addSubview(mainStack)

        NSLayoutConstraint.activate([
            mainStack.topAnchor.constraint(equalTo: container.topAnchor),
            mainStack.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            mainStack.trailingAnchor.constraint(equalTo: container.trailingAnchor),
            mainStack.bottomAnchor.constraint(equalTo: container.bottomAnchor),

            repoPopup.widthAnchor.constraint(equalToConstant: 250),
            branchField.widthAnchor.constraint(equalToConstant: 250),
            baseBranchPopup.widthAnchor.constraint(equalToConstant: 250),
        ])

        // Load branches for first repo
        if !repoPaths.isEmpty {
            loadBranches(for: repoPaths[0])
        }
    }

    private func makeLabeledRow(_ label: NSTextField, _ control: NSView) -> NSStackView {
        let row = NSStackView(views: [label, control])
        row.orientation = .horizontal
        row.spacing = 8
        label.setContentHuggingPriority(.defaultHigh, for: .horizontal)
        label.widthAnchor.constraint(equalToConstant: 100).isActive = true
        return row
    }

    @objc private func repoChanged() {
        let index = repoPopup.indexOfSelectedItem
        guard index >= 0, index < repoPaths.count else { return }
        loadBranches(for: repoPaths[index])
    }

    private func loadBranches(for repoPath: String) {
        let branches = WorktreeCreator.listBranches(repoPath: repoPath)
        baseBranchPopup.removeAllItems()
        baseBranchPopup.addItems(withTitles: branches)
        // Select "main" if available
        if let mainIndex = branches.firstIndex(of: "main") {
            baseBranchPopup.selectItem(at: mainIndex)
        }
    }

    @objc private func createClicked() {
        let branchName = branchField.stringValue.trimmingCharacters(in: .whitespaces)
        guard !branchName.isEmpty else {
            showError("Branch name cannot be empty")
            return
        }
        // Validate branch name (no spaces, basic check)
        if branchName.contains(" ") {
            showError("Branch name cannot contain spaces")
            return
        }

        let repoIndex = repoPopup.indexOfSelectedItem
        guard repoIndex >= 0, repoIndex < repoPaths.count else { return }
        let repoPath = repoPaths[repoIndex]
        let baseBranch = baseBranchPopup.titleOfSelectedItem ?? "main"

        createButton.isEnabled = false

        DispatchQueue.global().async { [weak self] in
            do {
                let info = try WorktreeCreator.createWorktree(
                    repoPath: repoPath,
                    branchName: branchName,
                    baseBranch: baseBranch
                )
                DispatchQueue.main.async {
                    self?.dismiss(nil)
                    self?.dialogDelegate?.newBranchDialog(self!, didCreateWorktree: info, inRepo: repoPath)
                }
            } catch {
                DispatchQueue.main.async {
                    self?.showError(error.localizedDescription)
                    self?.createButton.isEnabled = true
                }
            }
        }
    }

    @objc private func cancelClicked() {
        dismiss(nil)
    }

    private func showError(_ message: String) {
        errorLabel.stringValue = message
        errorLabel.isHidden = false
    }
}
