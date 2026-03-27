# TODO & Ideas Persistence Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace mock TODO and Ideas data with real JSON-persisted data, with CRUD operations wired through PanelCoordinator to the existing AIPanelView UI.

**Architecture:** Two singleton stores (`TodoStore`, `IdeaStore`) each manage a JSON file under `~/.config/amux/`. PanelCoordinator reads from stores and feeds display items to AIPanelView. AIPanelView delegates idea input back to PanelCoordinator. AgentHead gets a stub for future webhook-driven TODO status updates.

**Tech Stack:** Swift 5.10, AppKit, Foundation (Codable + JSONEncoder/Decoder)

---

## File Structure

| Action | File | Responsibility |
|--------|------|----------------|
| Create | `Sources/Core/TodoStore.swift` | TodoItem model + CRUD + JSON persistence |
| Create | `Sources/Core/IdeaStore.swift` | IdeaItem model + CRUD + JSON persistence |
| Create | `tests/TodoStoreTests.swift` | Unit tests for TodoStore |
| Create | `tests/IdeaStoreTests.swift` | Unit tests for IdeaStore |
| Modify | `Sources/UI/Panel/AIPanelView.swift` | Remove mock data, add delegate callback for idea input |
| Modify | `Sources/App/PanelCoordinator.swift` | Wire stores to panel, implement idea delegate |
| Modify | `Sources/App/AppDelegate.swift` | Load stores at launch |
| Modify | `Sources/Core/AgentHead.swift` | Stub `updateTodoFromWebhook` |

---

### Task 1: TodoStore — Model + Persistence

**Files:**
- Create: `Sources/Core/TodoStore.swift`
- Create: `tests/TodoStoreTests.swift`

- [ ] **Step 1: Write failing tests**

```swift
// tests/TodoStoreTests.swift
import XCTest
@testable import amux

final class TodoStoreTests: XCTestCase {
    private var store: TodoStore!
    private var tempDir: URL!

    override func setUp() {
        super.setUp()
        tempDir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try! FileManager.default.createDirectory(at: tempDir, withIntermediateDirectories: true)
        store = TodoStore(directory: tempDir)
    }

    override func tearDown() {
        try? FileManager.default.removeItem(at: tempDir)
        super.tearDown()
    }

    func testAddAndRetrieve() {
        let item = store.add(task: "Fix bug", project: "amux", branch: "fix-bug", issue: "#42")
        XCTAssertEqual(item.task, "Fix bug")
        XCTAssertEqual(item.project, "amux")
        XCTAssertEqual(item.branch, "fix-bug")
        XCTAssertEqual(item.issue, "#42")
        XCTAssertEqual(item.status, "pending_approval")
        XCTAssertEqual(store.allItems().count, 1)
    }

    func testUpdateStatus() {
        let item = store.add(task: "Task", project: "p", branch: nil, issue: nil)
        store.update(id: item.id, status: "running", progress: "Working on it")
        let updated = store.allItems().first!
        XCTAssertEqual(updated.status, "running")
        XCTAssertEqual(updated.progress, "Working on it")
        XCTAssertGreaterThan(updated.updatedAt, item.updatedAt)
    }

    func testRemove() {
        let item = store.add(task: "Task", project: "p", branch: nil, issue: nil)
        store.remove(id: item.id)
        XCTAssertTrue(store.allItems().isEmpty)
    }

    func testSaveAndLoad() {
        _ = store.add(task: "Task A", project: "amux", branch: "main", issue: nil)
        _ = store.add(task: "Task B", project: "pmux", branch: nil, issue: "#10")
        store.saveSync()

        let loaded = TodoStore(directory: tempDir)
        loaded.load()
        XCTAssertEqual(loaded.allItems().count, 2)
        XCTAssertEqual(loaded.allItems().first?.task, "Task A")
    }

    func testLoadMissingFileReturnsEmpty() {
        let emptyDir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try! FileManager.default.createDirectory(at: emptyDir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: emptyDir) }

        let fresh = TodoStore(directory: emptyDir)
        fresh.load()
        XCTAssertTrue(fresh.allItems().isEmpty)
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `xcodegen generate && xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/TodoStoreTests 2>&1 | tail -10`
Expected: Compilation error — `TodoStore` does not exist.

- [ ] **Step 3: Implement TodoStore**

```swift
// Sources/Core/TodoStore.swift
import Foundation

struct TodoItem: Codable, Identifiable {
    let id: String
    var task: String
    var status: String
    var project: String
    var branch: String?
    var issue: String?
    var progress: String?
    let createdAt: Date
    var updatedAt: Date
}

class TodoStore {
    static let shared = TodoStore(directory: Config.configDir)

    private let filePath: URL
    private var items: [TodoItem] = []
    private let saveQueue = DispatchQueue(label: "com.amux.todo-save", qos: .utility)
    private var pendingSave: DispatchWorkItem?

    init(directory: URL) {
        self.filePath = directory.appendingPathComponent("todos.json")
    }

    func load() {
        guard FileManager.default.fileExists(atPath: filePath.path) else { return }
        do {
            let data = try Data(contentsOf: filePath)
            let decoder = JSONDecoder()
            decoder.dateDecodingStrategy = .iso8601
            items = try decoder.decode([TodoItem].self, from: data)
        } catch {
            NSLog("[TodoStore] Failed to load: \(error)")
        }
    }

    func save() {
        let snapshot = items
        pendingSave?.cancel()
        let work = DispatchWorkItem {
            self.write(snapshot)
        }
        pendingSave = work
        saveQueue.asyncAfter(deadline: .now() + 0.3, execute: work)
    }

    /// Synchronous save for tests.
    func saveSync() {
        write(items)
    }

    private func write(_ snapshot: [TodoItem]) {
        do {
            try FileManager.default.createDirectory(at: filePath.deletingLastPathComponent(), withIntermediateDirectories: true)
            let encoder = JSONEncoder()
            encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
            encoder.dateEncodingStrategy = .iso8601
            let data = try encoder.encode(snapshot)
            try data.write(to: filePath, options: .atomic)
        } catch {
            NSLog("[TodoStore] Failed to save: \(error)")
        }
    }

    @discardableResult
    func add(task: String, project: String, branch: String?, issue: String?) -> TodoItem {
        let now = Date()
        let item = TodoItem(
            id: UUID().uuidString,
            task: task,
            status: "pending_approval",
            project: project,
            branch: branch,
            issue: issue,
            progress: nil,
            createdAt: now,
            updatedAt: now
        )
        items.append(item)
        save()
        return item
    }

    func update(id: String, status: String?, progress: String?) {
        guard let idx = items.firstIndex(where: { $0.id == id }) else { return }
        if let status { items[idx].status = status }
        if let progress { items[idx].progress = progress }
        items[idx].updatedAt = Date()
        save()
    }

    func remove(id: String) {
        items.removeAll { $0.id == id }
        save()
    }

    func allItems() -> [TodoItem] {
        items
    }
}
```

- [ ] **Step 4: Regenerate project and run tests**

Run: `xcodegen generate && xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/TodoStoreTests 2>&1 | tail -10`
Expected: All 5 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add Sources/Core/TodoStore.swift tests/TodoStoreTests.swift
git commit -m "feat: add TodoStore with JSON persistence and CRUD"
```

---

### Task 2: IdeaStore — Model + Persistence

**Files:**
- Create: `Sources/Core/IdeaStore.swift`
- Create: `tests/IdeaStoreTests.swift`

- [ ] **Step 1: Write failing tests**

```swift
// tests/IdeaStoreTests.swift
import XCTest
@testable import amux

final class IdeaStoreTests: XCTestCase {
    private var store: IdeaStore!
    private var tempDir: URL!

    override func setUp() {
        super.setUp()
        tempDir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try! FileManager.default.createDirectory(at: tempDir, withIntermediateDirectories: true)
        store = IdeaStore(directory: tempDir)
    }

    override func tearDown() {
        try? FileManager.default.removeItem(at: tempDir)
        super.tearDown()
    }

    func testAddAndRetrieve() {
        let item = store.add(text: "Add dark mode", project: "amux", source: "manual", tags: ["ui"])
        XCTAssertEqual(item.text, "Add dark mode")
        XCTAssertEqual(item.project, "amux")
        XCTAssertEqual(item.source, "manual")
        XCTAssertEqual(item.tags, ["ui"])
        XCTAssertEqual(store.allItems().count, 1)
    }

    func testRemove() {
        let item = store.add(text: "Idea", project: "p", source: "manual", tags: [])
        store.remove(id: item.id)
        XCTAssertTrue(store.allItems().isEmpty)
    }

    func testSaveAndLoad() {
        _ = store.add(text: "Idea A", project: "amux", source: "manual", tags: ["perf"])
        _ = store.add(text: "Idea B", project: "pmux", source: "wechat", tags: [])
        store.saveSync()

        let loaded = IdeaStore(directory: tempDir)
        loaded.load()
        XCTAssertEqual(loaded.allItems().count, 2)
        XCTAssertEqual(loaded.allItems().first?.text, "Idea A")
    }

    func testNewItemsInsertAtFront() {
        _ = store.add(text: "First", project: "p", source: "manual", tags: [])
        _ = store.add(text: "Second", project: "p", source: "manual", tags: [])
        XCTAssertEqual(store.allItems().first?.text, "Second")
    }

    func testLoadMissingFileReturnsEmpty() {
        let emptyDir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try! FileManager.default.createDirectory(at: emptyDir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: emptyDir) }

        let fresh = IdeaStore(directory: emptyDir)
        fresh.load()
        XCTAssertTrue(fresh.allItems().isEmpty)
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `xcodegen generate && xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/IdeaStoreTests 2>&1 | tail -10`
Expected: Compilation error — `IdeaStore` does not exist.

- [ ] **Step 3: Implement IdeaStore**

```swift
// Sources/Core/IdeaStore.swift
import Foundation

struct IdeaItem: Codable, Identifiable {
    let id: String
    var text: String
    var project: String
    var source: String
    var tags: [String]
    let createdAt: Date
}

class IdeaStore {
    static let shared = IdeaStore(directory: Config.configDir)

    private let filePath: URL
    private var items: [IdeaItem] = []
    private let saveQueue = DispatchQueue(label: "com.amux.idea-save", qos: .utility)
    private var pendingSave: DispatchWorkItem?

    init(directory: URL) {
        self.filePath = directory.appendingPathComponent("ideas.json")
    }

    func load() {
        guard FileManager.default.fileExists(atPath: filePath.path) else { return }
        do {
            let data = try Data(contentsOf: filePath)
            let decoder = JSONDecoder()
            decoder.dateDecodingStrategy = .iso8601
            items = try decoder.decode([IdeaItem].self, from: data)
        } catch {
            NSLog("[IdeaStore] Failed to load: \(error)")
        }
    }

    func save() {
        let snapshot = items
        pendingSave?.cancel()
        let work = DispatchWorkItem {
            self.write(snapshot)
        }
        pendingSave = work
        saveQueue.asyncAfter(deadline: .now() + 0.3, execute: work)
    }

    /// Synchronous save for tests.
    func saveSync() {
        write(items)
    }

    private func write(_ snapshot: [IdeaItem]) {
        do {
            try FileManager.default.createDirectory(at: filePath.deletingLastPathComponent(), withIntermediateDirectories: true)
            let encoder = JSONEncoder()
            encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
            encoder.dateEncodingStrategy = .iso8601
            let data = try encoder.encode(snapshot)
            try data.write(to: filePath, options: .atomic)
        } catch {
            NSLog("[IdeaStore] Failed to save: \(error)")
        }
    }

    @discardableResult
    func add(text: String, project: String, source: String, tags: [String]) -> IdeaItem {
        let item = IdeaItem(
            id: UUID().uuidString,
            text: text,
            project: project,
            source: source,
            tags: tags,
            createdAt: Date()
        )
        items.insert(item, at: 0)
        save()
        return item
    }

    func remove(id: String) {
        items.removeAll { $0.id == id }
        save()
    }

    func allItems() -> [IdeaItem] {
        items
    }
}
```

- [ ] **Step 4: Regenerate project and run tests**

Run: `xcodegen generate && xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/IdeaStoreTests 2>&1 | tail -10`
Expected: All 5 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add Sources/Core/IdeaStore.swift tests/IdeaStoreTests.swift
git commit -m "feat: add IdeaStore with JSON persistence and CRUD"
```

---

### Task 3: AIPanelView — Remove Mock Data + Add Delegate Callback

**Files:**
- Modify: `Sources/UI/Panel/AIPanelView.swift`

- [ ] **Step 1: Extend AIPanelDelegate with idea submit callback**

Replace the existing protocol (line 3-5):

```swift
protocol AIPanelDelegate: AnyObject {
    func aiPanelDidRequestClose()
}
```

With:

```swift
protocol AIPanelDelegate: AnyObject {
    func aiPanelDidRequestClose()
    func aiPanelDidSubmitIdea(_ text: String)
}
```

- [ ] **Step 2: Replace sendCurrentInput to use delegate**

Replace the `sendCurrentInput()` method (lines 742-766):

```swift
private func sendCurrentInput() {
    let text = inputTextView.string.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !text.isEmpty else { return }

    inputTextView.string = ""
    placeholderLabel.isHidden = false
    sendButton.contentTintColor = SemanticColors.muted
    updateInputHeight()

    delegate?.aiPanelDidSubmitIdea(text)
}
```

- [ ] **Step 3: Delete loadSampleData and its call**

Delete the entire `// MARK: - Sample Data` section (lines 800-816):

```swift
// MARK: - Sample Data

private func loadSampleData() {
    todoItems = [
        ...
    ]

    ideaItems = [
        ...
    ]
}
```

And remove the call in `setup()` (line 415):

```swift
// Load sample data for demo
loadSampleData()
```

- [ ] **Step 4: Verify build**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -5`

This will fail because `PanelCoordinator` conforms to `AIPanelDelegate` but doesn't implement the new method yet. That's expected — Task 4 fixes it.

- [ ] **Step 5: Commit (even with build warning — Task 4 completes it)**

```bash
git add Sources/UI/Panel/AIPanelView.swift
git commit -m "feat: remove mock data from AIPanelView, add idea submit delegate"
```

---

### Task 4: PanelCoordinator — Wire Stores to UI

**Files:**
- Modify: `Sources/App/PanelCoordinator.swift`

- [ ] **Step 1: Add store data loading in toggleAIPanel**

Replace the `toggleAIPanel()` method (lines 65-79):

```swift
func toggleAIPanel() {
    if aiPopover.isShown {
        aiPopover.performClose(nil)
        aiPanel.setOpen(false, animated: false)
        return
    }

    notificationPopover.performClose(nil)
    notificationPanel.setOpen(false, animated: false)

    // Feed real data from stores
    refreshAIPanelData()

    aiPanel.setOpen(true, animated: false)
    guard let titleBar else { return }
    let anchor = titleBar.aiAnchorView()
    aiPopover.show(relativeTo: anchor.bounds, of: anchor, preferredEdge: .maxY)
}
```

- [ ] **Step 2: Add refreshAIPanelData method**

Add after `toggleAIPanel()`:

```swift
private func refreshAIPanelData() {
    let formatter = DateFormatter()
    formatter.dateFormat = "HH:mm"

    let todoDisplayItems = TodoStore.shared.allItems().map { item in
        AIPanelView.TodoDisplayItem(
            id: item.id.hashValue,
            task: item.task,
            status: item.status,
            issue: item.issue,
            worktree: item.branch,
            progress: item.progress
        )
    }

    let ideaDisplayItems = IdeaStore.shared.allItems().map { item in
        AIPanelView.IdeaDisplayItem(
            timestamp: formatter.string(from: item.createdAt),
            text: item.text,
            source: item.source,
            tags: item.tags
        )
    }

    aiPanel.updateTodoItems(todoDisplayItems)
    aiPanel.updateIdeaItems(ideaDisplayItems)
}
```

- [ ] **Step 3: Implement aiPanelDidSubmitIdea in AIPanelDelegate conformance**

Replace the existing `AIPanelDelegate` extension (lines 102-107):

```swift
// MARK: - AIPanelDelegate

extension PanelCoordinator: AIPanelDelegate {
    func aiPanelDidRequestClose() {
        aiPopover.performClose(nil)
        aiPanel.setOpen(false, animated: false)
    }

    func aiPanelDidSubmitIdea(_ text: String) {
        IdeaStore.shared.add(text: text, project: "amux", source: "manual", tags: [])
        refreshAIPanelData()
    }
}
```

Note: The `project` field is hardcoded to "amux" for now. In the future when multi-project is needed, this can be derived from the active tab context.

- [ ] **Step 4: Verify build**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -5`
Expected: BUILD SUCCEEDED

- [ ] **Step 5: Commit**

```bash
git add Sources/App/PanelCoordinator.swift
git commit -m "feat: wire TodoStore and IdeaStore to AIPanelView via PanelCoordinator"
```

---

### Task 5: AppDelegate — Load Stores at Launch

**Files:**
- Modify: `Sources/App/AppDelegate.swift`

- [ ] **Step 1: Add store loading after config load**

In `applicationDidFinishLaunching`, add after the `ClaudeHooksSetup` block (after line 18) and before `GhosttyBridge.shared.initialize()`:

```swift
// Load TODO and Ideas stores
TodoStore.shared.load()
IdeaStore.shared.load()
```

- [ ] **Step 2: Verify build**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -5`
Expected: BUILD SUCCEEDED

- [ ] **Step 3: Commit**

```bash
git add Sources/App/AppDelegate.swift
git commit -m "feat: load TodoStore and IdeaStore at app launch"
```

---

### Task 6: AgentHead — Stub updateTodoFromWebhook

**Files:**
- Modify: `Sources/Core/AgentHead.swift`
- Modify: `Sources/App/TabCoordinator.swift`

- [ ] **Step 1: Add stub method to AgentHead**

Add at the end of the `AgentHead` class, before the closing brace:

```swift
// MARK: - TODO Status Updates (Future)

/// Stub for future webhook-driven TODO status updates.
/// Future logic: match event.cwd → worktree path → branch name → TodoItem.branch,
/// then update status based on event type:
///   - SessionStart → "running"
///   - Stop (end_turn) → "completed"
///   - StopFailure → "failed"
///   - SubagentStart → update progress
func updateTodoFromWebhook(_ event: WebhookEvent) {
    // Not yet implemented — will be filled when AgentHead status
    // pipeline is connected to TodoStore.
}
```

- [ ] **Step 2: Add commented-out call site in TabCoordinator**

In `Sources/App/TabCoordinator.swift`, find the webhook server setup block where events are dispatched (the `WebhookServer` callback closure). It looks like:

```swift
let server = WebhookServer(port: self.config.webhook.port) { [weak self] event in
    self?.statusPublisher.webhookProvider.handleEvent(event)
    AgentHead.shared.handleWebhookEvent(event)
}
```

Add a commented line after `AgentHead.shared.handleWebhookEvent(event)`:

```swift
let server = WebhookServer(port: self.config.webhook.port) { [weak self] event in
    self?.statusPublisher.webhookProvider.handleEvent(event)
    AgentHead.shared.handleWebhookEvent(event)
    // TODO: Enable when webhook→TODO matching logic is implemented
    // AgentHead.shared.updateTodoFromWebhook(event)
}
```

- [ ] **Step 3: Verify build**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -5`
Expected: BUILD SUCCEEDED

- [ ] **Step 4: Commit**

```bash
git add Sources/Core/AgentHead.swift Sources/App/TabCoordinator.swift
git commit -m "feat: stub AgentHead.updateTodoFromWebhook for future webhook-driven TODO updates"
```

---

### Task 7: Run Full Test Suite

- [ ] **Step 1: Run all unit tests**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/TodoStoreTests -only-testing:amuxTests/IdeaStoreTests -only-testing:amuxTests/PaneTransferTests 2>&1 | tail -15`
Expected: All tests PASS (TodoStore 5 + IdeaStore 5 + PaneTransfer 7 = 17 tests).

- [ ] **Step 2: Verify full build**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -5`
Expected: BUILD SUCCEEDED

- [ ] **Step 3: Manual verification**

1. Launch amux
2. Open AI panel (click the AI button in title bar)
3. Verify TODO tab shows "No tasks yet" (empty state, no mock data)
4. Switch to Ideas tab — verify "No ideas yet" empty state
5. Type an idea in the input box, press Enter
6. Verify idea appears in the list with timestamp and "manual" source
7. Quit and relaunch — verify the idea persists
8. Check `~/.config/amux/ideas.json` contains the saved idea
