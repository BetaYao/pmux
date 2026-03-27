# UI Performance Optimization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eliminate UI sluggishness caused by polling-driven full view rebuilds, excessive object allocation, and CPU-side drawing.

**Architecture:** Convert from "destroy-and-recreate" to "diff-and-update" pattern. Cache color objects. Move polling off main thread. Lazy-load layout hierarchies. Replace Grid manual frames with NSCollectionView. Add sidebar cell reuse.

**Tech Stack:** Swift 5.10, AppKit, NSCollectionView, CALayer

**Spec:** `docs/superpowers/specs/2026-03-20-ui-performance-optimization-design.md`

---

### Task 1: SemanticColors — Cache Instances (`static var` → `static let`)

**Files:**
- Modify: `Sources/UI/Shared/SemanticColors.swift:21-84`
- Test: `Tests/PerformanceTests.swift` (existing `testSemanticColorsAllocationPerformance`)

This is the simplest change with highest impact. `NSColor(name:)` already resolves dynamically per appearance — the closure is called by AppKit on demand. Caching the NSColor instance is safe.

- [ ] **Step 1: Change all `static var` to `static let`**

In `Sources/UI/Shared/SemanticColors.swift`, replace every `static var` with `static let`:

```swift
enum SemanticColors {
    // MARK: - Backgrounds
    static let bg = NSColor(name: nil) { $0.isDark ? NSColor(hex: 0x1a1a1a) : NSColor(hex: 0xf3f4f7) }
    static let panel = NSColor(name: nil) { $0.isDark ? NSColor(hex: 0x202020) : NSColor(hex: 0xffffff) }
    static let panel2 = NSColor(name: nil) { $0.isDark ? NSColor(hex: 0x282828) : NSColor(hex: 0xf7f8fb) }

    // MARK: - Text
    static let text = NSColor(name: nil) { $0.isDark ? NSColor(hex: 0xe8e8e8) : NSColor(hex: 0x1f232b) }
    static let muted = NSColor(name: nil) { $0.isDark ? NSColor(hex: 0x999999) : NSColor(hex: 0x636b78) }

    // MARK: - Borders
    static let line = NSColor(name: nil) { $0.isDark ? NSColor(hex: 0x3a3a3a) : NSColor(hex: 0xd7dbe3) }
    static let cardBorder = NSColor(name: nil) { $0.isDark ? NSColor(hex: 0x363636) : NSColor(hex: 0xe2e5eb) }
    static let cardBorderHover = NSColor(name: nil) { $0.isDark ? NSColor(hex: 0x505050) : NSColor(hex: 0xbcc2cc) }
    static let cardBorderSelected = NSColor(name: nil) { $0.isDark ? NSColor(hex: 0x2d8cf0) : NSColor(hex: 0x2563eb) }

    // MARK: - Status
    static let running = NSColor(name: nil) { $0.isDark ? NSColor(hex: 0x33c17b) : NSColor(hex: 0x1f9d63) }
    static let waiting = NSColor(name: nil) { $0.isDark ? NSColor(hex: 0x3b82f6) : NSColor(hex: 0x2563eb) }
    static let idle = NSColor(name: nil) { $0.isDark ? NSColor(hex: 0x9ca3af) : NSColor(hex: 0x8a93a1) }
    static let accent = NSColor(name: nil) { $0.isDark ? NSColor(hex: 0x0e72ed) : NSColor(hex: 0x2563eb) }
    static let danger = NSColor(name: nil) { $0.isDark ? NSColor(hex: 0xff453a) : NSColor(hex: 0xdc2626) }
}
```

- [ ] **Step 2: Run performance test to verify improvement**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/PerformanceTests/testSemanticColorsAllocationPerformance 2>&1 | grep measured`

Expected: Average time should drop significantly (from ~84ms to <10ms) since NSColor instances are now reused.

- [ ] **Step 3: Run full test suite to verify no regressions**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test 2>&1 | grep -E '(Test Suite|Executed|FAILED)'`

Expected: All tests pass.

- [ ] **Step 4: Commit**

```bash
git add Sources/UI/Shared/SemanticColors.swift
git commit -m "perf: cache SemanticColors as static let instead of computed var"
```

---

### Task 2: Theme — Remove Recursive Refresh

**Files:**
- Modify: `Sources/UI/Shared/Theme.swift:28-48`

With `SemanticColors` using `NSColor(name:)`, AppKit automatically propagates appearance changes to layer-backed views. The recursive `refreshSubviews()` call is unnecessary overhead.

- [ ] **Step 1: Simplify `applyAppearance` to remove recursive refresh**

Replace lines 28-34 in `Sources/UI/Shared/Theme.swift`:

```swift
// Before:
DispatchQueue.main.async {
    for window in NSApp.windows {
        refreshSubviews(window.contentView)
    }
    NotificationCenter.default.post(name: .themeDidChange, object: nil)
}

// After:
DispatchQueue.main.async {
    NotificationCenter.default.post(name: .themeDidChange, object: nil)
}
```

Remove the `refreshSubviews` method entirely (lines 37-48).

- [ ] **Step 2: Run full test suite**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test 2>&1 | grep -E '(Test Suite|Executed|FAILED)'`

Expected: All tests pass. Theme switching still works because AppKit's appearance propagation triggers `updateLayer()` on all layer-backed views.

- [ ] **Step 3: Commit**

```bash
git add Sources/UI/Shared/Theme.swift
git commit -m "perf: remove recursive refreshSubviews — AppKit propagates appearance natively"
```

---

### Task 3: StatusBadge — Layer-Backed Rendering

**Files:**
- Modify: `Sources/UI/Shared/StatusBadge.swift`

Replace CPU-side `draw(_:)` with GPU-cached CALayer properties.

- [ ] **Step 1: Rewrite StatusBadge to use updateLayer()**

```swift
import AppKit

class StatusBadge: NSView {
    var status: AgentStatus = .unknown {
        didSet {
            if status != oldValue { needsDisplay = true }
        }
    }

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        wantsLayer = true
    }

    required init?(coder: NSCoder) {
        super.init(coder: coder)
        wantsLayer = true
    }

    override var wantsUpdateLayer: Bool { true }

    override func updateLayer() {
        let size = min(bounds.width, bounds.height)
        layer?.cornerRadius = size / 2
        effectiveAppearance.performAsCurrentDrawingAppearance {
            layer?.backgroundColor = status.color.cgColor
        }
    }

    override var intrinsicContentSize: NSSize {
        NSSize(width: Theme.statusBadgeSize, height: Theme.statusBadgeSize)
    }
}
```

- [ ] **Step 2: Run test suite**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test 2>&1 | grep -E '(Test Suite|Executed|FAILED)'`

Expected: All tests pass.

- [ ] **Step 3: Commit**

```bash
git add Sources/UI/Shared/StatusBadge.swift
git commit -m "perf: StatusBadge uses layer cornerRadius instead of draw()"
```

---

### Task 4: StatusPublisher — Diff-Based Notification + Background Polling

**Files:**
- Modify: `Sources/Status/StatusPublisher.swift`
- Test: `Tests/PerformanceTests.swift` (existing `testStatusDetectorPerformance`)

Move the expensive `detect()` + `extractLastMessage()` off main thread. Only fire delegate when status or message actually changes.

**Thread safety:** `trackers` and `lastMessages` are accessed from both main thread (`start()`, `updateSurfaces()`, `status(for:)`, `lastMessage(for:)`) and the poll queue. All mutable state access is serialized through `pollQueue`. Main-thread read methods (`status(for:)`, `lastMessage(for:)`) use `pollQueue.sync` for safe reads.

- [ ] **Step 1: Add background queue and rewrite for thread safety**

Rewrite `StatusPublisher`:

```swift
class StatusPublisher {
    weak var delegate: StatusPublisherDelegate?

    private let detector = StatusDetector()
    private var trackers: [String: DebouncedStatusTracker] = [:]
    private var timer: Timer?
    private var surfaces: [String: TerminalSurface] = [:]
    private var agentConfig: AgentDetectConfig
    private var lastMessages: [String: String] = [:]
    private(set) var webhookProvider = WebhookStatusProvider()

    private let pollInterval: TimeInterval = 2.0
    private let pollQueue = DispatchQueue(label: "com.amux.statusPoll", qos: .utility)

    // ... init, start, stop, updateSurfaces unchanged except pollAll body ...

    private func pollAll() {
        // Snapshot surfaces on main thread (timer fires on main)
        let surfaceSnapshot = surfaces

        pollQueue.async { [weak self] in
            guard let self else { return }
            var updates: [(path: String, oldStatus: AgentStatus, newStatus: AgentStatus, lastMessage: String)] = []

            for (path, surface) in surfaceSnapshot {
                let tracker = self.trackers[path] ?? {
                    let t = DebouncedStatusTracker()
                    self.trackers[path] = t
                    return t
                }()

                let processStatus = surface.processStatus
                let content = surface.readViewportText() ?? ""
                let agentDef = self.findAgentDef(in: content)

                let textStatus = self.detector.detect(
                    processStatus: processStatus,
                    shellInfo: nil,
                    content: content,
                    agentDef: agentDef
                )
                let hookStatus = self.webhookProvider.status(for: path)
                let detected = AgentStatus.highestPriority([textStatus, hookStatus])
                let lastMessage = agentDef?.extractLastMessage(from: content, maxLen: 80) ?? ""

                let oldStatus = tracker.currentStatus
                let statusChanged = tracker.update(status: detected)
                let messageChanged = (self.lastMessages[path] != lastMessage)
                self.lastMessages[path] = lastMessage

                if statusChanged || messageChanged {
                    updates.append((path: path, oldStatus: oldStatus, newStatus: detected, lastMessage: lastMessage))
                }
            }

            if !updates.isEmpty {
                DispatchQueue.main.async { [weak self] in
                    guard let self else { return }
                    for update in updates {
                        self.delegate?.statusDidChange(
                            worktreePath: update.path,
                            oldStatus: update.oldStatus,
                            newStatus: update.newStatus,
                            lastMessage: update.lastMessage
                        )
                    }
                }
            }
        }
    }

    // Thread-safe reads from main thread
    func status(for path: String) -> AgentStatus {
        pollQueue.sync { trackers[path]?.currentStatus ?? .unknown }
    }

    func lastMessage(for path: String) -> String {
        pollQueue.sync { lastMessages[path] ?? "" }
    }
}
```

- [ ] **Step 2: Run performance test**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/PerformanceTests/testStatusDetectorPerformance 2>&1 | grep measured`

Expected: Test still passes (tests the detector directly, not the publisher).

- [ ] **Step 3: Run full test suite**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test 2>&1 | grep -E '(Test Suite|Executed|FAILED)'`

- [ ] **Step 4: Commit**

```bash
git add Sources/Status/StatusPublisher.swift
git commit -m "perf: poll status on background queue, only notify on actual changes"
```

---

### Task 5: AgentCardView / MiniCardView — Early-Return on Unchanged Status

**Files:**
- Modify: `Sources/UI/Dashboard/AgentCardView.swift`
- Modify: `Sources/UI/Dashboard/MiniCardView.swift`

`configure()` always calls `updateAppearance()`, which resolves dynamic colors via `performAsCurrentDrawingAppearance`. Skip this when only the message text changed and the visual border state (selected/hovered) is unchanged.

- [ ] **Step 1: Add status tracking to AgentCardView**

In `AgentCardView`, add a stored property:

```swift
private var currentStatus: String = ""
```

In `configure()`, track status and skip `updateAppearance()` when status unchanged:

```swift
func configure(id: String, project: String, thread: String, status: String, lastMessage: String, totalDuration: String, roundDuration: String) {
    agentId = id
    setAccessibilityIdentifier("dashboard.card.\(id)")

    titleLabel.stringValue = "\(project) - \(thread)"
    messageLabel.stringValue = lastMessage
    statusDot.layer?.backgroundColor = AgentDisplayHelpers.statusColor(status).cgColor

    let compactTotal = AgentDisplayHelpers.compactDuration(totalDuration)
    let compactRound = AgentDisplayHelpers.compactDuration(roundDuration)
    timeLabel.stringValue = "\u{03A3} \(compactTotal) \u{00B7} \u{27F3} \(compactRound)"

    if status != currentStatus {
        currentStatus = status
        updateAppearance()
    }
}
```

- [ ] **Step 2: Same change in MiniCardView**

Apply identical pattern: add `private var currentStatus: String = ""`, skip `updateAppearance()` when status unchanged.

- [ ] **Step 3: Run test suite**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test 2>&1 | grep -E '(Test Suite|Executed|FAILED)'`

- [ ] **Step 4: Commit**

```bash
git add Sources/UI/Dashboard/AgentCardView.swift Sources/UI/Dashboard/MiniCardView.swift
git commit -m "perf: skip updateAppearance() in card configure when status unchanged"
```

---

### Task 6: Dashboard — In-Place Card Updates (No Rebuild on Status Change)

**Files:**
- Modify: `Sources/UI/Dashboard/DashboardViewController.swift`

The core performance fix: stop destroying and recreating all cards every 2 seconds. Instead, look up existing cards by ID and call `configure()` to update in-place.

- [ ] **Step 1: Add card lookup dictionaries**

Add these properties to `DashboardViewController` after the existing card arrays:

```swift
// Card lookup for in-place updates (no grid — Task 8 replaces grid with NSCollectionView)
private var leftRightCardsByID: [String: MiniCardView] = [:]
private var topSmallCardsByID: [String: MiniCardView] = [:]
private var topLargeCardsByID: [String: MiniCardView] = [:]
```

- [ ] **Step 2: Rewrite `updateAgents()` to diff instead of rebuild**

Replace the existing `updateAgents()` method:

```swift
func updateAgents(_ newAgents: [AgentDisplayInfo]) {
    let oldIDs = Set(agents.map { $0.id })
    let newIDs = Set(newAgents.map { $0.id })
    agents = newAgents

    // Validate selectedAgentId
    if !agents.contains(where: { $0.id == selectedAgentId }) {
        selectedAgentId = sortedAgents().first?.id ?? ""
    }

    // If the set of agents changed (added/removed), do a full rebuild
    if oldIDs != newIDs {
        rebuildCurrentLayout()
        return
    }

    // Otherwise, update existing cards in-place
    updateCardsInPlace()
}
```

- [ ] **Step 3: Add `updateCardsInPlace()` method**

Add after `updateAgents()`:

```swift
private func updateCardsInPlace() {
    let sorted = sortedAgents()

    switch currentLayout {
    case .grid:
        // NSCollectionView handles updates via reloadData (see Task 8)
        rebuildGrid()
    case .leftRight:
        updateFocusModeInPlace(focusPanel: leftRightFocusPanel, cardsByID: leftRightCardsByID, sorted: sorted)
    case .topSmall:
        updateFocusModeInPlace(focusPanel: topSmallFocusPanel, cardsByID: topSmallCardsByID, sorted: sorted)
    case .topLarge:
        updateFocusModeInPlace(focusPanel: topLargeFocusPanel, cardsByID: topLargeCardsByID, sorted: sorted)
    }
}

/// Shared helper for focus-mode in-place updates (eliminates duplication across 3 layouts)
private func updateFocusModeInPlace(focusPanel: FocusPanelView, cardsByID: [String: MiniCardView], sorted: [AgentDisplayInfo]) {
    if let selected = sorted.first(where: { $0.id == selectedAgentId }) {
        configureFocusPanel(focusPanel, with: selected)
    }
    for agent in sorted {
        if let card = cardsByID[agent.id] {
            card.configure(
                id: agent.id, project: agent.project, thread: agent.thread,
                status: agent.status, lastMessage: agent.lastMessage,
                totalDuration: agent.totalDuration, roundDuration: agent.roundDuration
            )
            card.isSelected = (agent.id == selectedAgentId)
        }
    }
}
```

- [ ] **Step 4: Populate lookup dictionaries in rebuild methods**

In `rebuildLeftRight()`:
```swift
// At start:
leftRightCardsByID.removeAll()
// After creating each card:
leftRightCardsByID[agent.id] = card
```

Same pattern for `rebuildTopSmall()` → `topSmallCardsByID` and `rebuildTopLarge()` → `topLargeCardsByID`.

- [ ] **Step 5: Run performance test**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/PerformanceTests/testFullRebuildCyclePerformance 2>&1 | grep measured`

Expected: Test still passes (measures the rebuild path which still exists for adds/removes).

- [ ] **Step 6: Run full test suite**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test 2>&1 | grep -E '(Test Suite|Executed|FAILED)'`

- [ ] **Step 7: Commit**

```bash
git add Sources/UI/Dashboard/DashboardViewController.swift
git commit -m "perf: dashboard cards update in-place instead of full rebuild on status change"
```

---

### Task 7: Dashboard — Lazy Layout Instantiation

**Files:**
- Modify: `Sources/UI/Dashboard/DashboardViewController.swift`

Only instantiate the active layout mode on `loadView()`. Other layouts created on-demand.

- [ ] **Step 1: Track instantiated layouts**

Add property:
```swift
private var instantiatedLayouts: Set<DashboardLayout> = []
```

- [ ] **Step 2: Make setup methods idempotent and lazy**

Replace `loadView()` lines 86-91:

```swift
// Before:
setupGridLayout()
setupLeftRightLayout()
setupTopSmallLayout()
setupTopLargeLayout()
showLayout(currentLayout)

// After:
ensureLayoutInstantiated(currentLayout)
showLayout(currentLayout)
```

Add helper:
```swift
private func ensureLayoutInstantiated(_ layout: DashboardLayout) {
    guard !instantiatedLayouts.contains(layout) else { return }
    switch layout {
    case .grid: setupGridLayout()
    case .leftRight: setupLeftRightLayout()
    case .topSmall: setupTopSmallLayout()
    case .topLarge: setupTopLargeLayout()
    }
    instantiatedLayouts.insert(layout)
}
```

- [ ] **Step 3: Update `setLayout()` to lazy-instantiate**

```swift
func setLayout(_ layout: DashboardLayout) {
    guard layout != currentLayout else { return }
    detachTerminals()
    ensureLayoutInstantiated(layout)
    currentLayout = layout
    showLayout(layout)
    rebuildCurrentLayout()
}
```

- [ ] **Step 4: Update `showLayout()` — only hide instantiated layouts**

```swift
private func showLayout(_ layout: DashboardLayout) {
    if instantiatedLayouts.contains(.grid) { gridScrollView.isHidden = true }
    if instantiatedLayouts.contains(.leftRight) { leftRightContainer.isHidden = true }
    if instantiatedLayouts.contains(.topSmall) { topSmallContainer.isHidden = true }
    if instantiatedLayouts.contains(.topLarge) { topLargeContainer.isHidden = true }

    switch layout {
    case .grid: gridScrollView.isHidden = false
    case .leftRight: leftRightContainer.isHidden = false
    case .topSmall: topSmallContainer.isHidden = false
    case .topLarge: topLargeContainer.isHidden = false
    }
}
```

- [ ] **Step 5: Update `detachTerminals()` — only detach instantiated**

```swift
func detachTerminals() {
    if instantiatedLayouts.contains(.leftRight) {
        leftRightFocusPanel.terminalContainer.subviews.forEach { $0.removeFromSuperview() }
    }
    if instantiatedLayouts.contains(.topSmall) {
        topSmallFocusPanel.terminalContainer.subviews.forEach { $0.removeFromSuperview() }
    }
    if instantiatedLayouts.contains(.topLarge) {
        topLargeFocusPanel.terminalContainer.subviews.forEach { $0.removeFromSuperview() }
    }
}
```

- [ ] **Step 6: Run full test suite**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test 2>&1 | grep -E '(Test Suite|Executed|FAILED)'`

- [ ] **Step 7: Commit**

```bash
git add Sources/UI/Dashboard/DashboardViewController.swift
git commit -m "perf: lazy-instantiate dashboard layouts on first use"
```

---

### Task 8: Grid Mode — Replace Manual Frames with NSCollectionView

**Files:**
- Modify: `Sources/UI/Dashboard/DashboardViewController.swift`

Replace the manual frame-based grid with `NSCollectionView` for native cell reuse and layout.

- [ ] **Step 1: Create NSCollectionViewItem subclass**

Add at the bottom of `DashboardViewController.swift` (or in a new section):

```swift
// MARK: - Grid Collection View Item

private final class AgentCardItem: NSCollectionViewItem {
    static let identifier = NSUserInterfaceItemIdentifier("AgentCardItem")

    let cardView = AgentCardView()

    override func loadView() {
        self.view = cardView
    }
}
```

- [ ] **Step 2: Replace grid properties**

Replace the grid layout properties (lines 45-47):

```swift
// Before:
private let gridScrollView = NSScrollView()
private let gridContainer = DraggableGridView()
private var gridCards: [AgentCardView] = []

// After:
private let gridScrollView = NSScrollView()
private let gridCollectionView = NSCollectionView()
private let gridFlowLayout = NSCollectionViewFlowLayout()
```

- [ ] **Step 3: Rewrite `setupGridLayout()`**

```swift
private func setupGridLayout() {
    gridScrollView.translatesAutoresizingMaskIntoConstraints = false
    gridScrollView.hasVerticalScroller = true
    gridScrollView.hasHorizontalScroller = false
    gridScrollView.drawsBackground = false
    gridScrollView.borderType = .noBorder

    gridFlowLayout.minimumInteritemSpacing = gridSpacing
    gridFlowLayout.minimumLineSpacing = gridSpacing
    gridFlowLayout.sectionInset = NSEdgeInsets(top: 0, left: 0, bottom: 0, right: 0)

    gridCollectionView.collectionViewLayout = gridFlowLayout
    gridCollectionView.backgroundColors = [.clear]
    gridCollectionView.isSelectable = true
    gridCollectionView.dataSource = self
    gridCollectionView.delegate = self
    gridCollectionView.register(AgentCardItem.self, forItemWithIdentifier: AgentCardItem.identifier)
    gridCollectionView.setAccessibilityIdentifier("dashboard.layout.grid")

    gridScrollView.documentView = gridCollectionView
    view.addSubview(gridScrollView)

    NSLayoutConstraint.activate([
        gridScrollView.topAnchor.constraint(equalTo: view.topAnchor, constant: gridSpacing),
        gridScrollView.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: gridSpacing),
        gridScrollView.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -gridSpacing),
        gridScrollView.bottomAnchor.constraint(equalTo: view.bottomAnchor, constant: -gridSpacing),
    ])
}
```

- [ ] **Step 4: Add NSCollectionViewDataSource + Delegate conformance**

Add to class declaration and implement:

```swift
// Add to class declaration:
class DashboardViewController: NSViewController, AgentCardDelegate, FocusPanelDelegate, DraggableGridDelegate, NSCollectionViewDataSource, NSCollectionViewDelegateFlowLayout {

// MARK: - NSCollectionViewDataSource

func collectionView(_ collectionView: NSCollectionView, numberOfItemsInSection section: Int) -> Int {
    return sortedAgents().count
}

func collectionView(_ collectionView: NSCollectionView, itemForRepresentedObjectAt indexPath: IndexPath) -> NSCollectionViewItem {
    let item = collectionView.makeItem(withIdentifier: AgentCardItem.identifier, for: indexPath) as! AgentCardItem
    let sorted = sortedAgents()
    let agent = sorted[indexPath.item]
    item.cardView.delegate = self
    item.cardView.configure(
        id: agent.id, project: agent.project, thread: agent.thread,
        status: agent.status, lastMessage: agent.lastMessage,
        totalDuration: agent.totalDuration, roundDuration: agent.roundDuration
    )
    return item
}

// MARK: - NSCollectionViewDelegateFlowLayout

func collectionView(_ collectionView: NSCollectionView, layout collectionViewLayout: NSCollectionViewLayout, sizeForItemAt indexPath: IndexPath) -> NSSize {
    let availableWidth = collectionView.bounds.width
    let cols = max(1, Int(availableWidth / currentMinCardWidth))
    let totalSpacing = gridSpacing * CGFloat(cols - 1)
    let cardWidth = (availableWidth - totalSpacing) / CGFloat(cols)
    let cardHeight = cardWidth * aspectRatio
    return NSSize(width: cardWidth, height: cardHeight)
}
```

- [ ] **Step 5: Replace `rebuildGrid()` with collection view reload**

```swift
private func rebuildGrid() {
    gridCardsByID.removeAll()
    gridCollectionView.reloadData()
}
```

- [ ] **Step 6: Update `updateCardsInPlace()` grid case**

For the `.grid` case in `updateCardsInPlace()`, reload the collection view data (the items will be reconfigured via `itemForRepresentedObjectAt`):

```swift
case .grid:
    gridCollectionView.reloadData()
```

- [ ] **Step 7: Remove `layoutGridFrames()` and `currentGridLayout` and `viewDidLayout` grid handling**

Remove `layoutGridFrames()` method, `currentGridLayout` computed property. Simplify `viewDidLayout()`:

```swift
override func viewDidLayout() {
    super.viewDidLayout()
    if case .grid = currentLayout {
        gridCollectionView.collectionViewLayout?.invalidateLayout()
    }
}
```

- [ ] **Step 8: Update DraggableGridDelegate methods**

The drag delegate methods (`draggableGrid(_:dropIndexFor:)`, etc.) currently reference `currentGridLayout` and `gridCards`. These need to be adapted for NSCollectionView's built-in drag support, or simplified. For now, remove the DraggableGridView dependency:

Remove `DraggableGridDelegate` from class declaration. The drag-to-reorder can be reimplemented later using NSCollectionView's built-in drag support. Remove the three `draggableGrid` methods and `draggableGrid(_:didDropItemWithPath:atIndex:)`.

**Known regression:** Drag-to-reorder is removed. Re-implement via `NSCollectionViewDelegate` drag methods (`draggingSession`, `validateDrop`, `acceptDrop`) in a follow-up task if needed. The reorder data is persisted in config, so no data loss occurs — only the UI gesture is temporarily unavailable.

- [ ] **Step 9: Run full test suite**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test 2>&1 | grep -E '(Test Suite|Executed|FAILED)'`

- [ ] **Step 10: Commit**

```bash
git add Sources/UI/Dashboard/DashboardViewController.swift
git commit -m "perf: replace manual grid frame layout with NSCollectionView"
```

---

### Task 9: Sidebar — Cell Reuse + Targeted Reload

**Files:**
- Modify: `Sources/UI/Repo/SidebarViewController.swift`

Create a reusable cell view class and use `reloadData(forRowIndexes:)` for single-row status updates.

- [ ] **Step 1: Create `SidebarCellView` class**

Add before the `ThreadRowView` class in `SidebarViewController.swift`:

```swift
// MARK: - Reusable Cell View

private class SidebarCellView: NSTableCellView {
    static let identifier = NSUserInterfaceItemIdentifier("SidebarCell")

    let nameLabel = NSTextField(labelWithString: "")
    let dotView = NSView()
    let messageLabel = NSTextField(labelWithString: "")

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        setup()
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) not supported")
    }

    private func setup() {
        nameLabel.font = NSFont.systemFont(ofSize: 12, weight: .medium)
        nameLabel.lineBreakMode = .byTruncatingTail
        nameLabel.translatesAutoresizingMaskIntoConstraints = false
        nameLabel.drawsBackground = false
        nameLabel.isBezeled = false
        nameLabel.isEditable = false
        addSubview(nameLabel)

        dotView.wantsLayer = true
        dotView.layer?.cornerRadius = 4
        dotView.translatesAutoresizingMaskIntoConstraints = false
        addSubview(dotView)

        messageLabel.font = NSFont.systemFont(ofSize: 10)
        messageLabel.lineBreakMode = .byTruncatingTail
        messageLabel.maximumNumberOfLines = 1
        messageLabel.translatesAutoresizingMaskIntoConstraints = false
        messageLabel.drawsBackground = false
        messageLabel.isBezeled = false
        messageLabel.isEditable = false
        messageLabel.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        addSubview(messageLabel)

        let pad: CGFloat = 12
        NSLayoutConstraint.activate([
            nameLabel.topAnchor.constraint(equalTo: topAnchor, constant: 10),
            nameLabel.leadingAnchor.constraint(equalTo: leadingAnchor, constant: pad),
            nameLabel.trailingAnchor.constraint(lessThanOrEqualTo: dotView.leadingAnchor, constant: -8),

            dotView.widthAnchor.constraint(equalToConstant: 8),
            dotView.heightAnchor.constraint(equalToConstant: 8),
            dotView.centerYAnchor.constraint(equalTo: nameLabel.centerYAnchor),
            dotView.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -pad),

            messageLabel.topAnchor.constraint(equalTo: nameLabel.bottomAnchor, constant: 3),
            messageLabel.leadingAnchor.constraint(equalTo: leadingAnchor, constant: pad),
            messageLabel.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -pad),
        ])
    }

    func configure(info: WorktreeInfo, status: AgentStatus, message: String, isSelected: Bool) {
        let textColor = isSelected ? NSColor.white : SemanticColors.text
        let subtitleColor = isSelected ? NSColor.white.withAlphaComponent(0.75) : SemanticColors.muted

        nameLabel.stringValue = info.displayName
        nameLabel.textColor = textColor
        dotView.layer?.backgroundColor = isSelected ? NSColor.white.withAlphaComponent(0.8).cgColor : status.color.cgColor
        messageLabel.stringValue = message.isEmpty ? status.rawValue : message
        messageLabel.textColor = subtitleColor

        setAccessibilityIdentifier("sidebar.row.\(info.branch.isEmpty ? info.displayName : info.branch)")
    }
}
```

- [ ] **Step 2: Rewrite `tableView(_:viewFor:)` to use cell reuse**

```swift
func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView? {
    let info = worktrees[row]
    let status = statuses[info.path] ?? .unknown
    let message = lastMessages[info.path] ?? ""
    let isSelected = (row == selectedIndex)

    let cell: SidebarCellView
    if let reused = tableView.makeView(withIdentifier: SidebarCellView.identifier, owner: nil) as? SidebarCellView {
        cell = reused
    } else {
        cell = SidebarCellView()
        cell.identifier = SidebarCellView.identifier
    }

    cell.configure(info: info, status: status, message: message, isSelected: isSelected)
    return cell
}
```

- [ ] **Step 3: Replace full reloadData() in `updateStatus()` with targeted reload**

```swift
func updateStatus(for path: String, status: AgentStatus, lastMessage: String = "") {
    statuses[path] = status
    if !lastMessage.isEmpty {
        lastMessages[path] = lastMessage
    }
    // Find the row for this path and reload only that row
    if let rowIndex = worktrees.firstIndex(where: { $0.path == path }) {
        tableView.reloadData(forRowIndexes: IndexSet(integer: rowIndex),
                             columnIndexes: IndexSet(integer: 0))
    }
}
```

- [ ] **Step 4: Convert ThreadRowView to layer-backed**

```swift
private class ThreadRowView: NSTableRowView {
    private let isActive: Bool

    init(isActive: Bool, status: AgentStatus) {
        self.isActive = isActive
        super.init(frame: .zero)
        wantsLayer = true
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) not supported")
    }

    override func drawSelection(in dirtyRect: NSRect) {
        // Handled in updateLayer
    }

    override var wantsUpdateLayer: Bool { true }

    override func updateLayer() {
        if isActive {
            effectiveAppearance.performAsCurrentDrawingAppearance {
                layer?.backgroundColor = SemanticColors.accent.cgColor
            }
            layer?.cornerRadius = 8
            // Inset effect via layer bounds inset — use masking or accept full-width for simplicity
        } else {
            layer?.backgroundColor = nil
            layer?.cornerRadius = 0
        }
    }

    override var interiorBackgroundStyle: NSView.BackgroundStyle {
        isActive ? .emphasized : .normal
    }
}
```

Note: The original `draw(_:)` used `insetBy(dx: 6)` for left/right padding. With layer-backed rendering, we lose the inset effect unless we use a sublayer. For simplicity, accept the full-width highlight or use `layer?.frame = bounds.insetBy(dx: 6, dy: 0)` in `layout()`. If the visual difference matters, an inner highlight sublayer can be added.

- [ ] **Step 5: Run full test suite**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test 2>&1 | grep -E '(Test Suite|Executed|FAILED)'`

- [ ] **Step 6: Commit**

```bash
git add Sources/UI/Repo/SidebarViewController.swift
git commit -m "perf: sidebar uses cell reuse and targeted row reload"
```

---

### Task 10: Shadow Paths for Panels

**Files:**
- Modify: `Sources/UI/Panel/AIPanelView.swift`
- Modify: `Sources/UI/Panel/NotificationPanelView.swift`

Add `shadowPath` so Core Animation doesn't rasterize from shape each frame.

- [ ] **Step 1: Add shadowPath to AIPanelView**

In `AIPanelView.applyShadow()`, add after the existing shadow setup:

```swift
private func applyShadow() {
    shadow = NSShadow()
    layer?.shadowColor = NSColor.black.withAlphaComponent(0.12).cgColor
    layer?.shadowOffset = CGSize(width: -8, height: 0)
    layer?.shadowRadius = 16
    layer?.shadowOpacity = 1.0
    layer?.shadowPath = CGPath(rect: bounds, transform: nil)
}
```

Add layout override to keep shadowPath in sync:

```swift
override func layout() {
    super.layout()
    layer?.shadowPath = CGPath(rect: bounds, transform: nil)
}
```

- [ ] **Step 2: Same for NotificationPanelView**

Apply identical changes to `NotificationPanelView.applyShadow()` and add `layout()` override.

- [ ] **Step 3: Run full test suite**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test 2>&1 | grep -E '(Test Suite|Executed|FAILED)'`

- [ ] **Step 4: Commit**

```bash
git add Sources/UI/Panel/AIPanelView.swift Sources/UI/Panel/NotificationPanelView.swift
git commit -m "perf: add shadow paths to panel views for GPU-cached shadows"
```

---

### Task 11: Update Performance Tests + Final Validation

**Files:**
- Modify: `Tests/PerformanceTests.swift`

Update tests to reflect the new architecture and verify improvements.

- [ ] **Step 1: Run all performance tests**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/PerformanceTests 2>&1 | grep -E '(measured|FAILED)'`

Verify all tests compile and run. Some tests may need adjustments if APIs changed (e.g., grid-related tests).

- [ ] **Step 2: Fix any test compilation issues**

If `testFullRebuildCyclePerformance` or other tests reference removed APIs, update them to test the new patterns.

- [ ] **Step 3: Run the full test suite**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test 2>&1 | grep -E '(Test Suite|Executed|FAILED)'`

Expected: All tests pass with no regressions.

- [ ] **Step 4: Build the app**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -5`

Expected: Build succeeds.

- [ ] **Step 5: Commit any test fixes**

```bash
git add Tests/PerformanceTests.swift
git commit -m "test: update performance tests for new incremental update architecture"
```
