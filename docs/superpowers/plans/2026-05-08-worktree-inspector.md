# Worktree Inspector Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a read-only Worktree Inspector opened from mini-card and grid-card right-click menus, with Yazi-backed project browsing and native git change review.

**Architecture:** Card container views own the context-menu actions and forward them through existing delegate layers. `MainWindowController` presents a new `WorktreeInspectorViewController`; the inspector hosts a temporary non-agent `TerminalSurface` for Yazi and a reusable native `DiffReviewView` for changes. Git parsing stays in `Sources/Git/GitDiff.swift`.

**Tech Stack:** Swift 5.10, AppKit, Ghostty `TerminalSurface`, Yazi as optional runtime command, `/usr/bin/git`, XCTest, `xcodebuild`.

---

## File Structure

- Modify `Sources/UI/Dashboard/AgentCardView.swift`
  - Extend `AgentCardDelegate` with optional inspector actions.
- Modify `Sources/UI/Dashboard/StackedMiniCardContainerView.swift`
  - Add `Browse Files...` and `Show Changes...` to the focus-layout mini-card context menu.
- Modify `Sources/UI/Dashboard/StackedCardContainerView.swift`
  - Add the same context-menu actions to grid cards.
- Modify `Sources/UI/Dashboard/DashboardViewController.swift`
  - Resolve right-clicked `agentId` to `worktreePath` and forward to `DashboardDelegate`.
- Modify `Sources/App/MainWindowController.swift`
  - Present `WorktreeInspectorViewController` from dashboard delegate callbacks.
- Create `Sources/UI/Inspector/WorktreeInspectorViewController.swift`
  - Own the inspector sheet, tab selector, Yazi Files tab, and embedded Changes tab.
- Create `Sources/UI/Diff/DiffReviewView.swift`
  - Reusable native changed-files tree and diff renderer, extracted from the current diff overlay behavior.
- Modify `Sources/UI/Diff/DiffOverlayViewController.swift`
  - Reuse `DiffReviewView` so the legacy menu action and new inspector share rendering.
- Modify `Sources/Git/GitDiff.swift`
  - Add structured snapshot data for staged, unstaged, untracked, renamed, binary, and oversized cases.
- Modify `Sources/Terminal/TerminalSurface.swift`
  - Add a command-based, non-persistent creation method for Yazi.
- Modify tests:
  - `Tests/StackedMiniCardContainerViewTests.swift`
  - `Tests/StackedCardContainerDoubleClickTests.swift`
  - `Tests/DashboardViewControllerClickTests.swift`
  - `Tests/GitDiffTests.swift`
  - Create `Tests/WorktreeInspectorViewControllerTests.swift`
  - Create `Tests/DiffReviewViewTests.swift`

No `project.yml` change is required because it includes `Sources` and `Tests` as grouped source roots.

---

### Task 1: Add Card Context Menu Actions

**Files:**
- Modify: `Sources/UI/Dashboard/AgentCardView.swift`
- Modify: `Sources/UI/Dashboard/StackedMiniCardContainerView.swift`
- Modify: `Sources/UI/Dashboard/StackedCardContainerView.swift`
- Test: `Tests/StackedMiniCardContainerViewTests.swift`
- Test: `Tests/StackedCardContainerDoubleClickTests.swift`

- [ ] **Step 1: Write the failing tests**

Add these tests to `Tests/StackedMiniCardContainerViewTests.swift`:

```swift
func testContextMenuContainsInspectorActions() {
    let container = StackedMiniCardContainerView()
    let menu = container.menu(for: makeRightClickEvent())
    let titles = menu?.items.map(\.title) ?? []

    XCTAssertTrue(titles.contains("Browse Files..."))
    XCTAssertTrue(titles.contains("Show Changes..."))
    XCTAssertLessThan(
        titles.firstIndex(of: "Show Changes...") ?? Int.max,
        titles.firstIndex(of: "Delete Worktree") ?? Int.max
    )
}

func testBrowseFilesMenuItemForwardsAgentId() {
    let container = StackedMiniCardContainerView()
    let spy = InspectorDelegateSpy()
    container.delegate = spy
    container.miniCardView.configure(
        id: "agent-1",
        project: "proj",
        thread: "main",
        status: "idle",
        lastMessage: "",
        totalDuration: "",
        roundDuration: ""
    )

    let item = container.menu(for: makeRightClickEvent())?.items.first { $0.title == "Browse Files..." }
    XCTAssertNotNil(item)
    _ = item?.target?.perform(item?.action, with: item)

    XCTAssertEqual(spy.browseIds, ["agent-1"])
    XCTAssertTrue(spy.changeIds.isEmpty)
}

func testShowChangesMenuItemForwardsAgentId() {
    let container = StackedMiniCardContainerView()
    let spy = InspectorDelegateSpy()
    container.delegate = spy
    container.miniCardView.configure(
        id: "agent-2",
        project: "proj",
        thread: "branch",
        status: "idle",
        lastMessage: "",
        totalDuration: "",
        roundDuration: ""
    )

    let item = container.menu(for: makeRightClickEvent())?.items.first { $0.title == "Show Changes..." }
    XCTAssertNotNil(item)
    _ = item?.target?.perform(item?.action, with: item)

    XCTAssertEqual(spy.changeIds, ["agent-2"])
    XCTAssertTrue(spy.browseIds.isEmpty)
}

private func makeRightClickEvent() -> NSEvent {
    NSEvent.mouseEvent(
        with: .rightMouseDown,
        location: .zero,
        modifierFlags: [],
        timestamp: 0,
        windowNumber: 0,
        context: nil,
        eventNumber: 0,
        clickCount: 1,
        pressure: 1
    )!
}

private final class InspectorDelegateSpy: AgentCardDelegate {
    var browseIds: [String] = []
    var changeIds: [String] = []

    func agentCardClicked(agentId: String) {}
    func agentCardDidRequestBrowseFiles(agentId: String) { browseIds.append(agentId) }
    func agentCardDidRequestShowChanges(agentId: String) { changeIds.append(agentId) }
}
```

Add these tests to `Tests/StackedCardContainerDoubleClickTests.swift`:

```swift
func testGridCardContextMenuContainsInspectorActions() {
    let container = StackedCardContainerView(frame: NSRect(x: 0, y: 0, width: 200, height: 150))
    let menu = container.menu(for: makeRightClickEvent())
    let titles = menu?.items.map(\.title) ?? []

    XCTAssertTrue(titles.contains("Browse Files..."))
    XCTAssertTrue(titles.contains("Show Changes..."))
    XCTAssertLessThan(
        titles.firstIndex(of: "Show Changes...") ?? Int.max,
        titles.firstIndex(of: "Delete Worktree") ?? Int.max
    )
}

func testGridCardBrowseFilesMenuItemForwardsAgentId() {
    let container = StackedCardContainerView(frame: NSRect(x: 0, y: 0, width: 200, height: 150))
    let spy = DelegateSpy()
    container.delegate = spy
    container.cardView.configure(
        id: "grid-agent-1",
        project: "proj",
        thread: "main",
        status: "idle",
        lastMessage: "",
        totalDuration: "",
        roundDuration: ""
    )

    let item = container.menu(for: makeRightClickEvent())?.items.first { $0.title == "Browse Files..." }
    XCTAssertNotNil(item)
    _ = item?.target?.perform(item?.action, with: item)

    XCTAssertEqual(spy.browseIds, ["grid-agent-1"])
}

func testGridCardShowChangesMenuItemForwardsAgentId() {
    let container = StackedCardContainerView(frame: NSRect(x: 0, y: 0, width: 200, height: 150))
    let spy = DelegateSpy()
    container.delegate = spy
    container.cardView.configure(
        id: "grid-agent-2",
        project: "proj",
        thread: "feature",
        status: "idle",
        lastMessage: "",
        totalDuration: "",
        roundDuration: ""
    )

    let item = container.menu(for: makeRightClickEvent())?.items.first { $0.title == "Show Changes..." }
    XCTAssertNotNil(item)
    _ = item?.target?.perform(item?.action, with: item)

    XCTAssertEqual(spy.changeIds, ["grid-agent-2"])
}

private func makeRightClickEvent() -> NSEvent {
    NSEvent.mouseEvent(
        with: .rightMouseDown,
        location: .zero,
        modifierFlags: [],
        timestamp: 0,
        windowNumber: 0,
        context: nil,
        eventNumber: 0,
        clickCount: 1,
        pressure: 1
    )!
}
```

Extend the existing `DelegateSpy` in `Tests/StackedCardContainerDoubleClickTests.swift`:

```swift
private class DelegateSpy: AgentCardDelegate {
    var clickedIds: [String] = []
    var doubleClickedIds: [String] = []
    var browseIds: [String] = []
    var changeIds: [String] = []

    func agentCardClicked(agentId: String) { clickedIds.append(agentId) }
    func agentCardDoubleClicked(agentId: String) { doubleClickedIds.append(agentId) }
    func agentCardDidRequestBrowseFiles(agentId: String) { browseIds.append(agentId) }
    func agentCardDidRequestShowChanges(agentId: String) { changeIds.append(agentId) }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/StackedMiniCardContainerViewTests/testContextMenuContainsInspectorActions -only-testing:amuxTests/StackedCardContainerDoubleClickTests/testGridCardContextMenuContainsInspectorActions
```

Expected: FAIL because the new menu items and delegate methods do not exist.

- [ ] **Step 3: Implement the delegate and menu actions**

In `Sources/UI/Dashboard/AgentCardView.swift`, change the delegate protocol and defaults to:

```swift
protocol AgentCardDelegate: AnyObject {
    func agentCardClicked(agentId: String)
    func agentCardDoubleClicked(agentId: String)
    func agentCardDidRequestDelete(agentId: String)
    func agentCardDidRequestCloseRepo(agentId: String)
    func agentCardDidRequestBrowseFiles(agentId: String)
    func agentCardDidRequestShowChanges(agentId: String)
}

extension AgentCardDelegate {
    func agentCardDoubleClicked(agentId: String) {}
    func agentCardDidRequestDelete(agentId: String) {}
    func agentCardDidRequestCloseRepo(agentId: String) {}
    func agentCardDidRequestBrowseFiles(agentId: String) {}
    func agentCardDidRequestShowChanges(agentId: String) {}
}
```

In `Sources/UI/Dashboard/StackedMiniCardContainerView.swift`, replace `menu(for:)` with:

```swift
override func menu(for event: NSEvent) -> NSMenu? {
    let menu = NSMenu()

    let browseItem = NSMenuItem(title: "Browse Files...", action: #selector(browseFilesAction), keyEquivalent: "")
    browseItem.target = self
    menu.addItem(browseItem)

    let changesItem = NSMenuItem(title: "Show Changes...", action: #selector(showChangesAction), keyEquivalent: "")
    changesItem.target = self
    menu.addItem(changesItem)

    menu.addItem(NSMenuItem.separator())

    let deleteItem = NSMenuItem(title: "Delete Worktree", action: #selector(deleteWorktreeAction), keyEquivalent: "")
    deleteItem.target = self
    menu.addItem(deleteItem)

    menu.addItem(NSMenuItem.separator())

    let closeRepoItem = NSMenuItem(title: "Close Repo", action: #selector(closeRepoAction), keyEquivalent: "")
    closeRepoItem.target = self
    menu.addItem(closeRepoItem)

    return menu
}

@objc private func browseFilesAction() {
    delegate?.agentCardDidRequestBrowseFiles(agentId: miniCardView.agentId)
}

@objc private func showChangesAction() {
    delegate?.agentCardDidRequestShowChanges(agentId: miniCardView.agentId)
}
```

In `Sources/UI/Dashboard/StackedCardContainerView.swift`, replace `menu(for:)` with:

```swift
override func menu(for event: NSEvent) -> NSMenu? {
    let menu = NSMenu()

    let browseItem = NSMenuItem(title: "Browse Files...", action: #selector(browseFilesAction), keyEquivalent: "")
    browseItem.target = self
    menu.addItem(browseItem)

    let changesItem = NSMenuItem(title: "Show Changes...", action: #selector(showChangesAction), keyEquivalent: "")
    changesItem.target = self
    menu.addItem(changesItem)

    menu.addItem(NSMenuItem.separator())

    let deleteItem = NSMenuItem(title: "Delete Worktree", action: #selector(deleteWorktreeAction), keyEquivalent: "")
    deleteItem.target = self
    menu.addItem(deleteItem)

    menu.addItem(NSMenuItem.separator())

    let closeRepoItem = NSMenuItem(title: "Close Repo", action: #selector(closeRepoAction), keyEquivalent: "")
    closeRepoItem.target = self
    menu.addItem(closeRepoItem)

    return menu
}

@objc private func browseFilesAction() {
    delegate?.agentCardDidRequestBrowseFiles(agentId: cardView.agentId)
}

@objc private func showChangesAction() {
    delegate?.agentCardDidRequestShowChanges(agentId: cardView.agentId)
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run:

```bash
xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/StackedMiniCardContainerViewTests -only-testing:amuxTests/StackedCardContainerDoubleClickTests
```

Expected: PASS for both test classes.

- [ ] **Step 5: Commit**

```bash
git add Sources/UI/Dashboard/AgentCardView.swift Sources/UI/Dashboard/StackedMiniCardContainerView.swift Sources/UI/Dashboard/StackedCardContainerView.swift Tests/StackedMiniCardContainerViewTests.swift Tests/StackedCardContainerDoubleClickTests.swift
git commit -m "feat: add inspector card menu actions"
```

---

### Task 2: Route Card Requests To Worktree Paths And Present Inspector Shell

**Files:**
- Modify: `Sources/UI/Dashboard/DashboardViewController.swift`
- Modify: `Sources/App/MainWindowController.swift`
- Create: `Sources/UI/Inspector/WorktreeInspectorViewController.swift`
- Test: `Tests/DashboardViewControllerClickTests.swift`
- Create: `Tests/WorktreeInspectorViewControllerTests.swift`

- [ ] **Step 1: Write the failing dashboard routing tests**

Add these tests to `Tests/DashboardViewControllerClickTests.swift`:

```swift
func testBrowseFilesRequestUsesRightClickedAgentWorktreePath() {
    let vc = DashboardViewController()
    let spy = DashboardDelegateSpy()
    vc.dashboardDelegate = spy
    vc.loadViewIfNeeded()
    vc.updateAgents([
        makeAgent(id: "agent-a", worktreePath: "/repo/a"),
        makeAgent(id: "agent-b", worktreePath: "/repo/b"),
    ])
    vc.agentCardClicked(agentId: "agent-a")

    vc.agentCardDidRequestBrowseFiles(agentId: "agent-b")

    XCTAssertEqual(spy.browsePath, "/repo/b")
    XCTAssertNil(spy.changesPath)
}

func testShowChangesRequestUsesRightClickedAgentWorktreePath() {
    let vc = DashboardViewController()
    let spy = DashboardDelegateSpy()
    vc.dashboardDelegate = spy
    vc.loadViewIfNeeded()
    vc.updateAgents([
        makeAgent(id: "agent-a", worktreePath: "/repo/a"),
        makeAgent(id: "agent-b", worktreePath: "/repo/b"),
    ])
    vc.agentCardClicked(agentId: "agent-a")

    vc.agentCardDidRequestShowChanges(agentId: "agent-b")

    XCTAssertEqual(spy.changesPath, "/repo/b")
    XCTAssertNil(spy.browsePath)
}
```

Add this helper in the same test file:

```swift
private func makeAgent(id: String, worktreePath: String) -> AgentDisplayInfo {
    AgentDisplayInfo(
        id: id,
        name: id,
        project: "proj",
        thread: "main",
        paneStatuses: [.idle],
        mostRecentMessage: "No active task.",
        lastUserPrompt: "",
        mostRecentPaneIndex: 1,
        totalDuration: "00:00:00",
        roundDuration: "00:00:00",
        surface: TerminalSurface(),
        worktreePath: worktreePath,
        paneCount: 1,
        paneSurfaces: [],
        isMainWorktree: false,
        tasks: [],
        activityEvents: []
    )
}
```

Extend `DashboardDelegateSpy`:

```swift
private class DashboardDelegateSpy: DashboardDelegate {
    var didSelectProjectCalled = false
    var lastProject: String?
    var lastThread: String?
    var browsePath: String?
    var changesPath: String?

    func dashboardDidSelectProject(_ project: String, thread: String) {
        didSelectProjectCalled = true
        lastProject = project
        lastThread = thread
    }
    func dashboardDidRequestEnterProject(_ project: String) {}
    func dashboardDidReorderCards(order: [String]) {}
    func dashboardDidRequestDelete(_ terminalID: String) {}
    func dashboardDidRequestCloseRepo(_ project: String) {}
    func dashboardDidRequestAddProject() {}
    func dashboardDidChangeSelection(_ dashboard: DashboardViewController) {}
    func dashboardDidRequestBrowseFiles(worktreePath: String) { browsePath = worktreePath }
    func dashboardDidRequestShowChanges(worktreePath: String) { changesPath = worktreePath }
}
```

- [ ] **Step 2: Write the failing inspector shell tests**

Create `Tests/WorktreeInspectorViewControllerTests.swift`:

```swift
import XCTest
@testable import amux

final class WorktreeInspectorViewControllerTests: XCTestCase {
    func testInitialTabFilesSelectsFilesSegment() {
        let vc = WorktreeInspectorViewController(
            worktreePath: "/repo/project",
            initialTab: .files,
            yaziAvailability: { false }
        )

        vc.loadViewIfNeeded()

        XCTAssertEqual(vc.selectedTabForTesting, .files)
        XCTAssertNotNil(vc.view.viewWithAccessibilityIdentifier("worktreeInspector.filesMissingYazi"))
    }

    func testInitialTabChangesSelectsChangesSegment() {
        let vc = WorktreeInspectorViewController(
            worktreePath: "/repo/project",
            initialTab: .changes,
            yaziAvailability: { false }
        )

        vc.loadViewIfNeeded()

        XCTAssertEqual(vc.selectedTabForTesting, .changes)
        XCTAssertNotNil(vc.view.viewWithAccessibilityIdentifier("worktreeInspector.changesPlaceholder"))
    }
}

private extension NSView {
    func viewWithAccessibilityIdentifier(_ identifier: String) -> NSView? {
        if accessibilityIdentifier() == identifier { return self }
        for subview in subviews {
            if let found = subview.viewWithAccessibilityIdentifier(identifier) {
                return found
            }
        }
        return nil
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run:

```bash
xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/DashboardViewControllerClickTests/testBrowseFilesRequestUsesRightClickedAgentWorktreePath -only-testing:amuxTests/DashboardViewControllerClickTests/testShowChangesRequestUsesRightClickedAgentWorktreePath -only-testing:amuxTests/WorktreeInspectorViewControllerTests
```

Expected: FAIL because `WorktreeInspectorViewController`, dashboard delegate methods, and card request handlers do not exist.

- [ ] **Step 4: Create the inspector shell**

Create `Sources/UI/Inspector/WorktreeInspectorViewController.swift`:

```swift
import AppKit

enum WorktreeInspectorInitialTab: Int {
    case files = 0
    case changes = 1
}

final class WorktreeInspectorViewController: NSViewController {
    private let worktreePath: String
    private let yaziAvailability: () -> Bool
    private let segmentedControl = NSSegmentedControl(labels: ["Files", "Changes"], trackingMode: .selectOne, target: nil, action: nil)
    private let contentView = NSView()
    private var selectedTab: WorktreeInspectorInitialTab

    var selectedTabForTesting: WorktreeInspectorInitialTab { selectedTab }

    init(
        worktreePath: String,
        initialTab: WorktreeInspectorInitialTab,
        yaziAvailability: @escaping () -> Bool = { ProcessRunner.commandExists("yazi") }
    ) {
        self.worktreePath = worktreePath
        self.selectedTab = initialTab
        self.yaziAvailability = yaziAvailability
        super.init(nibName: nil, bundle: nil)
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) not supported")
    }

    override func loadView() {
        let root = NSView(frame: NSRect(x: 0, y: 0, width: 1080, height: 700))
        root.wantsLayer = true
        root.layer?.backgroundColor = Theme.background.cgColor
        root.setAccessibilityIdentifier("worktreeInspector")

        let title = NSTextField(labelWithString: URL(fileURLWithPath: worktreePath).lastPathComponent)
        title.font = NSFont.systemFont(ofSize: 14, weight: .semibold)
        title.textColor = Theme.textPrimary
        title.lineBreakMode = .byTruncatingMiddle
        title.translatesAutoresizingMaskIntoConstraints = false
        root.addSubview(title)

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

            segmentedControl.topAnchor.constraint(equalTo: root.topAnchor, constant: 8),
            segmentedControl.trailingAnchor.constraint(equalTo: root.trailingAnchor, constant: -16),
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

    private func showSelectedTab() {
        contentView.subviews.forEach { $0.removeFromSuperview() }
        switch selectedTab {
        case .files:
            showFilesTab()
        case .changes:
            showChangesTab()
        }
    }

    private func showFilesTab() {
        guard yaziAvailability() else {
            showMessage(
                "Yazi is not installed. Install yazi to browse files in this tab.",
                identifier: "worktreeInspector.filesMissingYazi"
            )
            return
        }
        showMessage("Yazi file browser will appear here.", identifier: "worktreeInspector.filesPlaceholder")
    }

    private func showChangesTab() {
        showMessage("Changes will appear here.", identifier: "worktreeInspector.changesPlaceholder")
    }

    private func showMessage(_ message: String, identifier: String) {
        let label = NSTextField(labelWithString: message)
        label.font = NSFont.systemFont(ofSize: 13)
        label.textColor = Theme.textSecondary
        label.alignment = .center
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
}
```

- [ ] **Step 5: Wire dashboard and main-window presentation**

In `Sources/UI/Dashboard/DashboardViewController.swift`, extend `DashboardDelegate`:

```swift
protocol DashboardDelegate: AnyObject {
    func dashboardDidSelectProject(_ project: String, thread: String)
    func dashboardDidRequestEnterProject(_ project: String)
    func dashboardDidReorderCards(order: [String])
    func dashboardDidRequestDelete(_ terminalID: String)
    func dashboardDidRequestCloseRepo(_ project: String)
    func dashboardDidRequestAddProject()
    func dashboardDidChangeSelection(_ dashboard: DashboardViewController)
    func dashboardDidRequestBrowseFiles(worktreePath: String)
    func dashboardDidRequestShowChanges(worktreePath: String)
}
```

In the `AgentCardDelegate` section of `DashboardViewController`, add:

```swift
func agentCardDidRequestBrowseFiles(agentId: String) {
    guard let agent = agents.first(where: { $0.id == agentId }) else { return }
    dashboardDelegate?.dashboardDidRequestBrowseFiles(worktreePath: agent.worktreePath)
}

func agentCardDidRequestShowChanges(agentId: String) {
    guard let agent = agents.first(where: { $0.id == agentId }) else { return }
    dashboardDelegate?.dashboardDidRequestShowChanges(worktreePath: agent.worktreePath)
}
```

In `Sources/App/MainWindowController.swift`, add:

```swift
private func presentWorktreeInspector(for worktreePath: String, initialTab: WorktreeInspectorInitialTab) {
    let inspector = WorktreeInspectorViewController(worktreePath: worktreePath, initialTab: initialTab)
    dialogPresenter.presentSheetOnActiveVC(inspector, tabCoordinator: tabCoordinator, dashboardVC: dashboardVC)
}
```

In the `DashboardDelegate` extension of `MainWindowController`, add:

```swift
func dashboardDidRequestBrowseFiles(worktreePath: String) {
    presentWorktreeInspector(for: worktreePath, initialTab: .files)
}

func dashboardDidRequestShowChanges(worktreePath: String) {
    presentWorktreeInspector(for: worktreePath, initialTab: .changes)
}
```

Update any test spy conforming to `DashboardDelegate` with no-op methods:

```swift
func dashboardDidRequestBrowseFiles(worktreePath: String) {}
func dashboardDidRequestShowChanges(worktreePath: String) {}
```

- [ ] **Step 6: Run tests to verify they pass**

Run:

```bash
xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/DashboardViewControllerClickTests -only-testing:amuxTests/WorktreeInspectorViewControllerTests
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add Sources/UI/Dashboard/DashboardViewController.swift Sources/App/MainWindowController.swift Sources/UI/Inspector/WorktreeInspectorViewController.swift Tests/DashboardViewControllerClickTests.swift Tests/WorktreeInspectorViewControllerTests.swift
git commit -m "feat: route card inspector requests"
```

---

### Task 3: Add Structured Git Diff Snapshots

**Files:**
- Modify: `Sources/Git/GitDiff.swift`
- Test: `Tests/GitDiffTests.swift`

- [ ] **Step 1: Write the failing tests**

Add these tests to `Tests/GitDiffTests.swift`:

```swift
func testParseDiffPreservesAddedDeletedAndRenamedStatus() {
    let diff = """
    diff --git a/new.txt b/new.txt
    new file mode 100644
    --- /dev/null
    +++ b/new.txt
    @@ -0,0 +1 @@
    +new
    diff --git a/old.txt b/old.txt
    deleted file mode 100644
    --- a/old.txt
    +++ /dev/null
    @@ -1 +0,0 @@
    -old
    diff --git a/before.txt b/after.txt
    similarity index 92%
    rename from before.txt
    rename to after.txt
    --- a/before.txt
    +++ b/after.txt
    @@ -1 +1 @@
    -before
    +after
    """

    let files = GitDiff.parseDiff(diff, stage: .unstaged)

    XCTAssertEqual(files[0].status, .added)
    XCTAssertEqual(files[1].status, .deleted)
    XCTAssertEqual(files[2].status, .renamed)
    XCTAssertEqual(files[2].oldPath, "before.txt")
    XCTAssertEqual(files[2].path, "after.txt")
}

func testSnapshotIncludesUntrackedTextFileAsSyntheticAddition() throws {
    let tempDir = FileManager.default.temporaryDirectory
        .appendingPathComponent("amux-diff-test-\(UUID().uuidString)")
    let repoPath = tempDir.appendingPathComponent("repo").path
    defer { try? FileManager.default.removeItem(at: tempDir) }

    try FileManager.default.createDirectory(atPath: repoPath, withIntermediateDirectories: true)
    git(["init", "-b", "main"], in: repoPath)
    git(["-c", "user.email=t@t.com", "-c", "user.name=T", "commit", "--allow-empty", "-m", "init"], in: repoPath)
    try "hello\nworld\n".write(toFile: "\(repoPath)/new.txt", atomically: true, encoding: .utf8)

    let snapshot = GitDiff.snapshot(worktreePath: repoPath)
    let file = snapshot.files.first { $0.path == "new.txt" }

    XCTAssertEqual(file?.status, .added)
    XCTAssertEqual(file?.stage, .untracked)
    XCTAssertEqual(file?.additions, 2)
    XCTAssertEqual(file?.deletions, 0)
}

func testSnapshotSeparatesStagedAndUnstagedChanges() throws {
    let tempDir = FileManager.default.temporaryDirectory
        .appendingPathComponent("amux-diff-test-\(UUID().uuidString)")
    let repoPath = tempDir.appendingPathComponent("repo").path
    defer { try? FileManager.default.removeItem(at: tempDir) }

    try FileManager.default.createDirectory(atPath: repoPath, withIntermediateDirectories: true)
    git(["init", "-b", "main"], in: repoPath)
    try "one\n".write(toFile: "\(repoPath)/tracked.txt", atomically: true, encoding: .utf8)
    git(["add", "tracked.txt"], in: repoPath)
    git(["-c", "user.email=t@t.com", "-c", "user.name=T", "commit", "-m", "add"], in: repoPath)

    try "one\ntwo\n".write(toFile: "\(repoPath)/tracked.txt", atomically: true, encoding: .utf8)
    git(["add", "tracked.txt"], in: repoPath)
    try "one\ntwo\nthree\n".write(toFile: "\(repoPath)/tracked.txt", atomically: true, encoding: .utf8)

    let snapshot = GitDiff.snapshot(worktreePath: repoPath)

    XCTAssertTrue(snapshot.files.contains { $0.path == "tracked.txt" && $0.stage == .staged })
    XCTAssertTrue(snapshot.files.contains { $0.path == "tracked.txt" && $0.stage == .unstaged })
}

func testSnapshotRepresentsUntrackedBinaryFileWithoutRenderingContents() throws {
    let tempDir = FileManager.default.temporaryDirectory
        .appendingPathComponent("amux-diff-test-\(UUID().uuidString)")
    let repoPath = tempDir.appendingPathComponent("repo").path
    defer { try? FileManager.default.removeItem(at: tempDir) }

    try FileManager.default.createDirectory(atPath: repoPath, withIntermediateDirectories: true)
    git(["init", "-b", "main"], in: repoPath)
    git(["-c", "user.email=t@t.com", "-c", "user.name=T", "commit", "--allow-empty", "-m", "init"], in: repoPath)
    FileManager.default.createFile(atPath: "\(repoPath)/image.bin", contents: Data([0, 1, 2, 3]))

    let snapshot = GitDiff.snapshot(worktreePath: repoPath)
    let file = snapshot.files.first { $0.path == "image.bin" }

    XCTAssertEqual(file?.status, .added)
    XCTAssertEqual(file?.stage, .untracked)
    XCTAssertEqual(file?.additions, 0)
    XCTAssertTrue(file?.hunks.isEmpty ?? false)
}

func testSnapshotRepresentsOversizedUntrackedTextWithoutRenderingContents() throws {
    let tempDir = FileManager.default.temporaryDirectory
        .appendingPathComponent("amux-diff-test-\(UUID().uuidString)")
    let repoPath = tempDir.appendingPathComponent("repo").path
    defer { try? FileManager.default.removeItem(at: tempDir) }

    try FileManager.default.createDirectory(atPath: repoPath, withIntermediateDirectories: true)
    git(["init", "-b", "main"], in: repoPath)
    git(["-c", "user.email=t@t.com", "-c", "user.name=T", "commit", "--allow-empty", "-m", "init"], in: repoPath)
    try "abcdef\n".write(toFile: "\(repoPath)/large.txt", atomically: true, encoding: .utf8)

    let snapshot = GitDiff.snapshot(worktreePath: repoPath, maxSyntheticFileBytes: 4)
    let file = snapshot.files.first { $0.path == "large.txt" }

    XCTAssertEqual(file?.status, .added)
    XCTAssertEqual(file?.stage, .untracked)
    XCTAssertEqual(file?.additions, 0)
    XCTAssertTrue(file?.hunks.isEmpty ?? false)
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/GitDiffTests/testParseDiffPreservesAddedDeletedAndRenamedStatus -only-testing:amuxTests/GitDiffTests/testSnapshotIncludesUntrackedTextFileAsSyntheticAddition -only-testing:amuxTests/GitDiffTests/testSnapshotSeparatesStagedAndUnstagedChanges -only-testing:amuxTests/GitDiffTests/testSnapshotRepresentsUntrackedBinaryFileWithoutRenderingContents -only-testing:amuxTests/GitDiffTests/testSnapshotRepresentsOversizedUntrackedTextWithoutRenderingContents
```

Expected: FAIL because `GitChangeStage`, `oldPath`, `stage`, and `snapshot(worktreePath:)` do not exist.

- [ ] **Step 3: Implement structured snapshot support**

In `Sources/Git/GitDiff.swift`, add these types above `DiffFile`:

```swift
enum GitChangeStage {
    case staged
    case unstaged
    case untracked
}

struct GitChangedFile {
    let path: String
    let oldPath: String?
    let status: DiffFile.FileStatus
    let stage: GitChangeStage
}

struct GitDiffSnapshot {
    let changedFiles: [GitChangedFile]
    let files: [DiffFile]
}
```

Replace `DiffFile` with:

```swift
struct DiffFile {
    let path: String
    let oldPath: String?
    let status: FileStatus
    let stage: GitChangeStage
    let additions: Int
    let deletions: Int
    let hunks: [DiffHunk]

    init(
        path: String,
        oldPath: String? = nil,
        status: FileStatus = .modified,
        stage: GitChangeStage = .unstaged,
        additions: Int,
        deletions: Int,
        hunks: [DiffHunk]
    ) {
        self.path = path
        self.oldPath = oldPath
        self.status = status
        self.stage = stage
        self.additions = additions
        self.deletions = deletions
        self.hunks = hunks
    }

    enum FileStatus: String {
        case added = "A"
        case modified = "M"
        case deleted = "D"
        case renamed = "R"
        case unknown = "?"
    }
}
```

Add snapshot and changed-entry methods inside `enum GitDiff`:

```swift
static func snapshot(worktreePath: String, maxSyntheticFileBytes: Int = 128 * 1024) -> GitDiffSnapshot {
    let changed = changedFileEntries(worktreePath: worktreePath)
    let stagedOutput = runGit(args: ["diff", "--cached", "--no-color"], in: worktreePath) ?? ""
    let unstagedOutput = runGit(args: ["diff", "--no-color"], in: worktreePath) ?? ""

    var files = parseDiff(stagedOutput, stage: .staged)
    files.append(contentsOf: parseDiff(unstagedOutput, stage: .unstaged))

    let untrackedDiffs = changed
        .filter { $0.stage == .untracked }
        .compactMap { syntheticUntrackedDiff(for: $0.path, worktreePath: worktreePath, maxBytes: maxSyntheticFileBytes) }
    files.append(contentsOf: untrackedDiffs)

    return GitDiffSnapshot(changedFiles: changed, files: files)
}

static func changedFileEntries(worktreePath: String) -> [GitChangedFile] {
    let output = runGit(args: ["status", "--porcelain"], in: worktreePath) ?? ""
    return output.components(separatedBy: .newlines).flatMap { line -> [GitChangedFile] in
        guard line.count >= 3 else { return [] }
        let x = line[line.startIndex]
        let y = line[line.index(after: line.startIndex)]
        let rawPath = String(line.dropFirst(3))
        let paths = parsePorcelainPath(rawPath)

        if x == "?" && y == "?" {
            return [GitChangedFile(path: paths.path, oldPath: nil, status: .added, stage: .untracked)]
        }

        var entries: [GitChangedFile] = []
        if x != " " {
            entries.append(GitChangedFile(path: paths.path, oldPath: paths.oldPath, status: status(from: x), stage: .staged))
        }
        if y != " " {
            entries.append(GitChangedFile(path: paths.path, oldPath: paths.oldPath, status: status(from: y), stage: .unstaged))
        }
        return entries
    }
}
```

Change `parseDiff` signature and parser state:

```swift
static func parseDiff(_ output: String, stage: GitChangeStage = .unstaged) -> [DiffFile] {
    var files: [DiffFile] = []
    var currentPath = ""
    var oldPath: String?
    var status: DiffFile.FileStatus = .modified
    var currentHunks: [DiffHunk] = []
    var currentHunkHeader = ""
    var currentLines: [DiffLine] = []
    var additions = 0
    var deletions = 0

    func flushHunk() {
        if !currentHunkHeader.isEmpty {
            currentHunks.append(DiffHunk(header: currentHunkHeader, lines: currentLines))
            currentLines = []
            currentHunkHeader = ""
        }
    }

    func flushFile() {
        flushHunk()
        if !currentPath.isEmpty {
            files.append(DiffFile(
                path: currentPath,
                oldPath: oldPath,
                status: status,
                stage: stage,
                additions: additions,
                deletions: deletions,
                hunks: currentHunks
            ))
        }
        currentPath = ""
        oldPath = nil
        status = .modified
        currentHunks = []
        additions = 0
        deletions = 0
    }

    for line in output.components(separatedBy: .newlines) {
        if line.hasPrefix("diff --git") {
            flushFile()
            let parts = line.components(separatedBy: " b/")
            if parts.count >= 2 {
                currentPath = parts.last ?? ""
            }
        } else if line.hasPrefix("new file mode") {
            status = .added
        } else if line.hasPrefix("deleted file mode") {
            status = .deleted
        } else if line.hasPrefix("rename from ") {
            oldPath = String(line.dropFirst("rename from ".count))
            status = .renamed
        } else if line.hasPrefix("rename to ") {
            currentPath = String(line.dropFirst("rename to ".count))
            status = .renamed
        } else if line.hasPrefix("@@") {
            flushHunk()
            currentHunkHeader = line
        } else if line.hasPrefix("+") && !line.hasPrefix("+++") {
            currentLines.append(DiffLine(type: .addition, content: String(line.dropFirst())))
            additions += 1
        } else if line.hasPrefix("-") && !line.hasPrefix("---") {
            currentLines.append(DiffLine(type: .deletion, content: String(line.dropFirst())))
            deletions += 1
        } else if line.hasPrefix(" ") {
            currentLines.append(DiffLine(type: .context, content: String(line.dropFirst())))
        }
    }
    flushFile()

    return files
}
```

Add helpers inside `enum GitDiff`:

```swift
private static func status(from char: Character) -> DiffFile.FileStatus {
    switch char {
    case "A": return .added
    case "M": return .modified
    case "D": return .deleted
    case "R": return .renamed
    default: return .unknown
    }
}

private static func parsePorcelainPath(_ raw: String) -> (oldPath: String?, path: String) {
    let parts = raw.components(separatedBy: " -> ")
    if parts.count == 2 {
        return (oldPath: parts[0], path: parts[1])
    }
    return (oldPath: nil, path: raw)
}

private static func syntheticUntrackedDiff(for relativePath: String, worktreePath: String, maxBytes: Int) -> DiffFile? {
    let url = URL(fileURLWithPath: worktreePath).appendingPathComponent(relativePath)
    guard let values = try? url.resourceValues(forKeys: [.isRegularFileKey, .fileSizeKey]),
          values.isRegularFile == true,
          let size = values.fileSize,
          size <= maxBytes,
          let data = try? Data(contentsOf: url),
          !data.contains(0),
          let text = String(data: data, encoding: .utf8)
    else {
        return DiffFile(
            path: relativePath,
            status: .added,
            stage: .untracked,
            additions: 0,
            deletions: 0,
            hunks: []
        )
    }

    let lines = text.components(separatedBy: .newlines)
    let effectiveLines = lines.last == "" ? Array(lines.dropLast()) : lines
    let diffLines = effectiveLines.map { DiffLine(type: .addition, content: $0) }
    let hunk = DiffHunk(header: "@@ -0,0 +1,\(diffLines.count) @@", lines: diffLines)
    return DiffFile(
        path: relativePath,
        status: .added,
        stage: .untracked,
        additions: diffLines.count,
        deletions: 0,
        hunks: [hunk]
    )
}
```

Update existing `diff(worktreePath:)` to preserve current behavior through the new parser:

```swift
static func diff(worktreePath: String) -> [DiffFile] {
    snapshot(worktreePath: worktreePath).files
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run:

```bash
xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/GitDiffTests
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add Sources/Git/GitDiff.swift Tests/GitDiffTests.swift
git commit -m "feat: add structured git diff snapshots"
```

---

### Task 4: Extract Native Diff Review View

**Files:**
- Create: `Sources/UI/Diff/DiffReviewView.swift`
- Modify: `Sources/UI/Diff/DiffOverlayViewController.swift`
- Test: `Tests/DiffReviewViewTests.swift`

- [ ] **Step 1: Write the failing tests**

Create `Tests/DiffReviewViewTests.swift`:

```swift
import XCTest
@testable import amux

final class DiffReviewViewTests: XCTestCase {
    func testDiffReviewViewLoadsInjectedSnapshot() {
        let file = DiffFile(
            path: "Sources/App/main.swift",
            status: .modified,
            stage: .unstaged,
            additions: 1,
            deletions: 1,
            hunks: [
                DiffHunk(header: "@@ -1 +1 @@", lines: [
                    DiffLine(type: .deletion, content: "old"),
                    DiffLine(type: .addition, content: "new"),
                ])
            ]
        )
        let view = DiffReviewView(
            worktreePath: "/repo",
            loadSnapshot: { GitDiffSnapshot(changedFiles: [], files: [file]) }
        )

        view.frame = NSRect(x: 0, y: 0, width: 800, height: 500)
        view.loadDiffForTesting()

        XCTAssertTrue(view.renderedTextForTesting.contains("Sources/App/main.swift"))
        XCTAssertTrue(view.renderedTextForTesting.contains("+new"))
        XCTAssertTrue(view.renderedTextForTesting.contains("-old"))
    }

    func testDiffReviewViewShowsNoChangesStateForEmptySnapshot() {
        let view = DiffReviewView(
            worktreePath: "/repo",
            loadSnapshot: { GitDiffSnapshot(changedFiles: [], files: []) }
        )

        view.frame = NSRect(x: 0, y: 0, width: 800, height: 500)
        view.loadDiffForTesting()

        XCTAssertTrue(view.renderedTextForTesting.contains("No changes"))
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/DiffReviewViewTests
```

Expected: FAIL because `DiffReviewView` does not exist.

- [ ] **Step 3: Create `DiffReviewView`**

Create `Sources/UI/Diff/DiffReviewView.swift` by moving the reusable parts of `DiffOverlayViewController` into an `NSView`. Use this public shape and initializer:

```swift
import AppKit

final class DiffReviewView: NSView {
    private let splitView = NSSplitView()
    private let outlineView = NSOutlineView()
    private let fileScrollView = NSScrollView()
    private let diffTextView = NSTextView()
    private let diffScrollView = NSScrollView()
    private let headerLabel = NSTextField(labelWithString: "")

    private let worktreePath: String
    private let loadSnapshot: () -> GitDiffSnapshot
    private var files: [DiffFile] = []
    private var treeNodes: [DiffReviewFileTreeNode] = []

    var renderedTextForTesting: String {
        diffTextView.string
    }

    init(worktreePath: String, loadSnapshot: (() -> GitDiffSnapshot)? = nil) {
        self.worktreePath = worktreePath
        self.loadSnapshot = loadSnapshot ?? { GitDiff.snapshot(worktreePath: worktreePath) }
        super.init(frame: .zero)
        setup()
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) not supported")
    }

    func loadDiffForTesting() {
        applySnapshot(loadSnapshot())
    }
}
```

Move the file-tree model from `DiffOverlayViewController` into this new file and rename it to avoid private-name collision:

```swift
private final class DiffReviewFileTreeNode {
    let name: String
    let fullPath: String
    var status: String?
    var children: [DiffReviewFileTreeNode] = []
    var isDirectory: Bool { status == nil }

    init(name: String, fullPath: String, status: String? = nil) {
        self.name = name
        self.fullPath = fullPath
        self.status = status
    }
}
```

Use the rendering logic already present in `DiffOverlayViewController` for:

- `setup()`
- `applySnapshot(_:)`
- `showAllDiffs()`
- `showDiffForFile(path:)`
- `showDiffsForPaths(_:)`
- `statusColor(_:)`
- `NSOutlineViewDataSource`
- `NSOutlineViewDelegate`

The implementation must set these identifiers:

```swift
setAccessibilityIdentifier("diffReview")
outlineView.setAccessibilityIdentifier("diffReview.fileTree")
diffTextView.setAccessibilityIdentifier("diffReview.text")
```

- [ ] **Step 4: Reuse `DiffReviewView` from `DiffOverlayViewController`**

Replace the current body-heavy `DiffOverlayViewController` with a thin wrapper:

```swift
class DiffOverlayViewController: NSViewController {
    private let worktreePath: String
    private let reviewView: DiffReviewView
    private let closeButton = NSButton()

    init(worktreePath: String) {
        self.worktreePath = worktreePath
        self.reviewView = DiffReviewView(worktreePath: worktreePath)
        super.init(nibName: nil, bundle: nil)
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) not supported")
    }

    override func loadView() {
        let container = NSView(frame: NSRect(x: 0, y: 0, width: 1000, height: 660))
        container.wantsLayer = true
        container.layer?.backgroundColor = Theme.background.cgColor
        container.setAccessibilityIdentifier("repo.diffOverlay")

        closeButton.title = "Close"
        closeButton.bezelStyle = .rounded
        closeButton.keyEquivalent = "\u{1b}"
        closeButton.target = self
        closeButton.action = #selector(closeClicked)
        closeButton.translatesAutoresizingMaskIntoConstraints = false
        container.addSubview(closeButton)

        reviewView.translatesAutoresizingMaskIntoConstraints = false
        container.addSubview(reviewView)

        NSLayoutConstraint.activate([
            closeButton.topAnchor.constraint(equalTo: container.topAnchor, constant: 8),
            closeButton.trailingAnchor.constraint(equalTo: container.trailingAnchor, constant: -12),

            reviewView.topAnchor.constraint(equalTo: container.topAnchor, constant: 8),
            reviewView.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            reviewView.trailingAnchor.constraint(equalTo: container.trailingAnchor),
            reviewView.bottomAnchor.constraint(equalTo: container.bottomAnchor),
        ])

        view = container
    }

    @objc private func closeClicked() {
        dismiss(nil)
    }
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run:

```bash
xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/DiffReviewViewTests -only-testing:amuxTests/GitDiffTests
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add Sources/UI/Diff/DiffReviewView.swift Sources/UI/Diff/DiffOverlayViewController.swift Tests/DiffReviewViewTests.swift
git commit -m "refactor: extract reusable diff review view"
```

---

### Task 5: Embed Changes Tab In Worktree Inspector

**Files:**
- Modify: `Sources/UI/Inspector/WorktreeInspectorViewController.swift`
- Test: `Tests/WorktreeInspectorViewControllerTests.swift`

- [ ] **Step 1: Write the failing test**

Add this test to `Tests/WorktreeInspectorViewControllerTests.swift`:

```swift
func testChangesTabEmbedsDiffReviewView() {
    let vc = WorktreeInspectorViewController(
        worktreePath: "/repo/project",
        initialTab: .changes,
        yaziAvailability: { false },
        makeDiffReviewView: { path in
            XCTAssertEqual(path, "/repo/project")
            return DiffReviewView(
                worktreePath: path,
                loadSnapshot: { GitDiffSnapshot(changedFiles: [], files: []) }
            )
        }
    )

    vc.loadViewIfNeeded()

    XCTAssertNotNil(vc.view.viewWithAccessibilityIdentifier("diffReview"))
}
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/WorktreeInspectorViewControllerTests/testChangesTabEmbedsDiffReviewView
```

Expected: FAIL because `makeDiffReviewView` injection and `DiffReviewView` embedding are not implemented.

- [ ] **Step 3: Add diff-review injection and embed the view**

Change the inspector initializer in `Sources/UI/Inspector/WorktreeInspectorViewController.swift`:

```swift
private let makeDiffReviewView: (String) -> DiffReviewView

init(
    worktreePath: String,
    initialTab: WorktreeInspectorInitialTab,
    yaziAvailability: @escaping () -> Bool = { ProcessRunner.commandExists("yazi") },
    makeDiffReviewView: @escaping (String) -> DiffReviewView = { DiffReviewView(worktreePath: $0) }
) {
    self.worktreePath = worktreePath
    self.selectedTab = initialTab
    self.yaziAvailability = yaziAvailability
    self.makeDiffReviewView = makeDiffReviewView
    super.init(nibName: nil, bundle: nil)
}
```

Replace `showChangesTab()` with:

```swift
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
```

Remove the `"worktreeInspector.changesPlaceholder"` assertion from `testInitialTabChangesSelectsChangesSegment` and assert `diffReview` exists instead:

```swift
XCTAssertNotNil(vc.view.viewWithAccessibilityIdentifier("diffReview"))
```

- [ ] **Step 4: Run tests to verify they pass**

Run:

```bash
xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/WorktreeInspectorViewControllerTests -only-testing:amuxTests/DiffReviewViewTests
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add Sources/UI/Inspector/WorktreeInspectorViewController.swift Tests/WorktreeInspectorViewControllerTests.swift
git commit -m "feat: embed changes review in inspector"
```

---

### Task 6: Add Ephemeral Yazi Terminal Surface

**Files:**
- Modify: `Sources/Terminal/TerminalSurface.swift`
- Modify: `Sources/UI/Inspector/WorktreeInspectorViewController.swift`
- Test: `Tests/WorktreeInspectorViewControllerTests.swift`

- [ ] **Step 1: Write the failing tests**

Add these tests to `Tests/WorktreeInspectorViewControllerTests.swift`:

```swift
func testYaziCommandUsesCurrentDirectory() {
    XCTAssertEqual(WorktreeInspectorViewController.yaziCommand, "yazi .")
}

func testFilesTabShowsYaziContainerWhenAvailable() {
    let vc = WorktreeInspectorViewController(
        worktreePath: "/repo/project",
        initialTab: .files,
        yaziAvailability: { true },
        makeDiffReviewView: { DiffReviewView(worktreePath: $0, loadSnapshot: { GitDiffSnapshot(changedFiles: [], files: []) }) },
        createYaziSurface: { _, _, _ in true }
    )

    vc.loadViewIfNeeded()

    XCTAssertNotNil(vc.view.viewWithAccessibilityIdentifier("worktreeInspector.yaziContainer"))
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/WorktreeInspectorViewControllerTests/testYaziCommandUsesCurrentDirectory -only-testing:amuxTests/WorktreeInspectorViewControllerTests/testFilesTabShowsYaziContainerWhenAvailable
```

Expected: FAIL because `yaziCommand` and `createYaziSurface` injection do not exist.

- [ ] **Step 3: Add command-based surface creation**

In `Sources/Terminal/TerminalSurface.swift`, add this method near `create(in:workingDirectory:sessionName:completion:)`:

```swift
@discardableResult
func createEphemeral(in container: NSView, workingDirectory: String? = nil, command: String) -> Bool {
    guard let app = GhosttyBridge.shared.app else {
        NSLog("GhosttyBridge not initialized")
        return false
    }
    _createWithCommand(app: app, container: container, workingDirectory: workingDirectory, command: command)
    return surface != nil
}
```

- [ ] **Step 4: Wire the Files tab to Yazi**

In `Sources/UI/Inspector/WorktreeInspectorViewController.swift`, add:

```swift
static let yaziCommand = "yazi ."

private let createYaziSurface: (NSView, String, String) -> Bool
private var yaziSurface: TerminalSurface?
```

Change the initializer to include the injectable creator. The creator parameter is optional so the default path can retain the created `TerminalSurface` on the controller:

```swift
init(
    worktreePath: String,
    initialTab: WorktreeInspectorInitialTab,
    yaziAvailability: @escaping () -> Bool = { ProcessRunner.commandExists("yazi") },
    makeDiffReviewView: @escaping (String) -> DiffReviewView = { DiffReviewView(worktreePath: $0) },
    createYaziSurface: ((NSView, String, String) -> Bool)? = nil
) {
    self.worktreePath = worktreePath
    self.selectedTab = initialTab
    self.yaziAvailability = yaziAvailability
    self.makeDiffReviewView = makeDiffReviewView
    self.createYaziSurface = createYaziSurface ?? { [weak self] container, path, command in
        let surface = TerminalSurface()
        self?.yaziSurface = surface
        return surface.createEphemeral(in: container, workingDirectory: path, command: command)
    }
    super.init(nibName: nil, bundle: nil)
}
```

Replace the available branch of `showFilesTab()`:

```swift
private func showFilesTab() {
    guard yaziAvailability() else {
        showMessage(
            "Yazi is not installed. Install yazi to browse files in this tab.",
            identifier: "worktreeInspector.filesMissingYazi"
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

    if !createYaziSurface(terminalContainer, worktreePath, Self.yaziCommand) {
        terminalContainer.removeFromSuperview()
        showMessage("Could not start yazi for this worktree.", identifier: "worktreeInspector.filesYaziFailed")
    }
}
```

Add cleanup:

```swift
deinit {
    yaziSurface?.destroy()
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run:

```bash
xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/WorktreeInspectorViewControllerTests
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add Sources/Terminal/TerminalSurface.swift Sources/UI/Inspector/WorktreeInspectorViewController.swift Tests/WorktreeInspectorViewControllerTests.swift
git commit -m "feat: embed yazi in worktree inspector"
```

---

### Task 7: Final Verification And Smoke Checks

**Files:**
- Verify only

- [ ] **Step 1: Run focused unit tests**

Run:

```bash
xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/StackedMiniCardContainerViewTests -only-testing:amuxTests/StackedCardContainerDoubleClickTests -only-testing:amuxTests/DashboardViewControllerClickTests -only-testing:amuxTests/WorktreeInspectorViewControllerTests -only-testing:amuxTests/DiffReviewViewTests -only-testing:amuxTests/GitDiffTests
```

Expected: PASS.

- [ ] **Step 2: Run app build**

Run:

```bash
xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build
```

Expected: BUILD SUCCEEDED.

- [ ] **Step 3: Manual smoke check with Yazi installed**

Run the app, then:

1. Right-click a focus-layout mini card.
2. Click `Browse Files...`.
3. Confirm `worktreeInspector` opens on the `Files` tab.
4. Confirm Yazi opens in the selected card's worktree.
5. Close the sheet.
6. Right-click a different mini card.
7. Click `Show Changes...`.
8. Confirm the inspector opens on `Changes`.
9. Confirm the diff belongs to the right-clicked card, not the previously selected card.

- [ ] **Step 4: Manual smoke check without Yazi**

Temporarily run the app with a `PATH` that does not include `yazi`, then:

1. Right-click a mini card.
2. Click `Browse Files...`.
3. Confirm the `Files` tab shows the missing-Yazi state.
4. Switch to `Changes`.
5. Confirm native diff still loads.

- [ ] **Step 5: Commit verification notes if code changed**

If manual smoke checks require code changes, commit them:

```bash
git add Sources Tests
git commit -m "fix: polish worktree inspector smoke issues"
```

If no code changed, do not create an empty commit.
