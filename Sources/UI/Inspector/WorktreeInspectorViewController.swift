import AppKit

enum WorktreeInspectorInitialTab: Int {
    case files = 0
    case changes = 1
}

final class WorktreeInspectorViewController: NSViewController {
    static func yaziCommand(yaziPath: String, configDirectory: URL) -> String? {
        do {
            try prepareYaziConfig(at: configDirectory)
        } catch {
            NSLog("WorktreeInspector: failed to prepare yazi config: \(error)")
            return nil
        }

        return "/usr/bin/env YAZI_CONFIG_HOME=\(shellQuote(configDirectory.path)) \(shellQuote(yaziPath)) ."
    }

    private let worktreePath: String
    private let yaziPathProvider: () -> String?
    private let yaziConfigDirectoryProvider: () -> URL
    private let makeDiffReviewView: (String) -> DiffReviewView
    private let createYaziSurface: ((NSView, String, String) -> Bool)?
    private let closeButton = NSButton()
    private let segmentedControl = NSSegmentedControl(
        labels: ["Files", "Changes"],
        trackingMode: .selectOne,
        target: nil,
        action: nil
    )
    private let contentView = NSView()
    private var selectedTab: WorktreeInspectorInitialTab
    private var yaziSurface: TerminalSurface?

    var selectedTabForTesting: WorktreeInspectorInitialTab { selectedTab }

    init(
        worktreePath: String,
        initialTab: WorktreeInspectorInitialTab,
        yaziAvailability: (() -> Bool)? = nil,
        yaziPathProvider: (() -> String?)? = nil,
        yaziConfigDirectoryProvider: @escaping () -> URL = { WorktreeInspectorViewController.defaultYaziConfigDirectory() },
        makeDiffReviewView: @escaping (String) -> DiffReviewView = { DiffReviewView(worktreePath: $0) },
        createYaziSurface: ((NSView, String, String) -> Bool)? = nil
    ) {
        self.worktreePath = worktreePath
        self.selectedTab = initialTab
        if let yaziPathProvider {
            self.yaziPathProvider = yaziPathProvider
        } else if let yaziAvailability {
            self.yaziPathProvider = {
                guard yaziAvailability() else { return nil }
                return ProcessRunner.commandPath("yazi") ?? "yazi"
            }
        } else {
            self.yaziPathProvider = { ProcessRunner.commandPath("yazi") }
        }
        self.yaziConfigDirectoryProvider = yaziConfigDirectoryProvider
        self.makeDiffReviewView = makeDiffReviewView
        self.createYaziSurface = createYaziSurface
        super.init(nibName: nil, bundle: nil)
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) not supported")
    }

    deinit {
        yaziSurface?.destroy()
    }

    override func loadView() {
        let root = NSView(frame: NSRect(x: 0, y: 0, width: 1080, height: 700))
        root.wantsLayer = true
        root.layer?.backgroundColor = Theme.background.cgColor
        root.setAccessibilityIdentifier("worktreeInspector")

        // Prefer the resolved semantic title (Claude session summary → stored
        // task description → prompt); fall back to the worktree directory name
        // while the title resolves off the main thread.
        let dirName = URL(fileURLWithPath: worktreePath).lastPathComponent
        let title = NSTextField(labelWithString:
            WorktreeTitleCache.shared.cachedTitle(worktreePath: worktreePath) ?? dirName)
        title.font = NSFont.systemFont(ofSize: 14, weight: .semibold)
        title.textColor = Theme.textPrimary
        title.lineBreakMode = .byTruncatingMiddle
        title.translatesAutoresizingMaskIntoConstraints = false
        root.addSubview(title)
        WorktreeTitleCache.shared.title(
            worktreePath: worktreePath, lastUserPrompt: "", branch: dirName
        ) { [weak title] resolved in
            title?.stringValue = resolved
        }

        closeButton.title = "Close"
        closeButton.bezelStyle = .rounded
        closeButton.target = self
        closeButton.action = #selector(closeClicked)
        closeButton.setAccessibilityIdentifier("worktreeInspector.closeButton")
        closeButton.translatesAutoresizingMaskIntoConstraints = false
        root.addSubview(closeButton)

        segmentedControl.target = self
        segmentedControl.action = #selector(tabChanged)
        segmentedControl.selectedSegment = selectedTab.rawValue
        segmentedControl.translatesAutoresizingMaskIntoConstraints = false
        root.addSubview(segmentedControl)

        contentView.translatesAutoresizingMaskIntoConstraints = false
        root.addSubview(contentView)

        NSLayoutConstraint.activate([
            title.topAnchor.constraint(equalTo: root.topAnchor, constant: 12),
            title.leadingAnchor.constraint(equalTo: root.leadingAnchor, constant: 16),
            title.trailingAnchor.constraint(lessThanOrEqualTo: segmentedControl.leadingAnchor, constant: -16),

            closeButton.topAnchor.constraint(equalTo: root.topAnchor, constant: 8),
            closeButton.trailingAnchor.constraint(equalTo: root.trailingAnchor, constant: -16),

            segmentedControl.topAnchor.constraint(equalTo: root.topAnchor, constant: 8),
            segmentedControl.trailingAnchor.constraint(equalTo: closeButton.leadingAnchor, constant: -8),
            segmentedControl.widthAnchor.constraint(equalToConstant: 180),

            contentView.topAnchor.constraint(equalTo: title.bottomAnchor, constant: 12),
            contentView.leadingAnchor.constraint(equalTo: root.leadingAnchor),
            contentView.trailingAnchor.constraint(equalTo: root.trailingAnchor),
            contentView.bottomAnchor.constraint(equalTo: root.bottomAnchor),
        ])

        view = root
        showSelectedTab()
    }

    @objc private func tabChanged() {
        selectedTab = WorktreeInspectorInitialTab(rawValue: segmentedControl.selectedSegment) ?? .files
        showSelectedTab()
    }

    @objc private func closeClicked() {
        dismiss(nil)
    }

    private func showSelectedTab() {
        yaziSurface?.destroy()
        yaziSurface = nil
        contentView.subviews.forEach { $0.removeFromSuperview() }

        switch selectedTab {
        case .files:
            showFilesTab()
        case .changes:
            showChangesTab()
        }
    }

    private func showFilesTab() {
        guard let yaziPath = yaziPathProvider() else {
            showMessage(
                "Yazi is not installed. Install yazi to browse files in this tab.",
                identifier: "worktreeInspector.filesMissingYazi"
            )
            return
        }

        guard let command = Self.yaziCommand(
            yaziPath: yaziPath,
            configDirectory: yaziConfigDirectoryProvider()
        ) else {
            showMessage(
                "Could not start yazi for this worktree.",
                identifier: "worktreeInspector.filesYaziFailed"
            )
            return
        }

        let terminalContainer = NSView()
        terminalContainer.wantsLayer = true
        terminalContainer.layer?.backgroundColor = Theme.background.cgColor
        terminalContainer.setAccessibilityIdentifier("worktreeInspector.yaziContainer")
        terminalContainer.translatesAutoresizingMaskIntoConstraints = false
        contentView.addSubview(terminalContainer)

        NSLayoutConstraint.activate([
            terminalContainer.topAnchor.constraint(equalTo: contentView.topAnchor),
            terminalContainer.leadingAnchor.constraint(equalTo: contentView.leadingAnchor),
            terminalContainer.trailingAnchor.constraint(equalTo: contentView.trailingAnchor),
            terminalContainer.bottomAnchor.constraint(equalTo: contentView.bottomAnchor),
        ])

        if !startYazi(in: terminalContainer, command: command) {
            terminalContainer.removeFromSuperview()
            showMessage(
                "Could not start yazi for this worktree.",
                identifier: "worktreeInspector.filesYaziFailed"
            )
        }
    }

    private func showChangesTab() {
        let review = makeDiffReviewView(worktreePath)
        review.translatesAutoresizingMaskIntoConstraints = false
        contentView.addSubview(review)

        NSLayoutConstraint.activate([
            review.topAnchor.constraint(equalTo: contentView.topAnchor),
            review.leadingAnchor.constraint(equalTo: contentView.leadingAnchor),
            review.trailingAnchor.constraint(equalTo: contentView.trailingAnchor),
            review.bottomAnchor.constraint(equalTo: contentView.bottomAnchor),
        ])
    }

    private func showMessage(_ message: String, identifier: String) {
        let label = NSTextField(labelWithString: message)
        label.font = NSFont.systemFont(ofSize: 13)
        label.textColor = Theme.textSecondary
        label.alignment = .center
        label.maximumNumberOfLines = 0
        label.lineBreakMode = .byWordWrapping
        label.setAccessibilityIdentifier(identifier)
        label.translatesAutoresizingMaskIntoConstraints = false
        contentView.addSubview(label)

        NSLayoutConstraint.activate([
            label.centerXAnchor.constraint(equalTo: contentView.centerXAnchor),
            label.centerYAnchor.constraint(equalTo: contentView.centerYAnchor),
            label.leadingAnchor.constraint(greaterThanOrEqualTo: contentView.leadingAnchor, constant: 24),
            label.trailingAnchor.constraint(lessThanOrEqualTo: contentView.trailingAnchor, constant: -24),
        ])
    }

    private func startYazi(in container: NSView, command: String) -> Bool {
        if let createYaziSurface {
            return createYaziSurface(container, worktreePath, command)
        }

        let surface = TerminalSurface()
        yaziSurface = surface
        let started = surface.createEphemeral(in: container, workingDirectory: worktreePath, command: command)
        if !started {
            yaziSurface = nil
        }
        return started
    }

    private static func defaultYaziConfigDirectory() -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("amux-yazi", isDirectory: true)
    }

    private static func prepareYaziConfig(at directory: URL) throws {
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let configURL = directory.appendingPathComponent("yazi.toml")
        try """
        [mgr]
        show_hidden = true
        """.write(to: configURL, atomically: true, encoding: .utf8)
    }

    private static func shellQuote(_ value: String) -> String {
        "'\(value.replacingOccurrences(of: "'", with: "'\\''"))'"
    }
}
