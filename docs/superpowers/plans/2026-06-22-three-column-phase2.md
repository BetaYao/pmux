# 3-Column Layout — Phase 2 (otty header + native panel + center overlay)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Flatten the title bar to otty style (centered title + flat right icons, no capsule); replace the right panel's yazi/in-panel-diff with a native file tree + a plain changes list; open file content and diffs as a full-cover overlay over the center terminal; then remove the dead grid/topSmall/topLarge layout modes.

**Architecture:** Builds on Phase 1 (HEAD `e32f442`, 3-column layout: worktrees | terminal | side panel). `WorktreeSidePanelViewController` becomes navigation-only and reports selections via a delegate to `DashboardViewController`, which shows the detail in a center overlay pinned over `leftRightFocusPanel`. The terminal `SplitContainerView` stays embedded underneath the overlay. `DiffReviewView` is reused only inside the overlay.

**Tech Stack:** Swift 5.10, AppKit, XcodeGen, XCTest. No new dependencies.

## Global Constraints

- macOS 14.0+, Swift 5.10, AppKit (not SwiftUI). No external SPM dependencies.
- Delegate pattern throughout (not Combine/async-await).
- After adding files / editing `project.yml`, run `xcodegen generate` if a new symbol isn't found.
- Build: `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build`
- Test: `xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test`
- Ghostty C calls go through `ghosttyLock` via `TerminalSurface` — don't add locking.
- Locate code by SYMBOL NAME (grep), not the line numbers cited here — they may drift.

## Reference facts (verified 2026-06-22)

- `FocusPanelView` (`Sources/UI/Dashboard/FocusPanelView.swift`): `let terminalContainer = NSView()`; has `showDimOverlay(opacity:)`/`hideDimOverlay()`. The center column instance is `DashboardViewController.leftRightFocusPanel`.
- `DiffReviewView(worktreePath: String, loadSnapshot: (() -> GitDiffSnapshot)? = nil)` (`Sources/UI/Diff/DiffReviewView.swift`). It loads the snapshot when it moves to a window. It has private `showDiffForFile(path:)` — Task 5 may make it `func` (non-private) to focus a single file; if exposing is awkward, just show the full DiffReviewView (acceptable).
- `GitDiff.changedFileEntries(worktreePath: String) -> [GitChangedFile]`; `struct GitChangedFile { let path: String; let oldPath: String?; let status: DiffFile.FileStatus; let stage: GitChangeStage }`. `DiffFile.FileStatus` = `.added/.modified/.deleted/.renamed/.unknown`.
- TitleBar callers in `MainWindowController`: `updateChromeState(...)` (~557), `updateFocusedWorktree(title:tokenText:)` (~568, ~580), `updatePrimaryCapsuleFrames(_:)` (~166), `updateNotificationSummary(...)` (~1016, ~1065), `setWindowHovered(_:)` (~534/538). TitleBar embedded via `setupNativeTitleBar()` (~468) at `TitleBarView.Layout.barHeight` = 45.
- Current `WorktreeSidePanelViewController` API: `init(worktreePath:initialTab:makeDiffReviewView:makeYaziSurface:)`, `func setWorktree(_:)`, `enum SidePanelTab { files=0, changes=1 }`, `var selectedTabForTesting`, `var worktreePathForTesting`. Tests in `Tests/WorktreeSidePanelViewControllerTests.swift`.

---

## Task 1: Flatten the TitleBar (remove capsule, otty style)

**Files:**
- Modify: `Sources/UI/TitleBar/TitleBarView.swift`
- Modify: `Sources/App/MainWindowController.swift` (caller updates)

**Interfaces:**
- Produces: `TitleBarView` with a centered `NSTextField` title and the existing right icon buttons; NO capsule.
  - Keep: `updateFocusedWorktree(title:tokenText:)` (sets centered title; ignores tokenText), `updateChromeState(isGridLayout:hasWorkspaces:canCleanWorktrees:)` (button visibility only), `setWindowHovered(_:)`, static `clampTitle(_:limit:)`, `updateNotificationSummary(entry:unreadCount:)` (becomes a no-op/keeps a stored value but no capsule border).
  - Remove: `updatePrimaryCapsuleFrames(_:)` and its `MainWindowController` call site; all capsule subviews/state (`leftArcBlock`, `capsule*` labels, `usageProgress*`, `secondaryUsageProgress*`, `primaryCapsuleStack`, `tipRotationTimer`, `usesFocusedWorktreeMode`, `currentPrimaryCapsuleIndex`, `primaryCapsuleFrames`, `isPrimaryCapsuleHovered`, `primaryCapsuleTrackingArea`, `setupLeftArcBlock`, `showCurrentPrimaryCapsuleFrame`, `updatePrimaryCapsuleSeparators`, `startTipRotationIfNeeded`).

- [ ] **Step 1: Rewrite TitleBarView to the flat layout.** Replace the capsule with a single centered title label; keep `rightArcBlock`'s buttons but drop its capsule background (flat). Concretely:
  - Add `private let titleLabel = NSTextField(labelWithString: "")` — centered, `SemanticColors.text`, 12pt semibold, `lineBreakMode = .byTruncatingTail`, single line.
  - In `setup()`: call only `setupRightArcBlock()` and a new `setupTitleLabel()`. Pin `titleLabel` centerX/centerY to the bar, with `leading >= leadingAnchor + 80` (clear traffic lights) and `trailing <= rightArcBlock.leadingAnchor - 8`.
  - Keep `setupRightArcBlock()` as-is but set `rightArcBlock.layer?.backgroundColor = .clear` (flat — no capsule fill) and remove `updateArcBlockColors()`'s references to `leftArcBlock`. Keep button hover tracking.
  - `updateFocusedWorktree(title:tokenText:)` → `titleLabel.stringValue = Self.clampTitle(title); titleLabel.toolTip = title` (ignore tokenText).
  - `updateNotificationSummary(...)` → keep signature; body becomes empty (or just stores unread count if used elsewhere — grep; if nothing reads it, leave empty).
  - Delete the capsule methods/properties listed above. Keep `Layout.barHeight = 45`.
  - `viewDidChangeEffectiveAppearance`/`applyColors` → drop capsule-label color lines; keep button tint + `titleLabel.textColor = SemanticColors.text`.

- [ ] **Step 2: Update MainWindowController callers.**
  - Remove the `titleBar.updatePrimaryCapsuleFrames(...)` call (~line 166) and, if it leaves an unused `UsageSummaryStore.onUpdate` closure body, simplify that closure (don't remove the store if used elsewhere — grep).
  - `updateChromeState` / `updateFocusedWorktree` / `setWindowHovered` call sites stay (signatures unchanged).
  - Build will reveal any other removed-symbol references — fix each by deletion.

- [ ] **Step 3: Build.** `xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -20` → BUILD SUCCEEDED.

- [ ] **Step 4: Grep for dangling refs.** `grep -rn "updatePrimaryCapsuleFrames\|leftArcBlock\|capsuleLeadingLabel\|usesFocusedWorktreeMode" Sources` → only matches (if any) inside TitleBarView are acceptable; none should remain. Expected: no hits.

- [ ] **Step 5: Run the app.** Confirm the header shows a centered title + flat right icons, no capsule box, no token/usage text.

- [ ] **Step 6: Commit.**
```bash
git add Sources/UI/TitleBar/TitleBarView.swift Sources/App/MainWindowController.swift
git commit -m "feat(ui): flatten title bar to otty style (remove capsule)"
```

---

## Task 2: Center overlay host over the terminal

**Files:**
- Create: `Sources/UI/SidePanel/CenterOverlayView.swift`
- Modify: `Sources/UI/Dashboard/DashboardViewController.swift`

**Interfaces:**
- Produces:
  - `final class CenterOverlayView: NSView` — a container with a header bar (title label + close button) and a content area; `init(title: String, content: NSView, onClose: @escaping () -> Void)`. Close button calls `onClose`. Background opaque (`SemanticColors.bg`) so the terminal doesn't bleed through.
  - On `DashboardViewController`: `func showCenterOverlay(_ content: NSView, title: String)` and `func dismissCenterOverlay()`. `showCenterOverlay` removes any existing overlay, builds a `CenterOverlayView`, pins it to all four edges of `leftRightFocusPanel` (covering `terminalContainer`), and stores it in `private var centerOverlay: CenterOverlayView?`. `dismissCenterOverlay` removes it and restores first responder to the active terminal pane (mirror the deferred `makeFirstResponder` pattern already in `embedSplitContainerForSelectedAgent`).
  - Esc dismiss: in `keyDown` add handling — but the dashboard's `keyDown` is gated by `isInDState`. Instead, `CenterOverlayView` overrides `performKeyEquivalent`/`keyDown` for keyCode 53 (Esc) → `onClose()`, and is made first responder when shown.

- [ ] **Step 1 (test): overlay show/dismiss state.** In a new `Tests/CenterOverlayTests.swift`, drive a `DashboardViewController` (no window needed): after `showCenterOverlay(NSView(), title: "x")`, the focus panel contains a `CenterOverlayView` subview; after `dismissCenterOverlay()`, it does not. Use the `descendantViews()` helper already in `WorktreeSidePanelViewControllerTests.swift` — but to avoid the duplicate-extension collision (noted in Phase 1 review), MOVE that extension into a new shared `Tests/TestViewHelpers.swift` and reference it from both test files.

```swift
// Tests/CenterOverlayTests.swift
import XCTest
import AppKit
@testable import amux

final class CenterOverlayTests: XCTestCase {
    func testShowThenDismiss() {
        let vc = DashboardViewController()
        vc.loadViewIfNeeded()
        vc.showCenterOverlay(NSView(), title: "Detail")
        XCTAssertTrue(vc.view.descendantViews().contains { $0 is CenterOverlayView })
        vc.dismissCenterOverlay()
        XCTAssertFalse(vc.view.descendantViews().contains { $0 is CenterOverlayView })
    }
}
```

- [ ] **Step 2: Run test → FAIL** (symbols undefined). `xcodebuild ... test -only-testing:amuxTests/CenterOverlayTests`.

- [ ] **Step 3: Implement** `CenterOverlayView` and the two dashboard methods. Header bar: title `NSTextField` left, close `NSButton` (SF `xmark`) right, 32pt tall; content fills below. Pin overlay to `leftRightFocusPanel` edges. Make the overlay first responder on show; Esc → onClose → `dismissCenterOverlay()`.

- [ ] **Step 4: Run test → PASS.**

- [ ] **Step 5: Commit.**
```bash
git add Sources/UI/SidePanel/CenterOverlayView.swift Sources/UI/Dashboard/DashboardViewController.swift Tests/CenterOverlayTests.swift Tests/TestViewHelpers.swift Tests/WorktreeSidePanelViewControllerTests.swift
git commit -m "feat(ui): center overlay host over the terminal"
```

---

## Task 3: Native file tree (NSOutlineView)

**Files:**
- Create: `Sources/UI/SidePanel/FileTreeOutlineController.swift`
- Test: `Tests/FileTreeOutlineControllerTests.swift`

**Interfaces:**
- Produces:
  - `final class FileTreeNode` — `let url: URL; let isDirectory: Bool; var children: [FileTreeNode]?` (lazy).
  - `final class FileTreeOutlineController: NSObject, NSOutlineViewDataSource, NSOutlineViewDelegate` with `let outlineView: NSOutlineView`, `init(rootPath: String)`, `func setRoot(_ path: String?)`, `var onSelectFile: ((String) -> Void)?`.
  - `static func childNodes(of directory: URL) -> [FileTreeNode]` — pure: lists `directory` via `FileManager`, hides dotfiles (name hasPrefix "."), directories first then files, alphabetical. This is the unit-tested seam.

- [ ] **Step 1 (test):** create a temp dir with `a.txt`, `.hidden`, and subdir `sub/`; assert `childNodes(of:)` returns `[sub, a.txt]` (dir first, dotfile excluded).

```swift
// Tests/FileTreeOutlineControllerTests.swift
import XCTest
@testable import amux

final class FileTreeOutlineControllerTests: XCTestCase {
    func testChildNodesHidesDotfilesAndSortsDirsFirst() throws {
        let fm = FileManager.default
        let root = fm.temporaryDirectory.appendingPathComponent("amux-filetree-test-\(ProcessInfo.processInfo.globallyUniqueString)")
        try fm.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? fm.removeItem(at: root) }
        try "x".write(to: root.appendingPathComponent("a.txt"), atomically: true, encoding: .utf8)
        try "x".write(to: root.appendingPathComponent(".hidden"), atomically: true, encoding: .utf8)
        try fm.createDirectory(at: root.appendingPathComponent("sub"), withIntermediateDirectories: true)

        let names = FileTreeOutlineController.childNodes(of: root).map { $0.url.lastPathComponent }
        XCTAssertEqual(names, ["sub", "a.txt"])
    }
}
```

- [ ] **Step 2: Run test → FAIL.**
- [ ] **Step 3: Implement** the controller + `childNodes`. Outline view: one text column, expandable directories (lazy populate `children` on `outlineViewItemWillExpand` or in the data source), single text cell with an SF folder/doc icon. `outlineViewSelectionDidChange` → if selection is a file, call `onSelectFile(node.url.path)`.
- [ ] **Step 4: Run test → PASS.**
- [ ] **Step 5: Commit.**
```bash
git add Sources/UI/SidePanel/FileTreeOutlineController.swift Tests/FileTreeOutlineControllerTests.swift
git commit -m "feat(ui): native NSOutlineView file tree controller"
```

---

## Task 4: File content viewer

**Files:**
- Create: `Sources/UI/SidePanel/FileContentView.swift`
- Test: `Tests/FileContentViewTests.swift`

**Interfaces:**
- Produces:
  - `final class FileContentView: NSView` — read-only mono `NSTextView` inside an `NSScrollView`; `init(path: String)`.
  - `static func readContent(at path: String, maxBytes: Int = 1_048_576) -> String?` — returns file text if it is valid UTF-8 and ≤ maxBytes; otherwise `nil` (caller shows a placeholder). This is the unit-tested seam.
  - When `readContent` returns nil, `FileContentView` shows a centered placeholder label ("Cannot preview this file").

- [ ] **Step 1 (test):** UTF-8 file → returns its text; a file exceeding `maxBytes` (pass a small `maxBytes`) → nil; missing path → nil.

```swift
// Tests/FileContentViewTests.swift
import XCTest
@testable import amux

final class FileContentViewTests: XCTestCase {
    func testReadsUTF8AndRejectsOversizeAndMissing() throws {
        let fm = FileManager.default
        let f = fm.temporaryDirectory.appendingPathComponent("amux-fcv-\(ProcessInfo.processInfo.globallyUniqueString).txt")
        try "hello".write(to: f, atomically: true, encoding: .utf8)
        defer { try? fm.removeItem(at: f) }
        XCTAssertEqual(FileContentView.readContent(at: f.path), "hello")
        XCTAssertNil(FileContentView.readContent(at: f.path, maxBytes: 2)) // oversize
        XCTAssertNil(FileContentView.readContent(at: f.path + ".nope"))    // missing
    }
}
```

- [ ] **Step 2: Run test → FAIL.**
- [ ] **Step 3: Implement** `readContent` (check file size via `FileManager.attributesOfItem` before reading; then `String(contentsOf:encoding:.utf8)`) and the view.
- [ ] **Step 4: Run test → PASS.**
- [ ] **Step 5: Commit.**
```bash
git add Sources/UI/SidePanel/FileContentView.swift Tests/FileContentViewTests.swift
git commit -m "feat(ui): read-only file content viewer"
```

---

## Task 5: Rewrite the side panel (native tree + list) and wire to the overlay

**Files:**
- Modify: `Sources/UI/SidePanel/WorktreeSidePanelViewController.swift` (major rewrite)
- Modify: `Sources/UI/Dashboard/DashboardViewController.swift` (own panel, route delegate to overlay)
- Modify: `Tests/WorktreeSidePanelViewControllerTests.swift` (drop yazi cases, add delegate-spy cases)
- Modify: `Sources/UI/Diff/DiffReviewView.swift` only if exposing `showDiffForFile(path:)` (optional)

**Interfaces:**
- Produces:
  - `protocol WorktreeSidePanelDelegate: AnyObject { func sidePanel(_ vc: WorktreeSidePanelViewController, didSelectFile path: String); func sidePanel(_ vc: WorktreeSidePanelViewController, didSelectChange path: String) }`
  - `WorktreeSidePanelViewController`: remove `makeYaziSurface` and the in-panel `makeDiffReviewView`/`DiffReviewView` usage. New init `init(worktreePath: String?, initialTab: SidePanelTab = .files)`. Add `weak var delegate: WorktreeSidePanelDelegate?`. Files tab hosts a `FileTreeOutlineController` (its `onSelectFile` → `delegate?.sidePanel(self, didSelectFile:)`). Changes tab hosts a plain list of `GitChangedFile` rows (status badge + path); row activation → `delegate?.sidePanel(self, didSelectChange:)`. Keep `setWorktree(_:)`, `SidePanelTab`, `selectedTabForTesting`, `worktreePathForTesting`. Default initial tab is now `.files`.
  - `DashboardViewController` conforms to `WorktreeSidePanelDelegate`: `didSelectFile` → `showCenterOverlay(FileContentView(path:), title: <basename>)`; `didSelectChange` → `showCenterOverlay(DiffReviewView(worktreePath: <selected worktree>), title: "Changes: <basename>")` (optionally focus the file via `showDiffForFile` if exposed). Set `sidePanelVC.delegate = self` where `sidePanelVC` is created.

- [ ] **Step 1 (test):** rewrite `WorktreeSidePanelViewControllerTests`:
  - Remove the three yazi tests and the `makeYaziSurface` usages.
  - Keep/adjust: init holds path; `setWorktree` updates path; nil → placeholder (`sidePanel.emptyPlaceholder`).
  - Add: a `SpyDelegate` conforming to `WorktreeSidePanelDelegate`; simulate a file selection by invoking the file-tree controller's `onSelectFile` (expose the controller for testing via a `fileTreeForTesting` accessor, or call a small internal `handleFileSelection(_:)`); assert the spy received `didSelectFile`. Likewise a `handleChangeSelection(_:)` path for `didSelectChange`.
  - Move the `descendantViews()` extension reference to the shared `Tests/TestViewHelpers.swift` (created in Task 2).

- [ ] **Step 2: Run tests → FAIL** (delegate/types undefined).

- [ ] **Step 3: Implement** the rewrite. Changes-list rendering: build from `GitDiff.changedFileEntries(worktreePath:)`; each row shows a one-letter status badge (`A/M/D/R/?`) + relative path; empty → "No changes"; non-git → reuse the empty placeholder. Files-tab: embed `FileTreeOutlineController.outlineView` in a scroll view; `setWorktree` re-roots the tree and reloads the changes list. Add `handleFileSelection`/`handleChangeSelection` (or expose controller) so tests can drive selection deterministically.

- [ ] **Step 4: Wire the dashboard.** Make `DashboardViewController` the panel delegate and implement the two methods to call `showCenterOverlay`. Build.

- [ ] **Step 5: Run the full side-panel + overlay tests → PASS.** Then build the app: `xcodebuild ... build`.

- [ ] **Step 6: Run the app.** Files tab shows a native tree; clicking a file opens its content over the terminal (Esc closes). Changes tab shows a list; clicking a changed file opens its diff over the terminal.

- [ ] **Step 7: Commit.**
```bash
git add -A
git commit -m "feat(ui): native file tree + changes list panel; details open in center overlay"
```

---

## Task 6: Remove dead layout modes (grid / topSmall / topLarge)

Same as Phase 1's deferred cleanup. This unwinds the unused layout modes now that the 3-column layout is the only one.

**Files:**
- Modify: `Sources/UI/Dashboard/DashboardViewController.swift`, `Sources/UI/Dashboard/DashboardFocusController.swift`, `Sources/Core/Config.swift`
- Modify/Delete: `Sources/UI/TitleBar/LayoutPopoverView.swift` (holds `DashboardLayout`), grid-only files (`DraggableGridView`, grid card/layout) if unreferenced
- Modify: `Tests/ConfigTests.swift`, `Tests/DashboardViewControllerClickTests.swift`

> Riskiest task — do it last, in small compiling increments, building after each. The compiler is the guide.

- [ ] **Step 1: Collapse `DashboardLayout` to a single concept.** Remove the enum and all `currentLayout`/`lastFocusLayout` branching; delete `setLayout`, `showLayout`'s non-leftRight arms, `rebuildCurrentLayout` switch arms; make `focusLayoutRefs(for:)` a parameterless accessor returning the leftRight refs. Build; fix errors.
- [ ] **Step 2: Remove grid + top* hierarchy.** Delete `setupGridLayout/rebuildGrid/currentGridLayout/layoutGridFrames`, `gridScrollView/gridContainer/gridCards`, `setupTopSmallLayout/setupTopLargeLayout`, all `topSmall*`/`topLarge*` props, their `LayoutMetrics` constants, and their `loadView()` calls. Remove `DraggableGridDelegate`/`GridCardReorderDelegate` conformances. Build; fix errors.
- [ ] **Step 3: Delete unused files.** `grep -rn "DraggableGridView\|GridLayout\|LayoutPopoverView\|DashboardLayout" Sources Tests` → delete files now unreferenced; `xcodegen generate` if removed.
- [ ] **Step 4: Simplify `DashboardFocusController`.** Remove `enterGrid`/grid `Mode` if unused; keep focus-layout nav. Drop `Snapshot.layout` if nothing reads it.
- [ ] **Step 5: Remove `Config.dashboardLayout`** (property, CodingKey, default, decode line). Old configs ignore the stale key (backward compatible).
- [ ] **Step 6: Update tests.** Delete `ConfigTests.testDefaultDashboardLayout`; rewrite/remove `DashboardViewControllerClickTests` grid cases (keep leftRight click coverage).
- [ ] **Step 7: Full build + full test suite.** `xcodebuild ... build` then `xcodebuild ... test 2>&1 | tail -30` → BUILD SUCCEEDED; ALL tests pass.
- [ ] **Step 8: Run the app — final smoke test.** 3 columns, flat header, native tree + changes list, center overlay, both collapse toggles, new-task creator. No console errors.
- [ ] **Step 9: Commit.**
```bash
git add -A
git commit -m "refactor(ui): remove dead grid/topSmall/topLarge layout modes"
```

---

## Self-Review Notes (for the executor)

- **Spec coverage (Revision 2):** flat header (T1); center overlay (T2); native file tree (T3); file content viewer (T4); changes list + side-panel rewrite + overlay wiring (T5); dead-mode cleanup (T6).
- **Verify-before-claiming:** T2/T3/T4/T5 end in passing focused tests; T1/T5/T6 also require a build (+ manual run). Don't check a box until the command output confirms it.
- **Shared test helper:** `descendantViews()` moves to `Tests/TestViewHelpers.swift` in Task 2 — later tasks reference it, not their own copy (resolves the Phase 1 collision risk).
- **Line numbers may have drifted** — find symbols by name.
