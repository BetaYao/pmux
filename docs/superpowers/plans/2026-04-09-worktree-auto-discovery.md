# Worktree Auto-Discovery Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `refreshBranches` detect new worktrees so the dashboard auto-updates when Claude Code (or any tool) creates a worktree.

**Architecture:** Extract the new-worktree integration logic from `handleNewWorktreeFromHook` into a shared method `integrateNewWorktrees`. Call it from both the webhook path and the `refreshBranches` timer. The 5s poll guarantees detection regardless of webhook reliability.

**Tech Stack:** Swift 5.10, AppKit, XCTest

---

### Task 1: Extract `integrateNewWorktrees` from `handleNewWorktreeFromHook`

**Files:**
- Modify: `Sources/App/TabCoordinator.swift:346-401`

This is a pure refactor — extract the inner loop (lines 358-395) into a reusable method. `handleNewWorktreeFromHook` calls the new method instead of inlining the logic.

- [ ] **Step 1: Add the new method**

Add this method to `TabCoordinator`, just before `handleNewWorktreeFromHook`:

```swift
// MARK: - Shared Worktree Integration

/// Integrate newly discovered worktrees into the dashboard.
/// Called from both webhook-triggered discovery and periodic polling.
private func integrateNewWorktrees(repoRoot: String, allDiscovered: [WorktreeInfo], newWorktrees: [WorktreeInfo]) {
    guard !newWorktrees.isEmpty else { return }

    NSLog("[TabCoordinator] Integrating \(newWorktrees.count) new worktree(s) for \(repoRoot)")

    // Update WorkspaceManager tab
    if let tabIndex = workspaceManager.tabs.firstIndex(where: { $0.repoPath == repoRoot }) {
        workspaceManager.updateWorktrees(at: tabIndex, worktrees: allDiscovered)
    }

    for info in newWorktrees {
        let proj = workspaceManager.tabs.first(where: { $0.repoPath == repoRoot })?.displayName
            ?? URL(fileURLWithPath: repoRoot).lastPathComponent

        // Check if this worktree has a pending transfer (created via hook from an existing pane)
        if let transfer = pendingTransfers.consume(newWorktreePath: info.path) {
            NSLog("[TabCoordinator] Transferring pane from \(transfer.sourceWorktreePath) to \(info.path)")
            performPaneTransfer(transfer: transfer, newInfo: info, repoRoot: repoRoot, project: proj, allDiscoveredWorktrees: allDiscovered)
        } else {
            // No pending transfer — create a fresh tree
            let tree = terminalCoordinator.resolveTree(for: info)
            allWorktrees.append((info: info, tree: tree))
            worktreeRepoCache[info.path] = repoRoot
            
            let sessionName = runtimeBackend == "local" ? nil : SessionManager.persistentSessionName(for: info.path)
            if let surface = terminalCoordinator.surfaceManager.primarySurface(forPath: info.path) {
                AgentHead.shared.register(surface: surface, worktreePath: info.path, branch: info.branch, project: proj, startedAt: Date(), tmuxSessionName: sessionName, backend: runtimeBackend)
            }
        }
    }

    // Record startedAt for new worktrees
    let now = Self.iso8601.string(from: Date())
    var configChanged = false
    for info in newWorktrees {
        if config.worktreeStartedAt[info.path] == nil {
            config.worktreeStartedAt[info.path] = now
            configChanged = true
        }
    }
    if configChanged { config.save() }

    dashboardVC?.updateAgents(buildAgentDisplayInfos())
    statusPublisher.updateSurfaces(terminalCoordinator.surfaceManager.all)
    delegate?.tabCoordinatorRequestUpdateTitleBar(self)
}
```

- [ ] **Step 2: Refactor `handleNewWorktreeFromHook` to use it**

Replace the body of `handleNewWorktreeFromHook` (lines 346-401) with:

```swift
private func handleNewWorktreeFromHook(_ worktreePath: String) {
    WorktreeDiscovery.findRepoRootAsync(from: worktreePath) { [weak self] repoRoot in
        guard let self, let repoRoot else {
            NSLog("[TabCoordinator] Could not find repo root for hook-discovered worktree: \(worktreePath)")
            return
        }

        if self.config.workspacePaths.contains(repoRoot) {
            WorktreeDiscovery.discoverAsync(repoPath: repoRoot) { [weak self] worktrees in
                guard let self else { return }
                let knownPaths = Set(self.allWorktrees.map { $0.info.path })
                let newWorktrees = worktrees.filter { !knownPaths.contains($0.path) }
                self.integrateNewWorktrees(repoRoot: repoRoot, allDiscovered: worktrees, newWorktrees: newWorktrees)
            }
        } else {
            NSLog("[TabCoordinator] Auto-adding new repo via hook: \(repoRoot)")
            self.addRepo(at: repoRoot)
        }
    }
}
```

- [ ] **Step 3: Build and verify no regressions**

Run:
```bash
xcodegen generate && xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -5
```
Expected: `** BUILD SUCCEEDED **`

- [ ] **Step 4: Commit**

```bash
git add Sources/App/TabCoordinator.swift
git commit -m "refactor: extract integrateNewWorktrees from handleNewWorktreeFromHook"
```

---

### Task 2: Extend `refreshBranches` to detect new worktrees

**Files:**
- Modify: `Sources/App/TabCoordinator.swift:549-577` (the `refreshBranches` method)

- [ ] **Step 1: Modify `refreshBranches` to detect new worktrees**

Replace the `refreshBranches` method (lines 549-577) with:

```swift
private func refreshBranches() {
    let tabs = workspaceManager.tabs
    for (tabIndex, tab) in tabs.enumerated() {
        WorktreeDiscovery.discoverAsync(repoPath: tab.repoPath) { [weak self] freshWorktrees in
            guard let self else { return }
            let oldWorktrees = tab.worktrees

            // Detect new worktrees not yet tracked
            let knownPaths = Set(self.allWorktrees.map { $0.info.path })
            let newWorktrees = freshWorktrees.filter { !knownPaths.contains($0.path) }
            if !newWorktrees.isEmpty {
                self.integrateNewWorktrees(repoRoot: tab.repoPath, allDiscovered: freshWorktrees, newWorktrees: newWorktrees)
                return  // integrateNewWorktrees already refreshes the dashboard
            }

            // Detect branch name changes (existing behavior)
            var changed = false
            for fresh in freshWorktrees {
                if let old = oldWorktrees.first(where: { $0.path == fresh.path }),
                   old.branch != fresh.branch {
                    changed = true
                    break
                }
            }
            guard changed else { return }

            self.workspaceManager.updateWorktrees(at: tabIndex, worktrees: freshWorktrees)

            for (i, entry) in self.allWorktrees.enumerated() {
                if let fresh = freshWorktrees.first(where: { $0.path == entry.info.path }) {
                    self.allWorktrees[i] = (info: fresh, tree: entry.tree)
                }
            }

            self.dashboardVC?.updateAgents(self.buildAgentDisplayInfos())
        }
    }
}
```

Key changes from the original:
- Added `knownPaths` set comparison to detect new worktrees
- When new worktrees found, calls `integrateNewWorktrees` and returns early (it handles all UI updates)
- Branch change detection remains unchanged as a fallback path

- [ ] **Step 2: Build and verify**

Run:
```bash
xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -5
```
Expected: `** BUILD SUCCEEDED **`

- [ ] **Step 3: Commit**

```bash
git add Sources/App/TabCoordinator.swift
git commit -m "feat: detect new worktrees in refreshBranches polling"
```

---

### Task 3: Manual integration test

This is not an automated test — it verifies the end-to-end flow in a running app.

- [ ] **Step 1: Build and launch amux**

```bash
xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build && open .build/Build/Products/Debug/amux.app
```

- [ ] **Step 2: Verify auto-discovery**

In any terminal (inside or outside amux), create a worktree for a repo that amux is tracking:

```bash
cd /Volumes/openbeta/workspace/teamclaw  # or any tracked repo
git worktree add .worktrees/test-auto-discovery -b test/auto-discovery
```

Within ~5 seconds, a new mini card should appear in the amux dashboard for `test/auto-discovery`.

- [ ] **Step 3: Clean up test worktree**

```bash
cd /Volumes/openbeta/workspace/teamclaw
git worktree remove .worktrees/test-auto-discovery
git branch -D test/auto-discovery
```
