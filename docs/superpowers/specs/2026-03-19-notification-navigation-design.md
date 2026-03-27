# 通知点击导航到对应工作树

> 日期：2026-03-19

## 问题

当前点击 macOS 通知只会激活应用窗口（`NSApp.activate`），不会导航到触发通知的工作树。用户需要手动查找对应分支，体验断裂。

## 目标

点击通知后自动导航到对应 Repo 标签页，并选中触发通知的 worktree item。如果该 Repo 标签页尚未打开，自动创建。

## 设计

### 方案：NSNotification 解耦

NotificationManager 通过 `Foundation.Notification`（NSNotification）广播导航请求，MainWindowController 监听并执行导航。两者无直接依赖。

### 数据流

```
用户点击系统通知
  → NotificationManager.didReceive(response:)
  → 从 userInfo["worktreePath"] 取值
  → DispatchQueue.main.async:
      → NSApp.activate(ignoringOtherApps: true)
      → window?.deminiaturize(nil)
      → NotificationCenter.default.post(.navigateToWorktree, userInfo: ["worktreePath": path])

MainWindowController 收到 .navigateToWorktree
  → handleNavigateToWorktree(_:)
  → 在 workspaceManager.tabs 中查找包含该 worktreePath 的 tab
  → 若标签不存在 → openRepoTab(repoPath:) 创建新标签
  → switchToTab(tabIndex + 1)  // +1 because Dashboard is index 0
  → repoVC.selectWorktree(byPath: worktreePath)
```

### 改动文件

#### 1. NotificationManager.swift

**新增** `Notification.Name` 扩展：

```swift
extension Notification.Name {
    static let navigateToWorktree = Notification.Name("amux.navigateToWorktree")
}
```

**修改** `didReceive(response:completionHandler:)`：

```swift
func userNotificationCenter(_ center: UNUserNotificationCenter,
                            didReceive response: UNNotificationResponse,
                            withCompletionHandler completionHandler: @escaping () -> Void) {
    let userInfo = response.notification.request.content.userInfo

    // didReceive 可能在非主线程回调，UI 操作需要切到主线程
    DispatchQueue.main.async {
        NSApp.activate(ignoringOtherApps: true)
        NSApp.mainWindow?.deminiaturize(nil)

        if let path = userInfo["worktreePath"] as? String {
            NotificationCenter.default.post(
                name: .navigateToWorktree,
                object: nil,
                userInfo: ["worktreePath": path]
            )
        }
    }

    completionHandler()
}
```

#### 2. MainWindowController.swift

**init 中注册监听：**

```swift
NotificationCenter.default.addObserver(
    self, selector: #selector(handleNavigateToWorktree(_:)),
    name: .navigateToWorktree, object: nil
)
```

**新增导航方法：**

```swift
@objc private func handleNavigateToWorktree(_ notification: Notification) {
    guard let worktreePath = notification.userInfo?["worktreePath"] as? String else { return }

    // 1. 在 workspaceManager.tabs 中查找包含该 worktree 的 tab
    guard let tabIndex = workspaceManager.tabs.firstIndex(where: { tab in
        tab.worktrees.contains(where: { $0.path == worktreePath })
    }) else { return }

    let repoPath = workspaceManager.tabs[tabIndex].repoPath

    // 2. 切换到该 Repo 标签（+1 because Dashboard is index 0）
    switchToTab(tabIndex + 1)

    // 3. 选中对应 worktree
    if let repoVC = repoVCs[repoPath] {
        repoVC.selectWorktree(byPath: worktreePath)
    }
}
```

注意：当 tab 不存在时（worktree 所属 repo 未在 workspaceManager 中），需要先创建标签：

```swift
@objc private func handleNavigateToWorktree(_ notification: Notification) {
    guard let worktreePath = notification.userInfo?["worktreePath"] as? String else { return }

    // 1. 查找包含该 worktree 的现有 tab
    var repoPath: String?
    if let tabIndex = workspaceManager.tabs.firstIndex(where: { tab in
        tab.worktrees.contains(where: { $0.path == worktreePath })
    }) {
        repoPath = workspaceManager.tabs[tabIndex].repoPath
        switchToTab(tabIndex + 1)  // +1 because Dashboard is index 0
    } else {
        // Tab 不存在，通过 worktree path 找到 repo 并自动打开
        guard let foundRepoPath = allWorktrees.first(where: { $0.info.path == worktreePath })
                .flatMap({ _ in
                    config.workspacePaths.first(where: { wsPath in
                        WorktreeDiscovery.discover(repoPath: wsPath).contains(where: { $0.path == worktreePath })
                    })
                }) else { return }
        repoPath = foundRepoPath
        openRepoTab(repoPath: foundRepoPath)
    }

    // 2. 选中对应 worktree
    if let rp = repoPath, let repoVC = repoVCs[rp] {
        repoVC.selectWorktree(byPath: worktreePath)
    }
}
```

#### 3. RepoViewController.swift

**新增方法：**

```swift
func selectWorktree(byPath path: String) {
    guard let index = worktrees.firstIndex(where: { $0.path == path }) else { return }
    showTerminal(at: index)  // showTerminal 内部已调用 sidebarVC.selectWorktree(at:)
}
```

### 不改动的部分

- `NotificationManager.notify()` — 已在 `userInfo` 中传递 `worktreePath`，无需修改
- `SidebarViewController.selectWorktree(at:)` — 已存在，由 `showTerminal(at:)` 调用
- `StatusPublisher` / `StatusDetector` — 不涉及

### 边界情况

| 场景 | 处理 |
|------|------|
| worktreePath 已被删除 | `workspaceManager.tabs` 查找失败，静默忽略 |
| Repo 标签已打开 | 直接 switchToTab，不重复创建 |
| Repo 标签未打开 | 调用 openRepoTab 自动创建 |
| 当前已在目标标签和 worktree | switchToTab guard 跳过，showTerminal 刷新选中态 |
| 窗口最小化 | `deminiaturize(nil)` 恢复窗口 |
| 非主线程回调 | `DispatchQueue.main.async` 保证主线程执行 |

### 改动量

约 35 行代码，3 个文件。无新依赖，无架构变更。
