# Grid Card Click Interactions

**Date:** 2026-04-02
**Status:** Approved

## Problem

In grid mode, single-clicking a card sets selection (border highlight) but doesn't embed the live terminal. Clicking an already-selected multi-pane card does nothing. Users want to cycle through panes by clicking.

## Design

### Interaction Flow

In `DashboardViewController.agentCardClicked()` for grid mode:

1. **Card NOT selected** — Select it, set `selectedPaneIndex = 0`, embed the first pane's terminal surface into the card's `terminalContainer`.
2. **Card already selected + single pane** — No-op.
3. **Card already selected + multi-pane** — Increment `selectedPaneIndex` (wrap to 0 after last pane), swap the terminal surface with a slide animation.

### Animation

Reuse the `CATransition` pattern from focus mode pane navigation (`DashboardViewController:799-805`):

- Type: `.push`, subtype: `.fromRight`
- Duration: 0.25s
- Timing: ease-in-out
- Applied to `container.cardView.terminalContainer.layer`

### Surface Embedding

When selecting a card, call `embedSurface(agent, in: container.cardView.terminalContainer)` to show the live terminal on the selected grid card. The existing `embedSurface` method already handles pane selection via `selectedPaneIndex`.

When deselecting (selecting a different card), detach the terminal from the previous card so it returns to showing the message overlay.

### Scope

- Only grid mode — focus/spotlight layouts already have their own pane navigation.
- No new UI elements (no nav buttons in grid cards) — clicking is the only interaction.
- Double-click behavior unchanged (enters project).
