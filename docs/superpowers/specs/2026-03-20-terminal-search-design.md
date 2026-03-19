# Terminal Search Design

## Overview

Add Cmd+F search to the Repo view's terminal, using Ghostty's native search API. Shows a bottom search bar with real-time matching, match count, and prev/next navigation.

## Decision

- Use Ghostty's built-in search: `GHOSTTY_ACTION_START_SEARCH`, `END_SEARCH`, `SEARCH_TOTAL`, `SEARCH_SELECTED`
- Ghostty handles highlighting and scrolling internally
- We provide the search bar UI and wire it to the Ghostty API

## Architecture

```
SearchBarView (UI)  ←→  TerminalSurface (Ghostty API)
     ↑                        ↓
  Cmd+F / Esc          START_SEARCH / END_SEARCH
  Enter / Shift+Enter  SEARCH_TOTAL / SEARCH_SELECTED callbacks
```

## SearchBarView

**File:** `Sources/UI/Repo/SearchBarView.swift`

A 32px bar at the bottom of the terminal area, hidden by default.

```
┌──────────────────────────────────────────────────────┐
│ 🔍 [search field............]  3/15  [↑] [↓]  [✕]  │
└──────────────────────────────────────────────────────┘
```

**Controls:**
- Search text field — real-time search on each keystroke
- Match count label — "3/15" (selected/total), "No matches" when 0
- Previous button (↑) — Shift+Enter or click
- Next button (↓) — Enter or click
- Close button (✕) — Esc or click

**Accessibility identifiers:**
- `search.bar` (role: `.group`)
- `search.field`
- `search.matchCount`
- `search.prevButton`
- `search.nextButton`
- `search.closeButton`

## TerminalSurface Search API

**File:** `Sources/Terminal/TerminalSurface.swift`

Add methods to TerminalSurface:

```swift
func startSearch(_ query: String)   // ghostty_surface_key binding or input
func endSearch()
func searchNext()                   // navigate to next match
func searchPrev()                   // navigate to previous match
```

**Ghostty interaction:**
- Search is triggered by sending the search keybinding to Ghostty, or by directly invoking the action via the C API
- Ghostty calls back with `GHOSTTY_ACTION_SEARCH_TOTAL` (total matches) and `GHOSTTY_ACTION_SEARCH_SELECTED` (current index)
- GhosttyBridge.handleAction needs to handle these callbacks and notify the SearchBarView

## GhosttyBridge Enhancement

**File:** `Sources/Terminal/GhosttyBridge.swift`

Handle search callbacks in `handleAction`:

```swift
case GHOSTTY_ACTION_START_SEARCH:
    // Ghostty asks us to show search UI with pre-filled needle
    return true
case GHOSTTY_ACTION_SEARCH_TOTAL:
    let total = action.value.search_total.total
    // Post notification with total count
    return true
case GHOSTTY_ACTION_SEARCH_SELECTED:
    let selected = action.value.search_selected.selected
    // Post notification with selected index
    return true
```

Use `NotificationCenter` to relay search state from the static C callback to the SearchBarView.

## Integration

### RepoViewController

- Cmd+F → show SearchBarView at bottom of terminal area
- Esc (when search bar focused) → hide SearchBarView, call `endSearch()`
- SearchBarView sits above the terminal, doesn't overlap

### MainWindowController

- Add Cmd+F menu item → `@objc func showTerminalSearch()`
- Only active when in Repo view

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| Cmd+F | Open search bar, focus search field |
| Esc | Close search bar |
| Enter | Next match |
| Shift+Enter | Previous match |
| Cmd+G | Next match (standard macOS) |
| Cmd+Shift+G | Previous match (standard macOS) |

## Testing

### Unit Tests
- SearchBarView state updates (match count display formatting)

### UI Tests
- `testCmdFOpensSearchBar` — Cmd+F in repo view shows search bar
- `testEscClosesSearchBar` — Esc hides search bar
- `testSearchFieldExists` — search field is accessible and typeable
