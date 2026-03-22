import AppKit

protocol SettingsDelegate: AnyObject {
    func settingsDidUpdateConfig(_ settings: SettingsViewController, config: Config)
}

/// Settings window with tabs: General, Agent Detection.
class SettingsViewController: NSViewController {
    weak var settingsDelegate: SettingsDelegate?

    private var config: Config
    private let tabView = NSTabView()

    // General tab controls
    private let pathListView = NSTableView()
    private let pathScrollView = NSScrollView()
    private var workspacePaths: [String] = []
    private let addButton = NSButton()
    private let removeButton = NSButton()
    private let backendPopup = NSPopUpButton()
    private let cacheSizeField = NSTextField()

    // Agent Detection tab controls
    private let agentTableView = NSTableView()
    private let agentScrollView = NSScrollView()
    private let ruleTextView = NSTextView()
    private let ruleScrollView = NSScrollView()

    init(config: Config) {
        self.config = config
        self.workspacePaths = config.workspacePaths
        super.init(nibName: nil, bundle: nil)
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) not supported")
    }

    override func loadView() {
        let container = NSView(frame: NSRect(x: 0, y: 0, width: 560, height: 420))
        container.wantsLayer = true
        container.setAccessibilityIdentifier("settings.sheet")
        container.setAccessibilityElement(true)
        container.setAccessibilityRole(.group)
        self.view = container

        tabView.translatesAutoresizingMaskIntoConstraints = false
        container.addSubview(tabView)

        // Tab 1: General
        let generalTab = NSTabViewItem(identifier: "general")
        generalTab.label = "General"
        generalTab.view = buildGeneralTab()
        tabView.addTabViewItem(generalTab)

        // Tab 2: Agent Detection
        let agentTab = NSTabViewItem(identifier: "agents")
        agentTab.label = "Agent Detection"
        agentTab.view = buildAgentTab()
        tabView.addTabViewItem(agentTab)

        // Buttons
        let saveButton = NSButton(title: "Save", target: self, action: #selector(saveClicked))
        saveButton.bezelStyle = .rounded
        saveButton.keyEquivalent = "\r"
        saveButton.translatesAutoresizingMaskIntoConstraints = false
        container.addSubview(saveButton)

        let cancelButton = NSButton(title: "Cancel", target: self, action: #selector(cancelClicked))
        cancelButton.bezelStyle = .rounded
        cancelButton.keyEquivalent = "\u{1b}"
        cancelButton.translatesAutoresizingMaskIntoConstraints = false
        container.addSubview(cancelButton)

        NSLayoutConstraint.activate([
            tabView.topAnchor.constraint(equalTo: container.topAnchor, constant: 8),
            tabView.leadingAnchor.constraint(equalTo: container.leadingAnchor, constant: 12),
            tabView.trailingAnchor.constraint(equalTo: container.trailingAnchor, constant: -12),
            tabView.bottomAnchor.constraint(equalTo: saveButton.topAnchor, constant: -12),

            saveButton.trailingAnchor.constraint(equalTo: container.trailingAnchor, constant: -16),
            saveButton.bottomAnchor.constraint(equalTo: container.bottomAnchor, constant: -12),

            cancelButton.trailingAnchor.constraint(equalTo: saveButton.leadingAnchor, constant: -8),
            cancelButton.bottomAnchor.constraint(equalTo: container.bottomAnchor, constant: -12),
        ])
    }

    // MARK: - General Tab

    private func buildGeneralTab() -> NSView {
        let view = NSView()

        // Workspace paths section
        let pathsLabel = NSTextField(labelWithString: "Workspace Paths:")
        pathsLabel.font = NSFont.systemFont(ofSize: 13, weight: .semibold)
        pathsLabel.textColor = Theme.textPrimary
        pathsLabel.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(pathsLabel)

        // Path list
        pathScrollView.hasVerticalScroller = true
        pathScrollView.borderType = .bezelBorder
        pathScrollView.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(pathScrollView)

        pathListView.headerView = nil
        pathListView.rowHeight = 22
        pathListView.delegate = self
        pathListView.dataSource = self
        let col = NSTableColumn(identifier: NSUserInterfaceItemIdentifier("path"))
        col.resizingMask = .autoresizingMask
        pathListView.addTableColumn(col)
        pathListView.setAccessibilityIdentifier("settings.workspacePaths")
        pathScrollView.documentView = pathListView

        // Add/Remove buttons
        addButton.title = "+"
        addButton.bezelStyle = .rounded
        addButton.target = self
        addButton.action = #selector(addPathClicked)
        addButton.setAccessibilityIdentifier("settings.addPath")
        addButton.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(addButton)

        removeButton.title = "−"
        removeButton.bezelStyle = .rounded
        removeButton.target = self
        removeButton.action = #selector(removePathClicked)
        removeButton.setAccessibilityIdentifier("settings.removePath")
        removeButton.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(removeButton)

        // Backend
        let backendLabel = NSTextField(labelWithString: "Backend:")
        backendLabel.font = NSFont.systemFont(ofSize: 12, weight: .medium)
        backendLabel.textColor = Theme.textSecondary
        backendLabel.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(backendLabel)

        backendPopup.removeAllItems()
        backendPopup.addItems(withTitles: ["zmx", "local"])
        backendPopup.selectItem(withTitle: config.backend)
        if backendPopup.indexOfSelectedItem < 0 {
            backendPopup.selectItem(withTitle: "zmx")
        }
        backendPopup.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(backendPopup)

        // Cache size
        let cacheLabel = NSTextField(labelWithString: "Terminal cache rows:")
        cacheLabel.font = NSFont.systemFont(ofSize: 12, weight: .medium)
        cacheLabel.textColor = Theme.textSecondary
        cacheLabel.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(cacheLabel)

        cacheSizeField.stringValue = "\(config.terminalRowCacheSize)"
        cacheSizeField.font = NSFont.monospacedSystemFont(ofSize: 12, weight: .regular)
        cacheSizeField.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(cacheSizeField)

        NSLayoutConstraint.activate([
            pathsLabel.topAnchor.constraint(equalTo: view.topAnchor, constant: 12),
            pathsLabel.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 12),

            pathScrollView.topAnchor.constraint(equalTo: pathsLabel.bottomAnchor, constant: 6),
            pathScrollView.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 12),
            pathScrollView.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -12),
            pathScrollView.heightAnchor.constraint(equalToConstant: 150),

            addButton.topAnchor.constraint(equalTo: pathScrollView.bottomAnchor, constant: 4),
            addButton.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 12),
            addButton.widthAnchor.constraint(equalToConstant: 32),

            removeButton.topAnchor.constraint(equalTo: pathScrollView.bottomAnchor, constant: 4),
            removeButton.leadingAnchor.constraint(equalTo: addButton.trailingAnchor, constant: 4),
            removeButton.widthAnchor.constraint(equalToConstant: 32),

            backendLabel.topAnchor.constraint(equalTo: addButton.bottomAnchor, constant: 16),
            backendLabel.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 12),
            backendLabel.widthAnchor.constraint(equalToConstant: 140),

            backendPopup.centerYAnchor.constraint(equalTo: backendLabel.centerYAnchor),
            backendPopup.leadingAnchor.constraint(equalTo: backendLabel.trailingAnchor, constant: 8),
            backendPopup.widthAnchor.constraint(equalToConstant: 120),

            cacheLabel.topAnchor.constraint(equalTo: backendLabel.bottomAnchor, constant: 12),
            cacheLabel.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 12),
            cacheLabel.widthAnchor.constraint(equalToConstant: 140),

            cacheSizeField.centerYAnchor.constraint(equalTo: cacheLabel.centerYAnchor),
            cacheSizeField.leadingAnchor.constraint(equalTo: cacheLabel.trailingAnchor, constant: 8),
            cacheSizeField.widthAnchor.constraint(equalToConstant: 80),
        ])

        return view
    }

    // MARK: - Agent Detection Tab

    private func buildAgentTab() -> NSView {
        let view = NSView()

        let infoLabel = NSTextField(labelWithString: "Agent detection rules (JSON). Edit and save to apply.")
        infoLabel.font = NSFont.systemFont(ofSize: 11)
        infoLabel.textColor = Theme.textSecondary
        infoLabel.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(infoLabel)

        ruleScrollView.hasVerticalScroller = true
        ruleScrollView.borderType = .bezelBorder
        ruleScrollView.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(ruleScrollView)

        ruleTextView.isEditable = true
        ruleTextView.isSelectable = true
        ruleTextView.font = NSFont.monospacedSystemFont(ofSize: 11, weight: .regular)
        ruleTextView.textContainerInset = NSSize(width: 6, height: 6)
        ruleTextView.isAutomaticQuoteSubstitutionEnabled = false
        ruleTextView.isAutomaticDashSubstitutionEnabled = false
        ruleTextView.isAutomaticTextReplacementEnabled = false
        ruleScrollView.documentView = ruleTextView

        // Populate with current agent config as pretty JSON
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        if let data = try? encoder.encode(config.agentDetect),
           let json = String(data: data, encoding: .utf8) {
            ruleTextView.string = json
        }

        NSLayoutConstraint.activate([
            infoLabel.topAnchor.constraint(equalTo: view.topAnchor, constant: 12),
            infoLabel.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 12),

            ruleScrollView.topAnchor.constraint(equalTo: infoLabel.bottomAnchor, constant: 6),
            ruleScrollView.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 12),
            ruleScrollView.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -12),
            ruleScrollView.bottomAnchor.constraint(equalTo: view.bottomAnchor, constant: -12),
        ])

        return view
    }

    // MARK: - Actions

    @objc private func addPathClicked() {
        let panel = NSOpenPanel()
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.allowsMultipleSelection = true
        panel.message = "Select workspace directories"

        panel.beginSheetModal(for: view.window!) { [weak self] response in
            guard response == .OK, let self else { return }
            for url in panel.urls {
                let path = url.path
                if !self.workspacePaths.contains(path) {
                    self.workspacePaths.append(path)
                }
            }
            self.pathListView.reloadData()
        }
    }

    @objc private func removePathClicked() {
        let row = pathListView.selectedRow
        guard row >= 0, row < workspacePaths.count else { return }
        workspacePaths.remove(at: row)
        pathListView.reloadData()
    }

    @objc private func saveClicked() {
        // Update config from UI
        config.workspacePaths = workspacePaths
        config.backend = backendPopup.titleOfSelectedItem ?? "zmx"
        config.terminalRowCacheSize = Int(cacheSizeField.stringValue) ?? 200

        // Parse agent detection JSON
        let jsonString = ruleTextView.string
        if let data = jsonString.data(using: .utf8),
           let agentConfig = try? JSONDecoder().decode(AgentDetectConfig.self, from: data) {
            config.agentDetect = agentConfig
        }

        config.save()
        settingsDelegate?.settingsDidUpdateConfig(self, config: config)
        dismiss(nil)
    }

    @objc private func cancelClicked() {
        dismiss(nil)
    }
}

// MARK: - NSTableViewDataSource

extension SettingsViewController: NSTableViewDataSource {
    func numberOfRows(in tableView: NSTableView) -> Int {
        return workspacePaths.count
    }
}

// MARK: - NSTableViewDelegate

extension SettingsViewController: NSTableViewDelegate {
    func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView? {
        let path = workspacePaths[row]
        let cell = NSView()

        let label = NSTextField(labelWithString: path)
        label.font = NSFont.monospacedSystemFont(ofSize: 11, weight: .regular)
        label.textColor = Theme.textPrimary
        label.lineBreakMode = .byTruncatingHead
        label.frame = NSRect(x: 4, y: 1, width: 500, height: 20)
        cell.addSubview(label)

        return cell
    }
}
