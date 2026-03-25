# Stacked Card Effect for Multi-Pane Worktrees

**Date:** 2026-03-25
**Status:** Approved

## Overview

When a worktree has multiple split panes, the dashboard grid card for that worktree should visually convey "there are multiple panes here" using a physical card-stack metaphor: ghost cards layered behind the main card, offset to the bottom-right.

## Scope

- **In scope:** Grid layout (`AgentCardView` in `DashboardViewController.rebuildGrid`)
- **Out of scope:** Focus layouts (`MiniCardView` in leftRight/topSmall/topLarge sidebars) — not worth the complexity for small thumbnail cards

## Visual Design

### Stacking rules

| paneCount | Ghost cards | Offsets (x, y) |
|-----------|-------------|----------------|
| 1         | 0 (no change) | — |
| 2         | 1 ghost | (6, 6)px |
| 3+        | 2 ghosts | (6, 6)px and (12, 12)px |

### Ghost card appearance

- **Size:** Same width and height as the main card
- **Position:** Main card sits at (0, 0) in the container; ghost cards offset right and down
- **Background:** Slightly darker than main card (`#1a1a2e` for ghost-1, `#161625` for ghost-2 in dark mode — resolved via `SemanticColors` at draw time)
- **Border:** Same corner radius as main card (4px); border color follows `SemanticColors.line` but slightly dimmer
- **Content:** None — purely decorative background + border layers
- **No transparency/opacity changes** — color difference alone creates the depth cue

## Architecture

### New component: `StackedCardContainerView`

A lightweight `NSView` wrapper with no background, no border, and `clipsToBounds = false`. It holds the ghost layers and the `AgentCardView`:

```
StackedCardContainerView  (frame = cardFrame + 12pt padding bottom-right)
  ├── ghostView2  (NSView, frame at offset (12, 12), z-order: bottom)
  ├── ghostView1  (NSView, frame at offset (6, 6), z-order: middle)
  └── AgentCardView  (frame at (0, 0), same size as ghost views)
```

`StackedCardContainerView` exposes:
- `configure(paneCount:)` — adds/removes ghost views as needed
- A `cardView: AgentCardView` property for the grid to wire up delegate and configure

### Grid layout changes

`DashboardViewController.rebuildGrid` currently creates `AgentCardView` directly. It will instead create `StackedCardContainerView`, which internally owns the `AgentCardView`.

Frame calculation in `layoutGridFrames`: the `StackedCardContainerView` gets a frame that is 12pt wider and 12pt taller than the logical card slot (the extra space absorbs the ghost offset without overlapping adjacent cards). `AgentCardView` inside occupies the top-left `(cardWidth × cardHeight)` of that container.

### Color resolution

Ghost views use `wantsLayer = true` and override `updateLayer()` to resolve colors against the current appearance (same pattern as `AgentCardView.applyColors()`). `SemanticColors` is extended with two new entries:

- `SemanticColors.tileGhost1Bg` — one step darker than `tileBg`
- `SemanticColors.tileGhost2Bg` — two steps darker than `tileBg`
- `SemanticColors.tileGhostBorder` — slightly dimmer than `line`

### No changes to

- `AgentCardView` internals — it remains self-contained
- `AgentDisplayInfo` — `paneCount` field already exists
- `TerminalSurface` lifecycle — surfaces are embedded into `AgentCardView.terminalContainer` as before
- Focus layouts — unchanged

## Testing

- Unit test: `StackedCardContainerView` with paneCount 1/2/3/5 produces 0/1/2/2 ghost views
- Visual: build and verify in grid layout with a worktree that has 2 and 3 panes open
- Regression: existing single-pane cards look unchanged; grid drag-reorder still works
