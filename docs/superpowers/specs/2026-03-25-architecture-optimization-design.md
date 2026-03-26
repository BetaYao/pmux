# Architecture Optimization: Coordinator Extraction + Unified State

**Date:** 2026-03-25
**Status:** Phase 1 Implemented
**Scope:** MainWindowController decomposition (Phase 1), AppState introduction (Phase 2), Dashboard refactor + test coverage (Phase 3)

---

## Problem Statement

`MainWindowController` is a 1,882-line god object with 15 protocol conformances and 50+ methods. It owns all terminal surfaces, repo view controllers, tab state, update logic, notification panels, and status polling. This makes it:

- **Hard to test** — zero test coverage, cannot instantiate without wiring everything
- **Hard to extend** — every new feature touches MainWindowController
- **Hard to reason about** — data flow is implicit, state mutations are scattered
- **Tightly coupled** — all delegate chains terminate here

### Current Responsibility Map

| Responsibility | Lines (approx) | Protocols |
|---|---|---|
| Window lifecycle & layout | ~200 | NSWindowDelegate |
| Tab switching & navigation | ~350 | TitleBarDelegate (partial), DashboardDelegate, QuickSwitcherDelegate |
| Terminal surface management | ~300 | — |
| Split pane operations | ~150 | — |
| Update checking & install | ~150 | UpdateCheckerDelegate, UpdateManagerDelegate, UpdateBannerDelegate |
| Panel/popover management | ~150 | NotificationPanelDelegate, AIPanelDelegate, NSPopoverDelegate, NotificationHistoryDelegate |
| Workspace loading & config | ~250 | SettingsDelegate |
| Status change handling | ~100 | StatusPublisherDelegate |
| Repo view management | ~100 | RepoViewDelegate |
| New branch dialog | ~50 | NewBranchDialogDelegate |
| Menu shortcut dispatch | ~80 | — |

---

## Phased Approach

### Phase 1 — Coordinator Extraction (this spec's primary focus)
### Phase 2 — Unified State Container (AppState + AppStore)
### Phase 3 — Dashboard Refactor + Test Coverage

Each phase is independently deliverable and verifiable.

---

## Phase 1: Coordinator Extraction

### Goal

Reduce MainWindowController from 1,882 lines to ~600 lines by extracting four Coordinator objects, each owning a single domain of responsibility.

### Architecture After Phase 1

```
MainWindowController (~700 lines)
│
│  Retained responsibilities:
│  - windowDidLoad / window lifecycle (NSWindowDelegate)
│  - setupLayout / embedViewController
│  - Menu @objc actions (dispatch to coordinators, some with real logic)
│  - TitleBar non-tab callbacks (theme, window controls)
│  - StatusPublisherDelegate (dispatches to TabCoordinator for UI update)
│  - SettingsDelegate (replaces config, triggers reload)
│  - normalizeBackendAvailabilityIfNeeded() and runtimeBackend resolution
│  - Coordinator assembly and inter-coordinator wiring
│  - PmuxWindow forwarding methods (see PmuxWindow Integration below)
│
├── TabCoordinator (~400 lines)
├── TerminalCoordinator (~350 lines)
├── UpdateCoordinator (~150 lines)
└── PanelCoordinator (~150 lines)
```

### 1. TabCoordinator

**File:** `Sources/App/TabCoordinator.swift`

**Owns:**
- `repoVCs: [String: RepoViewController]`
- `activeTabIndex: Int`
- `allWorktrees: [(info: WorktreeInfo, tree: SplitTree)]`
- `worktreeRepoCache: [String: String]`
- `branchRefreshTimer: Timer?`
- Reference to `WorkspaceManager`

**Rationale for `allWorktrees` ownership:** Despite being terminal-related data, `allWorktrees` is primarily read and mutated by tab/workspace operations (`loadWorkspaces`, `integrateDiscoveredRepoForTesting`, `refreshBranches`, `buildAgentDisplayInfos`, `worktreeDidDelete`). TerminalCoordinator accesses it only through TabCoordinator's API when it needs to resolve trees.

**Methods extracted from MainWindowController:**
- `switchToTab(_:)` — tab switching with terminal detach/attach
- `openRepoTab(repoPath:completion:)` — async worktree discovery + tab creation
- `getOrCreateRepoVC(for:)` — lazy RepoViewController creation
- `addRepo(at:)` — add new repo to workspace
- `integrateDiscoveredRepoForTesting(...)` — worktree integration
- `performCloseRepo(projectName:)` — close repo tab and cleanup
- `updateStatusPollPreferences()` — sync status publisher preferred paths
- `showCloseProjectModal(_:)` — close confirmation dialog
- `showAddProjectModal()` — open panel for adding repo
- `showNewThreadModal()` — new thread dialog
- `buildAgentDisplayInfos()` — collect agent display data for dashboard
- `loadWorkspaces()` — discover worktrees, build allWorktrees, configure dashboard
- `startBranchRefreshTimer()` / `refreshBranches()` — periodic worktree re-discovery
- `handleNavigateToWorktree(worktreePath:)` — notification-driven navigation with async fallback discovery
- `worktreeDidDelete(_:)` — cleanup allWorktrees, AgentHead, repoVCs, dashboard after deletion

**Adopts protocols:**
- `DashboardDelegate`
- `QuickSwitcherDelegate`
- `RepoViewDelegate` (delegates `didRequestDeleteWorktree` to TerminalCoordinator, handles `didRequestNewThread` and `didRequestShowDiff` locally)
- `NewBranchDialogDelegate` (creates terminal via TerminalCoordinator, updates allWorktrees/AgentHead/dashboard/repoVC locally)
- `TitleBarDelegate` (tab-related callbacks only: `didSelectDashboard`, `didSelectProject`, `didRequestCloseProject`, `didRequestAddProject`, `didRequestNewThread`)

**Delegate protocol defined:**

```swift
protocol TabCoordinatorDelegate: AnyObject {
    func tabCoordinator(_ coordinator: TabCoordinator, embedViewController vc: NSViewController)
    func tabCoordinatorDidSwitchTab(_ coordinator: TabCoordinator)
    func tabCoordinatorRequestUpdateTitleBar(_ coordinator: TabCoordinator)
    func tabCoordinatorRequestShowNewBranchDialog(_ coordinator: TabCoordinator)
    func tabCoordinatorRequestShowDiff(_ coordinator: TabCoordinator, worktreePath: String)
}
```

MainWindowController implements `TabCoordinatorDelegate` to handle view embedding, title bar updates, and diff overlay presentation.

**Dependencies:**
- `TerminalCoordinator` — to resolve trees, detach terminals on tab switch, request worktree deletion
- `PanelCoordinator` — to close panels on modal show (accessed via delegate, not direct reference)
- `config` (reference) — to read/write workspacePaths, cardOrder
- `statusPublisher` (reference) — to update preferred paths and surfaces

**Note on AgentHead:** `AgentHead.shared` is called from both TabCoordinator (register, unregister, reorder in `loadWorkspaces`, `worktreeDidDelete`, `newBranchDialog`) and TerminalCoordinator (unregister in `performDeleteWorktree`). Since `AgentHead` is a singleton accessed via `AgentHead.shared`, this split is acceptable — both coordinators call it directly. All mutations happen on the main thread.

### 2. TerminalCoordinator

**File:** `Sources/App/TerminalCoordinator.swift`

**Owns:**
- `surfaceManager: TerminalSurfaceManager`
- `webhookServer: WebhookServer?`

**Methods extracted from MainWindowController:**
- `splitFocusedPane(axis:)` — create new split pane
- `closeFocusedPane()` — close focused pane, cleanup session
- `moveFocus(_:positive:)` — navigate between split panes
- `resizeSplit(_:delta:)` — adjust split ratio
- `resetSplitRatio()` — reset to 50/50
- `saveSplitLayout(_:)` — persist split layout to config
- `resolveTree(for:)` — restore or create SplitTree for worktree
- `confirmAndDeleteWorktree(_:)` — delete worktree with confirmation
- `performDeleteWorktree(...)` — execute worktree deletion
- `setupWebhookServer()` — start webhook server for external status events

**Note:** Split pane methods (`splitFocusedPane`, `closeFocusedPane`, `moveFocus`, `resizeSplit`, `resetSplitRatio`) require access to the current `RepoViewController.activeSplitContainer`. TerminalCoordinator obtains this via a `currentRepoVC` closure provided at init, rather than holding a reference to TabCoordinator.

**Delegate protocol defined:**

```swift
protocol TerminalCoordinatorDelegate: AnyObject {
    func terminalCoordinatorDidUpdateSurfaces(_ coordinator: TerminalCoordinator)
    func terminalCoordinator(_ coordinator: TerminalCoordinator, didDeleteWorktree info: WorktreeInfo)
}
```

MainWindowController implements this to update StatusPublisher. TabCoordinator's `worktreeDidDelete` handles the UI cleanup.

**Dependencies:**
- `config` (reference) — to read/write splitLayouts and backend
- `statusPublisher` (reference) — to call `updateSurfaces` after split changes
- `currentRepoVC: () -> RepoViewController?` — closure to access active repo view for split operations

**Testing note:** `splitFocusedPane` and `closeFocusedPane` require a `SplitContainerView` with a view hierarchy. Tests should use a mock `SplitContainerView` stub or test the tree operations separately (already covered by `SplitNodeTests`). Focus TerminalCoordinator tests on coordination logic: session cleanup, surface registry updates, layout persistence.

### 3. UpdateCoordinator

**File:** `Sources/App/UpdateCoordinator.swift`

**Owns:**
- `updateChecker: UpdateChecker`
- `updateManager: UpdateManager`
- `updateBanner: UpdateBanner`
- `pendingRelease: ReleaseInfo?`

**Methods extracted from MainWindowController:**
- `setupAutoUpdate()` — configure update checking timer
- `checkForUpdates()` — trigger manual check

**Adopts protocols:**
- `UpdateCheckerDelegate`
- `UpdateManagerDelegate`
- `UpdateBannerDelegate`

**Delegate protocol defined:**

```swift
protocol UpdateCoordinatorDelegate: AnyObject {
    func updateCoordinator(_ coordinator: UpdateCoordinator, showBanner banner: UpdateBanner)
}
```

**Dependencies:**
- `window` (weak reference) — to position/show banner
- No dependencies on other coordinators

### 4. PanelCoordinator

**File:** `Sources/App/PanelCoordinator.swift`

**Owns:**
- `notificationPanel: NotificationPanelView`
- `aiPanel: AIPanelView`
- `notificationPopover: NSPopover`
- `aiPopover: NSPopover`

**Methods extracted from MainWindowController:**
- `setupPanelPopovers()` — configure popover appearance
- `toggleNotificationPanel()` — show/hide notification popover
- `toggleAIPanel()` — show/hide AI popover
- `closeBothPanels()` — dismiss all panels

**Adopts protocols:**
- `NotificationPanelDelegate`
- `AIPanelDelegate`
- `NSPopoverDelegate`
- `NotificationHistoryDelegate`

**Delegate protocol defined:**

```swift
protocol PanelCoordinatorDelegate: AnyObject {
    func panelCoordinator(_ coordinator: PanelCoordinator, navigateToWorktreePath path: String)
}
```

**Dependencies:**
- `titleBar` (reference) — to get anchor views for popover positioning
- `TabCoordinator` (weak reference) — to navigate to worktree on notification click

### Inter-Coordinator Communication

```
                     MainWindowController
                    /    |      |       \
                   /     |      |        \
     TabCoordinator  Terminal  Update   Panel
                     Coordinator Coordinator Coordinator

  All cross-coordinator calls route through MainWindowController:
  - Tab switch → MWC → TerminalCoordinator.detach + PanelCoordinator.close
  - Worktree delete → MWC → TabCoordinator.worktreeDidDelete
  - Notification click → MWC → TabCoordinator.handleNavigateToWorktree
```

**Communication pattern:** Protocol-based delegates, all routed through MainWindowController.

- Coordinators never reference each other directly
- MainWindowController is the sole strong owner of all coordinators
- All cross-coordinator operations flow through MainWindowController's delegate implementations
- TerminalCoordinator receives a `currentRepoVC` closure (not a coordinator reference) for split pane access

### PmuxWindow Integration

`PmuxWindow` (a subclass of `NSWindow` defined in the same file) overrides `performKeyEquivalent` and directly calls `MainWindowController` methods that will move to TerminalCoordinator: `splitFocusedPane`, `closeFocusedPane`, `moveFocus`, `resizeSplit`, `resetSplitRatio`.

**Solution:** MainWindowController retains thin forwarding methods for these split operations:

```swift
// MainWindowController — forwarding methods for PmuxWindow
func splitFocusedPane(axis: SplitAxis) {
    terminalCoordinator.splitFocusedPane(axis: axis)
}
func closeFocusedPane() {
    terminalCoordinator.closeFocusedPane()
}
// ... etc
```

This keeps PmuxWindow's code unchanged. The forwarding methods are ~15 lines total, included in the ~700 line estimate.

`ViewHostController` (also in the same file, ~25 lines) is a trivial utility class. It stays in the same file — no extraction needed.

### Config Sharing

The `config` object is shared mutably across coordinators via reference. All mutations happen on the main thread (enforced by AppKit's event loop). Phase 2's AppState will eventually subsume config as the single source of truth. Until then, each coordinator accesses config directly for its domain-specific properties only:

- TabCoordinator: `workspacePaths`, `cardOrder`, `agentDetect`
- TerminalCoordinator: `splitLayouts`, `backend`
- UpdateCoordinator: none (reads update URL from plist)
- PanelCoordinator: none

### Menu @objc Actions

Some `@objc` menu actions contain real logic beyond thin dispatch:

- `showQuickSwitcher()` — builds worktree list, presents QuickSwitcherViewController → moves to TabCoordinator
- `showSettings()` — creates SettingsViewController, presents as sheet → stays in MainWindowController
- `showNewBranchDialog()` — creates NewBranchDialog, presents as sheet → stays in MainWindowController (delegates to TabCoordinator via NewBranchDialogDelegate)
- `showDiffOverlay()` / `presentDiffOverlay(for:)` — creates DiffOverlayViewController → stays in MainWindowController
- `showKeyboardShortcuts()` — creates and presents help panel → stays in MainWindowController
- `dashboardZoomIn/Out()` — delegates to dashboardVC → stays in MainWindowController
- `closePaneOrTab()` — dispatches to TerminalCoordinator or TabCoordinator based on context → stays in MainWindowController

### Migration Strategy

**Order of extraction (each step is a standalone commit):**

1. **UpdateCoordinator** — most isolated, zero cross-coordinator deps
2. **PanelCoordinator** — minimal deps (only titleBar anchor)
3. **TerminalCoordinator** — moderate deps (surfaceManager, config)
4. **TabCoordinator** — most deps, extracted last when other coordinators are stable

**Per-extraction steps:**
1. Create new Coordinator file with extracted methods
2. Define delegate protocol
3. Move protocol conformances from MainWindowController
4. Update MainWindowController to hold coordinator, implement delegate
5. Add unit tests for the new Coordinator
6. Verify build and existing tests pass

### Testing Strategy (Phase 1)

Each Coordinator gets a dedicated test file:

- `Tests/UpdateCoordinatorTests.swift` — test update state transitions, banner display
- `Tests/PanelCoordinatorTests.swift` — test panel toggle logic, mutual exclusion
- `Tests/TerminalCoordinatorTests.swift` — test split operations, layout persistence
- `Tests/TabCoordinatorTests.swift` — test tab switching, repo open/close

UpdateCoordinator and PanelCoordinator can be tested with mock delegates, without instantiating NSWindow. TabCoordinator tests need mock WorkspaceManager and mock delegate. TerminalCoordinator split pane methods need a view hierarchy — test coordination logic (session cleanup, registry updates, layout persistence) with mocks; tree operations are already covered by SplitNodeTests.

---

## Phase 2: Unified State Container (Future)

### Goal

Introduce `AppState` as single source of truth. Coordinators dispatch `Action`s to `AppStore`, UI subscribes to state changes.

### Core Components

```swift
struct AppState {
    var tabs: [TabState]
    var activeTabIndex: Int
    var terminals: [String: TerminalState]  // keyed by worktree path
    var dashboard: DashboardState
    var notifications: [NotificationEntry]
    var updateState: UpdateState
}

enum Action {
    case switchTab(Int)
    case openRepo(String)
    case closeRepo(String)
    case createTerminal(worktreePath: String)
    case splitPane(axis: SplitAxis)
    case closeFocusedPane
    case statusChanged(worktreePath: String, status: AgentStatus)
    case configChanged(Config)
    // ...
}

class AppStore {
    private(set) var state: AppState
    var onChange: ((AppState, AppState) -> Void)?  // (old, new)

    func dispatch(_ action: Action) {
        let oldState = state
        state = reduce(state, action)
        onChange?(oldState, state)
    }
}
```

### Migration Path

1. Introduce `AppState` and `AppStore` alongside existing mutable properties
2. Migrate one domain at a time (tabs first, then terminals, then notifications)
3. Coordinators become action dispatchers instead of direct state mutators
4. UI components subscribe to `AppStore.onChange` for updates
5. Remove redundant mutable properties as each domain migrates

### Prerequisites

Phase 1 must be complete. Coordinator boundaries define the natural domain boundaries for AppState.

---

## Phase 3: Dashboard Refactor + Test Coverage (Future)

### Goal

Split DashboardViewController (968 lines) and fill critical test gaps.

### Dashboard Decomposition

- `DashboardLayoutManager` — grid vs focus layout switching, frame calculations
- `DashboardDataSource` — agent data binding, card ordering
- `DashboardViewController` — view lifecycle, delegates to layout manager and data source

### Test Coverage Targets

| Area | Current | Target |
|---|---|---|
| MainWindowController orchestration | 0 tests | 10+ integration tests |
| Tab switching flows | 0 tests | 5+ tests |
| Terminal lifecycle | indirect only | 5+ direct tests |
| Config persistence roundtrip | 0 tests | 3+ tests |
| Coordinator unit tests (Phase 1) | 0 tests | 15+ tests |

---

## Success Criteria

### Phase 1
- [ ] MainWindowController reduced to ~700 lines (including forwarding methods and retained @objc actions)
- [ ] 4 Coordinator files created, each < 450 lines
- [ ] All existing tests pass
- [ ] 15+ new Coordinator unit tests
- [ ] No behavior changes visible to users

### Phase 2
- [ ] AppState struct covers tabs, terminals, notifications, update state
- [ ] AppStore dispatches actions, notifies subscribers
- [ ] Coordinators use AppStore instead of direct mutation
- [ ] State flow is traceable through Action log

### Phase 3
- [ ] DashboardViewController reduced to ~400 lines
- [ ] 30+ total new tests across all phases
- [ ] Integration tests cover tab switching, terminal lifecycle, config persistence

---

## Risks and Mitigations

| Risk | Mitigation |
|---|---|
| Coordinator extraction breaks subtle timing | Extract in order of isolation; run full test suite after each |
| Cross-coordinator communication becomes spaghetti | Limit to delegate protocols; no direct method calls between coordinators |
| AppState migration (Phase 2) is too disruptive | Run old and new state side-by-side; migrate one domain at a time |
| Over-engineering for project size | Phase 2 and 3 are optional; Phase 1 alone delivers significant value |
