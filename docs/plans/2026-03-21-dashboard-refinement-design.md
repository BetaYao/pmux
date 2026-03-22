# Dashboard Refinement Design (Option A)

## Context

After native titlebar migration, dashboard cards in some layouts visually overlap the titlebar area. Typography in multiple card components is undersized (8-9pt in places), and several card/header interactions look more custom/complex than standard macOS UI.

## Goals

1. All four dashboard layouts start below titlebar with a small, consistent gap.
2. Card typography follows a macOS-style baseline using SF at 12/13pt (minimum auxiliary 11pt).
3. Simplify card/panel UI to reduce custom complexity and align better with native macOS patterns.

## Non-Goals

- No terminal rendering changes.
- No data-model changes to agent/worktree status.
- No redesign of dashboard information architecture beyond simplification.

## Approved Approach (A)

### 1) Unified top safe inset for all layouts

- Add a shared top spacing token in `DashboardViewController` (e.g. `topContentInset = 8`).
- Apply this as the baseline for grid, left-right, top-small, and top-large containers so card content always starts below titlebar.
- Remove per-layout conflicting top offsets where needed to avoid stacked offsets and overlap.

### 2) macOS typography baseline across cards

- Standardize text hierarchy:
  - Primary: 13pt SF Semibold
  - Body: 12pt SF Regular/Medium
  - Secondary/meta: 11pt SF Regular
- Apply to:
  - `AgentCardView` bottom bar labels
  - `MiniCardView` line1/line2/message typography
  - `FocusPanelView` header labels
- Eliminate 8-9pt text from card surfaces.

### 3) Strong simplification toward native feel

- Remove duplicated actions in focus header (single entry affordance for project navigation).
- Reduce over-custom hover/decorative states where they do not add clear value.
- Replace scattered hardcoded grayscale text colors with semantic tokens to improve consistency and light/dark behavior.
- Keep interaction model intact (no feature removal), only simplify visual/interaction redundancy.

## UI Principles for this pass

- Prefer consistency over novelty.
- Prefer one clear action over multiple equivalent affordances.
- Keep hierarchy readable at a glance with fewer visual competing signals.
- Match AppKit-style restraint in typography and contrast.

## Validation Plan

1. Functional checks (UI tests):
   - layout switch tests still pass for top-small/top-large
   - project-tab related regressions remain green
2. Visual/manual checks:
   - each layout has visible gap under titlebar (no overlap)
   - no card text appears too small to read comfortably
   - focus header has a single clear project-entry control
3. Safety checks:
   - `GridLayoutTests` pass
   - no accessibility identifier regressions for existing UI tests

## Risks and Mitigations

- Risk: changing text sizes increases truncation in narrow cards.
  - Mitigation: preserve truncation priorities and adjust label compression resistance where needed.
- Risk: top inset changes create uneven spacing between layouts.
  - Mitigation: centralize top inset and reuse across all layout container constraints.
- Risk: simplifying hover/controls may break test hooks.
  - Mitigation: preserve identifiers and verify focused regression tests.
