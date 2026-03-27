# Split Pane Terminal Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add recursive split-pane terminal support within worktrees in the repo detail view, using independent zmx sessions per pane.

**Architecture:** A `SplitNode` recursive enum models the binary split tree. `SplitTree` wraps it with mutations and focus tracking. `SplitContainerView` does frame-based recursive layout with draggable `DividerView` dividers. `TerminalSurfaceManager` changes from `[path: Surface]` to `[path: SplitTree]`. StatusPublisher polls all leaves and aggregates per-worktree.

**Tech Stack:** Swift 5.10, AppKit (frame-based layout), XCTest

**Spec:** `docs/superpowers/specs/2026-03-25-split-pane-design.md`

---

## File Structure

| File | Responsibility |
|------|----------------|
| `Sources/Terminal/SplitNode.swift` | `SplitAxis` enum, `SplitNode` recursive enum, Codable conformance, `CodableSplitNode` for JSON persistence |
| `Sources/Terminal/SplitTree.swift` | `SplitTree` class: tree mutations (split, close, resize), focus tracking, leaf enumeration, all-surfaces accessor |
| `Sources/UI/Split/SplitContainerView.swift` | NSView subclass: recursive frame-based layout, divider lifecycle, resize handling |
| `Sources/UI/Split/DividerView.swift` | NSView subclass: draggable divider with hover highlight, cursor changes, double-click reset |
| `Sources/Core/TerminalSurfaceManager.swift` | (modify) Storage `[String: TerminalSurface]` → `[String: SplitTree]`, new tree-level APIs |
| `Sources/Core/SessionManager.swift` | (modify) Add `indexedSessionName(base:index:)` for pane session naming |
| `Sources/Core/Config.swift` | (modify) Add `splitLayouts: [String: CodableSplitNode]` field |
| `Sources/UI/Repo/RepoViewController.swift` | (modify) Embed `SplitContainerView` instead of direct surface |
| `Sources/App/MainWindowController.swift` | (modify) Keybindings in `AmuxWindow`, surface creation adapted for SplitTree |
| `Sources/Status/StatusPublisher.swift` | (modify) Accept `[String: SplitTree]`, poll all leaves, aggregate |
| `Sources/Core/SurfaceRegistry.swift` | Singleton mapping `surfaceId: String` → `TerminalSurface` for cross-component lookup |
| `Tests/SplitNodeTests.swift` | Unit tests for SplitNode and SplitTree |
| `Tests/SplitContainerLayoutTests.swift` | Unit tests for layout computation |
| `UITests/Tests/SplitPaneTests.swift` | UI automation tests for split pane operations |
| `UITests/Pages/SplitPanePage.swift` | Page object for split pane UI elements |

**Important:** Run `xcodegen generate` after creating any new `.swift` file to ensure it's included in the Xcode project. Each task that creates new files includes an explicit xcodegen step.

---

### Task 1: SplitNode Data Model + Codable

**Files:**
- Create: `Sources/Terminal/SplitNode.swift`
- Test: `Tests/SplitNodeTests.swift`

- [ ] **Step 1: Write failing tests for SplitNode**

```swift
// Tests/SplitNodeTests.swift
import XCTest
@testable import amux

final class SplitNodeTests: XCTestCase {

    func testSingleLeaf() {
        let node = SplitNode.leaf(id: "a", surfaceId: "s1", sessionName: "amux-repo-main")
        XCTAssertEqual(node.leafCount, 1)
        XCTAssertEqual(node.allLeaves.count, 1)
        XCTAssertEqual(node.allLeaves.first?.id, "a")
    }

    func testSplitNodeLeafCount() {
        let left = SplitNode.leaf(id: "a", surfaceId: "s1", sessionName: "amux-repo-main")
        let right = SplitNode.leaf(id: "b", surfaceId: "s2", sessionName: "amux-repo-main-1")
        let split = SplitNode.split(id: "s", axis: .horizontal, ratio: 0.5, first: left, second: right)
        XCTAssertEqual(split.leafCount, 2)
        XCTAssertEqual(split.allLeaves.count, 2)
    }

    func testFindLeafById() {
        let left = SplitNode.leaf(id: "a", surfaceId: "s1", sessionName: "amux-repo-main")
        let right = SplitNode.leaf(id: "b", surfaceId: "s2", sessionName: "amux-repo-main-1")
        let split = SplitNode.split(id: "s", axis: .horizontal, ratio: 0.5, first: left, second: right)
        XCTAssertNotNil(split.findLeaf(id: "a"))
        XCTAssertNotNil(split.findLeaf(id: "b"))
        XCTAssertNil(split.findLeaf(id: "c"))
    }

    func testCodableRoundTrip_Leaf() throws {
        let node = CodableSplitNode.leaf(sessionName: "amux-repo-main")
        let data = try JSONEncoder().encode(node)
        let decoded = try JSONDecoder().decode(CodableSplitNode.self, from: data)
        if case .leaf(let name) = decoded {
            XCTAssertEqual(name, "amux-repo-main")
        } else {
            XCTFail("Expected leaf")
        }
    }

    func testCodableRoundTrip_Split() throws {
        let node = CodableSplitNode.split(
            axis: "horizontal",
            ratio: 0.6,
            first: .leaf(sessionName: "amux-repo-main"),
            second: .leaf(sessionName: "amux-repo-main-1")
        )
        let encoder = JSONEncoder()
        encoder.outputFormatting = .sortedKeys
        let data = try encoder.encode(node)
        let decoded = try JSONDecoder().decode(CodableSplitNode.self, from: data)
        if case .split(let axis, let ratio, _, _) = decoded {
            XCTAssertEqual(axis, "horizontal")
            XCTAssertEqual(ratio, 0.6)
        } else {
            XCTFail("Expected split")
        }
    }

    func testNextPaneIndex_NoPanes() {
        let node = SplitNode.leaf(id: "a", surfaceId: "s1", sessionName: "amux-repo-main")
        XCTAssertEqual(node.nextPaneIndex(baseName: "amux-repo-main"), 1)
    }

    func testNextPaneIndex_WithExistingPanes() {
        let left = SplitNode.leaf(id: "a", surfaceId: "s1", sessionName: "amux-repo-main")
        let right = SplitNode.leaf(id: "b", surfaceId: "s2", sessionName: "amux-repo-main-1")
        let split = SplitNode.split(id: "s", axis: .horizontal, ratio: 0.5, first: left, second: right)
        XCTAssertEqual(split.nextPaneIndex(baseName: "amux-repo-main"), 2)
    }
}
```

- [ ] **Step 2: Regenerate Xcode project**

Run: `xcodegen generate`

- [ ] **Step 3: Run tests to verify they fail**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/SplitNodeTests 2>&1 | tail -5`
Expected: FAIL — `SplitNode` type not found

- [ ] **Step 4: Implement SplitNode and CodableSplitNode**

```swift
// Sources/Terminal/SplitNode.swift
import Foundation

enum SplitAxis: String, Codable {
    case horizontal  // left | right
    case vertical    // top / bottom
}

/// Runtime split tree node. Leaves hold live TerminalSurface references.
indirect enum SplitNode {
    case leaf(id: String, surfaceId: String, sessionName: String)
    case split(id: String, axis: SplitAxis, ratio: CGFloat, first: SplitNode, second: SplitNode)

    var id: String {
        switch self {
        case .leaf(let id, _, _): return id
        case .split(let id, _, _, _, _): return id
        }
    }

    var leafCount: Int {
        switch self {
        case .leaf: return 1
        case .split(_, _, _, let first, let second):
            return first.leafCount + second.leafCount
        }
    }

    struct LeafInfo {
        let id: String
        let surfaceId: String
        let sessionName: String
    }

    var allLeaves: [LeafInfo] {
        switch self {
        case .leaf(let id, let surfaceId, let sessionName):
            return [LeafInfo(id: id, surfaceId: surfaceId, sessionName: sessionName)]
        case .split(_, _, _, let first, let second):
            return first.allLeaves + second.allLeaves
        }
    }

    func findLeaf(id: String) -> LeafInfo? {
        allLeaves.first { $0.id == id }
    }

    /// Derive next pane index from existing session names.
    func nextPaneIndex(baseName: String) -> Int {
        let leaves = allLeaves
        var maxIndex = 0
        for leaf in leaves {
            let name = leaf.sessionName
            if name == baseName {
                // The original pane (index 0 implicitly)
                continue
            }
            if name.hasPrefix(baseName + "-"),
               let suffix = Int(name.dropFirst(baseName.count + 1)) {
                maxIndex = max(maxIndex, suffix)
            }
        }
        return maxIndex + 1
    }

    /// Replace a leaf node (by id) with a new subtree. Returns modified tree.
    func replacing(leafId: String, with replacement: SplitNode) -> SplitNode {
        switch self {
        case .leaf(let id, _, _):
            return id == leafId ? replacement : self
        case .split(let id, let axis, let ratio, let first, let second):
            return .split(
                id: id, axis: axis, ratio: ratio,
                first: first.replacing(leafId: leafId, with: replacement),
                second: second.replacing(leafId: leafId, with: replacement)
            )
        }
    }

    /// Remove a leaf by id, promoting its sibling. Returns nil if this node IS the leaf.
    func removing(leafId: String) -> SplitNode? {
        switch self {
        case .leaf(let id, _, _):
            return id == leafId ? nil : self
        case .split(_, _, _, let first, let second):
            if first.id == leafId { return second }
            if second.id == leafId { return first }
            // Recurse
            if let newFirst = first.removing(leafId: leafId) {
                return .split(id: self.id, axis: axis, ratio: ratio, first: newFirst, second: second)
            }
            if let newSecond = second.removing(leafId: leafId) {
                return .split(id: self.id, axis: axis, ratio: ratio, first: first, second: newSecond)
            }
            return self
        }
    }

    private var axis: SplitAxis {
        if case .split(_, let axis, _, _, _) = self { return axis }
        fatalError("Not a split node")
    }

    private var ratio: CGFloat {
        if case .split(_, _, let ratio, _, _) = self { return ratio }
        fatalError("Not a split node")
    }

    /// Update ratio on a specific split node by id.
    func updatingRatio(splitId: String, newRatio: CGFloat) -> SplitNode {
        switch self {
        case .leaf: return self
        case .split(let id, let axis, let ratio, let first, let second):
            let r = id == splitId ? newRatio : ratio
            return .split(
                id: id, axis: axis, ratio: r,
                first: first.updatingRatio(splitId: splitId, newRatio: newRatio),
                second: second.updatingRatio(splitId: splitId, newRatio: newRatio)
            )
        }
    }

    /// Find the nearest ancestor split node with the given axis, for a leaf.
    func nearestAncestorSplit(forLeaf leafId: String, axis targetAxis: SplitAxis) -> String? {
        switch self {
        case .leaf: return nil
        case .split(let id, let axis, _, let first, let second):
            // Check if leaf is in first or second subtree
            let inFirst = first.findLeaf(id: leafId) != nil
            let inSecond = second.findLeaf(id: leafId) != nil
            guard inFirst || inSecond else { return nil }
            // Recurse deeper first
            let deeper = inFirst
                ? first.nearestAncestorSplit(forLeaf: leafId, axis: targetAxis)
                : second.nearestAncestorSplit(forLeaf: leafId, axis: targetAxis)
            if let deeper = deeper { return deeper }
            // This node is the nearest if axis matches
            return axis == targetAxis ? id : nil
        }
    }
}

// MARK: - Codable representation for config persistence

/// Serializable split layout (no live surface references).
indirect enum CodableSplitNode: Codable {
    case leaf(sessionName: String)
    case split(axis: String, ratio: Double, first: CodableSplitNode, second: CodableSplitNode)

    private enum CodingKeys: String, CodingKey {
        case type, sessionName, axis, ratio, first, second
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let type = try container.decode(String.self, forKey: .type)
        if type == "leaf" {
            let name = try container.decode(String.self, forKey: .sessionName)
            self = .leaf(sessionName: name)
        } else {
            let axis = try container.decode(String.self, forKey: .axis)
            let ratio = try container.decode(Double.self, forKey: .ratio)
            let first = try container.decode(CodableSplitNode.self, forKey: .first)
            let second = try container.decode(CodableSplitNode.self, forKey: .second)
            self = .split(axis: axis, ratio: ratio, first: first, second: second)
        }
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .leaf(let sessionName):
            try container.encode("leaf", forKey: .type)
            try container.encode(sessionName, forKey: .sessionName)
        case .split(let axis, let ratio, let first, let second):
            try container.encode("split", forKey: .type)
            try container.encode(axis, forKey: .axis)
            try container.encode(ratio, forKey: .ratio)
            try container.encode(first, forKey: .first)
            try container.encode(second, forKey: .second)
        }
    }

    /// Convert runtime SplitNode to serializable form.
    static func from(_ node: SplitNode) -> CodableSplitNode {
        switch node {
        case .leaf(_, _, let sessionName):
            return .leaf(sessionName: sessionName)
        case .split(_, let axis, let ratio, let first, let second):
            return .split(axis: axis.rawValue, ratio: Double(ratio), first: from(first), second: from(second))
        }
    }
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/SplitNodeTests 2>&1 | tail -5`
Expected: All 7 tests PASS

- [ ] **Step 6: Commit**

```bash
git add Sources/Terminal/SplitNode.swift Tests/SplitNodeTests.swift
xcodegen generate && git add amux.xcodeproj
git commit -m "feat: add SplitNode data model with Codable persistence"
```

---

### Task 2: SplitTree Class

**Files:**
- Create: `Sources/Terminal/SplitTree.swift`
- Modify test: `Tests/SplitNodeTests.swift` (add SplitTree tests)

- [ ] **Step 1: Write failing tests for SplitTree**

Add to `Tests/SplitNodeTests.swift`:

```swift
final class SplitTreeTests: XCTestCase {

    func testInitialState() {
        let tree = SplitTree(worktreePath: "/repo/main", rootLeafId: "a", surfaceId: "s1", sessionName: "amux-repo-main")
        XCTAssertEqual(tree.focusedId, "a")
        XCTAssertEqual(tree.leafCount, 1)
        XCTAssertEqual(tree.allSurfaceIds.count, 1)
    }

    func testSplitFocusedLeaf() {
        let tree = SplitTree(worktreePath: "/repo/main", rootLeafId: "a", surfaceId: "s1", sessionName: "amux-repo-main")
        let newLeafId = tree.splitFocusedLeaf(axis: .horizontal, newLeafId: "b", newSurfaceId: "s2", newSessionName: "amux-repo-main-1")
        XCTAssertEqual(newLeafId, "b")
        XCTAssertEqual(tree.focusedId, "b")
        XCTAssertEqual(tree.leafCount, 2)
    }

    func testCloseLeaf_PromotesSibling() {
        let tree = SplitTree(worktreePath: "/repo/main", rootLeafId: "a", surfaceId: "s1", sessionName: "amux-repo-main")
        _ = tree.splitFocusedLeaf(axis: .horizontal, newLeafId: "b", newSurfaceId: "s2", newSessionName: "amux-repo-main-1")
        // Focus is on "b", close it
        let closed = tree.closeFocusedLeaf()
        XCTAssertEqual(closed?.id, "b")
        XCTAssertEqual(tree.focusedId, "a")
        XCTAssertEqual(tree.leafCount, 1)
    }

    func testCloseLeaf_LastPaneCannotClose() {
        let tree = SplitTree(worktreePath: "/repo/main", rootLeafId: "a", surfaceId: "s1", sessionName: "amux-repo-main")
        let closed = tree.closeFocusedLeaf()
        XCTAssertNil(closed)
        XCTAssertEqual(tree.leafCount, 1)
    }

    func testNextSessionName() {
        let tree = SplitTree(worktreePath: "/repo/main", rootLeafId: "a", surfaceId: "s1", sessionName: "amux-repo-main")
        XCTAssertEqual(tree.nextSessionName(), "amux-repo-main-1")
        _ = tree.splitFocusedLeaf(axis: .horizontal, newLeafId: "b", newSurfaceId: "s2", newSessionName: "amux-repo-main-1")
        XCTAssertEqual(tree.nextSessionName(), "amux-repo-main-2")
    }

    func testAllSurfaceIds() {
        let tree = SplitTree(worktreePath: "/repo/main", rootLeafId: "a", surfaceId: "s1", sessionName: "amux-repo-main")
        _ = tree.splitFocusedLeaf(axis: .horizontal, newLeafId: "b", newSurfaceId: "s2", newSessionName: "amux-repo-main-1")
        let ids = tree.allSurfaceIds
        XCTAssertTrue(ids.contains("s1"))
        XCTAssertTrue(ids.contains("s2"))
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/SplitTreeTests 2>&1 | tail -5`
Expected: FAIL — `SplitTree` type not found

- [ ] **Step 3: Implement SplitTree**

```swift
// Sources/Terminal/SplitTree.swift
import Foundation

/// Manages a split-pane tree for a single worktree.
/// Each leaf corresponds to one TerminalSurface + zmx session.
class SplitTree {
    private(set) var root: SplitNode
    var focusedId: String
    let worktreePath: String
    private let baseSessionName: String

    var leafCount: Int { root.leafCount }
    var allLeaves: [SplitNode.LeafInfo] { root.allLeaves }
    var allSurfaceIds: [String] { root.allLeaves.map(\.surfaceId) }

    init(worktreePath: String, rootLeafId: String, surfaceId: String, sessionName: String) {
        self.worktreePath = worktreePath
        self.baseSessionName = sessionName
        self.root = .leaf(id: rootLeafId, surfaceId: surfaceId, sessionName: sessionName)
        self.focusedId = rootLeafId
    }

    /// Derive next session name from existing leaves.
    func nextSessionName() -> String {
        let index = root.nextPaneIndex(baseName: baseSessionName)
        return "\(baseSessionName)-\(index)"
    }

    /// Split the currently focused leaf. Returns the new leaf's id.
    @discardableResult
    func splitFocusedLeaf(axis: SplitAxis, newLeafId: String, newSurfaceId: String, newSessionName: String) -> String {
        let newLeaf = SplitNode.leaf(id: newLeafId, surfaceId: newSurfaceId, sessionName: newSessionName)
        let splitId = UUID().uuidString
        // Find the focused leaf and replace it with a split containing [focused, newLeaf]
        guard root.findLeaf(id: focusedId) != nil else { return newLeafId }
        let focusedNode = extractSubnode(id: focusedId)
        let replacement = SplitNode.split(id: splitId, axis: axis, ratio: 0.5, first: focusedNode, second: newLeaf)
        root = root.replacing(leafId: focusedId, with: replacement)
        focusedId = newLeafId
        return newLeafId
    }

    /// Close the focused leaf. Returns the closed leaf info, or nil if it's the last pane.
    func closeFocusedLeaf() -> SplitNode.LeafInfo? {
        guard leafCount > 1 else { return nil }
        guard let leafInfo = root.findLeaf(id: focusedId) else { return nil }
        guard let newRoot = root.removing(leafId: focusedId) else { return nil }
        root = newRoot
        // Focus the first leaf in the remaining tree
        focusedId = root.allLeaves.first?.id ?? focusedId
        return leafInfo
    }

    /// Update ratio on a split node.
    func updateRatio(splitId: String, newRatio: CGFloat) {
        let clamped = min(max(newRatio, 0.1), 0.9)
        root = root.updatingRatio(splitId: splitId, newRatio: clamped)
    }

    /// Find nearest ancestor split with given axis for the focused leaf.
    func nearestAncestorSplit(axis: SplitAxis) -> String? {
        root.nearestAncestorSplit(forLeaf: focusedId, axis: axis)
    }

    /// Convert to serializable form for config persistence.
    func toCodable() -> CodableSplitNode {
        CodableSplitNode.from(root)
    }

    // Helper: extract the subtree rooted at a node id (for the focused leaf, it's just the leaf itself)
    private func extractSubnode(id: String) -> SplitNode {
        // For leaves, just return the matching leaf from root
        if case .leaf(let leafId, let surfaceId, let sessionName) = root, leafId == id {
            return .leaf(id: leafId, surfaceId: surfaceId, sessionName: sessionName)
        }
        // For deeper nodes, we need to find the leaf — but since we only split leaves,
        // the focused node is always a leaf at this point
        guard let info = root.findLeaf(id: id) else { return root }
        return .leaf(id: info.id, surfaceId: info.surfaceId, sessionName: info.sessionName)
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/SplitTreeTests 2>&1 | tail -5`
Expected: All 6 tests PASS

- [ ] **Step 5: Regenerate Xcode project**

Run: `xcodegen generate`

- [ ] **Step 6: Commit**

```bash
git add Sources/Terminal/SplitTree.swift Tests/SplitNodeTests.swift amux.xcodeproj
git commit -m "feat: add SplitTree class with split/close/focus mutations"
```

---

### Task 3: DividerView

**Files:**
- Create: `Sources/UI/Split/DividerView.swift`

- [ ] **Step 1: Implement DividerView**

```swift
// Sources/UI/Split/DividerView.swift
import AppKit

protocol DividerDelegate: AnyObject {
    func dividerDidMove(_ splitNodeId: String, newRatio: CGFloat)
    func dividerDidDoubleClick(_ splitNodeId: String)
}

/// Draggable divider between split panes.
class DividerView: NSView {
    let splitNodeId: String
    let axis: SplitAxis
    weak var delegate: DividerDelegate?

    static let thickness: CGFloat = 4

    private var isDragging = false
    private var dragStartPoint: CGPoint = .zero
    private var dragStartRatio: CGFloat = 0
    /// The total size (width or height) of the parent split region, set by SplitContainerView during layout.
    var parentSplitSize: CGFloat = 0
    /// Current ratio, updated by SplitContainerView during layout.
    var currentRatio: CGFloat = 0.5

    init(splitNodeId: String, axis: SplitAxis) {
        self.splitNodeId = splitNodeId
        self.axis = axis
        super.init(frame: .zero)
        wantsLayer = true
        layer?.backgroundColor = NSColor.separatorColor.cgColor
    }

    required init?(coder: NSCoder) { fatalError() }

    override func resetCursorRects() {
        let cursor: NSCursor = axis == .horizontal ? .resizeLeftRight : .resizeUpDown
        addCursorRect(bounds, cursor: cursor)
    }

    override func mouseDown(with event: NSEvent) {
        if event.clickCount == 2 {
            delegate?.dividerDidDoubleClick(splitNodeId)
            return
        }
        isDragging = true
        dragStartPoint = convert(event.locationInWindow, from: nil)
        dragStartRatio = currentRatio
    }

    override func mouseDragged(with event: NSEvent) {
        guard isDragging, parentSplitSize > 0 else { return }
        let point = convert(event.locationInWindow, from: nil)
        let delta: CGFloat
        if axis == .horizontal {
            delta = point.x - dragStartPoint.x
        } else {
            delta = -(point.y - dragStartPoint.y) // NSView Y is flipped vs visual
        }
        let ratioDelta = delta / parentSplitSize
        let newRatio = min(max(dragStartRatio + ratioDelta, 0.1), 0.9)
        delegate?.dividerDidMove(splitNodeId, newRatio: newRatio)
    }

    override func mouseUp(with event: NSEvent) {
        isDragging = false
    }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        for area in trackingAreas { removeTrackingArea(area) }
        addTrackingArea(NSTrackingArea(
            rect: bounds,
            options: [.mouseEnteredAndExited, .activeInKeyWindow],
            owner: self
        ))
    }

    override func mouseEntered(with event: NSEvent) {
        layer?.backgroundColor = NSColor.controlAccentColor.withAlphaComponent(0.5).cgColor
    }

    override func mouseExited(with event: NSEvent) {
        layer?.backgroundColor = NSColor.separatorColor.cgColor
    }
}
```

- [ ] **Step 2: Regenerate Xcode project and verify it compiles**

Run: `xcodegen generate && xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -5`
Expected: BUILD SUCCEEDED

- [ ] **Step 3: Commit**

```bash
git add Sources/UI/Split/DividerView.swift amux.xcodeproj
git commit -m "feat: add DividerView with drag, hover, and double-click"
```

---

### Task 4: SplitContainerView

**Files:**
- Create: `Sources/UI/Split/SplitContainerView.swift`
- Create: `Tests/SplitContainerLayoutTests.swift`

- [ ] **Step 1: Write failing layout tests**

```swift
// Tests/SplitContainerLayoutTests.swift
import XCTest
@testable import amux

final class SplitContainerLayoutTests: XCTestCase {

    func testComputeFrames_SingleLeaf() {
        let frames = SplitContainerView.computeFrames(
            node: .leaf(id: "a", surfaceId: "s1", sessionName: "test"),
            in: CGRect(x: 0, y: 0, width: 800, height: 600)
        )
        XCTAssertEqual(frames.count, 1)
        XCTAssertEqual(frames["a"], CGRect(x: 0, y: 0, width: 800, height: 600))
    }

    func testComputeFrames_HorizontalSplit() {
        let node = SplitNode.split(
            id: "s", axis: .horizontal, ratio: 0.5,
            first: .leaf(id: "a", surfaceId: "s1", sessionName: "t1"),
            second: .leaf(id: "b", surfaceId: "s2", sessionName: "t2")
        )
        let frames = SplitContainerView.computeFrames(
            node: node,
            in: CGRect(x: 0, y: 0, width: 800, height: 600)
        )
        XCTAssertEqual(frames.count, 2)
        let a = frames["a"]!
        let b = frames["b"]!
        // Left pane: x=0, width ≈ 398 (half minus divider)
        XCTAssertEqual(a.origin.x, 0)
        XCTAssertTrue(a.width > 390 && a.width < 405)
        // Right pane: x ≈ 402
        XCTAssertTrue(b.origin.x > 395)
        XCTAssertEqual(a.height, 600)
        XCTAssertEqual(b.height, 600)
    }

    func testComputeFrames_VerticalSplit() {
        let node = SplitNode.split(
            id: "s", axis: .vertical, ratio: 0.5,
            first: .leaf(id: "a", surfaceId: "s1", sessionName: "t1"),
            second: .leaf(id: "b", surfaceId: "s2", sessionName: "t2")
        )
        let frames = SplitContainerView.computeFrames(
            node: node,
            in: CGRect(x: 0, y: 0, width: 800, height: 600)
        )
        let a = frames["a"]!
        let b = frames["b"]!
        XCTAssertEqual(a.width, 800)
        XCTAssertEqual(b.width, 800)
        XCTAssertTrue(a.height > 290 && a.height < 305)
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/SplitContainerLayoutTests 2>&1 | tail -5`
Expected: FAIL

- [ ] **Step 3: Implement SplitContainerView**

```swift
// Sources/UI/Split/SplitContainerView.swift
import AppKit

protocol SplitContainerDelegate: AnyObject {
    func splitContainer(_ view: SplitContainerView, didChangeFocus leafId: String)
    func splitContainer(_ view: SplitContainerView, didRequestSplit axis: SplitAxis)
    func splitContainer(_ view: SplitContainerView, didRequestClosePane leafId: String)
    func splitContainerDidChangeLayout(_ view: SplitContainerView)
}

/// Recursively lays out a SplitTree using frame-based positioning.
class SplitContainerView: NSView, DividerDelegate {
    var tree: SplitTree? { didSet { layoutTree() } }
    /// Map of surface ID → TerminalSurface view, provided externally.
    var surfaceViews: [String: NSView] = [:]
    weak var delegate: SplitContainerDelegate?

    private var dividers: [String: DividerView] = [:]
    private var leafFrames: [String: CGRect] = [:]

    override var isFlipped: Bool { true }

    override init(frame: NSRect) {
        super.init(frame: frame)
        translatesAutoresizingMaskIntoConstraints = true
    }

    required init?(coder: NSCoder) { fatalError() }

    override func resizeSubviews(withOldSize oldSize: NSSize) {
        super.resizeSubviews(withOldSize: oldSize)
        layoutTree()
    }

    // MARK: - Layout

    func layoutTree() {
        guard let tree = tree else { return }
        // Compute frames
        leafFrames = Self.computeFrames(node: tree.root, in: bounds)
        // Position leaf surface views
        for leaf in tree.allLeaves {
            guard let frame = leafFrames[leaf.id],
                  let view = surfaceViews[leaf.surfaceId] else { continue }
            if view.superview != self { addSubview(view) }
            view.frame = frame
        }
        // Manage dividers
        layoutDividers(node: tree.root, in: bounds)
        // Remove stale dividers
        let activeSplitIds = collectSplitIds(tree.root)
        for (id, divider) in dividers where !activeSplitIds.contains(id) {
            divider.removeFromSuperview()
            dividers.removeValue(forKey: id)
        }
    }

    /// Pure function: compute leaf frames without side effects (testable).
    static func computeFrames(node: SplitNode, in rect: CGRect) -> [String: CGRect] {
        var result: [String: CGRect] = [:]
        computeFramesRecursive(node: node, in: rect, result: &result)
        return result
    }

    private static func computeFramesRecursive(node: SplitNode, in rect: CGRect, result: inout [String: CGRect]) {
        switch node {
        case .leaf(let id, _, _):
            result[id] = rect
        case .split(_, let axis, let ratio, let first, let second):
            let dividerSize = DividerView.thickness
            switch axis {
            case .horizontal:
                let firstWidth = floor((rect.width - dividerSize) * ratio)
                let secondX = rect.origin.x + firstWidth + dividerSize
                let secondWidth = rect.width - firstWidth - dividerSize
                let firstRect = CGRect(x: rect.origin.x, y: rect.origin.y, width: firstWidth, height: rect.height)
                let secondRect = CGRect(x: secondX, y: rect.origin.y, width: secondWidth, height: rect.height)
                computeFramesRecursive(node: first, in: firstRect, result: &result)
                computeFramesRecursive(node: second, in: secondRect, result: &result)
            case .vertical:
                let firstHeight = floor((rect.height - dividerSize) * ratio)
                let secondY = rect.origin.y + firstHeight + dividerSize
                let secondHeight = rect.height - firstHeight - dividerSize
                let firstRect = CGRect(x: rect.origin.x, y: rect.origin.y, width: rect.width, height: firstHeight)
                let secondRect = CGRect(x: rect.origin.x, y: secondY, width: rect.width, height: secondHeight)
                computeFramesRecursive(node: first, in: firstRect, result: &result)
                computeFramesRecursive(node: second, in: secondRect, result: &result)
            }
        }
    }

    private func layoutDividers(node: SplitNode, in rect: CGRect) {
        guard case .split(let id, let axis, let ratio, let first, let second) = node else { return }
        let dividerSize = DividerView.thickness

        // Create or reuse divider
        let divider: DividerView
        if let existing = dividers[id] {
            divider = existing
        } else {
            divider = DividerView(splitNodeId: id, axis: axis)
            divider.delegate = self
            addSubview(divider)
            dividers[id] = divider
        }

        // Position divider
        switch axis {
        case .horizontal:
            let firstWidth = floor((rect.width - dividerSize) * ratio)
            divider.frame = CGRect(x: rect.origin.x + firstWidth, y: rect.origin.y, width: dividerSize, height: rect.height)
            divider.parentSplitSize = rect.width
            divider.currentRatio = ratio
            // Recurse
            let firstRect = CGRect(x: rect.origin.x, y: rect.origin.y, width: firstWidth, height: rect.height)
            let secondRect = CGRect(x: rect.origin.x + firstWidth + dividerSize, y: rect.origin.y, width: rect.width - firstWidth - dividerSize, height: rect.height)
            layoutDividers(node: first, in: firstRect)
            layoutDividers(node: second, in: secondRect)
        case .vertical:
            let firstHeight = floor((rect.height - dividerSize) * ratio)
            divider.frame = CGRect(x: rect.origin.x, y: rect.origin.y + firstHeight, width: rect.width, height: dividerSize)
            divider.parentSplitSize = rect.height
            divider.currentRatio = ratio
            let firstRect = CGRect(x: rect.origin.x, y: rect.origin.y, width: rect.width, height: firstHeight)
            let secondRect = CGRect(x: rect.origin.x, y: rect.origin.y + firstHeight + dividerSize, width: rect.width, height: rect.height - firstHeight - dividerSize)
            layoutDividers(node: first, in: firstRect)
            layoutDividers(node: second, in: secondRect)
        }
    }

    private func collectSplitIds(_ node: SplitNode) -> Set<String> {
        switch node {
        case .leaf: return []
        case .split(let id, _, _, let first, let second):
            return [id].union(collectSplitIds(first)).union(collectSplitIds(second))
        }
    }

    // MARK: - Focus Navigation

    /// Spatial focus navigation: find nearest leaf in direction from current focused leaf.
    func focusLeaf(direction: SplitAxis, positive: Bool) -> String? {
        guard let tree = tree else { return nil }
        guard let currentFrame = leafFrames[tree.focusedId] else { return nil }
        let center = CGPoint(x: currentFrame.midX, y: currentFrame.midY)

        var bestLeaf: String?
        var bestDistance: CGFloat = .greatestFiniteMagnitude

        for leaf in tree.allLeaves where leaf.id != tree.focusedId {
            guard let frame = leafFrames[leaf.id] else { continue }
            let leafCenter = CGPoint(x: frame.midX, y: frame.midY)

            // Check direction
            let inDirection: Bool
            switch (direction, positive) {
            case (.horizontal, true):  inDirection = leafCenter.x > center.x  // right
            case (.horizontal, false): inDirection = leafCenter.x < center.x  // left
            case (.vertical, true):    inDirection = leafCenter.y > center.y  // down (flipped)
            case (.vertical, false):   inDirection = leafCenter.y < center.y  // up (flipped)
            }
            guard inDirection else { continue }

            // Check overlap on perpendicular axis
            let overlaps: Bool
            if direction == .horizontal {
                overlaps = frame.minY < currentFrame.maxY && frame.maxY > currentFrame.minY
            } else {
                overlaps = frame.minX < currentFrame.maxX && frame.maxX > currentFrame.minX
            }
            guard overlaps else { continue }

            let dist = hypot(leafCenter.x - center.x, leafCenter.y - center.y)
            if dist < bestDistance {
                bestDistance = dist
                bestLeaf = leaf.id
            }
        }

        if let best = bestLeaf {
            tree.focusedId = best
            delegate?.splitContainer(self, didChangeFocus: best)
        }
        return bestLeaf
    }

    // MARK: - DividerDelegate

    func dividerDidMove(_ splitNodeId: String, newRatio: CGFloat) {
        tree?.updateRatio(splitId: splitNodeId, newRatio: newRatio)
        layoutTree()
        delegate?.splitContainerDidChangeLayout(self)
    }

    func dividerDidDoubleClick(_ splitNodeId: String) {
        tree?.updateRatio(splitId: splitNodeId, newRatio: 0.5)
        layoutTree()
        delegate?.splitContainerDidChangeLayout(self)
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/SplitContainerLayoutTests 2>&1 | tail -5`
Expected: All 3 tests PASS

- [ ] **Step 5: Regenerate Xcode project and verify full build**

Run: `xcodegen generate && xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -5`
Expected: BUILD SUCCEEDED

- [ ] **Step 6: Commit**

```bash
git add Sources/UI/Split/SplitContainerView.swift Tests/SplitContainerLayoutTests.swift amux.xcodeproj
git commit -m "feat: add SplitContainerView with frame-based recursive layout"
```

---

### Task 5: SurfaceRegistry + Config Persistence

**Files:**
- Create: `Sources/Core/SurfaceRegistry.swift`
- Modify: `Sources/Core/Config.swift:3-65`

- [ ] **Step 1: Create SurfaceRegistry**

A singleton that maps `surfaceId → TerminalSurface`. All components (SplitContainerView, StatusPublisher, etc.) use this to look up live surface references from string IDs stored in SplitNode.

```swift
// Sources/Core/SurfaceRegistry.swift
import Foundation

/// Global registry mapping surface IDs to live TerminalSurface instances.
/// Used by SplitContainerView and StatusPublisher to resolve surfaceId strings
/// stored in SplitNode leaves to actual TerminalSurface objects.
class SurfaceRegistry {
    static let shared = SurfaceRegistry()
    private var surfaces: [String: TerminalSurface] = [:]

    func register(_ surface: TerminalSurface) {
        surfaces[surface.id] = surface
    }

    func unregister(_ surfaceId: String) {
        surfaces.removeValue(forKey: surfaceId)
    }

    func surface(forId id: String) -> TerminalSurface? {
        surfaces[id]
    }

    func removeAll() {
        surfaces.removeAll()
    }
}
```

- [ ] **Step 2: Add splitLayouts field to Config**

In `Sources/Core/Config.swift`, add the new field:

1. Add property at line 15 (after `worktreeStartedAt`):
```swift
    var splitLayouts: [String: CodableSplitNode]
```

2. Add coding key in `CodingKeys` enum (after `worktreeStartedAt`):
```swift
        case splitLayouts = "split_layouts"
```

3. In `init()`, add default (after `worktreeStartedAt = [:]`):
```swift
        splitLayouts = [:]
```

4. In `init(from decoder:)`, add decode (after the `worktreeStartedAt` line):
```swift
        splitLayouts = try container.decodeIfPresent([String: CodableSplitNode].self, forKey: .splitLayouts) ?? [:]
```

- [ ] **Step 2: Verify build and existing tests pass**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test 2>&1 | tail -10`
Expected: All tests PASS (backward compatible — `decodeIfPresent` defaults to `[:]`)

- [ ] **Step 3: Commit**

```bash
git add Sources/Core/SurfaceRegistry.swift Sources/Core/Config.swift
xcodegen generate && git add amux.xcodeproj
git commit -m "feat: add SurfaceRegistry and splitLayouts Config field"
```

---

### Task 6: SessionManager Indexed Names

**Files:**
- Modify: `Sources/Core/SessionManager.swift:3-13`

- [ ] **Step 1: Add indexed session name method**

Add to `SessionManager` enum in `Sources/Core/SessionManager.swift`:

```swift
    /// Generate an indexed session name for an additional pane.
    static func indexedSessionName(base: String, index: Int) -> String {
        "\(base)-\(index)"
    }
```

- [ ] **Step 2: Verify build**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -5`
Expected: BUILD SUCCEEDED

- [ ] **Step 3: Commit**

```bash
git add Sources/Core/SessionManager.swift
git commit -m "feat: add indexed session naming for split panes"
```

---

### Task 7: Integration — SurfaceManager + StatusPublisher + RepoViewController

**Files:**
- Modify: `Sources/Core/TerminalSurfaceManager.swift` (full rewrite — 52 lines)
- Modify: `Sources/Status/StatusPublisher.swift:45-94`
- Modify: `Sources/UI/Repo/RepoViewController.swift:10-231`

**Note:** These three files are modified together in one task because they form a dependency chain — changing TerminalSurfaceManager's storage type breaks StatusPublisher and RepoViewController callers. Modifying them atomically keeps the build green at every commit.

- [ ] **Step 1: Rewrite TerminalSurfaceManager to use SplitTree**

```swift
// Sources/Core/TerminalSurfaceManager.swift
import Foundation

/// Manages SplitTree instances, keyed by worktree path.
/// Each worktree has one SplitTree (which may contain one or more panes).
class TerminalSurfaceManager {
    private var trees: [String: SplitTree] = [:]

    /// Get or create a SplitTree for the given worktree info.
    func tree(for info: WorktreeInfo, backend: String) -> SplitTree {
        if let existing = trees[info.path] {
            return existing
        }
        let surface = TerminalSurface()
        let sessionName: String
        if backend != "local" {
            let name = SessionManager.persistentSessionName(for: info.path)
            surface.sessionName = name
            surface.backend = backend
            sessionName = name
        } else {
            sessionName = "local-\(info.path)"
        }
        SurfaceRegistry.shared.register(surface)
        let tree = SplitTree(
            worktreePath: info.path,
            rootLeafId: UUID().uuidString,
            surfaceId: surface.id,
            sessionName: sessionName
        )
        trees[info.path] = tree
        return tree
    }

    func tree(forPath path: String) -> SplitTree? { trees[path] }

    @discardableResult
    func removeTree(forPath path: String) -> SplitTree? {
        guard let tree = trees.removeValue(forKey: path) else { return nil }
        // Unregister all surfaces and destroy them
        for leaf in tree.allLeaves {
            if let surface = SurfaceRegistry.shared.surface(forId: leaf.surfaceId) {
                surface.destroy()
            }
            SurfaceRegistry.shared.unregister(leaf.surfaceId)
        }
        return tree
    }

    func removeAll() {
        for (_, tree) in trees {
            for leaf in tree.allLeaves {
                if let surface = SurfaceRegistry.shared.surface(forId: leaf.surfaceId) {
                    surface.destroy()
                }
                SurfaceRegistry.shared.unregister(leaf.surfaceId)
            }
        }
        trees.removeAll()
    }

    var all: [String: SplitTree] { trees }
    var count: Int { trees.count }
}
```

- [ ] **Step 2: Update StatusPublisher to accept SplitTrees**

Modify `start(surfaces:)` and `updateSurfaces(_:)` in `Sources/Status/StatusPublisher.swift`. Change signature from `[String: TerminalSurface]` to `[String: SplitTree]`:

```swift
    func start(trees: [String: SplitTree]) {
        let inputWorktreePaths = Array(trees.keys)
        lock.lock()
        self.surfaces = [:]
        self.worktreePaths = [:]
        for (worktreePath, tree) in trees {
            for leaf in tree.allLeaves {
                if let surface = SurfaceRegistry.shared.surface(forId: leaf.surfaceId) {
                    self.surfaces[surface.id] = surface
                    self.worktreePaths[surface.id] = worktreePath
                }
            }
        }
        for terminalID in self.surfaces.keys {
            if trackers[terminalID] == nil {
                trackers[terminalID] = DebouncedStatusTracker()
            }
        }
        lock.unlock()
        stop()
        webhookProvider.updateWorktrees(inputWorktreePaths)
        timer = Timer.scheduledTimer(withTimeInterval: pollInterval, repeats: true) { [weak self] _ in
            self?.schedulePoll()
        }
        schedulePoll()
    }

    func updateSurfaces(_ trees: [String: SplitTree]) {
        let inputWorktreePaths = Array(trees.keys)
        lock.lock()
        self.surfaces = [:]
        self.worktreePaths = [:]
        for (worktreePath, tree) in trees {
            for leaf in tree.allLeaves {
                if let surface = SurfaceRegistry.shared.surface(forId: leaf.surfaceId) {
                    self.surfaces[surface.id] = surface
                    self.worktreePaths[surface.id] = worktreePath
                }
            }
        }
        for terminalID in self.surfaces.keys {
            if trackers[terminalID] == nil {
                trackers[terminalID] = DebouncedStatusTracker()
            }
        }
        lock.unlock()
        webhookProvider.updateWorktrees(inputWorktreePaths)
    }
```

The polling loop and `highestPriority` aggregation remain unchanged — multiple surfaces with the same worktreePath naturally aggregate via the existing webhook provider pattern at `StatusPublisher.swift:156`.

- [ ] **Step 3: Update RepoViewController to use SplitContainerView**

In `RepoViewController`, replace `activeSurface` pattern with `SplitContainerView`:

1. Replace properties (lines 25-28):
```swift
    private var worktrees: [WorktreeInfo] = []
    private var trees: [String: SplitTree] = [:]
    private var activeWorktreeIndex: Int = 0
    private var splitContainers: [String: SplitContainerView] = [:]
    private var activeSplitContainer: SplitContainerView?
    private var needsTerminalOnLayout = false
```

2. Replace `configure()`:
```swift
    func configure(worktrees: [WorktreeInfo], trees: [String: SplitTree]) {
        self.worktrees = worktrees
        self.trees = trees
        sidebarVC.setWorktrees(worktrees)
        if !worktrees.isEmpty {
            showTerminal(at: 0)
        }
    }
```

3. Rewrite `showTerminal(at:)`:
```swift
    func showTerminal(at index: Int) {
        guard index >= 0, index < worktrees.count else { return }
        activeWorktreeIndex = index
        let info = worktrees[index]
        guard let tree = trees[info.path] else { return }

        activeSplitContainer?.removeFromSuperview()

        let container: SplitContainerView
        if let existing = splitContainers[info.path] {
            container = existing
        } else {
            container = SplitContainerView(frame: terminalContainer.bounds)
            container.tree = tree
            container.delegate = self
            // Populate surface views from SurfaceRegistry
            for leaf in tree.allLeaves {
                if let surface = SurfaceRegistry.shared.surface(forId: leaf.surfaceId) {
                    container.surfaceViews[leaf.surfaceId] = surface.view
                }
            }
            splitContainers[info.path] = container
        }

        container.frame = terminalContainer.bounds
        container.autoresizingMask = [.width, .height]
        terminalContainer.addSubview(container)
        activeSplitContainer = container
        container.layoutTree()

        sidebarVC.selectWorktree(at: index)

        // Focus the focused leaf's terminal
        if let focusedLeaf = tree.allLeaves.first(where: { $0.id == tree.focusedId }),
           let surface = SurfaceRegistry.shared.surface(forId: focusedLeaf.surfaceId),
           let terminalView = surface.view {
            view.window?.makeFirstResponder(terminalView)
        }
    }
```

4. Replace `detachActiveTerminal()`:
```swift
    func detachActiveTerminal() {
        activeSplitContainer?.removeFromSuperview()
    }
```

- [ ] **Step 4: Implement SplitContainerDelegate on RepoViewController**

```swift
extension RepoViewController: SplitContainerDelegate {
    func splitContainer(_ view: SplitContainerView, didChangeFocus leafId: String) {
        guard let tree = trees[worktrees[activeWorktreeIndex].path],
              let leaf = tree.root.findLeaf(id: leafId),
              let surface = SurfaceRegistry.shared.surface(forId: leaf.surfaceId),
              let terminalView = surface.view else { return }
        self.view.window?.makeFirstResponder(terminalView)
    }

    func splitContainer(_ view: SplitContainerView, didRequestSplit axis: SplitAxis) {
        // Delegated to MainWindowController via RepoViewDelegate (wired in Task 8)
    }

    func splitContainer(_ view: SplitContainerView, didRequestClosePane leafId: String) {
        // Delegated to MainWindowController via RepoViewDelegate (wired in Task 8)
    }

    func splitContainerDidChangeLayout(_ view: SplitContainerView) {
        // Trigger config save (wired in Task 8)
    }
}
```

- [ ] **Step 5: Update MainWindowController callers**

In `MainWindowController`, update all calls from old API to new:
- `surfaceManager.surface(for: info)` → `surfaceManager.tree(for: info, backend:)`
- `surfaceManager.all` now returns `[String: SplitTree]`
- `statusPublisher.start(surfaces:)` → `statusPublisher.start(trees:)`
- `statusPublisher.updateSurfaces(surfaceManager.all)` → `statusPublisher.updateSurfaces(surfaceManager.all)`
- `repoVC.configure(worktrees:surfaces:)` → `repoVC.configure(worktrees:trees:)`
- `allWorktrees` type: `[(info: WorktreeInfo, surface: TerminalSurface)]` → `[(info: WorktreeInfo, tree: SplitTree)]`

- [ ] **Step 6: Verify full build**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -10`
Expected: BUILD SUCCEEDED

- [ ] **Step 7: Run all tests**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test 2>&1 | tail -10`
Expected: All tests PASS

- [ ] **Step 8: Commit**

```bash
git add Sources/Core/TerminalSurfaceManager.swift Sources/Status/StatusPublisher.swift Sources/UI/Repo/RepoViewController.swift Sources/App/MainWindowController.swift
git commit -m "feat: integrate SplitTree into SurfaceManager, StatusPublisher, and RepoViewController"
```

---

### Task 8: MainWindowController — Keybindings + Integration

**Files:**
- Modify: `Sources/App/MainWindowController.swift:1120-1129` (AmuxWindow)
- Modify: `Sources/App/MainWindowController.swift:880-927` (surface creation)

- [ ] **Step 1: Add split keybindings to AmuxWindow**

Expand `AmuxWindow.sendEvent()` at line 1120:

```swift
class AmuxWindow: NSWindow {
    override func sendEvent(_ event: NSEvent) {
        if event.type == .keyDown {
            let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)

            // Escape: exit spotlight
            if event.keyCode == 53, MainWindowController.shouldHandleEscShortcut() {
                return
            }

            // Cmd+D: horizontal split
            if flags == .command && event.charactersIgnoringModifiers == "d" {
                if let mwc = windowController as? MainWindowController {
                    mwc.splitFocusedPane(axis: .horizontal)
                    return
                }
            }

            // Cmd+Shift+D: vertical split
            if flags == [.command, .shift] && event.charactersIgnoringModifiers == "D" {
                if let mwc = windowController as? MainWindowController {
                    mwc.splitFocusedPane(axis: .vertical)
                    return
                }
            }

            // Cmd+Shift+W: close pane
            if flags == [.command, .shift] && event.charactersIgnoringModifiers == "W" {
                if let mwc = windowController as? MainWindowController {
                    mwc.closeFocusedPane()
                    return
                }
            }

            // Cmd+Option+Arrows: focus navigation
            if flags == [.command, .option] {
                switch event.keyCode {
                case 123: // left
                    (windowController as? MainWindowController)?.moveFocus(.horizontal, positive: false); return
                case 124: // right
                    (windowController as? MainWindowController)?.moveFocus(.horizontal, positive: true); return
                case 125: // down
                    (windowController as? MainWindowController)?.moveFocus(.vertical, positive: true); return
                case 126: // up
                    (windowController as? MainWindowController)?.moveFocus(.vertical, positive: false); return
                default: break
                }
            }

            // Cmd+Ctrl+Arrows: resize
            if flags == [.command, .control] {
                switch event.keyCode {
                case 123: // left
                    (windowController as? MainWindowController)?.resizeSplit(.horizontal, delta: -0.05); return
                case 124: // right
                    (windowController as? MainWindowController)?.resizeSplit(.horizontal, delta: 0.05); return
                case 125: // down
                    (windowController as? MainWindowController)?.resizeSplit(.vertical, delta: 0.05); return
                case 126: // up
                    (windowController as? MainWindowController)?.resizeSplit(.vertical, delta: -0.05); return
                default: break
                }
            }

            // Cmd+Ctrl+=: reset ratio
            if flags == [.command, .control] && event.charactersIgnoringModifiers == "=" {
                (windowController as? MainWindowController)?.resetSplitRatio(); return
            }
        }
        super.sendEvent(event)
    }
}
```

- [ ] **Step 2: Add split action methods to MainWindowController**

Add these methods to `MainWindowController`:

```swift
    // MARK: - Split Pane Actions

    func splitFocusedPane(axis: SplitAxis) {
        guard let repoVC = currentRepoVC,
              let container = repoVC.activeSplitContainer,
              let tree = container.tree else { return }

        let sessionName = tree.nextSessionName()
        let surface = TerminalSurface()
        surface.sessionName = sessionName
        surface.backend = config.backend

        let leafId = UUID().uuidString
        tree.splitFocusedLeaf(axis: axis, newLeafId: leafId, newSurfaceId: surface.id, newSessionName: sessionName)

        // Create the surface in the terminal container
        let worktreePath = tree.worktreePath
        _ = surface.create(in: container, workingDirectory: worktreePath, sessionName: sessionName)

        // Register view and re-layout
        container.surfaceViews[surface.id] = surface.view
        container.layoutTree()

        // Register with status publisher
        statusPublisher.updateSurfaces(surfaceManager.all)

        // Persist layout
        saveSplitLayout(tree)
    }

    func closeFocusedPane() {
        guard let repoVC = currentRepoVC,
              let container = repoVC.activeSplitContainer,
              let tree = container.tree else { return }

        guard let closed = tree.closeFocusedLeaf() else { return }

        // Kill zmx session
        SessionManager.killSession(closed.sessionName, backend: config.backend)

        // Remove view
        container.surfaceViews.removeValue(forKey: closed.surfaceId)
        container.layoutTree()

        // Update status publisher
        statusPublisher.updateSurfaces(surfaceManager.all)

        // Persist layout
        saveSplitLayout(tree)
    }

    func moveFocus(_ axis: SplitAxis, positive: Bool) {
        guard let repoVC = currentRepoVC,
              let container = repoVC.activeSplitContainer else { return }
        _ = container.focusLeaf(direction: axis, positive: positive)
    }

    func resizeSplit(_ axis: SplitAxis, delta: CGFloat) {
        guard let repoVC = currentRepoVC,
              let container = repoVC.activeSplitContainer,
              let tree = container.tree else { return }
        guard let splitId = tree.nearestAncestorSplit(axis: axis) else { return }
        // Find current ratio and adjust
        if case .split(_, _, let ratio, _, _) = findNode(id: splitId, in: tree.root) ?? tree.root {
            tree.updateRatio(splitId: splitId, newRatio: ratio + delta)
            container.layoutTree()
            saveSplitLayout(tree)
        }
    }

    func resetSplitRatio() {
        guard let repoVC = currentRepoVC,
              let container = repoVC.activeSplitContainer,
              let tree = container.tree else { return }
        // Reset both axes' nearest ancestor
        for axis in [SplitAxis.horizontal, .vertical] {
            if let splitId = tree.nearestAncestorSplit(axis: axis) {
                tree.updateRatio(splitId: splitId, newRatio: 0.5)
            }
        }
        container.layoutTree()
        saveSplitLayout(tree)
    }

    private func saveSplitLayout(_ tree: SplitTree) {
        config.splitLayouts[tree.worktreePath] = tree.toCodable()
        config.save()
    }

    private func findNode(id: String, in node: SplitNode) -> SplitNode? {
        if node.id == id { return node }
        if case .split(_, _, _, let first, let second) = node {
            return findNode(id: id, in: first) ?? findNode(id: id, in: second)
        }
        return nil
    }
```

- [ ] **Step 3: Verify full build**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -10`
Expected: BUILD SUCCEEDED

- [ ] **Step 4: Run all tests**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test 2>&1 | tail -10`
Expected: All tests PASS

- [ ] **Step 5: Commit**

```bash
git add Sources/App/MainWindowController.swift
git commit -m "feat: add split pane keybindings and integration in MainWindowController"
```

---

### Task 9: Dashboard Pane Count Display

**Files:**
- Modify: `Sources/UI/Dashboard/DashboardViewController.swift`

- [ ] **Step 1: Add pane count to agent card display**

In the method that builds `AgentDisplayInfo` (in `MainWindowController.buildAgentDisplayInfos()`), add a `paneCount` field derived from `tree.leafCount`. In the dashboard card view, display "N panes" when `paneCount > 1`.

This is a minor UI addition — the exact location depends on `AgentCardView` or `MiniCardView` layout. Add a small label below the status badge.

- [ ] **Step 2: Verify build**

Run: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -5`
Expected: BUILD SUCCEEDED

- [ ] **Step 3: Commit**

```bash
git add Sources/UI/Dashboard/DashboardViewController.swift Sources/App/MainWindowController.swift
git commit -m "feat: show pane count on dashboard agent cards"
```

---

### Task 10: Layout Restoration on Launch

**Files:**
- Modify: `Sources/App/MainWindowController.swift` (in `loadWorkspaces()`)
- Modify: `Sources/Terminal/SplitTree.swift` (add static factory)

- [ ] **Step 1: Add SplitTree factory method for restoring from CodableSplitNode**

Add to `Sources/Terminal/SplitTree.swift`:

```swift
    /// Rebuild a SplitTree from a persisted CodableSplitNode layout.
    /// Creates TerminalSurface instances for each leaf and registers them in SurfaceRegistry.
    /// zmx `attach` is idempotent — creates if session doesn't exist.
    static func restore(
        from codable: CodableSplitNode,
        worktreePath: String,
        backend: String
    ) -> SplitTree? {
        let (node, firstLeafId) = restoreNode(from: codable, backend: backend)
        guard let node = node, let firstLeafId = firstLeafId else { return nil }
        let baseName = SessionManager.persistentSessionName(for: worktreePath)
        let tree = SplitTree(worktreePath: worktreePath, root: node, baseSessionName: baseName)
        tree.focusedId = firstLeafId
        return tree
    }

    /// Convenience init that accepts a pre-built root node.
    private init(worktreePath: String, root: SplitNode, baseSessionName: String) {
        self.worktreePath = worktreePath
        self.root = root
        self.baseSessionName = baseSessionName
        self.focusedId = root.allLeaves.first?.id ?? ""
    }

    private static func restoreNode(
        from codable: CodableSplitNode,
        backend: String
    ) -> (SplitNode?, String?) {
        switch codable {
        case .leaf(let sessionName):
            let surface = TerminalSurface()
            surface.sessionName = sessionName
            surface.backend = backend
            SurfaceRegistry.shared.register(surface)
            let leafId = UUID().uuidString
            let node = SplitNode.leaf(id: leafId, surfaceId: surface.id, sessionName: sessionName)
            return (node, leafId)

        case .split(let axisStr, let ratio, let first, let second):
            guard let axis = SplitAxis(rawValue: axisStr) else { return (nil, nil) }
            let (firstNode, firstLeaf) = restoreNode(from: first, backend: backend)
            let (secondNode, _) = restoreNode(from: second, backend: backend)
            guard let firstNode = firstNode, let secondNode = secondNode else { return (nil, nil) }
            let node = SplitNode.split(
                id: UUID().uuidString,
                axis: axis,
                ratio: CGFloat(ratio),
                first: firstNode,
                second: secondNode
            )
            return (node, firstLeaf)
        }
    }
```

- [ ] **Step 2: Use restore in MainWindowController.loadWorkspaces()**

In `loadWorkspaces()`, after building the worktree list but before creating default single-leaf trees, check for a persisted layout:

```swift
    for info in worktrees {
        let tree: SplitTree
        if let savedLayout = config.splitLayouts[info.path] {
            // Restore multi-pane layout from config
            if let restored = SplitTree.restore(from: savedLayout, worktreePath: info.path, backend: config.backend) {
                tree = restored
            } else {
                // Fallback: corrupted layout, create single-pane
                tree = surfaceManager.tree(for: info, backend: config.backend)
            }
        } else {
            // No saved layout: single pane (default)
            tree = surfaceManager.tree(for: info, backend: config.backend)
        }
        // ... register with AgentHead, add to allWorktrees, etc.
    }
```

zmx `attach` is idempotent — if the session exists it reattaches, if not it creates a new one. No special stale-session handling needed.

- [ ] **Step 3: Verify build + test**

Run: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test 2>&1 | tail -10`
Expected: All tests PASS

- [ ] **Step 4: Commit**

```bash
git add Sources/Terminal/SplitTree.swift Sources/App/MainWindowController.swift
git commit -m "feat: restore split pane layouts from config on launch"
```

---

### Task 11: UI Automation Tests for Split Pane

**Files:**
- Create: `UITests/Pages/SplitPanePage.swift`
- Create: `UITests/Tests/SplitPaneTests.swift`

- [ ] **Step 1: Add accessibility identifiers to SplitContainerView and DividerView**

In `Sources/UI/Split/SplitContainerView.swift`, set accessibility identifiers during layout:

```swift
    // In layoutTree(), after positioning each leaf view:
    // surface view identifier set to "splitPane.leaf.<leafId>"
    // In layoutDividers(), for each divider:
    // divider.setAccessibilityIdentifier("splitPane.divider.\(id)")
```

In `Sources/UI/Split/SplitContainerView.swift` init:
```swift
    setAccessibilityIdentifier("splitPane.container")
```

- [ ] **Step 2: Create SplitPanePage page object**

```swift
// UITests/Pages/SplitPanePage.swift
import XCTest

/// Page object for split pane UI elements within the repo view.
class SplitPanePage {
    let app: XCUIApplication

    init(_ app: XCUIApplication) { self.app = app }

    var container: XCUIElement { app.groups["splitPane.container"] }

    var panes: XCUIElementQuery {
        app.groups.matching(NSPredicate(format: "identifier BEGINSWITH 'splitPane.leaf.'"))
    }

    var dividers: XCUIElementQuery {
        app.groups.matching(NSPredicate(format: "identifier BEGINSWITH 'splitPane.divider.'"))
    }

    var paneCount: Int { panes.count }
    var dividerCount: Int { dividers.count }
}
```

- [ ] **Step 3: Add SplitPanePage to AppPage**

In `UITests/Pages/AppPage.swift`, add:
```swift
    lazy var splitPane = SplitPanePage(app)
```

- [ ] **Step 4: Write UI automation tests**

```swift
// UITests/Tests/SplitPaneTests.swift
import XCTest

class SplitPaneTests: AmuxUITestCase {

    /// Test: Cmd+D creates a horizontal split, producing 2 panes and 1 divider.
    func testHorizontalSplit() {
        // Navigate to a repo tab (assumes at least one workspace configured)
        // The exact navigation depends on test config — may need to open a project first
        guard page.repo.terminal.waitForExistence(timeout: 10) else {
            XCTFail("Terminal not visible — need a configured workspace for this test")
            return
        }

        // Initial state: 1 pane, 0 dividers
        XCTAssertEqual(page.splitPane.paneCount, 1)
        XCTAssertEqual(page.splitPane.dividerCount, 0)

        // Cmd+D: horizontal split
        page.app.typeKey("d", modifierFlags: .command)

        // Wait for second pane to appear
        let secondPane = page.splitPane.panes.element(boundBy: 1)
        XCTAssertTrue(secondPane.waitForExistence(timeout: 5), "Second pane should appear after Cmd+D")
        XCTAssertEqual(page.splitPane.paneCount, 2)
        XCTAssertEqual(page.splitPane.dividerCount, 1)
    }

    /// Test: Cmd+Shift+D creates a vertical split.
    func testVerticalSplit() {
        guard page.repo.terminal.waitForExistence(timeout: 10) else {
            XCTFail("Terminal not visible")
            return
        }

        page.app.typeKey("d", modifierFlags: [.command, .shift])

        let secondPane = page.splitPane.panes.element(boundBy: 1)
        XCTAssertTrue(secondPane.waitForExistence(timeout: 5), "Second pane should appear after Cmd+Shift+D")
        XCTAssertEqual(page.splitPane.paneCount, 2)
    }

    /// Test: Cmd+Shift+W closes a pane, returning to single pane.
    func testClosePane() {
        guard page.repo.terminal.waitForExistence(timeout: 10) else {
            XCTFail("Terminal not visible")
            return
        }

        // Split first
        page.app.typeKey("d", modifierFlags: .command)
        let secondPane = page.splitPane.panes.element(boundBy: 1)
        XCTAssertTrue(secondPane.waitForExistence(timeout: 5))

        // Close the focused (new) pane
        page.app.typeKey("w", modifierFlags: [.command, .shift])

        // Should return to 1 pane
        XCTAssertTrue(secondPane.waitForNonExistence(timeout: 5), "Second pane should disappear after Cmd+Shift+W")
        XCTAssertEqual(page.splitPane.paneCount, 1)
        XCTAssertEqual(page.splitPane.dividerCount, 0)
    }

    /// Test: Cannot close the last remaining pane.
    func testCannotCloseLastPane() {
        guard page.repo.terminal.waitForExistence(timeout: 10) else {
            XCTFail("Terminal not visible")
            return
        }

        XCTAssertEqual(page.splitPane.paneCount, 1)

        // Try to close
        page.app.typeKey("w", modifierFlags: [.command, .shift])

        // Still 1 pane
        XCTAssertEqual(page.splitPane.paneCount, 1)
    }

    /// Test: Recursive splits — split twice to get 3 panes.
    func testRecursiveSplit() {
        guard page.repo.terminal.waitForExistence(timeout: 10) else {
            XCTFail("Terminal not visible")
            return
        }

        // First split: horizontal
        page.app.typeKey("d", modifierFlags: .command)
        let secondPane = page.splitPane.panes.element(boundBy: 1)
        XCTAssertTrue(secondPane.waitForExistence(timeout: 5))

        // Second split: vertical on the new pane
        page.app.typeKey("d", modifierFlags: [.command, .shift])
        let thirdPane = page.splitPane.panes.element(boundBy: 2)
        XCTAssertTrue(thirdPane.waitForExistence(timeout: 5))

        XCTAssertEqual(page.splitPane.paneCount, 3)
        XCTAssertEqual(page.splitPane.dividerCount, 2)
    }
}
```

- [ ] **Step 5: Regenerate Xcode project and verify build**

Run: `xcodegen generate && xcodebuild -project amux.xcodeproj -scheme amuxUITests -configuration Debug build 2>&1 | tail -5`
Expected: BUILD SUCCEEDED

- [ ] **Step 6: Commit**

```bash
git add UITests/Pages/SplitPanePage.swift UITests/Tests/SplitPaneTests.swift UITests/Pages/AppPage.swift Sources/UI/Split/SplitContainerView.swift Sources/UI/Split/DividerView.swift amux.xcodeproj
git commit -m "test: add UI automation tests for split pane operations"
```
