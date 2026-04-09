# Worktree Auto-Discovery via Periodic Polling

## Problem

When Claude Code (or any tool) creates a new git worktree, the AMUX dashboard does not show it until the app is restarted. The existing webhook-based discovery (`WorktreeCreate` hook -> `handleNewWorktreeFromHook`) is unreliable — the webhook payload may lack `worktree_path`, or the agent may not change its cwd, preventing detection.

## Solution

Extend the existing `refreshBranches` timer (5s interval) to also detect new worktrees. When a new worktree appears, integrate it into the dashboard and attempt a pane transfer if a pending webhook transfer exists.

## Design

### Extract shared integration method

The new-worktree integration logic currently lives in `handleNewWorktreeFromHook` (TabCoordinator.swift lines 358-395). Extract it into a reusable method:

```swift
func integrateNewWorktrees(repoRoot: String, allDiscovered: [WorktreeInfo], newWorktrees: [WorktreeInfo])
```

This method:
1. Updates `WorkspaceManager` tab worktrees
2. For each new worktree, checks `PendingTransferTracker` for a matching transfer
3. If match found: calls `performPaneTransfer` (terminal moves to new card)
4. If no match: creates fresh tree, registers in `AgentHead`
5. Records `worktreeStartedAt` in config
6. Refreshes dashboard via `updateAgents(buildAgentDisplayInfos())`
7. Updates `statusPublisher.updateSurfaces(...)`

### Extend `refreshBranches`

After discovering fresh worktrees for a repo, compare against known paths in `allWorktrees`. If new paths exist, call `integrateNewWorktrees`.

```
refreshBranches (5s timer)
  -> WorktreeDiscovery.discoverAsync per repo
  -> detect branch changes (existing behavior)
  -> detect new worktree paths (new behavior)
  -> if new paths: integrateNewWorktrees(repoRoot, allDiscovered, newWorktrees)
```

### Webhook flow unchanged

- `WebhookStatusProvider` continues handling `WorktreeCreate` events
- `onWorktreeCreateReceived` still records in `PendingTransferTracker`
- `onNewWorktreeDetected` still triggers immediate discovery (faster than 5s poll)
- The 5s poll acts as a guaranteed fallback when webhooks fail

### Pane transfer interaction

When Claude Code creates a worktree:
1. **Webhook fires first** (if working): `PendingTransferTracker.record(sourceWorktreePath, worktreeName, sessionId)`
2. **Poll detects new worktree** (within 5s): `integrateNewWorktrees` checks tracker, finds match, calls `performPaneTransfer`
3. Terminal moves from source worktree card to new worktree card
4. Source worktree gets a fresh terminal

If webhooks don't fire, step 1 is skipped — the new worktree still gets a card (step 2), just without the pane transfer.

## Scope

- New worktree detection and integration
- Pane transfer when webhook data is available
- No deleted-worktree detection (matches current behavior)

## Files changed

- `Sources/App/TabCoordinator.swift` — extract `integrateNewWorktrees`, extend `refreshBranches`
