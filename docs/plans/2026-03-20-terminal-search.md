# Implementation Plan: Terminal Search

**Spec:** `docs/superpowers/specs/2026-03-20-terminal-search-design.md`

## Task 1: Add search methods to TerminalSurface

**File:** `Sources/Terminal/TerminalSurface.swift`

Add search methods that send key bindings to Ghostty to trigger search:

```swift
func startSearch(_ query: String)
func endSearch()
func searchNext()
func searchPrev()
```

Implementation: Use `ghostty_surface_key` to send Cmd+F (start search), then type the query as text input. For next/prev, send Enter/Shift+Enter.

## Task 2: Handle search callbacks in GhosttyBridge

**File:** `Sources/Terminal/GhosttyBridge.swift`

In `handleAction`, add cases for:
- `GHOSTTY_ACTION_START_SEARCH` — post `.ghosttySearchStarted` notification
- `GHOSTTY_ACTION_SEARCH_TOTAL` — post `.ghosttySearchTotal` with total count
- `GHOSTTY_ACTION_SEARCH_SELECTED` — post `.ghosttySearchSelected` with index

## Task 3: Create SearchBarView

**File:** `Sources/UI/Repo/SearchBarView.swift` (new)

32px NSView with:
- NSTextField (search field)
- NSTextField label (match count "3/15")
- NSButton ↑ (prev), ↓ (next), ✕ (close)
- SearchBarDelegate protocol: didChangeQuery, didClickNext, didClickPrev, didClickClose

Accessibility: `search.bar`, `search.field`, `search.matchCount`, etc.

## Task 4: Integrate SearchBarView into RepoViewController

**File:** `Sources/UI/Repo/RepoViewController.swift`

- Add `searchBar` property, hidden by default
- Layout: pin to bottom of terminal area
- `showSearch()` — unhide, focus search field
- `hideSearch()` — hide, call endSearch on surface
- Wire SearchBarDelegate to TerminalSurface search methods
- Listen for search total/selected notifications to update match count

## Task 5: Add Cmd+F menu item

**File:** `Sources/App/MainWindowController.swift`

Add to View menu:
```swift
let searchItem = NSMenuItem(title: "Find...", action: #selector(showTerminalSearch), keyEquivalent: "f")
```

`showTerminalSearch()` → forward to active RepoViewController.showSearch()

## Task 6: Write UI tests

**File:** `UITests/Tests/SearchTests.swift` (new)

- testCmdFOpensSearchBar
- testEscClosesSearchBar
- testSearchFieldExists

## Task 7: Compile and verify

## Execution Order

Task 1-2 (Ghostty API) → Task 3 (SearchBarView) → Task 4 (integration) → Task 5 (menu) → Task 6 (tests) → Task 7 (verify)
