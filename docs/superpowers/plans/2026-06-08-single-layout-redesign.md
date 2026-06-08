# Single-Layout Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Collapse AMUX's 4-layout dashboard into a single LeftRight layout, repurpose the top capsule to show the focused worktree's title + token placeholder, add a bottom global status bar, shrink mini cards to a fixed 3–4 lines, and replace the new-worktree dialog with an inline sticky input.

**Architecture:** Net-new read-only units (`SessionTitleLookup`, `WorktreeTitleResolver`) and a `WorktreeCreator` env-copy helper are added first with TDD. UI changes layer on top: `MiniCardView` and `TitleBarView` are rewired to the title resolver, `DashboardViewController`/`Config` drop the 3 extra layouts, a new `StatusBarView` takes over global usage + notification display, and a new `InlineWorktreeCreateView` replaces `NewBranchDialog`.

**Tech Stack:** Swift 5.10, AppKit, XCTest, `git worktree`, Claude/Codex session JSONL files.

**Spec:** `docs/superpowers/specs/2026-06-08-single-layout-redesign-design.md`

**Conventions for executing agents:**
- Build: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build`
- Test (single class): `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/<ClassName>`
- After adding any new `Sources/` file, it is picked up automatically (project uses folder references via XcodeGen). If a build complains about a missing file, run `xcodegen generate`.
- **Before editing any existing UI file (`TitleBarView`, `DashboardViewController`, `MainWindowController`, `StackedMiniCardContainerView`), Read it in full first** — these are large and this plan quotes only the regions that change.

---

## Task 1: `SessionTitleLookup` — read Claude session summary by worktree path

**Files:**
- Create: `Sources/Core/SessionTitleLookup.swift`
- Test: `Tests/SessionTitleLookupTests.swift`

Mirrors the existing `Sources/Core/CodexSessionPromptLookup.swift` pattern (read JSONL, parse line-by-line). Claude stores sessions at `~/.claude/projects/<encoded-cwd>/<sessionId>.jsonl`, where `<encoded-cwd>` is the absolute worktree path with `/` and `.` replaced by `-`. Each session's auto-generated title is a line with `{"type":"summary","summary":"..."}`. We pick the most recently modified `.jsonl` in the worktree's project dir and return its last `summary`.

- [ ] **Step 1: Write the failing test**

```swift
import XCTest
@testable import amux

final class SessionTitleLookupTests: XCTestCase {
    private var root: URL!

    override func setUpWithError() throws {
        root = FileManager.default.temporaryDirectory
            .appendingPathComponent("amux-sessiontitle-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    }

    override func tearDownWithError() throws {
        try? FileManager.default.removeItem(at: root)
    }

    private func projectDir(for worktreePath: String) -> URL {
        let encoded = SessionTitleLookup.encodedProjectComponent(worktreePath: worktreePath)
        let dir = root.appendingPathComponent(encoded, isDirectory: true)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }

    func testReturnsLastSummary() throws {
        let wt = "/Users/me/repo-worktrees/feature-x"
        let dir = projectDir(for: wt)
        let lines = [
            #"{"type":"summary","summary":"First title"}"#,
            #"{"type":"user","message":{"role":"user","content":"hi"}}"#,
            #"{"type":"summary","summary":"Refactor the parser"}"#,
        ].joined(separator: "\n")
        try lines.write(to: dir.appendingPathComponent("s1.jsonl"), atomically: true, encoding: .utf8)

        let title = SessionTitleLookup.title(worktreePath: wt, projectsRoot: root)
        XCTAssertEqual(title, "Refactor the parser")
    }

    func testReturnsNilWhenNoSummary() throws {
        let wt = "/Users/me/repo-worktrees/feature-y"
        let dir = projectDir(for: wt)
        try #"{"type":"user","message":{"role":"user","content":"hi"}}"#
            .write(to: dir.appendingPathComponent("s1.jsonl"), atomically: true, encoding: .utf8)

        XCTAssertNil(SessionTitleLookup.title(worktreePath: wt, projectsRoot: root))
    }

    func testReturnsNilWhenDirMissing() {
        XCTAssertNil(SessionTitleLookup.title(worktreePath: "/nope/missing", projectsRoot: root))
    }

    func testEncodingReplacesSlashesAndDots() {
        XCTAssertEqual(
            SessionTitleLookup.encodedProjectComponent(worktreePath: "/Users/me/repo.app/feature"),
            "-Users-me-repo-app-feature"
        )
    }
}
```

- [ ] **Step 2: Run the test, verify it fails**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/SessionTitleLookupTests`
Expected: FAIL — `cannot find 'SessionTitleLookup' in scope`.

- [ ] **Step 3: Write the implementation**

```swift
import Foundation

/// Reads Claude Code's auto-generated session title (the `summary` record) for a
/// given worktree. Claude stores sessions under
/// `~/.claude/projects/<encoded-cwd>/<sessionId>.jsonl`, where `<encoded-cwd>` is
/// the absolute path with `/` and `.` replaced by `-`.
enum SessionTitleLookup {
    /// Title from the most recently modified session JSONL in the worktree's
    /// project directory, or nil if none has a `summary` record.
    static func title(
        worktreePath: String,
        fileManager: FileManager = .default,
        projectsRoot: URL = defaultProjectsRoot()
    ) -> String? {
        guard !worktreePath.isEmpty else { return nil }
        let dir = projectsRoot.appendingPathComponent(
            encodedProjectComponent(worktreePath: worktreePath), isDirectory: true
        )
        guard let entries = try? fileManager.contentsOfDirectory(
            at: dir,
            includingPropertiesForKeys: [.contentModificationDateKey],
            options: [.skipsHiddenFiles]
        ) else {
            return nil
        }

        let sessions = entries
            .filter { $0.pathExtension == "jsonl" }
            .sorted { lhs, rhs in
                let l = (try? lhs.resourceValues(forKeys: [.contentModificationDateKey]))?.contentModificationDate ?? .distantPast
                let r = (try? rhs.resourceValues(forKeys: [.contentModificationDateKey]))?.contentModificationDate ?? .distantPast
                return l > r
            }

        for session in sessions {
            if let summary = lastSummary(in: session) {
                return summary
            }
        }
        return nil
    }

    /// Encodes an absolute path the way Claude Code names its project directories.
    static func encodedProjectComponent(worktreePath: String) -> String {
        var result = ""
        for ch in worktreePath {
            result.append(ch == "/" || ch == "." ? "-" : ch)
        }
        return result
    }

    private static func lastSummary(in fileURL: URL) -> String? {
        guard let contents = try? String(contentsOf: fileURL, encoding: .utf8) else { return nil }
        var last: String?
        contents.enumerateLines { line, _ in
            guard
                let data = line.data(using: .utf8),
                let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
                object["type"] as? String == "summary",
                let summary = object["summary"] as? String,
                !summary.isEmpty
            else { return }
            last = summary
        }
        return last
    }

    private static func defaultProjectsRoot() -> URL {
        FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent(".claude", isDirectory: true)
            .appendingPathComponent("projects", isDirectory: true)
    }
}
```

- [ ] **Step 4: Run the test, verify it passes**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/SessionTitleLookupTests`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add Sources/Core/SessionTitleLookup.swift Tests/SessionTitleLookupTests.swift
git commit -m "feat: read Claude session title by worktree path"
```

---

## Task 2: `WorktreeTitleResolver` — single source for a worktree's display title

**Files:**
- Create: `Sources/Core/WorktreeTitleResolver.swift`
- Test: `Tests/WorktreeTitleResolverTests.swift`

Both the top capsule (Task 5) and the mini card (Task 3) need the same title. Resolution order: Claude session summary → `lastUserPrompt` (already on `AgentInfo`) → branch name. Codex worktrees won't have a Claude summary, so they naturally fall through to `lastUserPrompt`.

- [ ] **Step 1: Write the failing test**

```swift
import XCTest
@testable import amux

final class WorktreeTitleResolverTests: XCTestCase {
    func testFallsBackToPromptWhenNoSummary() {
        let title = WorktreeTitleResolver.resolve(
            worktreePath: "/nonexistent/path",
            lastUserPrompt: "Fix the login bug",
            branch: "feature/login",
            sessionTitle: { _ in nil }
        )
        XCTAssertEqual(title, "Fix the login bug")
    }

    func testPrefersSessionTitle() {
        let title = WorktreeTitleResolver.resolve(
            worktreePath: "/p",
            lastUserPrompt: "prompt",
            branch: "br",
            sessionTitle: { _ in "Session Title" }
        )
        XCTAssertEqual(title, "Session Title")
    }

    func testFallsBackToBranchWhenEmpty() {
        let title = WorktreeTitleResolver.resolve(
            worktreePath: "/p",
            lastUserPrompt: "",
            branch: "feature/x",
            sessionTitle: { _ in nil }
        )
        XCTAssertEqual(title, "feature/x")
    }
}
```

- [ ] **Step 2: Run the test, verify it fails**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/WorktreeTitleResolverTests`
Expected: FAIL — `cannot find 'WorktreeTitleResolver' in scope`.

- [ ] **Step 3: Write the implementation**

```swift
import Foundation

/// Resolves the human-facing title for a worktree, shared by the top capsule and
/// the mini cards. Order: Claude session summary → last user prompt → branch.
enum WorktreeTitleResolver {
    static func resolve(
        worktreePath: String,
        lastUserPrompt: String,
        branch: String,
        sessionTitle: (String) -> String? = { SessionTitleLookup.title(worktreePath: $0) }
    ) -> String {
        if let summary = sessionTitle(worktreePath)?.trimmingCharacters(in: .whitespacesAndNewlines),
           !summary.isEmpty {
            return summary
        }
        let prompt = lastUserPrompt.trimmingCharacters(in: .whitespacesAndNewlines)
        if !prompt.isEmpty { return prompt }
        return branch
    }
}
```

- [ ] **Step 4: Run the test, verify it passes**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/WorktreeTitleResolverTests`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add Sources/Core/WorktreeTitleResolver.swift Tests/WorktreeTitleResolverTests.swift
git commit -m "feat: add WorktreeTitleResolver"
```

---

## Task 3: `WorktreeCreator` env-file copy

**Files:**
- Modify: `Sources/Git/WorktreeCreator.swift`
- Test: `Tests/WorktreeCreatorEnvCopyTests.swift`

Net-new helper that copies environment files (`.env`, `.env.*`, `.envrc`) from a source worktree's root into a destination. Used by the inline create form when "reuse environment" is on. Does NOT copy `node_modules`/build artifacts. Missing source files must not error.

- [ ] **Step 1: Write the failing test**

```swift
import XCTest
@testable import amux

final class WorktreeCreatorEnvCopyTests: XCTestCase {
    private var src: URL!
    private var dst: URL!

    override func setUpWithError() throws {
        let base = FileManager.default.temporaryDirectory
            .appendingPathComponent("amux-envcopy-\(UUID().uuidString)", isDirectory: true)
        src = base.appendingPathComponent("src", isDirectory: true)
        dst = base.appendingPathComponent("dst", isDirectory: true)
        try FileManager.default.createDirectory(at: src, withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: dst, withIntermediateDirectories: true)
    }

    override func tearDownWithError() throws {
        try? FileManager.default.removeItem(at: src.deletingLastPathComponent())
    }

    func testCopiesEnvFilesOnly() throws {
        try "A=1".write(to: src.appendingPathComponent(".env"), atomically: true, encoding: .utf8)
        try "B=2".write(to: src.appendingPathComponent(".env.local"), atomically: true, encoding: .utf8)
        try "use nix".write(to: src.appendingPathComponent(".envrc"), atomically: true, encoding: .utf8)
        let nodeModules = src.appendingPathComponent("node_modules", isDirectory: true)
        try FileManager.default.createDirectory(at: nodeModules, withIntermediateDirectories: true)
        try "x".write(to: nodeModules.appendingPathComponent("pkg.js"), atomically: true, encoding: .utf8)

        WorktreeCreator.copyEnvironmentFiles(from: src.path, to: dst.path)

        XCTAssertEqual(try String(contentsOf: dst.appendingPathComponent(".env")), "A=1")
        XCTAssertEqual(try String(contentsOf: dst.appendingPathComponent(".env.local")), "B=2")
        XCTAssertEqual(try String(contentsOf: dst.appendingPathComponent(".envrc")), "use nix")
        XCTAssertFalse(FileManager.default.fileExists(atPath: dst.appendingPathComponent("node_modules").path))
    }

    func testNoEnvFilesDoesNotThrow() {
        WorktreeCreator.copyEnvironmentFiles(from: src.path, to: dst.path)
        XCTAssertFalse(FileManager.default.fileExists(atPath: dst.appendingPathComponent(".env").path))
    }
}
```

- [ ] **Step 2: Run the test, verify it fails**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/WorktreeCreatorEnvCopyTests`
Expected: FAIL — `type 'WorktreeCreator' has no member 'copyEnvironmentFiles'`.

- [ ] **Step 3: Add the implementation** to `Sources/Git/WorktreeCreator.swift`, inside the `enum WorktreeCreator`, after `createWorktree(...)`:

```swift
    /// Copy environment files (.env, .env.*, .envrc) from one worktree root to
    /// another. Best-effort: missing files and copy failures are ignored.
    static func copyEnvironmentFiles(from sourcePath: String, to destPath: String) {
        let fm = FileManager.default
        guard let entries = try? fm.contentsOfDirectory(atPath: sourcePath) else { return }
        for name in entries where isEnvironmentFile(name) {
            let srcFile = (sourcePath as NSString).appendingPathComponent(name)
            var isDir: ObjCBool = false
            guard fm.fileExists(atPath: srcFile, isDirectory: &isDir), !isDir.boolValue else { continue }
            let dstFile = (destPath as NSString).appendingPathComponent(name)
            try? fm.removeItem(atPath: dstFile)
            try? fm.copyItem(atPath: srcFile, toPath: dstFile)
        }
    }

    private static func isEnvironmentFile(_ name: String) -> Bool {
        name == ".env" || name == ".envrc" || name.hasPrefix(".env.")
    }
```

- [ ] **Step 4: Run the test, verify it passes**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/WorktreeCreatorEnvCopyTests`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add Sources/Git/WorktreeCreator.swift Tests/WorktreeCreatorEnvCopyTests.swift
git commit -m "feat: copy env files into new worktree"
```

---

## Task 4: Shrink `MiniCardView` to a fixed 3–4 line card

**Files:**
- Modify: `Sources/UI/Dashboard/MiniCardView.swift`
- Test: `Tests/MiniCardViewTests.swift` (create if absent)

New layout, top to bottom: **title (1–2 lines, wrapping)** → **status dots + status text + duration (one line)** → **repo · worktree (one line)**. Remove the `promptLabel` and `messageLabel` regions, the 16:9 aspect constraint, and the activity/task rendering. The card sizes to its content height.

`configure(...)` keeps its existing signature (callers in `DashboardViewController` pass all args) but ignores `lastMessage`, `tasks`, `activityEvents`. The title is computed by the caller via `WorktreeTitleResolver` and passed in `lastUserPrompt` — to avoid changing the call sites' arg list, repurpose the `lastUserPrompt` parameter as the resolved title (callers updated in Task 7's plumbing note below). For now, derive the title locally: `let title = lastUserPrompt.isEmpty ? project : lastUserPrompt`.

- [ ] **Step 1: Write the failing test**

```swift
import XCTest
@testable import amux

final class MiniCardViewTests: XCTestCase {
    func testConfigureSetsTitleAndRepoWorktree() {
        let card = MiniCardView(frame: NSRect(x: 0, y: 0, width: 220, height: 80))
        card.configure(
            id: "t1", project: "teamclaw-next", thread: "test-trsyt",
            status: "running", lastMessage: "ignored",
            lastUserPrompt: "Why are classics unread?",
            totalDuration: "120", roundDuration: "30"
        )
        XCTAssertEqual(card.agentId, "t1")
        XCTAssertEqual(card.titleTextForTesting, "Why are classics unread?")
        XCTAssertTrue(card.repoWorktreeTextForTesting.contains("teamclaw-next"))
        XCTAssertTrue(card.repoWorktreeTextForTesting.contains("test-trsyt"))
    }

    func testTitleFallsBackToProject() {
        let card = MiniCardView(frame: .zero)
        card.configure(
            id: "t2", project: "repo", thread: "wt",
            status: "idle", lastMessage: "",
            lastUserPrompt: "",
            totalDuration: "0", roundDuration: "0"
        )
        XCTAssertEqual(card.titleTextForTesting, "repo")
    }
}
```

- [ ] **Step 2: Run the test, verify it fails**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/MiniCardViewTests`
Expected: FAIL — `value of type 'MiniCardView' has no member 'titleTextForTesting'`.

- [ ] **Step 3: Rewrite `MiniCardView`.** Replace the whole file with the following (preserves `agentId`, delegate, selection/hover/dim behavior, drops prompt/message/aspect):

```swift
import AppKit

final class MiniCardView: NSView {
    enum Typography {
        static let primaryPointSize: CGFloat = 12
        static let secondaryPointSize: CGFloat = 10
    }

    weak var delegate: AgentCardDelegate?
    private(set) var agentId: String = ""
    var isSelected: Bool = false { didSet { updateAppearance() } }
    var isKeyboardFocused: Bool = false { didSet { updateAppearance() } }

    // Line 1–2: title (wraps up to 2 lines)
    private let titleLabel = NSTextField(labelWithString: "")
    // Line 3: status text (right) + duration (left), with leading status dots
    private let durationLabel = NSTextField(labelWithString: "")
    private let statusTextLabel = NSTextField(labelWithString: "")
    private var statusDots: [NSView] = []
    private var durationLeadingConstraint: NSLayoutConstraint?
    // Line 4: repo · worktree
    private let repoWorktreeLabel = NSTextField(labelWithString: "")

    private var isHovered = false
    private var dimOverlayLayer: CALayer?

    // Test hooks
    var titleTextForTesting: String { titleLabel.stringValue }
    var repoWorktreeTextForTesting: String { repoWorktreeLabel.stringValue }

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        setup()
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) not supported") }

    func configure(id: String, project: String, thread: String, status: String, lastMessage: String, lastUserPrompt: String = "", totalDuration: String, roundDuration: String, paneStatuses: [AgentStatus] = [], isMainWorktree: Bool = false, tasks: [TaskItem] = [], activityEvents: [ActivityEvent] = []) {
        agentId = id
        setAccessibilityIdentifier("dashboard.miniCard.\(id)")

        let title = lastUserPrompt.isEmpty ? project : lastUserPrompt
        titleLabel.stringValue = title

        // Line 4: repo · worktree
        repoWorktreeLabel.stringValue = "\(project)  \u{00B7}  \(thread)"

        // Status dots before the duration line
        statusDots.forEach { $0.removeFromSuperview() }
        statusDots.removeAll()
        durationLeadingConstraint?.isActive = false
        let statuses = paneStatuses.isEmpty ? [AgentStatus(rawValue: status) ?? .unknown] : paneStatuses
        var previousDot: NSView? = nil
        for agentStatus in statuses {
            let dot = NSView()
            dot.wantsLayer = true
            dot.layer?.cornerRadius = 3
            dot.layer?.backgroundColor = agentStatus.color.cgColor
            dot.translatesAutoresizingMaskIntoConstraints = false
            addSubview(dot)
            NSLayoutConstraint.activate([
                dot.widthAnchor.constraint(equalToConstant: 6),
                dot.heightAnchor.constraint(equalToConstant: 6),
                dot.centerYAnchor.constraint(equalTo: durationLabel.centerYAnchor),
                dot.leadingAnchor.constraint(equalTo: previousDot?.trailingAnchor ?? leadingAnchor,
                                             constant: previousDot != nil ? 3 : 8),
            ])
            statusDots.append(dot)
            previousDot = dot
        }
        if let lastDot = statusDots.last {
            durationLeadingConstraint = durationLabel.leadingAnchor.constraint(equalTo: lastDot.trailingAnchor, constant: 5)
            durationLeadingConstraint?.isActive = true
        }

        let compactTotal = AgentDisplayHelpers.compactDuration(totalDuration)
        let compactRound = AgentDisplayHelpers.compactDuration(roundDuration)
        durationLabel.stringValue = "\u{23F1} \(compactTotal) \u{00B7} \(compactRound)"

        statusTextLabel.stringValue = status.capitalized
        statusTextLabel.textColor = AgentDisplayHelpers.statusColor(status)

        updateAppearance()
    }

    private func setup() {
        wantsLayer = true
        layer?.cornerRadius = 6

        titleLabel.font = NSFont.systemFont(ofSize: Typography.primaryPointSize, weight: .semibold)
        titleLabel.textColor = SemanticColors.text
        titleLabel.lineBreakMode = .byTruncatingTail
        titleLabel.maximumNumberOfLines = 2
        titleLabel.cell?.wraps = true
        titleLabel.translatesAutoresizingMaskIntoConstraints = false
        titleLabel.setContentCompressionResistancePriority(.defaultHigh, for: .vertical)
        addSubview(titleLabel)

        statusTextLabel.font = NSFont.systemFont(ofSize: Typography.secondaryPointSize, weight: .regular)
        statusTextLabel.lineBreakMode = .byTruncatingTail
        statusTextLabel.maximumNumberOfLines = 1
        statusTextLabel.alignment = .right
        statusTextLabel.translatesAutoresizingMaskIntoConstraints = false
        statusTextLabel.setContentCompressionResistancePriority(.required, for: .horizontal)
        statusTextLabel.setContentHuggingPriority(.defaultHigh, for: .horizontal)
        addSubview(statusTextLabel)

        durationLabel.font = NSFont.systemFont(ofSize: Typography.secondaryPointSize, weight: .regular)
        durationLabel.textColor = SemanticColors.muted
        durationLabel.lineBreakMode = .byTruncatingTail
        durationLabel.maximumNumberOfLines = 1
        durationLabel.translatesAutoresizingMaskIntoConstraints = false
        durationLabel.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        addSubview(durationLabel)

        repoWorktreeLabel.font = NSFont.systemFont(ofSize: Typography.secondaryPointSize, weight: .regular)
        repoWorktreeLabel.textColor = SemanticColors.muted
        repoWorktreeLabel.lineBreakMode = .byTruncatingTail
        repoWorktreeLabel.maximumNumberOfLines = 1
        repoWorktreeLabel.translatesAutoresizingMaskIntoConstraints = false
        addSubview(repoWorktreeLabel)

        let padding: CGFloat = 8
        let durationFallbackLeading = durationLabel.leadingAnchor.constraint(equalTo: leadingAnchor, constant: padding)
        durationFallbackLeading.priority = .defaultLow

        NSLayoutConstraint.activate([
            titleLabel.topAnchor.constraint(equalTo: topAnchor, constant: padding),
            titleLabel.leadingAnchor.constraint(equalTo: leadingAnchor, constant: padding),
            titleLabel.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -padding),

            durationLabel.topAnchor.constraint(equalTo: titleLabel.bottomAnchor, constant: 4),
            durationFallbackLeading,
            durationLabel.trailingAnchor.constraint(lessThanOrEqualTo: statusTextLabel.leadingAnchor, constant: -4),

            statusTextLabel.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -padding),
            statusTextLabel.centerYAnchor.constraint(equalTo: durationLabel.centerYAnchor),

            repoWorktreeLabel.topAnchor.constraint(equalTo: durationLabel.bottomAnchor, constant: 4),
            repoWorktreeLabel.leadingAnchor.constraint(equalTo: leadingAnchor, constant: padding),
            repoWorktreeLabel.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -padding),
            repoWorktreeLabel.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -padding),
        ])

        let click = NSClickGestureRecognizer(target: self, action: #selector(handleClick))
        addGestureRecognizer(click)
        addTrackingArea(NSTrackingArea(
            rect: .zero,
            options: [.mouseEnteredAndExited, .activeInActiveApp, .inVisibleRect],
            owner: self, userInfo: nil
        ))
        updateAppearance()
    }

    @objc private func handleClick() { delegate?.agentCardClicked(agentId: agentId) }
    override func mouseEntered(with event: NSEvent) { isHovered = true; updateAppearance() }
    override func mouseExited(with event: NSEvent) { isHovered = false; updateAppearance() }

    func showDimOverlay(opacity: CGFloat) {
        if dimOverlayLayer == nil {
            let overlay = CALayer()
            overlay.backgroundColor = NSColor.white.withAlphaComponent(opacity).cgColor
            overlay.frame = bounds
            overlay.autoresizingMask = [.layerWidthSizable, .layerHeightSizable]
            layer?.addSublayer(overlay)
            dimOverlayLayer = overlay
        }
    }

    func hideDimOverlay() {
        dimOverlayLayer?.removeFromSuperlayer()
        dimOverlayLayer = nil
    }

    override var acceptsFirstResponder: Bool { false }
    override var wantsUpdateLayer: Bool { true }
    override func updateLayer() { updateAppearance() }
    override func viewDidChangeEffectiveAppearance() { super.viewDidChangeEffectiveAppearance(); needsDisplay = true }

    private func updateAppearance() {
        guard let layer = layer else { return }
        if isKeyboardFocused {
            layer.backgroundColor = resolvedCGColor(SemanticColors.panel2)
            layer.borderColor = resolvedCGColor(SemanticColors.accent)
            layer.borderWidth = 2
            layer.shadowColor = resolvedCGColor(SemanticColors.accent)
            layer.shadowOpacity = 0.6
            layer.shadowRadius = 8
            layer.shadowOffset = .zero
            layer.masksToBounds = false
        } else if isSelected {
            layer.backgroundColor = resolvedCGColor(SemanticColors.panel2)
            layer.borderColor = resolvedCGColor(SemanticColors.accent)
            layer.borderWidth = 1.5
            layer.shadowOpacity = 0
        } else if isHovered {
            layer.backgroundColor = resolvedCGColor(SemanticColors.arcBlockHover)
            layer.borderColor = resolvedCGColor(SemanticColors.lineAlpha40)
            layer.borderWidth = 1.5
            layer.shadowOpacity = 0
        } else {
            layer.backgroundColor = resolvedCGColor(SemanticColors.tileBarBg)
            layer.borderColor = resolvedCGColor(SemanticColors.lineAlpha45)
            layer.borderWidth = 1
            layer.shadowColor = resolvedCGColor(SemanticColors.miniCardShadowDefault)
            layer.shadowOpacity = 1
            layer.shadowRadius = 8
            layer.shadowOffset = NSSize(width: 0, height: -2)
        }
        titleLabel.textColor = SemanticColors.text
        repoWorktreeLabel.textColor = SemanticColors.muted
    }
}
```

- [ ] **Step 4: Run the test, verify it passes**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/MiniCardViewTests`
Expected: PASS (2 tests).

- [ ] **Step 5: Build the app to catch removed-symbol references**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build`
Expected: BUILD SUCCEEDED. If `StackedMiniCardContainerView` or layout code referenced the removed `promptLabel`/`messageLabel`/aspect, fix those references to drop the removed members (none are public on `MiniCardView`, so only internal callers of `configure` should remain — they are unaffected because the signature is unchanged).

- [ ] **Step 6: Commit**

```bash
git add Sources/UI/Dashboard/MiniCardView.swift Tests/MiniCardViewTests.swift
git commit -m "feat: shrink mini card to fixed 3-4 line layout"
```

---

## Task 5: Collapse the dashboard to a single LeftRight layout

**Files:**
- Modify: `Sources/UI/TitleBar/TitleBarView.swift` (remove the 4 layout buttons + `titleBarDidSelectLayout`)
- Modify: `Sources/UI/Dashboard/DashboardViewController.swift` (force LeftRight; remove Grid/TopSmall/TopLarge layout paths)
- Modify: `Sources/Core/Config.swift` (stop persisting `layout`, keep backward-compat decode)
- Modify any call sites the build flags (e.g. `MainWindowController` layout-switch wiring)

> Read each file fully before editing. This task is a deletion + force-default; verify by build, not unit test.

- [ ] **Step 1: In `TitleBarView.swift`**, remove the layout button declarations (`gridLayoutButton`, `leftLayoutButton`, `topSmallLayoutButton`, `topLargeLayoutButton`, `layoutButtons`), their setup/layout code, the `currentLayout` tracking used only for them, and the `func titleBarDidSelectLayout` from the `TitleBarDelegate` protocol. Keep `addProjectButton`/`newWorktreeButton` for now (removed in Task 8). Keep `themeButton` and `collapseSidebarButton`.

- [ ] **Step 2: In `DashboardViewController.swift`**, make the layout constant. Find the stored layout property (e.g. `var currentLayout: DashboardLayout`) and the `switch` that dispatches to `layoutGrid()/layoutLeftRight()/layoutTopSmall()/layoutTopLarge()`. Replace the dispatch so it always calls the LeftRight path; delete the `layoutGrid`, `layoutTopSmall`, `layoutTopLarge` methods and any helpers used only by them. Remove the public API that sets layout from the title bar.

- [ ] **Step 3: In `Config.swift`**, remove `layout` from the encoded output but keep a `decodeIfPresent` read (ignored) so old config files still load. Remove `selectedLayout`-style accessors if present.

- [ ] **Step 4: Build, fix all call sites**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build`
Expected: a series of compile errors at former layout call sites (MainWindowController keyboard shortcuts, TabCoordinator restore). Remove each reference to layout switching. Re-run until BUILD SUCCEEDED.

- [ ] **Step 5: Run the full existing test suite to catch regressions**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test`
Expected: PASS. Fix any test that asserted on layout switching by deleting the obsolete assertions (note in commit message).

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "refactor: collapse dashboard to single LeftRight layout"
```

---

## Task 6: Repurpose the top capsule to worktree title + token placeholder

**Files:**
- Modify: `Sources/UI/TitleBar/TitleBarView.swift`
- Modify: `Sources/App/MainWindowController.swift` (feed focused-worktree title on selection change)

The left primary capsule currently rotates global usage/tips. Repurpose it: left label = focused worktree title (via `WorktreeTitleResolver`), right label = token placeholder (`"—"`). Stop the usage rotation here (usage moves to the status bar in Task 7).

- [ ] **Step 1: In `TitleBarView.swift`**, add a public method and stop the tip/usage rotation:

```swift
    /// Show the focused worktree's title on the left and a token placeholder on the right.
    func updateFocusedWorktree(title: String, tokenText: String = "\u{2014}") {
        tipRotationTimer?.invalidate()
        tipRotationTimer = nil
        capsuleIconView.isHidden = true
        capsuleLeadingLabel.stringValue = title
        capsuleLeadingLabel.lineBreakMode = .byTruncatingTail
        capsuleBodyLabel.isHidden = true
        capsuleTrailingLabel.stringValue = tokenText
        capsuleTrailingLabel.isHidden = false
        capsuleSecondaryLabel.isHidden = true
        capsuleSecondaryTrailingLabel.isHidden = true
        capsuleSep1Label.isHidden = true
        capsuleSep2Label.isHidden = true
        capsuleSep3Label.isHidden = true
        usageProgressTrack.isHidden = true
        secondaryUsageProgressTrack.isHidden = true
    }
```

Also neutralize `startTipRotationIfNeeded()` so it no longer starts rotation (make it a no-op or guard it out), and stop calling `updatePrimaryCapsuleFrames` from the usage path in this view.

- [ ] **Step 2: In `MainWindowController.swift`**, find where worktree selection changes are observed (the `tabCoordinator.selectedAgent` flow / `dashboardVC` selection callback). On selection change, resolve the title off the main thread and push to the title bar:

```swift
    private func refreshFocusedWorktreeCapsule() {
        guard let agent = tabCoordinator.selectedAgent else {
            titleBar.updateFocusedWorktree(title: "")
            return
        }
        let path = agent.worktreePath
        let prompt = agent.lastUserPrompt   // use the field available on the display info
        let branch = agent.branch
        DispatchQueue.global(qos: .userInitiated).async {
            let title = WorktreeTitleResolver.resolve(
                worktreePath: path, lastUserPrompt: prompt, branch: branch
            )
            DispatchQueue.main.async { self.titleBar.updateFocusedWorktree(title: title) }
        }
    }
```

Call `refreshFocusedWorktreeCapsule()` wherever selection changes (initial load + on select). If `AgentDisplayInfo` lacks `lastUserPrompt`/`branch`, read them from `AgentHead.shared` by worktree path instead — check the available accessor and use it.

- [ ] **Step 3: Build & smoke-test**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build`
Expected: BUILD SUCCEEDED. Launch is covered in Task 9.

- [ ] **Step 4: Commit**

```bash
git add Sources/UI/TitleBar/TitleBarView.swift Sources/App/MainWindowController.swift
git commit -m "feat: top capsule shows focused worktree title + token placeholder"
```

---

## Task 7: Bottom global status bar (usage + shortcuts + notification)

**Files:**
- Create: `Sources/UI/StatusBar/StatusBarView.swift`
- Modify: `Sources/App/MainWindowController.swift` (lay it out below content; route usage + notification here)
- Test: `Tests/StatusBarViewTests.swift`

`StatusBarView` is a fixed-height bar: left = Claude/Codex plan balance (fed by the existing `UsageSummaryStore` frames), right = high-frequency shortcut hints, plus a notification summary slot (moved from the title bar).

- [ ] **Step 1: Write the failing test**

```swift
import XCTest
@testable import amux

final class StatusBarViewTests: XCTestCase {
    func testUpdateUsageSetsText() {
        let bar = StatusBarView(frame: NSRect(x: 0, y: 0, width: 800, height: 28))
        bar.updateUsage(text: "Claude 42% · Codex 1.2k")
        XCTAssertEqual(bar.usageTextForTesting, "Claude 42% · Codex 1.2k")
    }

    func testUpdateNotificationSetsText() {
        let bar = StatusBarView(frame: .zero)
        bar.updateNotification(text: "3 unread")
        XCTAssertEqual(bar.notificationTextForTesting, "3 unread")
    }
}
```

- [ ] **Step 2: Run the test, verify it fails**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/StatusBarViewTests`
Expected: FAIL — `cannot find 'StatusBarView' in scope`.

- [ ] **Step 3: Create `Sources/UI/StatusBar/StatusBarView.swift`**

```swift
import AppKit

/// Fixed-height bottom bar: global Claude/Codex usage (left), notification
/// summary (center), high-frequency shortcuts (right).
final class StatusBarView: NSView {
    static let height: CGFloat = 26

    private let usageLabel = NSTextField(labelWithString: "")
    private let notificationLabel = NSTextField(labelWithString: "")
    private let shortcutsLabel = NSTextField(labelWithString: "")

    var usageTextForTesting: String { usageLabel.stringValue }
    var notificationTextForTesting: String { notificationLabel.stringValue }

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        setup()
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) not supported") }

    func updateUsage(text: String) { usageLabel.stringValue = text }
    func updateNotification(text: String) { notificationLabel.stringValue = text }
    func updateShortcuts(text: String) { shortcutsLabel.stringValue = text }

    private func setup() {
        wantsLayer = true
        layer?.backgroundColor = resolvedCGColor(SemanticColors.tileBarBg)

        for label in [usageLabel, notificationLabel, shortcutsLabel] {
            label.font = NSFont.systemFont(ofSize: 10, weight: .regular)
            label.textColor = SemanticColors.muted
            label.lineBreakMode = .byTruncatingTail
            label.maximumNumberOfLines = 1
            label.translatesAutoresizingMaskIntoConstraints = false
            addSubview(label)
        }
        shortcutsLabel.alignment = .right
        notificationLabel.alignment = .center

        // Default shortcut hints.
        shortcutsLabel.stringValue = "\u{2318}N New  \u{00B7}  \u{2318}D Split  \u{00B7}  \u{2318}P Switch"

        NSLayoutConstraint.activate([
            usageLabel.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 12),
            usageLabel.centerYAnchor.constraint(equalTo: centerYAnchor),
            notificationLabel.centerXAnchor.constraint(equalTo: centerXAnchor),
            notificationLabel.centerYAnchor.constraint(equalTo: centerYAnchor),
            shortcutsLabel.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -12),
            shortcutsLabel.centerYAnchor.constraint(equalTo: centerYAnchor),
            usageLabel.trailingAnchor.constraint(lessThanOrEqualTo: notificationLabel.leadingAnchor, constant: -8),
            notificationLabel.trailingAnchor.constraint(lessThanOrEqualTo: shortcutsLabel.leadingAnchor, constant: -8),
        ])
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        layer?.backgroundColor = resolvedCGColor(SemanticColors.tileBarBg)
    }
}
```

- [ ] **Step 4: Run the test, verify it passes**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/StatusBarViewTests`
Expected: PASS (2 tests).

- [ ] **Step 5: Mount it in `MainWindowController.swift`.** Read the window layout (around the `contentContainer` constraints pinned to `contentView.bottomAnchor`). Add a `StatusBarView` instance, constrain it to the window bottom with `heightAnchor == StatusBarView.height`, and re-pin `contentContainer.bottomAnchor` to `statusBar.topAnchor`. Then route data:
  - In the existing `usageSummaryStore.onUpdate` closure, instead of (or in addition to) updating the title bar, format the frames to a short string and call `statusBar.updateUsage(text:)`. Build the string from the same `frames` already produced (reuse `UsageSummaryFormatter`); a minimal mapping is acceptable — concatenate each frame's leading + trailing text.
  - In `updateNotificationSummary` call sites, call `statusBar.updateNotification(text: unreadCount > 0 ? "\(unreadCount) unread" : "")`.

- [ ] **Step 6: Build**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build`
Expected: BUILD SUCCEEDED.

- [ ] **Step 7: Commit**

```bash
git add Sources/UI/StatusBar/StatusBarView.swift Sources/App/MainWindowController.swift Tests/StatusBarViewTests.swift
git commit -m "feat: add bottom global status bar"
```

---

## Task 8: Inline create-worktree input (replaces NewBranchDialog)

**Files:**
- Create: `Sources/UI/Dashboard/InlineWorktreeCreateView.swift`
- Modify: `Sources/UI/Dashboard/StackedMiniCardContainerView.swift` (host the input sticky at the sidebar bottom) — confirm exact host by reading the sidebar/right-column container; if the right column is built in `DashboardViewController`, host it there instead.
- Modify: `Sources/App/MainWindowController.swift` (remove the title-bar new-worktree/add-repo buttons' wiring; route inline create through `TabCoordinator.handleNewBranch`)
- Modify: `Sources/UI/TitleBar/TitleBarView.swift` (remove `addProjectButton` + `newWorktreeButton`)
- Test: `Tests/InlineWorktreeCreateViewTests.swift`

The view is one line by default (name field). On focus it expands to two lines: line 2 = repo popup + "reuse environment" checkbox. Enter submits via a callback `onCreate(name, repoPath, reuseEnv)`. Base branch defaults to `main`/`master` (resolved by the handler). Submitting calls `WorktreeCreator.createWorktree` then, if `reuseEnv`, `WorktreeCreator.copyEnvironmentFiles(from: currentWorktreePath, to: newPath)`.

- [ ] **Step 1: Write the failing test**

```swift
import XCTest
@testable import amux

final class InlineWorktreeCreateViewTests: XCTestCase {
    func testSubmitInvokesCallbackWithValues() {
        let view = InlineWorktreeCreateView()
        view.configure(repoPaths: ["/Users/me/repoA", "/Users/me/repoB"])
        var captured: (String, String, Bool)?
        view.onCreate = { name, repo, reuse in captured = (name, repo, reuse) }

        view.setNameForTesting("feature-x")
        view.setReuseEnvForTesting(true)
        view.submitForTesting()

        XCTAssertEqual(captured?.0, "feature-x")
        XCTAssertEqual(captured?.1, "/Users/me/repoA")  // first repo default
        XCTAssertEqual(captured?.2, true)
    }

    func testBlankNameDoesNotSubmit() {
        let view = InlineWorktreeCreateView()
        view.configure(repoPaths: ["/r"])
        var called = false
        view.onCreate = { _, _, _ in called = true }
        view.setNameForTesting("   ")
        view.submitForTesting()
        XCTAssertFalse(called)
    }

    func testExpandedStateTogglesOnFocus() {
        let view = InlineWorktreeCreateView()
        view.configure(repoPaths: ["/r"])
        XCTAssertFalse(view.isExpandedForTesting)
        view.setExpandedForTesting(true)
        XCTAssertTrue(view.isExpandedForTesting)
    }
}
```

- [ ] **Step 2: Run the test, verify it fails**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/InlineWorktreeCreateViewTests`
Expected: FAIL — `cannot find 'InlineWorktreeCreateView' in scope`.

- [ ] **Step 3: Create `Sources/UI/Dashboard/InlineWorktreeCreateView.swift`**

```swift
import AppKit

/// Sticky single-line worktree creator at the bottom of the sidebar. Expands to
/// a second row (repo popup + reuse-environment toggle) on focus.
final class InlineWorktreeCreateView: NSView, NSTextFieldDelegate {
    /// (name, repoPath, reuseEnvironment)
    var onCreate: ((String, String, Bool) -> Void)?

    private let nameField = NSTextField()
    private let repoPopup = NSPopUpButton()
    private let reuseEnvCheckbox = NSButton(checkboxWithTitle: "Reuse env", target: nil, action: nil)
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

        secondRowHeight = secondRow.heightAnchor.constraint(equalToConstant: 0)

        NSLayoutConstraint.activate([
            nameField.topAnchor.constraint(equalTo: topAnchor, constant: 6),
            nameField.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 8),
            nameField.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -8),

            secondRow.topAnchor.constraint(equalTo: nameField.bottomAnchor, constant: 6),
            secondRow.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 8),
            secondRow.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -8),
            secondRow.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -6),
            secondRowHeight,
        ])
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
        nameField.stringValue = ""
    }

    // Expand on focus, collapse when the field loses focus while empty.
    func controlTextDidBeginEditing(_ obj: Notification) { setExpanded(true) }
    func controlTextDidEndEditing(_ obj: Notification) {
        if nameField.stringValue.trimmingCharacters(in: .whitespaces).isEmpty { setExpanded(false) }
    }
}
```

- [ ] **Step 4: Run the test, verify it passes**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/InlineWorktreeCreateViewTests`
Expected: PASS (3 tests).

- [ ] **Step 5: Host the view + wire creation.** Read `StackedMiniCardContainerView.swift` and the right-column construction in `DashboardViewController.swift`. Add an `InlineWorktreeCreateView` pinned to the bottom of the right column (above the window's status bar), with the scrolling card list pinned above it. Wire its `onCreate`:

```swift
inlineCreateView.configure(repoPaths: config.workspacePaths)
inlineCreateView.onCreate = { [weak self] name, repoPath, reuseEnv in
    guard let self else { return }
    let currentPath = self.tabCoordinator.selectedAgent?.worktreePath
    DispatchQueue.global(qos: .userInitiated).async {
        let base = WorktreeCreator.listBranches(repoPath: repoPath).contains("main") ? "main" : "master"
        do {
            let info = try WorktreeCreator.createWorktree(repoPath: repoPath, branchName: name, baseBranch: base)
            if reuseEnv, let currentPath { WorktreeCreator.copyEnvironmentFiles(from: currentPath, to: info.path) }
            DispatchQueue.main.async { self.tabCoordinator.handleNewBranch(didCreateWorktree: info, inRepo: repoPath) }
        } catch {
            DispatchQueue.main.async { NSSound.beep() }
        }
    }
}
```

> Verify the exact signature of `TabCoordinator.handleNewBranch` by reading it (the design notes `handleNewBranch()` → `integrateNewWorktrees`); adapt the call to match. If the dashboard owns `config`, read it from there; otherwise pass `workspacePaths` in from `MainWindowController`.

- [ ] **Step 6: Remove the dialog entry points.** In `TitleBarView.swift` delete `addProjectButton` and `newWorktreeButton` (and `titleBarDidRequestNewThread`/`titleBarDidRequestAddProject` from the protocol if no longer used elsewhere — grep first). In `MainWindowController.swift` remove `showNewBranchDialog()` wiring tied to those buttons. Leave `NewBranchDialog.swift` on disk (unreferenced) — do not delete in this task to keep the diff focused; note it as dead code in the commit message.

- [ ] **Step 7: Build & run full tests**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build`
Then: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test`
Expected: BUILD SUCCEEDED; all tests PASS. Fix `TabCoordinatorTests`/title-bar tests that referenced removed buttons.

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "feat: inline worktree creation replaces new-branch dialog"
```

---

## Task 9: Manual launch verification

**Files:** none (verification only)

- [ ] **Step 1: Build and launch** using the project's screenshot skill if available (`pmux-screenshot-imessage`) or `xcodebuild ... build` then open the app. Verify visually:
  - Single LeftRight layout; no layout switcher buttons in the title bar.
  - Top capsule shows the focused worktree's title (left) + `—` token placeholder (right).
  - Mini cards are compact: title (1–2 lines), status+duration line, repo · worktree line. No inner scroll.
  - Bottom status bar shows usage on the left and shortcut hints on the right.
  - Right-column bottom has the inline create input; clicking it expands to a second row (repo popup + reuse-env checkbox); typing a name + Enter creates a worktree and focuses it.

- [ ] **Step 2: Commit** any final tweaks discovered during the walkthrough.

---

## Self-Review Notes (for the executing agent)

- **Spec coverage:** §1 layout collapse → Task 5; §2 capsule title+token → Tasks 1,2,6; §3 status bar → Task 7; §4 mini cards → Tasks 2,4; §5 inline create → Tasks 3(env),8; §6 env reuse → Task 3. All covered.
- **Title plumbing caveat:** The capsule (Task 6) resolves the title via `WorktreeTitleResolver`, but the mini card (Task 4) currently uses `lastUserPrompt` as the title locally. If the displayed card title must also use the Claude `summary`, extend Task 4's caller in `DashboardViewController` to resolve via `WorktreeTitleResolver` (off main thread) and pass the result through the existing `lastUserPrompt` argument. This is intentionally deferred to keep the card change non-blocking; raise it during review if the live `summary` on cards is required for v1.
- **`AgentDisplayInfo` fields:** Task 6 assumes `lastUserPrompt` and `branch` are reachable from `tabCoordinator.selectedAgent`. If not, read them from `AgentHead.shared` by worktree path — confirm the accessor during implementation.
- **`TabCoordinator.handleNewBranch` signature:** confirm by reading before wiring Task 8 Step 5.
