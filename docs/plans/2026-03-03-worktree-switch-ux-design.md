# Worktree switch UX: smooth like Ghostty/Zed (design)

## Context

- **Backend**: tmux (control mode). When switching worktree within the same session we reuse the existing `-CC` connection and call `switch_window`; when switching repo/session we tear down and create a new runtime.
- **Current behavior**:
  1. **Sidebar/worktree click** → `process_pending_worktree_selection`: sets `worktree_switch_loading = Some(idx)`, calls `detach_ui_from_runtime()` (drops `terminal_area_entity`, clears `terminal_buffers`), `cx.notify()`, then async `switch_window` → on completion `attach_runtime` and clear loading.
  2. **Result**: User sees a full-screen **“Connecting to worktree...”** for the duration of the async `switch_window` (often 100–300ms+), then the new terminal appears.
  3. **Additional flash**: After `detach_ui_from_runtime()` we have `terminal_area_entity = None`. The render branch is: if `worktree_switch_loading.is_some()` → show “Connecting to worktree...”; else if `term_entity` → terminal; else → **fallback** `SplitPaneContainer` with empty buffers. So we can also get a brief frame of the fallback (empty panes with “—” or similar) if loading is cleared before the new entity is set, or on other code paths that detach without setting loading (e.g. sync `switch_to_worktree`).
- **“connect to window”**: Not found in pmux UI strings. Likely either (1) our “Connecting to worktree...” or (2) tmux control-mode/attach output briefly visible in the terminal. Design below assumes we control (1); (2) would need tmux/client handling or filtering.

## User goal

- Switching worktree (same session) should feel **as smooth as Ghostty/Zed switching tab names**: no full-screen loading, no obvious “connecting” or empty-state flashes.

## Design options

### Option A — Keep last frame, hide loading (recommended)

- **Idea**: When reusing the same tmux session, **do not show** “Connecting to worktree...” and **do not tear down the terminal area** until the new session is ready.
- **Behavior**:
  - On worktree switch (same session): leave current terminal UI and buffers as-is; start async `switch_window`; only when it finishes run `detach_ui_from_runtime()` (or equivalent) and then immediately `attach_runtime()` in the same update, so the UI never shows loading or empty fallback.
- **Implementation sketch**:
  - In `process_pending_worktree_selection` (same-session path): **do not** set `worktree_switch_loading`, **do not** call `detach_ui_from_runtime()` before spawning the async task. In the async completion: call `detach_ui_from_runtime()` then `attach_runtime()` in one `entity.update()` and `cx.notify()`.
  - Optional: keep showing the **previous** worktree’s terminal content until the new one is attached (no loading overlay). If `switch_window` is slow, user briefly sees old worktree—acceptable and less jarring than a spinner.
- **Pros**: Minimal code change; removes loading and avoids the fallback flash for the async path. Matches “tab name switch” feel.
- **Cons**: For a short period the visible content can be the previous worktree (same as many tab switches in other apps).

### Option B — Double-buffer / single surface

- **Idea**: Keep one `TerminalAreaEntity` and only swap the data source (pane target / stream) when switching worktree, instead of destroy + recreate.
- **Behavior**: No teardown of the terminal area; we only change which pane’s output is shown and update title/sidebar. No “Connecting to worktree...” and no empty state.
- **Pros**: Smoothest possible; no flash.
- **Cons**: Larger refactor: terminal area would need to support “replace stream for same entity” and possibly clear or swap the VTE buffer when the pane target changes.

### Option C — Shorten perceived delay only

- **Idea**: Keep current flow but make the loading state less visible (e.g. very small or inline “Switching…” near the tab, or a short delay before showing “Connecting to worktree...” so fast switches don’t show it).
- **Pros**: Small change.
- **Cons**: Does not fix the underlying teardown/reattach; user may still see a flash or loading when switch is slow.

## Recommendation

- **Option A** for immediate improvement with minimal risk: same-session worktree switch should not set `worktree_switch_loading` and should not detach before the async switch; detach + attach only in the async completion so we never paint “Connecting to worktree...” or the empty fallback for that path.
- **Sync path** (`switch_to_worktree` when same session): currently it does detach → sync `switch_window` → attach. That can still produce one frame with `term_entity = None` (fallback with “—” ). To fix that too, we could either (1) run the sync switch in a blocking way but only call `detach_ui_from_runtime()` right before `attach_runtime()` (so we only have one “empty” frame at the same time as we set the new entity), or (2) move this path to the same “async then detach+attach once” pattern as Option A.
- **“connect to window”**: If it appears in the **terminal content** (tmux’s own message), we can add a note in the implementation plan to filter or hide that line in the control-mode parser or in the initial capture (if it’s a one-line message from tmux).

## Success criteria

- When switching worktree within the same tmux session, the user does not see a full-screen “Connecting to worktree...” state.
- No visible flash of the empty fallback (SplitPaneContainer with “—”).
- Tab/sidebar selection updates immediately; terminal content may lag by one switch duration (old content → new content) and that is acceptable.
- Optional: if “connect to window” is tmux output, document or filter it so it doesn’t appear in the terminal pane.

## Next step

- Get your approval on this direction (Option A + sync-path alignment). Then an implementation plan (in `docs/plans/`) can be written with concrete steps and file edits.
