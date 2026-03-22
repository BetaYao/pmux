# Sidebar Diff Entry Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Expose a visible `Diff` entry beside `New Thread`, enable it only after explicit sidebar selection, and remove the temporary `⌘D` shortcut.

**Architecture:** Keep diff presentation in `MainWindowController`, but route the new button event through existing repo/sidebar delegate layers so UI ownership stays local to sidebar while presentation remains centralized. Preserve current diff overlay implementation and only change entry points and enablement state. Keep menu item for discoverability but remove key equivalent.

**Tech Stack:** Swift 5.10, AppKit (`NSButton`, `NSTableView`, delegate patterns), XCTest/xcodebuild.

---

### Task 1: Add failing UI-state tests for sidebar diff button

**Files:**
- Modify: `Tests/GridLayoutTests.swift`
- Test: `Tests/GridLayoutTests.swift`

**Step 1: Write the failing test**

Add tests that assert:
1. Sidebar includes a `sidebar.showDiff` button.
2. Button exists and is disabled by default.

**Step 2: Run test to verify it fails**

Run: `xcodebuild -project pmux.xcodeproj -scheme pmuxTests -configuration Debug test -only-testing:pmuxTests/GridLayoutTests/testSidebar_DiffButtonExistsAndDefaultDisabled`
Expected: FAIL because button identifier or state is missing.

**Step 3: Write minimal implementation**

Implement only enough sidebar UI to create the button with default disabled state.

**Step 4: Run test to verify it passes**

Run same command.
Expected: PASS.

**Step 5: Commit**

```bash
git add Tests/GridLayoutTests.swift Sources/UI/Repo/SidebarViewController.swift
git commit -m "test: cover sidebar diff entry default state"
```

### Task 2: Wire sidebar -> repo -> window diff action path

**Files:**
- Modify: `Sources/UI/Repo/SidebarViewController.swift`
- Modify: `Sources/UI/Repo/RepoViewController.swift`
- Modify: `Sources/App/MainWindowController.swift`

**Step 1: Write the failing test**

Add/adjust tests that verify delegate plumbing for diff requests from sidebar and that action is available after selection change.

**Step 2: Run test to verify it fails**

Run targeted unit tests for sidebar/repo integration.
Expected: FAIL due missing delegate methods.

**Step 3: Write minimal implementation**

1. Add `sidebarDidRequestDiffOnSelectedWorktree(_:)` to `SidebarDelegate`.
2. Add `repoView(_:didRequestShowDiffForWorktreePath:)` to `RepoViewDelegate`.
3. In `RepoViewController`, forward selected/active worktree path to delegate.
4. In `MainWindowController`, present `DiffOverlayViewController` for explicit path.

**Step 4: Run tests to verify pass**

Run targeted tests and build.
Expected: PASS.

**Step 5: Commit**

```bash
git add Sources/UI/Repo/SidebarViewController.swift Sources/UI/Repo/RepoViewController.swift Sources/App/MainWindowController.swift
git commit -m "feat: add sidebar diff entry delegate flow"
```

### Task 3: Remove `⌘D` shortcut and update help text

**Files:**
- Modify: `Sources/App/MainWindowController.swift`

**Step 1: Write the failing test**

If menu/shortcut tests exist, add one for `Show Diff...` with no key equivalent and no `⌘D` in help text.

**Step 2: Run test to verify it fails**

Run targeted menu test.
Expected: FAIL because `keyEquivalent` is still `d` and shortcut text still includes `⌘D`.

**Step 3: Write minimal implementation**

1. Set `Show Diff...` menu item `keyEquivalent` to empty string.
2. Remove `⌘D  Show Diff` line from keyboard-shortcuts alert text.

**Step 4: Run tests/build to verify pass**

Run:
- `xcodebuild -project pmux.xcodeproj -scheme pmux -configuration Debug build`
- targeted tests for menu/sidebar if present.

Expected: PASS.

**Step 5: Commit**

```bash
git add Sources/App/MainWindowController.swift
git commit -m "refactor: remove temporary diff keyboard shortcut"
```

### Task 4: Final verification

**Files:**
- Verify only (no file edits)

**Step 1: Run focused verification**

Run:
- `xcodebuild -project pmux.xcodeproj -scheme pmux -configuration Debug build`
- `xcodebuild -project pmux.xcodeproj -scheme pmuxTests -configuration Debug test -only-testing:pmuxTests/GridLayoutTests`

Expected: build succeeds; grid/sidebar-related tests pass.

**Step 2: Manual smoke check**

1. Open repo tab.
2. Confirm `Diff` button is disabled initially.
3. Click a left sidebar item.
4. Confirm `Diff` becomes enabled.
5. Click `Diff` and confirm `repo.diffOverlay` appears.
6. Confirm `⌘D` no longer triggers diff.

**Step 3: Commit final polish (if any)**

```bash
git add -A
git commit -m "test: verify sidebar diff entry behavior"
```
