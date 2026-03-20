# pmux Zoom-Style UI Redesign

## Overview

Redesign pmux's UI to match Zoom's visual language: near-black background, minimal tile gaps, rounded-rect "arc" groupings in the title bar, and terminal tiles replacing video tiles. All four dashboard layout modes are retained but unified under the Zoom aesthetic.

## Design Tokens

| Token | Value | Usage |
|-------|-------|-------|
| Background | `#0b0b0b` | Window/content area |
| Tile bg | `#111111` | Terminal tile background |
| Surface | `#1a1a1a` | Card backgrounds, panel backgrounds, bottom bars |
| Arc block bg (hover) | `#232323` | TitleBar rounded-rect blocks when window hovered |
| Arc block bg (inactive) | `#1a1a1a` | TitleBar blocks when window not hovered |
| Border subtle | `#222222` | Tile separators, panel borders |
| Text primary | `#eeeeee` | Active labels |
| Text secondary | `#888888` | Inactive labels |
| Text dim | `#555555` | Meta info, timestamps |
| Accent | `#4f8cff` | Dashboard tab, selected states, AI send button |
| Running | `#33c17b` | Status dot, selected tab border |
| Waiting | `#3b82f6` | Status dot |
| Error | `#ff453a` | Status dot, error notifications |
| Idle | `#9ca3af` | Status dot |
| Tile gap | `3px` | Between grid tiles |
| Tile corner radius | `4px` | Terminal tiles |
| Arc block radius | `10px` | TitleBar rounded-rect blocks |
| Tab radius | `7px` | Project tab pills |
| Dashboard tab radius | `14px` | Dashboard pill (distinct) |

## TitleBar

### Structure

Two rounded-rect "arc blocks" separated by an 8px gap:

```
┌─ Left Arc Block (flex:1) ──────────────────────────────────┐
│ 🔴🟡🟢 │ ⊞ Dashboard │ ●project-1 × │ ●project-2 × │ + │
└────────────────────────────────────────────────────────────┘
                                                    8px gap
                                        ┌─ Right Arc Block ─┐
                                        │ ⊞  🔔  ✨  ◐      │
                                        └───────────────────┘
```

### Left Arc Block

- Traffic lights (system standard, colored on hover, grey on inactive)
- Separator `1px #3a3a3a`
- Dashboard tab: pill shape (radius 14px), blue tint bg `#4f8cff22`, blue grid icon + "Dashboard" text. When not selected: grey icon + grey text, no tint bg.
- Separator `1px #3a3a3a`
- Project tabs: status dot (6px) + name + close "×". Scrollable if overflow.

### Right Arc Block

Four icon buttons (30×30px, radius 7px), pure SVG stroke icons:
1. **View switcher** (grid icon) — opens layout dropdown
2. **Notifications** (bell icon) — opens notification slide panel. Red dot badge when unread.
3. **AI** (sparkles icon) — opens AI slide panel
4. **Theme** (half-circle icon) — toggles light/dark/system

### Window Hover States

- **Window hovered**: Arc blocks bg `#232323`, traffic lights colored, text brighter
- **Window not hovered**: Arc blocks bg `#1a1a1a`, traffic lights grey (`#555`), text dimmer

### Project Tab States

| State | Background | Border | Text color |
|-------|-----------|--------|------------|
| Selected | `#1a2a1a` | `1.5px #33c17b` | `#eeeeee` |
| Hover | `#222222` | `1.5px rgba(255,255,255,0.08)` | `#cccccc` |
| Default | transparent | transparent | `#888888` |

### Dashboard Tab States

| State | Background | Icon/Text color |
|-------|-----------|-----------------|
| Selected (on Dashboard) | `#4f8cff22` | `#4f8cff` |
| Not selected (on Project) | transparent | `#888` / `#666` |

## StatusBar

**Removed.** No bottom bar. Terminal content fills to window edge.

## Dashboard Layouts

All four layouts retained, unified Zoom style. Background `#0b0b0b`, tile gap `3px`, tile radius `4px`.

### Grid View

- Responsive columns: auto-fit based on tile count (1×1, 2×2, 3×3, etc.)
- Tiles maintain 16:9 aspect ratio
- Each tile: terminal content area (top, flex) + independent bottom bar (fixed height)

### Left-Right (Speaker View)

- Left: main terminal (flex:1)
- Right: sidebar column (240px) of 16:9 mini cards, vertically scrollable
- Gap: 3px

### Top-Small

- Top: horizontal scrolling row of 16:9 mini cards (240×135px each), 3px gap
- Bottom: main terminal (flex:1)

### Top-Large

- Top: main terminal (flex:1)
- Bottom: horizontal scrolling row of 16:9 mini cards, 3px gap

## Terminal Tile (Grid)

### Structure

```
┌─────────────────────────────┐
│  Terminal content            │  ← flex:1, monospace, status-colored text
│  (Ghostty surface)           │
│                              │
├─────────────────────────────┤
│ ● branch-name     Running   │  ← independent bottom bar, bg #1a1a1a
└─────────────────────────────┘
```

- Corner radius: 4px
- Bottom bar: `bg #1a1a1a`, border-top `1px #222`, padding 5px 8px
- Bottom bar content: status dot (6px) + branch name (white, 9px, weight 500) + status text (right-aligned, dim)

### Click Behavior

- Single click → switch to Speaker View (current tile becomes main)
- In Speaker View, click sidebar card → swap to main
- In Speaker View, click `>` icon on main tile → enter Project Detail tab

## Mini Card (Speaker View sidebar / Top layouts)

### Dimensions

- Fixed 16:9 aspect ratio (240×135px base, scales with sidebar width)

### Fields (top to bottom)

1. Status dot + `project-name / branch-name` (first line)
2. `⏱ duration · last-update-time` + status text right-aligned (second line)
3. Last message, monospace, 2-3 line clamp (remaining space)

### States

| State | Background | Border | Branch text | Message text |
|-------|-----------|--------|-------------|-------------|
| Selected | `#1a1a1a` | `1.5px #33c17b` | `#ddd` | `#666` |
| Hover | `#222222` | `1.5px rgba(255,255,255,0.08)` | `#fff` | `#888` |
| Default | `#1a1a1a` | transparent | `#ddd` | `#666` |

## Main Tile (Speaker View focus)

Same structure as grid tile but larger. Additional element:

- Bottom bar right side: `>` arrow icon (22×22px, radius 5px) linking to Project Detail
- Icon states: default `bg #ffffff0a stroke #999`, hover `bg #ffffff18 stroke #fff`

## Project Detail

Accessed by clicking `>` icon on main tile, or clicking a project tab.

### Structure

- TitleBar: Dashboard tab dimmed (not selected), project tab has green border (selected)
- Left sidebar (260px): thread list
- Right: immersive full-height terminal + minimal bottom bar
- Responsive: sidebar stacks above terminal when window ≤ 900px

### Sidebar

Header: "Threads" label + count + "+" button (24×24, radius 6px, `bg #ffffff0a`)

"+" button click → New Thread dialog (modal).

### Thread Row States

| State | Background | Border |
|-------|-----------|--------|
| Selected | `#1a2a1a` | `1px rgba(51,193,123,0.25)` |
| Hover | `rgba(255,255,255,0.03)` | `1px rgba(255,255,255,0.04)` |
| Default | transparent | transparent |

### Thread Row Content

- Status dot (7px) + branch name (11px, weight 600 selected / 500 default)
- Last message below (9px, monospace, 2-line clamp)

## New Thread Dialog

Modal overlay with `rgba(0,0,0,0.6)` backdrop.

- Width: 400px, bg `#1a1a1a`, radius 10px, border `1px #333`
- Header: "New Thread" title + subtitle
- Body: branch name input + base branch selector
- Footer: Cancel (ghost button `bg #ffffff08`) + Create (primary `bg #4f8cff`)

## View Switcher Dropdown

Triggered by clicking the grid icon in the right arc block.

- Width: 180px, bg `#1a1a1a`, radius 8px, border `1px #333`, shadow
- 4 items: Grid, Left-Right, Top-Small, Top-Large
- Each item: layout thumbnail SVG icon + label
- Selected item: blue icon + blue text + checkmark
- Hover: `bg #ffffff05`, text brightened

## Notification Slide Panel

Triggered by bell icon. Slides from right edge.

- Width: 320px, bg `#1a1a1a`, border-left `1px #222`, shadow `-8px 0 24px rgba(0,0,0,0.3)`
- Backdrop: `rgba(0,0,0,0.4)` overlay on content area, click to dismiss
- Header: bell icon + "Notifications" + count + close button
- Items: status dot + branch name + timestamp + status change description
- Error items: tinted red background `#1f1515`, red-tinted border

## AI Slide Panel

Triggered by sparkles icon. Slides from right edge.

- Width: 340px, same panel styling as notifications
- Header: sparkles icon + "AI Assistant" + close button
- Body: chat bubbles (assistant `bg #222`, user `bg #263554`), scrollable
- Footer: text input (bg `#111`, border `1px #333`) + send button (bg `#4f8cff`, 28×28, radius 6px)

## Toolbar Button Spec

All four buttons in right arc block:

- Size: 30×30px, radius 7px
- Icon: 16×16px SVG, stroke only, 1.2px weight
- Default: stroke `#888`, no background
- Hover: stroke `#fff`, background `rgba(255,255,255,0.07)`
- Notification badge: 8px red dot (`#ff453a`) with 1.5px border matching arc block bg, positioned top-right
