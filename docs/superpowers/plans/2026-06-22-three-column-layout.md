# 3-Column Layout Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace amux's multi-mode dashboard with a single fixed 3-column layout — worktree cards + new-task on the left, terminal in the center, and a collapsible Files/Changes panel (reusing the existing inspector content) on the right.

**Architecture:** The app already runs permanently in the `.leftRight` focus layout (`setLayout()` is never called at runtime; grid/topSmall/topLarge are dead code). We restructure `setupLeftRightLayout()` into three columns: move the existing worktree sidebar (mini-card stack + `InlineWorktreeCreateView`) from the right edge to the left, keep `FocusPanelView` in the center, and add a new right column hosting a `WorktreeSidePanelViewController`. The right panel reuses `DiffReviewView` (Changes) and a yazi `TerminalSurface` (Files) — the same content the modal `WorktreeInspectorViewController` shows today. Two title-bar toggles independently collapse the left and right columns. Finally we delete the three dead layout modes and their support code.

**Tech Stack:** Swift 5.10, AppKit, XcodeGen (`project.yml`), XCTest. No new dependencies.

## Global Constraints

- macOS 14.0+, Swift 5.10, AppKit (not SwiftUI) — copied verbatim from CLAUDE.md.
- No external SPM dependencies — pure system frameworks + Ghostty.
- Delegate pattern throughout (not Combine/async-await for UI updates).
- Config decoding uses `decodeIfPresent()` for backward compatibility.
- After editing `project.yml`, regenerate with `xcodegen generate`. New source files under existing `Sources/` globs are picked up automatically; run `xcodegen generate` before building if a new file isn't found.
- Build: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build`
- Test: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test`
- All Ghostty C API calls go through `ghosttyLock`; `TerminalSurface.sendText`/`createEphemeral`/`destroy` already handle this — don't add locking.

---

## Task Ordering & Strategy

Tasks 1–6 deliver the feature (build new, leave dead modes in place). Tasks 7–8 remove the now-dead layout modes. This ordering means the app stays buildable and the 3-column layout is fully working **before** any risky deletion, so a reviewer can approve the feature independently of the cleanup.

A note on existing entanglement: `DashboardViewController` branches on `currentLayout` in many places (`toggleSidebarCollapse`, `previewFocusedCard`, `viewDidAppear`, `viewDidLayout`, D-state nav). Tasks 1–6 keep `currentLayout` as `.leftRight` and do **not** touch grid/topSmall/topLarge code paths. Task 8 removes them.

---

## Task 1: WorktreeSidePanelViewController — skeleton + Changes tab

**Files:**
- Create: `Sources/UI/SidePanel/WorktreeSidePanelViewController.swift`
- Test: `Tests/WorktreeSidePanelViewControllerTests.swift`

**Interfaces:**
- Consumes: `DiffReviewView(worktreePath: String)` (existing, `Sources/UI/Diff/DiffReviewView.swift`); `Theme.background`, `Theme.textSecondary` (existing).
- Produces:
  - `final class WorktreeSidePanelViewController: NSViewController`
  - `enum SidePanelTab: Int { case files = 0; case changes = 1 }`
  - `init(worktreePath: String?, initialTab: SidePanelTab = .changes, makeDiffReviewView: @escaping (String) -> DiffReviewView = { DiffReviewView(worktreePath: $0) }, makeYaziSurface: @escaping (NSView, String) -> Bool = WorktreeSidePanelViewController.defaultYaziLauncher)`
  - `func setWorktree(_ path: String?)`
  - `var selectedTabForTesting: SidePanelTab { get }`
  - `var worktreePathForTesting: String? { get }`
  - `static func defaultYaziLauncher(in container: NSView, worktreePath: String) -> Bool`

This task builds the controller with the segmented control, content view, worktree state, and the **Changes** tab (DiffReviewView). The Files tab (yazi) is added in Task 2. Default `initialTab` is `.changes` so this task is fully testable without yazi.

- [ ] **Step 1: Write the failing test**

```swift
// Tests/WorktreeSidePanelViewControllerTests.swift
import XCTest
import AppKit
@testable import amux

final class WorktreeSidePanelViewControllerTests: XCTestCase {
    // A DiffReviewView is an NSView subclass; we can make a throwaway one per path
    // without touching git by injecting makeDiffReviewView.
    private func makeVC(worktreePath: String?) -> WorktreeSidePanelViewController {
        WorktreeSidePanelViewController(
            worktreePath: worktreePath,
            initialTab: .changes,
            makeDiffReviewView: { _ in DiffReviewView(worktreePath: "/tmp") { GitDiffSnapshot(files: []) } },
            makeYaziSurface: { _, _ in true }
        )
    }

    func testInitHoldsWorktreePath() {
        let vc = makeVC(worktreePath: "/tmp/wt-a")
        vc.loadViewIfNeeded()
        XCTAssertEqual(vc.worktreePathForTesting, "/tmp/wt-a")
        XCTAssertEqual(vc.selectedTabForTesting, .changes)
    }

    func testSetWorktreeUpdatesHeldPath() {
        let vc = makeVC(worktreePath: "/tmp/wt-a")
        vc.loadViewIfNeeded()
        vc.setWorktree("/tmp/wt-b")
        XCTAssertEqual(vc.worktreePathForTesting, "/tmp/wt-b")
    }

    func testNilWorktreeShowsPlaceholder() {
        let vc = makeVC(worktreePath: nil)
        vc.loadViewIfNeeded()
        let hasPlaceholder = vc.view.descendantViews().contains {
            $0.accessibilityIdentifier() == "sidePanel.emptyPlaceholder"
        }
        XCTAssertTrue(hasPlaceholder)
    }
}

// Small helper used by the test (and reused in later tests).
extension NSView {
    func descendantViews() -> [NSView] {
        subviews + subviews.flatMap { $0.descendantViews() }
    }
}
```

> Note: confirm `GitDiffSnapshot(files:)` and `DiffReviewView`'s `loadSnapshot` closure signature by opening `Sources/UI/Diff/DiffReviewView.swift` (constructor is `init(worktreePath:String, loadSnapshot:(() -> GitDiffSnapshot)? = nil)`). If the snapshot initializer differs, adjust the test's injected closure to construct an empty snapshot the way that file does. The production code does NOT depend on this — only the test's injected `makeDiffReviewView`.

- [ ] **Step 2: Run test to verify it fails**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/WorktreeSidePanelViewControllerTests`
Expected: FAIL to compile — `WorktreeSidePanelViewController` undefined.

- [ ] **Step 3: Write minimal implementation**

```swift
// Sources/UI/SidePanel/WorktreeSidePanelViewController.swift
import AppKit

enum SidePanelTab: Int {
    case files = 0
    case changes = 1
}

final class WorktreeSidePanelViewController: NSViewController {
    private var worktreePath: String?
    private var selectedTab: SidePanelTab
    private let makeDiffReviewView: (String) -> DiffReviewView
    private let makeYaziSurface: (NSView, String) -> Bool

    private let segmentedControl = NSSegmentedControl(
        labels: ["Files", "Changes"],
        trackingMode: .selectOne,
        target: nil,
        action: nil
    )
    private let contentView = NSView()
    private var yaziSurface: TerminalSurface?

    var selectedTabForTesting: SidePanelTab { selectedTab }
    var worktreePathForTesting: String? { worktreePath }

    init(
        worktreePath: String?,
        initialTab: SidePanelTab = .changes,
        makeDiffReviewView: @escaping (String) -> DiffReviewView = { DiffReviewView(worktreePath: $0) },
        makeYaziSurface: @escaping (NSView, String) -> Bool = WorktreeSidePanelViewController.defaultYaziLauncher
    ) {
        self.worktreePath = worktreePath
        self.selectedTab = initialTab
        self.makeDiffReviewView = makeDiffReviewView
        self.makeYaziSurface = makeYaziSurface
        super.init(nibName: nil, bundle: nil)
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) not supported") }

    deinit { yaziSurface?.destroy() }

    override func loadView() {
        let root = NSView()
        root.wantsLayer = true
        root.layer?.backgroundColor = Theme.background.cgColor
        root.setAccessibilityIdentifier("sidePanel.view")

        segmentedControl.target = self
        segmentedControl.action = #selector(tabChanged)
        segmentedControl.selectedSegment = selectedTab.rawValue
        segmentedControl.translatesAutoresizingMaskIntoConstraints = false
        root.addSubview(segmentedControl)

        contentView.translatesAutoresizingMaskIntoConstraints = false
        root.addSubview(contentView)

        NSLayoutConstraint.activate([
            segmentedControl.topAnchor.constraint(equalTo: root.topAnchor, constant: 8),
            segmentedControl.leadingAnchor.constraint(equalTo: root.leadingAnchor, constant: 8),
            segmentedControl.trailingAnchor.constraint(lessThanOrEqualTo: root.trailingAnchor, constant: -8),

            contentView.topAnchor.constraint(equalTo: segmentedControl.bottomAnchor, constant: 8),
            contentView.leadingAnchor.constraint(equalTo: root.leadingAnchor),
            contentView.trailingAnchor.constraint(equalTo: root.trailingAnchor),
            contentView.bottomAnchor.constraint(equalTo: root.bottomAnchor),
        ])

        view = root
        rebuildContent()
    }

    func setWorktree(_ path: String?) {
        guard path != worktreePath else { return }
        worktreePath = path
        if isViewLoaded { rebuildContent() }
    }

    @objc private func tabChanged() {
        selectedTab = SidePanelTab(rawValue: segmentedControl.selectedSegment) ?? .changes
        rebuildContent()
    }

    private func rebuildContent() {
        yaziSurface?.destroy()
        yaziSurface = nil
        contentView.subviews.forEach { $0.removeFromSuperview() }

        guard let path = worktreePath else {
            showPlaceholder("No worktree selected", identifier: "sidePanel.emptyPlaceholder")
            return
        }

        switch selectedTab {
        case .files:
            showFilesTab(path) // implemented in Task 2; placeholder for now
        case .changes:
            showChangesTab(path)
        }
    }

    private func showChangesTab(_ path: String) {
        let review = makeDiffReviewView(path)
        review.translatesAutoresizingMaskIntoConstraints = false
        contentView.addSubview(review)
        NSLayoutConstraint.activate([
            review.topAnchor.constraint(equalTo: contentView.topAnchor),
            review.leadingAnchor.constraint(equalTo: contentView.leadingAnchor),
            review.trailingAnchor.constraint(equalTo: contentView.trailingAnchor),
            review.bottomAnchor.constraint(equalTo: contentView.bottomAnchor),
        ])
    }

    // Replaced with the real yazi implementation in Task 2.
    private func showFilesTab(_ path: String) {
        showPlaceholder("Files", identifier: "sidePanel.filesPlaceholder")
    }

    private func showPlaceholder(_ message: String, identifier: String) {
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
            label.leadingAnchor.constraint(greaterThanOrEqualTo: contentView.leadingAnchor, constant: 16),
            label.trailingAnchor.constraint(lessThanOrEqualTo: contentView.trailingAnchor, constant: -16),
        ])
    }

    // Placeholder launcher; real implementation in Task 2.
    static func defaultYaziLauncher(in container: NSView, worktreePath: String) -> Bool {
        false
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/WorktreeSidePanelViewControllerTests`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add Sources/UI/SidePanel/WorktreeSidePanelViewController.swift Tests/WorktreeSidePanelViewControllerTests.swift
git commit -m "feat(ui): WorktreeSidePanelViewController with Changes tab"
```

---

## Task 2: Side panel — Files tab (yazi) + leak-free tab switching

**Files:**
- Modify: `Sources/UI/SidePanel/WorktreeSidePanelViewController.swift` (replace `showFilesTab` and `defaultYaziLauncher`)
- Test: `Tests/WorktreeSidePanelViewControllerTests.swift` (add cases)

**Interfaces:**
- Consumes: `WorktreeInspectorViewController.yaziCommand(yaziPath:configDirectory:)` and `WorktreeInspectorViewController.defaultYaziConfigDirectory()` — both `static` (existing, `Sources/UI/Inspector/WorktreeInspectorViewController.swift`); `ProcessRunner.commandPath(_:)` (existing); `TerminalSurface.createEphemeral(in:workingDirectory:command:)` and `.destroy()` (existing).
- Produces: working Files tab; `makeYaziSurface` closure invoked exactly once per Files rebuild; yazi surface destroyed on rebuild/switch-away.

> `defaultYaziConfigDirectory()` is currently `private static` in `WorktreeInspectorViewController`. Change it to non-private `static` so the side panel can reuse it. (One-word edit: remove `private`.)

- [ ] **Step 1: Write the failing test (add to existing test file)**

```swift
    func testFilesTabUsesInjectedYaziLauncher() {
        var launched = 0
        let vc = WorktreeSidePanelViewController(
            worktreePath: "/tmp/wt-a",
            initialTab: .files,
            makeDiffReviewView: { _ in DiffReviewView(worktreePath: "/tmp") { GitDiffSnapshot(files: []) } },
            makeYaziSurface: { _, _ in launched += 1; return true }
        )
        vc.loadViewIfNeeded()
        XCTAssertEqual(launched, 1)
    }

    func testSwitchingAwayFromFilesDoesNotLaunchYaziAgain() {
        var launched = 0
        let vc = WorktreeSidePanelViewController(
            worktreePath: "/tmp/wt-a",
            initialTab: .files,
            makeDiffReviewView: { _ in DiffReviewView(worktreePath: "/tmp") { GitDiffSnapshot(files: []) } },
            makeYaziSurface: { _, _ in launched += 1; return true }
        )
        vc.loadViewIfNeeded()
        vc.setWorktree("/tmp/wt-b") // still Files tab -> relaunch
        XCTAssertEqual(launched, 2)
    }

    func testFailedYaziShowsMissingMessage() {
        let vc = WorktreeSidePanelViewController(
            worktreePath: "/tmp/wt-a",
            initialTab: .files,
            makeDiffReviewView: { _ in DiffReviewView(worktreePath: "/tmp") { GitDiffSnapshot(files: []) } },
            makeYaziSurface: { _, _ in false }
        )
        vc.loadViewIfNeeded()
        let hasMsg = vc.view.descendantViews().contains {
            $0.accessibilityIdentifier() == "sidePanel.filesMissingYazi"
        }
        XCTAssertTrue(hasMsg)
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/WorktreeSidePanelViewControllerTests/testFilesTabUsesInjectedYaziLauncher`
Expected: FAIL — `defaultYaziLauncher` returns false, injected launcher not yet wired into `showFilesTab` correctly (current placeholder ignores `makeYaziSurface`).

- [ ] **Step 3: Write the implementation**

Replace `showFilesTab(_:)` and `defaultYaziLauncher(...)`:

```swift
    private func showFilesTab(_ path: String) {
        let terminalContainer = NSView()
        terminalContainer.wantsLayer = true
        terminalContainer.layer?.backgroundColor = Theme.background.cgColor
        terminalContainer.setAccessibilityIdentifier("sidePanel.yaziContainer")
        terminalContainer.translatesAutoresizingMaskIntoConstraints = false
        contentView.addSubview(terminalContainer)
        NSLayoutConstraint.activate([
            terminalContainer.topAnchor.constraint(equalTo: contentView.topAnchor),
            terminalContainer.leadingAnchor.constraint(equalTo: contentView.leadingAnchor),
            terminalContainer.trailingAnchor.constraint(equalTo: contentView.trailingAnchor),
            terminalContainer.bottomAnchor.constraint(equalTo: contentView.bottomAnchor),
        ])

        if !makeYaziSurface(terminalContainer, path) {
            terminalContainer.removeFromSuperview()
            showPlaceholder(
                "Yazi is not installed. Install yazi to browse files in this tab.",
                identifier: "sidePanel.filesMissingYazi"
            )
        }
    }

    static func defaultYaziLauncher(in container: NSView, worktreePath: String) -> Bool {
        guard let yaziPath = ProcessRunner.commandPath("yazi") else { return false }
        guard let command = WorktreeInspectorViewController.yaziCommand(
            yaziPath: yaziPath,
            configDirectory: WorktreeInspectorViewController.defaultYaziConfigDirectory()
        ) else { return false }
        let surface = TerminalSurface()
        let started = surface.createEphemeral(in: container, workingDirectory: worktreePath, command: command)
        // NOTE: ownership — see Step 3b.
        if started { container.amux_yaziSurface = surface }
        return started
    }
```

- [ ] **Step 3b: Fix yazi surface ownership so it can be destroyed**

The `defaultYaziLauncher` is static, so the VC must still own the surface to destroy it on rebuild. Change the launcher closure contract to hand the surface back. Replace the `makeYaziSurface` type and default, and the call site:

In the type/init, change `makeYaziSurface: @escaping (NSView, String) -> Bool` to:

```swift
    private let makeYaziSurface: (NSView, String) -> TerminalSurface?
```

and in `init` default param: `makeYaziSurface: @escaping (NSView, String) -> TerminalSurface? = WorktreeSidePanelViewController.defaultYaziLauncher`

Update `showFilesTab`:

```swift
        if let surface = makeYaziSurface(terminalContainer, path) {
            yaziSurface = surface
        } else {
            terminalContainer.removeFromSuperview()
            showPlaceholder(
                "Yazi is not installed. Install yazi to browse files in this tab.",
                identifier: "sidePanel.filesMissingYazi"
            )
        }
```

Update `defaultYaziLauncher` to return `TerminalSurface?`:

```swift
    static func defaultYaziLauncher(in container: NSView, worktreePath: String) -> TerminalSurface? {
        guard let yaziPath = ProcessRunner.commandPath("yazi") else { return nil }
        guard let command = WorktreeInspectorViewController.yaziCommand(
            yaziPath: yaziPath,
            configDirectory: WorktreeInspectorViewController.defaultYaziConfigDirectory()
        ) else { return nil }
        let surface = TerminalSurface()
        guard surface.createEphemeral(in: container, workingDirectory: worktreePath, command: command) else { return nil }
        return surface
    }
```

Update the Task 1 tests' `makeYaziSurface` closures: `{ _, _ in true }` → `{ _, _ in TerminalSurface() }`, `{ _, _ in false }` → `{ _, _ in nil }`, and the counting closures return `TerminalSurface()` / `nil` accordingly. (Constructing a bare `TerminalSurface()` does not start a PTY — `createEphemeral`/`create` does — so it's inert in tests.)

Then make `WorktreeInspectorViewController.defaultYaziConfigDirectory()` non-private:

```swift
    static func defaultYaziConfigDirectory() -> URL {  // was: private static
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/WorktreeSidePanelViewControllerTests`
Expected: PASS (all cases).

- [ ] **Step 5: Commit**

```bash
git add Sources/UI/SidePanel/WorktreeSidePanelViewController.swift Sources/UI/Inspector/WorktreeInspectorViewController.swift Tests/WorktreeSidePanelViewControllerTests.swift
git commit -m "feat(ui): side panel Files tab via reused yazi launcher"
```

---

## Task 3: Restructure the dashboard into 3 columns

**Files:**
- Modify: `Sources/UI/Dashboard/DashboardViewController.swift` — `setupLeftRightLayout()` (lines ~630-699), add properties near lines ~143-150, `LayoutMetrics` (~59-72).

**Interfaces:**
- Consumes: `WorktreeSidePanelViewController` (Task 1/2); `FocusPanelView` (existing).
- Produces: a 3-column `.leftRight` container — left worktree sidebar, center focus panel, right side-panel host. New properties:
  - `let rightColumnContainer = NSView()`
  - `private(set) var sidePanelVC: WorktreeSidePanelViewController`
  - collapse constraints: `leftColumnWidthExpanded/Collapsed`, `rightColumnWidthExpanded/Collapsed` (NSLayoutConstraint?)
  - `private var isLeftColumnCollapsed = false`, `private var isRightColumnCollapsed = false`

This task only restructures the **leftRight** layout. Grid/topSmall/topLarge remain untouched and still build.

- [ ] **Step 1: Add properties**

Near the "Left-Right layout" property group (~line 143):

```swift
    // Right column (3-column layout)
    private let rightColumnContainer = NSView()
    private(set) lazy var sidePanelVC = WorktreeSidePanelViewController(worktreePath: nil)
    private var leftColumnWidthExpanded: NSLayoutConstraint?
    private var leftColumnWidthCollapsed: NSLayoutConstraint?
    private var rightColumnWidthExpanded: NSLayoutConstraint?
    private var rightColumnWidthCollapsed: NSLayoutConstraint?
    private var isLeftColumnCollapsed = false
    private var isRightColumnCollapsed = false
```

- [ ] **Step 2: Add layout metrics**

In `LayoutMetrics` (~line 59):

```swift
        static let leftColumnWidth: CGFloat = 260
        static let rightColumnWidth: CGFloat = 320
        static let columnSpacing: CGFloat = 8
```

- [ ] **Step 3: Rewrite `setupLeftRightLayout()`**

Replace the body of `setupLeftRightLayout()` (lines ~630-699) with a 3-column assembly. Left column = sidebar scroll (mini cards) + inline create. Center = focus panel. Right = `rightColumnContainer` hosting `sidePanelVC.view`.

```swift
    private func setupLeftRightLayout() {
        leftRightContainer.translatesAutoresizingMaskIntoConstraints = false
        leftRightContainer.wantsLayer = true
        leftRightContainer.isHidden = true
        leftRightContainer.setAccessibilityIdentifier("dashboard.layout.left-right")
        leftRightContainer.setAccessibilityElement(true)
        view.addSubview(leftRightContainer)

        // --- Left column: worktree sidebar scroll + inline create ---
        leftRightSidebarScroll.translatesAutoresizingMaskIntoConstraints = false
        leftRightSidebarScroll.hasVerticalScroller = true
        leftRightSidebarScroll.scrollerStyle = .overlay
        leftRightSidebarScroll.drawsBackground = false
        leftRightSidebarScroll.borderType = .noBorder

        leftRightSidebarStack.orientation = .vertical
        leftRightSidebarStack.spacing = 8
        leftRightSidebarStack.alignment = .leading
        leftRightSidebarStack.translatesAutoresizingMaskIntoConstraints = false
        leftRightSidebarScroll.documentView = leftRightSidebarStack
        leftRightContainer.addSubview(leftRightSidebarScroll)

        inlineCreateView.translatesAutoresizingMaskIntoConstraints = false
        leftRightContainer.addSubview(inlineCreateView)
        inlineCreateView.onPreferredHeightChange = { [weak self] height, animated in
            self?.setInlineCreateHeight(height, animated: animated)
        }
        let inlineHeight = inlineCreateView.heightAnchor.constraint(equalToConstant: inlineCreateView.preferredHeight)
        inlineCreateHeightConstraint = inlineHeight

        // --- Center column: focus panel ---
        leftRightFocusPanel.translatesAutoresizingMaskIntoConstraints = false
        leftRightFocusPanel.setCornerMask(
            [.layerMinXMinYCorner, .layerMaxXMinYCorner, .layerMinXMaxYCorner, .layerMaxXMaxYCorner],
            radius: LayoutMetrics.focusPanelCornerRadius
        )
        leftRightContainer.addSubview(leftRightFocusPanel)

        // --- Right column: side panel host ---
        rightColumnContainer.translatesAutoresizingMaskIntoConstraints = false
        rightColumnContainer.wantsLayer = true
        rightColumnContainer.setAccessibilityIdentifier("dashboard.rightColumn")
        leftRightContainer.addSubview(rightColumnContainer)

        addChild(sidePanelVC)
        sidePanelVC.view.translatesAutoresizingMaskIntoConstraints = false
        rightColumnContainer.addSubview(sidePanelVC.view)

        let spacing = LayoutMetrics.columnSpacing
        let edge: CGFloat = 8

        // Fixed widths for side columns; centre fills the gap.
        leftColumnWidthExpanded = leftRightSidebarScroll.widthAnchor.constraint(equalToConstant: LayoutMetrics.leftColumnWidth)
        leftColumnWidthCollapsed = leftRightSidebarScroll.widthAnchor.constraint(equalToConstant: 0)
        rightColumnWidthExpanded = rightColumnContainer.widthAnchor.constraint(equalToConstant: LayoutMetrics.rightColumnWidth)
        rightColumnWidthCollapsed = rightColumnContainer.widthAnchor.constraint(equalToConstant: 0)
        leftColumnWidthExpanded?.isActive = true
        rightColumnWidthExpanded?.isActive = true

        NSLayoutConstraint.activate([
            leftRightContainer.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor, constant: layoutTopInset),
            leftRightContainer.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: LayoutMetrics.containerHorizontalInset),
            leftRightContainer.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -LayoutMetrics.containerHorizontalInset),
            leftRightContainer.bottomAnchor.constraint(equalTo: view.bottomAnchor, constant: -LayoutMetrics.containerBottomInset),

            // Left column
            leftRightSidebarScroll.topAnchor.constraint(equalTo: leftRightContainer.topAnchor),
            leftRightSidebarScroll.leadingAnchor.constraint(equalTo: leftRightContainer.leadingAnchor, constant: edge),
            leftRightSidebarScroll.bottomAnchor.constraint(equalTo: inlineCreateView.topAnchor, constant: -10),

            inlineCreateView.leadingAnchor.constraint(equalTo: leftRightSidebarScroll.leadingAnchor),
            inlineCreateView.trailingAnchor.constraint(equalTo: leftRightSidebarScroll.trailingAnchor),
            inlineCreateView.bottomAnchor.constraint(equalTo: leftRightContainer.bottomAnchor, constant: -8),
            inlineHeight,

            // Centre column
            leftRightFocusPanel.topAnchor.constraint(equalTo: leftRightContainer.topAnchor),
            leftRightFocusPanel.leadingAnchor.constraint(equalTo: leftRightSidebarScroll.trailingAnchor, constant: spacing),
            leftRightFocusPanel.bottomAnchor.constraint(equalTo: leftRightContainer.bottomAnchor),
            leftRightFocusPanel.trailingAnchor.constraint(equalTo: rightColumnContainer.leadingAnchor, constant: -spacing),

            // Right column
            rightColumnContainer.topAnchor.constraint(equalTo: leftRightContainer.topAnchor),
            rightColumnContainer.trailingAnchor.constraint(equalTo: leftRightContainer.trailingAnchor, constant: -edge),
            rightColumnContainer.bottomAnchor.constraint(equalTo: leftRightContainer.bottomAnchor),

            sidePanelVC.view.topAnchor.constraint(equalTo: rightColumnContainer.topAnchor),
            sidePanelVC.view.leadingAnchor.constraint(equalTo: rightColumnContainer.leadingAnchor),
            sidePanelVC.view.trailingAnchor.constraint(equalTo: rightColumnContainer.trailingAnchor),
            sidePanelVC.view.bottomAnchor.constraint(equalTo: rightColumnContainer.bottomAnchor),
        ])
    }
```

> The old `leftRightFocusWidthExpanded/Collapsed` constraints (lines ~696-698) are removed by this rewrite. Task 4 updates `toggleSidebarCollapse()` and `resetSidebarConstraints()` which still reference them — expect a compile error until Task 4. To keep this task independently buildable, in this step also comment out the two `leftRightFocusWidthExpanded`/`leftRightFocusWidthCollapsed` lines in `toggleSidebarCollapse()` (the `.leftRight` case) and `resetSidebarConstraints()`, replacing the `.leftRight` case body with `break`. Task 4 rewrites them properly.

- [ ] **Step 4: Build to verify it compiles**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -20`
Expected: BUILD SUCCEEDED.

- [ ] **Step 5: Run the app to verify columns render**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build` then launch the built app (or use the `pmux-screenshot-imessage` skill / `run` skill). Visually confirm: worktree cards + new-task on the LEFT, terminal in the CENTER, side panel (segmented Files/Changes) on the RIGHT. Side panel may show "No worktree selected" until Task 5 wires selection — that's expected.

- [ ] **Step 6: Commit**

```bash
git add Sources/UI/Dashboard/DashboardViewController.swift
git commit -m "feat(ui): restructure dashboard into 3 columns (worktrees | terminal | side panel)"
```

---

## Task 4: Two independent collapse toggles

**Files:**
- Modify: `Sources/UI/TitleBar/TitleBarView.swift` — protocol (lines 3-7), buttons (lines ~48-50, ~315-352), actions (~457-467).
- Modify: `Sources/App/MainWindowController.swift` — `TitleBarDelegate` conformance (~lines 790-821).
- Modify: `Sources/UI/Dashboard/DashboardViewController.swift` — replace `toggleSidebarCollapse()` and `resetSidebarConstraints()`.

**Interfaces:**
- Consumes: collapse constraints from Task 3.
- Produces:
  - TitleBarDelegate gains `func titleBarDidRequestCollapseLeftColumn()` and `func titleBarDidRequestCollapseRightColumn()`.
  - DashboardViewController gains `func toggleLeftColumnCollapse()` and `func toggleRightColumnCollapse()`.
  - `titleBarDidRequestCollapseSidebar()` is removed (replaced by the two new ones).

- [ ] **Step 1: Add the two toggle methods on DashboardViewController**

Replace `toggleSidebarCollapse()` (lines ~360-390) and `resetSidebarConstraints()` (~415-422) with:

```swift
    func toggleLeftColumnCollapse() {
        isLeftColumnCollapsed.toggle()
        leftColumnWidthExpanded?.isActive = !isLeftColumnCollapsed
        leftColumnWidthCollapsed?.isActive = isLeftColumnCollapsed
        animateColumnLayout {
            self.leftRightSidebarScroll.animator().alphaValue = self.isLeftColumnCollapsed ? 0 : 1
            self.inlineCreateView.animator().alphaValue = self.isLeftColumnCollapsed ? 0 : 1
        }
    }

    func toggleRightColumnCollapse() {
        isRightColumnCollapsed.toggle()
        rightColumnWidthExpanded?.isActive = !isRightColumnCollapsed
        rightColumnWidthCollapsed?.isActive = isRightColumnCollapsed
        animateColumnLayout {
            self.rightColumnContainer.animator().alphaValue = self.isRightColumnCollapsed ? 0 : 1
        }
    }

    private func animateColumnLayout(_ extra: @escaping () -> Void) {
        NSAnimationContext.runAnimationGroup { context in
            context.duration = 0.2
            context.allowsImplicitAnimation = true
            extra()
            self.view.layoutSubtreeIfNeeded()
        }
    }

    private func resetSidebarConstraints() {
        leftColumnWidthExpanded?.isActive = true
        leftColumnWidthCollapsed?.isActive = false
        rightColumnWidthExpanded?.isActive = true
        rightColumnWidthCollapsed?.isActive = false
    }
```

> If `setLayout()` (which calls `resetSidebarConstraints()` and `isSidebarCollapsed`) still references `isSidebarCollapsed`, change that line to reset both new flags: `isLeftColumnCollapsed = false; isRightColumnCollapsed = false`. (Task 8 removes `setLayout()` entirely; this keeps it compiling until then.)

- [ ] **Step 2: Update TitleBarView protocol + buttons**

In `TitleBarView.swift`, change the protocol (lines 3-7):

```swift
protocol TitleBarDelegate: AnyObject {
    func titleBarDidToggleTheme()
    func titleBarDidRequestCollapseLeftColumn()
    func titleBarDidRequestCollapseRightColumn()
    func titleBarDidRequestCleanMergedWorktrees()
}
```

Add a button property next to `collapseSidebarButton` (~line 48-50):

```swift
    private let collapseLeftButton = NSButton()
    private let collapseRightButton = NSButton()
```

Replace the single collapse button configuration (~lines 331-334) with two:

```swift
        configureArcIconButton(collapseLeftButton, symbol: "sidebar.left",
                               identifier: "titlebar.collapseLeft", label: "Toggle Worktrees",
                               action: #selector(collapseLeftClicked))
        actionStack.addArrangedSubview(collapseLeftButton)

        configureArcIconButton(collapseRightButton, symbol: "sidebar.right",
                               identifier: "titlebar.collapseRight", label: "Toggle Side Panel",
                               action: #selector(collapseRightClicked))
        actionStack.addArrangedSubview(collapseRightButton)
```

Replace the old `collapseSidebarClicked` action (~line 461) with:

```swift
    @objc private func collapseLeftClicked() { delegate?.titleBarDidRequestCollapseLeftColumn() }
    @objc private func collapseRightClicked() { delegate?.titleBarDidRequestCollapseRightColumn() }
```

> Remove the `private let collapseSidebarButton = NSButton()` property and any leftover reference to it (e.g. in `updateChromeState`, which previously disabled it in grid mode — that whole grid-disable concern goes away; delete that line).

- [ ] **Step 3: Update MainWindowController delegate**

Replace `titleBarDidRequestCollapseSidebar()` (~line 814-816) with:

```swift
    func titleBarDidRequestCollapseLeftColumn() {
        tabCoordinator.dashboardVC?.toggleLeftColumnCollapse()
    }

    func titleBarDidRequestCollapseRightColumn() {
        tabCoordinator.dashboardVC?.toggleRightColumnCollapse()
    }
```

- [ ] **Step 4: Build**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -20`
Expected: BUILD SUCCEEDED.

- [ ] **Step 5: Run the app and verify both toggles**

Launch the app. Click the left-sidebar icon → worktree column collapses/expands; click the right-sidebar icon → side panel column collapses/expands. Center terminal grows/shrinks to fill. Confirm they are independent.

- [ ] **Step 6: Commit**

```bash
git add Sources/UI/TitleBar/TitleBarView.swift Sources/App/MainWindowController.swift Sources/UI/Dashboard/DashboardViewController.swift
git commit -m "feat(ui): independent collapse toggles for worktree and side-panel columns"
```

---

## Task 5: Wire side panel to the focused worktree

**Files:**
- Modify: `Sources/UI/Dashboard/DashboardViewController.swift` — call `sidePanelVC.setWorktree(...)` wherever the selected worktree changes.

**Interfaces:**
- Consumes: `sidePanelVC.setWorktree(_:)` (Task 1); `agents`, `selectedAgentId`, `worktreePath` (existing).
- Produces: side panel always reflects the selected worktree.

- [ ] **Step 1: Add a helper + call sites**

Add a private helper:

```swift
    private func syncSidePanelToSelection() {
        let path = agents.first(where: { $0.id == selectedAgentId })?.worktreePath
        sidePanelVC.setWorktree(path)
    }
```

Call `syncSidePanelToSelection()` at the end of each of these existing methods (they all change the visible worktree):
- `selectAgent(byWorktreePath:)` (~line 346)
- `embedSplitContainerForSelectedAgent(focusTerminal:)` (~line 905) — at the end, after embedding
- `agentCardClicked(agentId:)` (~line 1307) — in the `default:` branch after `embedSplitContainerForSelectedAgent()`
- `updateAgents(_:)` (~line 196) — after the selectedAgentId validation block (~line 233), so the panel is populated on first load and stays valid when worktrees are added/removed.

Example for `updateAgents`, after the `if !agents.contains(where:...)` block:

```swift
        syncSidePanelToSelection()
```

- [ ] **Step 2: Build**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -20`
Expected: BUILD SUCCEEDED.

- [ ] **Step 3: Run the app and verify scoping**

Launch with ≥2 worktrees. Click worktree A → side panel "Changes" shows A's git changes; switch to "Files" → yazi rooted at A. Click worktree B → both tabs now reflect B. Confirm the center terminal and right panel are scoped to the same worktree.

- [ ] **Step 4: Commit**

```bash
git add Sources/UI/Dashboard/DashboardViewController.swift
git commit -m "feat(ui): scope side panel to the focused worktree"
```

---

## Task 6: Default the runtime layout and verify end-to-end

**Files:**
- Modify: `Sources/UI/Dashboard/DashboardViewController.swift` — ensure `currentLayout` initial value is `.leftRight` (already is, line ~102) and the empty-state path still works.

**Interfaces:** none new — this is a verification + small-fix task.

- [ ] **Step 1: Confirm initial layout**

Confirm `var currentLayout: DashboardLayout = .leftRight` (line ~102) is unchanged. No code change unless it differs.

- [ ] **Step 2: Verify empty-state interaction**

In `updateAgents` (lines ~217-228), the empty-state branch hides all layout containers and returns early before `syncSidePanelToSelection()`. Add a `sidePanelVC.setWorktree(nil)` call inside the `if agents.isEmpty` branch so the panel clears (and destroys any yazi surface) when no worktrees exist:

```swift
        if agents.isEmpty {
            emptyStateView.isHidden = false
            showLayout(currentLayout)
            gridScrollView.isHidden = true
            leftRightContainer.isHidden = true
            topSmallContainer.isHidden = true
            topLargeContainer.isHidden = true
            sidePanelVC.setWorktree(nil)
            return
        }
```

- [ ] **Step 3: Build + full test run**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -5`
Then: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test 2>&1 | tail -30`
Expected: BUILD SUCCEEDED; tests pass except any that reference removed layout switching (handled in Task 7).

- [ ] **Step 4: Commit**

```bash
git add Sources/UI/Dashboard/DashboardViewController.swift
git commit -m "feat(ui): clear side panel in empty state; confirm leftRight as the layout"
```

---

## Task 7: Update tests for removed layout switching

**Files:**
- Modify: `Tests/ConfigTests.swift` (~lines 279-281, `testDefaultDashboardLayout`)
- Modify: `Tests/DashboardViewControllerClickTests.swift` (cases that set `vc.currentLayout = .grid`)

**Interfaces:** none — test maintenance ahead of the Task 8 deletions.

- [ ] **Step 1: Inspect the affected tests**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/ConfigTests -only-testing:amuxTests/DashboardViewControllerClickTests 2>&1 | tail -40`
Note which assertions depend on `.grid`/`.topSmall`/`.topLarge` or the `dashboardLayout` default.

- [ ] **Step 2: Update `ConfigTests.testDefaultDashboardLayout`**

The `config.dashboardLayout` field is being removed in Task 8. Delete `testDefaultDashboardLayout` (the field and its persistence are removed). If other ConfigTests reference `dashboardLayout`, remove those references.

- [ ] **Step 3: Update `DashboardViewControllerClickTests`**

Remove or rewrite cases that set `vc.currentLayout = .grid` (grid is being removed). Keep coverage for the leftRight click behavior (`agentCardClicked` → selection + embed). For any test that exercised grid-specific reorder/drag, delete it — that behavior no longer exists.

- [ ] **Step 4: Run the updated tests**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/ConfigTests -only-testing:amuxTests/DashboardViewControllerClickTests 2>&1 | tail -20`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add Tests/ConfigTests.swift Tests/DashboardViewControllerClickTests.swift
git commit -m "test: drop assertions for removed dashboard layout modes"
```

---

## Task 8: Remove dead layout modes (grid / topSmall / topLarge)

**Files:**
- Modify: `Sources/UI/Dashboard/DashboardViewController.swift` (largest change)
- Modify: `Sources/UI/TitleBar/LayoutPopoverView.swift` (or delete if fully unused)
- Modify: `Sources/UI/Dashboard/DashboardFocusController.swift` (simplify mode)
- Modify: `Sources/Core/Config.swift` (remove `dashboardLayout` field + coding key + default + decode)
- Possibly delete: `Sources/UI/Dashboard/DraggableGridView.swift` and grid-only helpers (only if no longer referenced)

**Interfaces:** removal only. After this task, `DashboardViewController` has a single layout and no `DashboardLayout` enum branching.

> This is the riskiest task — do it last, in small compiling increments, building after each. The compiler is your guide: remove a symbol, build, fix the next error.

- [ ] **Step 1: Collapse `DashboardLayout` to a single concept**

Remove the `DashboardLayout` enum (`LayoutPopoverView.swift`). Replace all `currentLayout`/`lastFocusLayout` reads in `DashboardViewController` — since there's only one layout now, delete the `currentLayout` property and the guards that compare against it. Concretely:
- Delete `var currentLayout`, `lastFocusLayout`, `setLayout(_:)`, `showLayout(_:)`, `rebuildCurrentLayout()` switch arms for non-leftRight.
- `focusLayoutRefs(for:)` → make it a single `focusLayoutRefs()` returning the leftRight refs directly (no switch).
- Remove `case .grid`/`.topSmall`/`.topLarge` arms throughout (`toggle...`, `previewFocusedCard`, `viewDidAppear`, `viewDidLayout`, `applyKeyboardFocusVisuals`, `dispatch`, `agentCardClicked`, `miniCardReorderEnded`).

Build after this step; fix errors until BUILD SUCCEEDED.

- [ ] **Step 2: Remove grid view hierarchy + setup**

Delete `setupGridLayout()`, `rebuildGrid()`, `currentGridLayout`, `layoutGridFrames()`, `gridScrollView`, `gridContainer`, `gridCards`, `setupTopSmallLayout()`, `setupTopLargeLayout()`, `topSmall*`/`topLarge*` properties, and the corresponding `LayoutMetrics` constants. Remove their calls in `loadView()`. Remove the `GridCardReorderDelegate` and `DraggableGridDelegate` conformances/extensions if grid is gone.

Build; fix errors.

- [ ] **Step 3: Delete now-unused files**

If `DraggableGridView.swift`, `StackedCardContainerView` (grid card), `GridLayout`, and `LayoutPopoverView.swift` are no longer referenced (grep to confirm), delete them. Run `xcodegen generate` if you removed files referenced by `project.yml`.

Run: `grep -rn "DraggableGridView\|GridLayout\|LayoutPopoverView\|DashboardLayout" Sources Tests`
Expected: no hits (or only in files you're about to delete).

- [ ] **Step 4: Simplify `DashboardFocusController`**

If `enterGrid` / grid `Mode` are now unused (grep to confirm no callers), remove them, keeping `enterFocusLayout`, `move`, `jump`, `exit`, `removeCurrentCard`, `refreshCards`, `captureSnapshot`. Keep the `Snapshot` struct but drop its `layout` field if nothing reads it.

Run: `grep -rn "enterGrid\|\.grid" Sources/UI/Dashboard/DashboardFocusController.swift Sources/UI/Dashboard/DashboardViewController.swift`
Expected: no remaining grid references.

- [ ] **Step 5: Remove `Config.dashboardLayout`**

In `Config.swift` remove: the `dashboardLayout` stored property (line ~13), its `CodingKeys` case (~line 34), its default in `init` (~line 56), and its `decodeIfPresent` line (~line 84). (Backward compat is preserved — an old config file with a `dashboardLayout` key is simply ignored by the decoder.)

- [ ] **Step 6: Full build + full test suite**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -5`
Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test 2>&1 | tail -30`
Expected: BUILD SUCCEEDED; ALL tests pass.

- [ ] **Step 7: Run the app — final smoke test**

Launch. Confirm: 3 columns render, worktree selection scopes center + right panel, both collapse toggles work, new-task creator works, no console errors about missing layouts.

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "refactor(ui): remove dead grid/topSmall/topLarge layout modes"
```

---

## Self-Review Notes (for the executor)

- **Spec coverage:** Col 1 worktrees+new-task moved left (Task 3); Col 2 terminal center (Task 3); Col 3 Files/Changes toggle reusing inspector content (Tasks 1–2); both columns collapsible (Task 4); worktree-scoped (Task 5); replace all modes (Tasks 6–8); tests (Tasks 1,2,7); cleanup (Task 8).
- **Verify-before-claiming:** every task ends in either a passing test command (1,2,7) or a build + manual run (3,4,5,6,8). Do not check a step's box until its command output confirms success.
- **If a referenced line number has drifted,** locate the symbol by name (grep) rather than trusting the number — the codebase may have changed since this plan was written (2026-06-22).
