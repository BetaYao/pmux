# Top Bar Usage Capsule Design

## Goal

Replace the top bar long capsule's notification-focused content with a rotating utility display:

1. Shortcut tips.
2. Claude Code usage.
3. Codex usage.

Usage frames follow the compact reference style: a short label, a horizontal usage bar, a percentage/reset indicator, and today's token usage when there is enough room. The capsule remains non-blocking and shows placeholders instead of disrupting the title bar when local usage data is unavailable.

## Non-Goals

- Do not remove notification collection, persistence, or the notification panel.
- Do not use private Claude OAuth usage APIs.
- Do not replace the user's existing Claude HUD behavior.
- Do not make the top bar a detailed usage dashboard.

## User Experience

The primary capsule rotates through these frames on the existing timer:

- Shortcut frame: keeps the current shortcut-tip behavior and copy style.
- Claude frame: shows `Claude`, 5-hour remaining, weekly remaining, and the 5-hour reset countdown when available.
- Codex frame: shows `Codex`, a usage bar, remaining/used percentage and reset window when available, plus today's token total in compact form.

For the usage bar, filled width represents used percentage. The text communicates remaining amount, for example `剩余 72%`, while the bar itself visually matches the reference's usage-progress style. For Claude, the bar represents the 5-hour window and the compact text is `5h 剩余 90% · 周剩余 87% · 重置 3h 11m`. For Codex, if reset data is available, show a short form such as `2h 19m`; if today's token count is available, show a compact count such as `Today 5.7M`.

When space is tight, the priority order is:

1. Provider label.
2. Usage bar.
3. Remaining or used percentage.
4. Reset countdown.
5. Today's token count.

If a field is unavailable, use `--` or omit the secondary detail rather than resizing the capsule or blocking rendering.

## Architecture

Add a small usage-summary layer that is separate from `TitleBarView`:

- `UsageSnapshot`: provider, current rate-limit window, reset time, today's token count, freshness, and availability state.
- `UsageSummaryProvider`: async interface that returns latest Claude and Codex snapshots.
- `UsageSummaryStore`: caches the last successful snapshots and refreshes them on a low-frequency background timer.
- `UsageSummaryFormatter`: converts snapshots into compact title-bar presentation data.

`TitleBarView` receives already-formatted capsule frames. It does not read files, talk to CLIs, parse JSONL, or query SQLite directly.

`MainWindowController` owns wiring between the store and the title bar, similar to how it currently owns title bar state updates.

## Data Sources

### Claude Code

For plan remaining, add a statusline wrapper script:

- Reads Claude Code statusline JSON from stdin.
- Extracts `rate_limits` when present.
- Writes a small JSON cache in an amux-owned cache path.
- Forwards the same stdin payload to the user's current Claude HUD command so existing HUD behavior continues.

The app reads only the cache written by the wrapper. It uses `rate_limits.five_hour` for the 5-hour bar/remaining/reset and `rate_limits.seven_day` for weekly remaining. If the wrapper has not been installed, has not seen `rate_limits`, or the cache is stale, Claude remaining displays `--`.

For today's token usage, scan Claude transcript JSONL files under `~/.claude/projects`. Count `message.usage` fields for messages in the current local day and dedupe repeated request/message identifiers where present. Include:

- `input_tokens`
- `cache_creation_input_tokens`
- `cache_read_input_tokens`
- `output_tokens`

### Codex

For plan remaining, use the local Codex app-server rate-limit data when available. The supported shape includes a snapshot with plan type, credits, primary/secondary windows, `usedPercent`, `resetsAt`, and optional per-limit buckets.

For today's token usage, query the local Codex state database and sum usage for sessions updated in the current local day. The first implementation uses the local database's existing token aggregate; if later per-turn token data becomes stable, the provider can be swapped without changing the title bar.

## Error Handling

- Usage refresh runs off the main thread.
- Missing files, unreadable SQLite databases, malformed JSONL, stale caches, or unavailable CLIs produce unavailable fields, not UI errors.
- A stale but recent-enough cache may be displayed with its freshness timestamp. A clearly stale cache falls back to `--`.
- The title bar always has at least shortcut frames, even if usage providers fail.

## Testing

Add focused tests before production code:

- Format snapshots into compact title-bar strings and progress values.
- Parse Claude statusline `rate_limits` cache.
- Aggregate Claude JSONL usage with duplicate request/message identifiers.
- Aggregate Codex daily token totals from a fixture SQLite database or a narrow database adapter fixture.
- Verify unavailable/stale data formats as `--` without crashing.

UI behavior can be covered with small unit tests around frame construction and existing title bar rotation behavior, rather than screenshot tests for this first pass.

## Rollout

Implement in two steps:

1. Add the usage-summary model, formatters, providers, cache reader, and tests.
2. Connect the formatted frames into the primary capsule and add the Claude statusline wrapper installation path.

The wrapper installation is explicit: the app never silently overwrites `~/.claude/settings.json`. Installation records the existing statusline command, writes a wrapper command, and the wrapper invokes the recorded command after caching stdin. If the existing command cannot be parsed or preserved, the installer makes no settings change and Claude remaining stays unavailable as `--`.
