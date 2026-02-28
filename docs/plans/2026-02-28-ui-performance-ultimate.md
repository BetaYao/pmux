# pmux 界面性能终极方案

> **For Claude:** 实施时遵循 TDD。可考虑 `subagent-driven-development`  skill 并行执行任务。架构改动较多，建议按 Phase 顺序推进，每 Phase 完成后验证再进入下一阶段。

**目标：** 消除点击 worktree、new branch、通知 icon 的迟滞感，使界面响应达到 Zed 级流畅。

**架构：** 1) 移除 render 路径中的阻塞 I/O；2) 所有重量级操作异步化并立即反馈；3) 状态分层，细粒度重绘；4) 渲染树瘦身，避免无效 clone 和重复构建。

**技术栈：** Rust, GPUI, `cx.spawn` 异步任务, `blocking::unblock` 线程池

**预估总工期：** 4~6 天（按 Phase 分步实施）

---

## 问题根因摘要

| 现象 | 根因 |
|------|------|
| 点击 worktree 切换卡顿 | `switch_to_worktree` 在主线程同步执行 `create_runtime_from_env`（spawn shell / tmux 命令） |
| 点击 new branch 有延迟 | `cx.notify()` 触发全量 AppRoot 重绘，render 内调用 `discover_worktrees`（git 子进程） |
| 通知 icon 开/关弹窗慢 | 同上，全量重绘 + render 内 git 调用 |
| **terminal 输入文字慢** | 16ms 轮询循环**无条件** `cx.notify()`，约 60fps 全量重绘（含 discover_worktrees） |
| 所有交互都有隐性成本 | `render_workspace_view` 每次重绘执行 `discover_worktrees(&repo_path)` |

---

## Phase 1：Worktree 缓存 —— 从 render 中移除 git 调用

**目标：** 任何 `cx.notify()` 不再触发 `git worktree list` 子进程。

### Task 1.1：在 AppRoot 中增加 worktree 缓存字段

**文件：**
- 修改：`src/ui/app_root.rs`（struct 定义区约 42~104 行）

**步骤 1：添加字段**

在 `per_repo_worktree_index` 之后添加：

```rust
/// Cached worktrees for active repo. Refreshed on workspace change, branch create/delete, explicit refresh.
/// Avoids calling discover_worktrees in render path.
cached_worktrees: Vec<crate::worktree::WorktreeInfo>,
/// Repo path for which cached_worktrees is valid
cached_worktrees_repo: Option<PathBuf>,
```

**步骤 2：在 `AppRoot::new()` 中初始化**

在 `new()` 的字段初始化中添加：

```rust
cached_worktrees: Vec::new(),
cached_worktrees_repo: None,
```

**步骤 3：运行验证**
```
cargo check
```
Expected: 无编译错误。

---

### Task 1.2：实现 refresh 与缓存读取逻辑

**文件：**
- 修改：`src/ui/app_root.rs`

**步骤 1：添加刷新方法**

在 `impl AppRoot` 中（例如 `update_status_counts` 附近）添加：

```rust
/// Refresh worktree cache for the given repo. Call when:
/// - Switching workspace tab
/// - After create_branch / delete worktree
/// - On explicit user refresh (future)
fn refresh_worktrees_for_repo(&mut self, repo_path: &Path) {
    match crate::worktree::discover_worktrees(repo_path) {
        Ok(wt) => {
            self.cached_worktrees = wt;
            self.cached_worktrees_repo = Some(repo_path.to_path_buf());
        }
        Err(_) => {
            self.cached_worktrees.clear();
            self.cached_worktrees_repo = None;
        }
    }
}

/// Get worktrees for current repo (from cache). Call from render.
fn worktrees_for_render(&self, repo_path: &Path) -> &[crate::worktree::WorktreeInfo] {
    if self.cached_worktrees_repo.as_deref() == Some(repo_path) {
        &self.cached_worktrees
    } else {
        &[]
    }
}
```

**步骤 2：在需要刷新的调用点调用 `refresh_worktrees_for_repo`**

在以下位置添加或确保调用（搜索 `discover_worktrees` 定位）：
- `init_workspace_restoration`：在进入 workspace 且取得 `path` 后，调用 `self.refresh_worktrees_for_repo(&path)` 替代首次 `discover_worktrees` 后的逻辑，仍用缓存结果做后续判断
- `create_branch` 成功回调（NewBranchOrchestrator 完成）后
- `delete worktree` 确认后
- Workspace tab 切换（`switch_to_tab`）后
- 删除 worktree 的 `handle_delete_worktree` 等逻辑完成后

**步骤 3：修改 `render_workspace_view`**

将约 1595-1607 行：

```rust
// Load worktrees from git and sync Sidebar selection with active worktree
let worktrees = crate::worktree::discover_worktrees(&repo_path).unwrap_or_default();
if !worktrees.is_empty() {
    sidebar.set_worktrees(worktrees);
```

替换为：

```rust
// Use cached worktrees (never call git in render)
let worktrees = self.worktrees_for_render(&repo_path).to_vec();
if !worktrees.is_empty() {
    sidebar.set_worktrees(worktrees);
```

**步骤 4：确保首次进入 workspace 时缓存已填充**

在 `init_workspace_restoration` 中，在首次使用 worktrees 之前调用 `self.refresh_worktrees_for_repo(&path)`。原有的 `discover_worktrees` 调用改为使用 `self.cached_worktrees` 或通过 `refresh_worktrees_for_repo` 填充后的缓存。

**步骤 5：Workspace tab 切换时刷新**

在 `handle_workspace_tab_switch`（或等价逻辑）中，`switch_to_tab(idx)` 之后对新的 `repo_path` 调用 `self.refresh_worktrees_for_repo(&repo_path)`。

**步骤 6：运行验证**
```
cargo run
```
手动验证：切换 worktree、点 new branch、点通知 icon，观察响应速度。确认不再在每次点击时执行 `git worktree list`（可临时在 `discover_worktrees` 内加 `eprintln!` 验证）。

---

### Task 1.3：统一替换所有业务逻辑中的 discover_worktrees

**文件：**
- 修改：`src/ui/app_root.rs`

**说明：** 将所有直接调用 `discover_worktrees` 的地方分为两类：
1. **在非 render 路径**（事件回调、初始化等）：先 `refresh_worktrees_for_repo`，再使用 `self.cached_worktrees`
2. **在 render 路径**：一律改为 `self.worktrees_for_render(&repo_path)`

**需检查的位置**（grep `discover_worktrees`）：
- 约 227, 245, 520, 564, 580, 604, 619, 898, 1094, 1119, 1234, 1361, 1646, 1866 行

**步骤：** 逐个替换，保证逻辑等价。完成后 `cargo test` 全绿。

---

## Phase 2：Worktree 切换异步化

**目标：** 点击 worktree 后立即显示选中态和 loading，runtime 创建在后台完成，不阻塞主线程。

### Task 2.1：引入 worktree 切换中的 loading 状态

**文件：**
- 修改：`src/ui/app_root.rs`

**步骤 1：添加状态字段**

在 `pending_worktree_selection` 附近添加：

```rust
/// When Some(idx): switching to worktree idx, show loading in terminal area
worktree_switch_loading: Option<usize>,
```

在 `new()` 中初始化为 `None`。

**步骤 2：修改 `process_pending_worktree_selection`**

将同步的 `switch_to_worktree` 调用改为：
1. 设置 `self.worktree_switch_loading = Some(idx)`
2. `cx.notify()` 立即刷新 UI（显示 loading）
3. `cx.spawn` 异步任务：在 `blocking::unblock` 中执行 `create_runtime_from_env`，完成后在主线程 `entity.update` 中执行 `attach_runtime`，并清除 `worktree_switch_loading`

**代码骨架：**

```rust
fn process_pending_worktree_selection(&mut self, cx: &mut Context<Self>) {
    let idx = match self.pending_worktree_selection.take() {
        Some(i) => i,
        None => return,
    };
    let (repo_path, path, branch) = {
        let tab = self.workspace_manager.active_tab()?;
        let repo_path = tab.path.clone();
        let worktrees = self.worktrees_for_render(&repo_path);
        let worktree = worktrees.get(idx)?;
        (repo_path, worktree.path.clone(), worktree.short_branch_name().to_string())
    };

    self.active_worktree_index = Some(idx);
    self.worktree_switch_loading = Some(idx);
    self.stop_current_session();
    cx.notify();

    let workspace_path = self.workspace_manager.active_tab()
        .map(|t| t.path.clone())
        .unwrap_or_else(|| repo_path.clone());
    let config = Config::load().ok();
    let entity = cx.entity();
    cx.spawn(async move |entity, cx| {
        let result = blocking::unblock(move || {
            create_runtime_from_env(&workspace_path, &path, &branch, 80, 24, config.as_ref())
        }).await;

        let (runtime, pane_target) = match result {
            Ok(rt) => {
                let pt = rt.primary_pane_id().unwrap_or_else(|| format!("local:{}", path.display()));
                (rt, pt)
            }
            Err(e) => {
                let _ = entity.update(&cx, |this, cx| {
                    this.worktree_switch_loading = None;
                    this.state.error_message = Some(format!("Runtime error: {}", e));
                    cx.notify();
                });
                return;
            }
        };

        entity.update(&cx, |this, cx| {
            this.worktree_switch_loading = None;
            this.attach_runtime(runtime, pane_target, &path, &branch, cx, None);
            cx.notify();
        }).ok();
    }).detach();
}
```

（注意：需处理 `?` 的 early return，以及 `entity.update` 的正确用法，此处为骨架）

**步骤 3：在 render 中显示 loading**

在 `render_workspace_view` 的 terminal 区域，当 `self.worktree_switch_loading.is_some()` 时，显示「Connecting to worktree...」占位内容，而不是 terminal 内容。

---

### Task 2.2：init_workspace_restoration 与 try_recover 的异步化

**文件：**
- 修改：`src/ui/app_root.rs`

**说明：** `init_workspace_restoration` 目前也同步调用 `switch_to_worktree` / `start_local_session`，会导致启动时卡顿。将其改为：
1. 若 `try_recover_then_switch` / `try_recover_then_start` 可快速完成（无阻塞 I/O），可保留同步
2. 否则将 `switch_to_worktree` / `start_local_session` 放入 `cx.spawn`，启动页或 workspace 视图先显示 loading，再在异步完成后刷新

**步骤：** 根据实际 `try_recover_*` 的实现决定是否异步；若内部有 `Command::output` 等阻塞调用，建议一并放入 `blocking::unblock`。

---

## Phase 3：面板与对话框的细粒度重绘

**目标：** 点击通知 icon、new branch 时，仅更新对应面板/对话框的显隐，不触发整棵 workspace 树的重量级重建。

### 设计思路

GPUI 中，当 `cx.update_entity(&app_root_entity, ...)` 后 `cx.notify()` 会令 AppRoot 整树重绘。要减少重绘范围，有两种路线：

**方案 A：子 Entity + 独立 Model（若 GPUI 支持）**  
将 NotificationPanel、NewBranchDialog 抽成持有 `EntityId` 的子组件，其显隐由独立 Model 驱动，toggle 时只更新该 Model，仅子 Entity 重绘。

**方案 B：条件渲染 + 轻量 render 路径（当前可行）**  
保持结构不变，但通过 Phase 1 的 worktree 缓存，render 路径已无阻塞 I/O。此时重绘成本主要来自：
- `terminal_buffers.lock().map(|g| g.clone())` 等 clone
- 庞大的 `render_workspace_view` 树构建

Phase 3 聚焦：**将通知面板、new branch 对话框的显隐与主内容区解耦**，使它们的 toggle 不依赖「整棵 workspace 树重建」。

**实施策略（方案 B 增强）：**

1. **通知面板**：将 `show_notification_panel` 的 toggle 与「获取 notifications」分离。toggle 时只做布尔翻转，不触发 `notification_manager.lock()` 的耗时逻辑。notifications 的拉取可延后到面板真正显示时（例如首次展开时再拉取）。
2. **New Branch 对话框**：同理，`open_new_branch_dialog` 仅设置 `is_open`，不做额外重活。
3. **渲染结构**：把 NotificationPanel、NewBranchDialog 放到与主内容区平行的顶层 div，通过 `show_notification_panel` / `new_branch_dialog.is_open()` 条件渲染，避免它们受主内容区复杂子树的牵连。

若 GPUI 支持 `observe` / `Model`，可进一步将 `show_notification_panel`、`new_branch_dialog` 抽到独立 Model，由对应子组件 subscribe，实现真正的细粒度更新。

### Task 3.1：NotificationPanel 轻量化 toggle

**文件：**
- 修改：`src/ui/app_root.rs`

**步骤 1：拆分「toggle」与「拉取数据」**

当前 `when(show_notifications, |el| { ... })` 内会执行：
- `notification_manager.lock().map(|m| m.recent(100)...)` 
- 构建整个 `NotificationPanel`

优化：将 `when(show_notifications, ...)` 保留，但在 **进入该分支时** 才拉取 `recent(100)`。若 GPUI 的 `when` 会短路未选分支，则已避免不必要计算；否则可把「拉取 notifications」放到一个惰性计算的 helper 中，只在 `show_notifications == true` 时调用。

**步骤 2：确保 toggle 回调极简**

```rust
.on_toggle_notifications(move |_window, cx| {
    let _ = cx.update_entity(&app_root_entity_for_notif, |this: &mut AppRoot, cx| {
        this.show_notification_panel = !this.show_notification_panel;
        cx.notify();
    });
})
```

保持此逻辑不变。Phase 1 已移除 render 中的 `discover_worktrees`，此时 `cx.notify()` 触发的重绘应明显变轻。

---

### Task 3.2：NewBranchDialog 轻量化

**文件：**
- 修改：`src/ui/app_root.rs`

**说明：** `open_new_branch_dialog` 本身应只做状态设置。检查是否有在 open 时执行 `discover_worktrees` 或其它重逻辑；若有，移出或延后。Phase 1 完成后，open/close 应已足够轻。

---

### Task 3.3（可选）：将 overlay 类 UI 抽成 Window 级子 Entity

**说明：** 若 Phase 1~2 完成后，通知面板和对话框的响应仍不达标，可调研 GPUI 是否支持：
- 将 NotificationPanel、NewBranchDialog 作为 Window 的独立 child entity 挂载
- 通过 `window.appendChild` 或类似 API 渲染在顶层
- 其显隐由自身 Entity 的 state 驱动，与 AppRoot 解耦

该任务为可选，待 Phase 1~2 验证后再决定是否实施。

---

## Phase 4：渲染树瘦身与 clone 优化

**目标：** 减少每次 render 的 clone 和重复构建。

### Task 4.1：terminal_buffers 使用 Arc 引用

**文件：**
- 修改：`src/ui/app_root.rs`

**现状：**
```rust
let terminal_buffers = self.terminal_buffers.lock()
    .map(|g| g.clone())
    .unwrap_or_default();
```

**问题：** 每次 render 都 clone 整个 `HashMap<String, TerminalBuffer>`，其中 `TerminalBuffer::Term` 包含 `Arc<TerminalEngine>`，clone 成本较高。

**方案：** 将 `terminal_buffers` 改为 `Arc<Mutex<HashMap<...>>>`（若尚未是 Arc），在 render 中只 `lock()` 取得 guard，不 clone；或通过 `Arc::clone` 共享，避免 clone 内部大量数据。需确保 `SplitPaneContainer` 等子组件接受的类型兼容（例如接受 `Arc<Mutex<HashMap>>` 或只读视图）。

---

### Task 4.2：split_tree、pane_statuses 等避免整块 clone

**文件：**
- 修改：`src/ui/app_root.rs`、`src/ui/split_pane_container.rs`、`src/ui/sidebar.rs` 等

**现状：** `split_tree.clone()`、`pane_statuses.clone()` 在每次 render 中执行。

**方案：** 
- `split_tree`：若仅为只读遍历，可改为 `&self.split_tree` 引用传递，或定义 `split_tree: Arc<SplitNode>` 共享
- `pane_statuses`：已是 `Arc<Mutex<HashMap>>`，传 `Arc::clone(&self.pane_statuses)` 而非 `lock().map(|g| g.clone())`

具体需根据各子组件的 `IntoElement` 签名调整，确保不引入不必要的 clone。

---

### Task 4.3：Terminal 轮询 —— 仅在内容变化时 cx.notify()

**目标：** 消除 terminal 输入迟滞。当前 16ms 轮询**无论是否有新字节**都调用 `cx.notify()`，导致约 60fps 的全量重绘。改为仅在 terminal 内容真正变化时重绘。

**文件：**
- 修改：`src/terminal/engine.rs`（`advance_bytes`）
- 修改：`src/ui/app_root.rs`（`setup_local_terminal`、`setup_pane_terminal_output` 中的轮询循环）

**步骤 1：`advance_bytes` 返回是否有新内容**

在 `src/terminal/engine.rs` 中，将 `advance_bytes` 改为返回 `bool`：

```rust
/// Process all pending bytes from the PTY channel.
/// Returns true if any bytes were processed (caller should redraw).
pub fn advance_bytes(&self) -> bool {
    let Ok(mut term) = self.terminal.try_lock() else { return false };
    let Ok(mut processor) = self.processor.try_lock() else { return false };
    let mut had_input = false;
    while let Ok(bytes) = self.byte_rx.try_recv() {
        had_input = true;
        processor.advance(&mut *term, &bytes);
    }
    had_input
}
```

**步骤 2：轮询循环中条件性 cx.notify()**

在 `src/ui/app_root.rs` 的轮询循环内，将：

```rust
engine.advance_bytes();
// ... status detection ...
if entity.update(cx, |_, cx| cx.notify()).is_err() { break; }
```

改为：

```rust
let content_changed = engine.advance_bytes();
// ... status detection（status 变化时也需 redraw，若 check_status 发布变更则后续会 notify）...
if content_changed {
    if entity.update(cx, |_, cx| cx.notify()).is_err() { break; }
}
```

**注意：** status 检测若发现状态变化会通过 EventBus 触发 `status_change_tx`，最终也会导致重绘。轮询循环中 `content_changed` 为 true 时必定需要 notify；为 false 时，若无 status 变化则可跳过，减少无效重绘。若 status 检测与 content 在同一轮询中，可简化为：仅当 `content_changed` 时 notify（status 变化会在下一轮 content 到达时一起刷新，或依赖 EventBus 订阅者）。

**最简实现：** 先只按 `content_changed` 条件 notify，验证 terminal 输入流畅度。若 status 图标更新有延迟，再考虑 status 变化时额外 notify。

**步骤 3：多 pane 场景**

`setup_pane_terminal_output` 中每个 pane 有独立 engine，需对每个 engine 调用 `advance_bytes()`，任一返回 true 则本轮应 notify。

**步骤 4：运行验证**
```
cargo run
```
快速连续输入文字，观察 echo 延迟是否明显改善。

---

## Phase 5：阻塞 I/O 全面异步化

**目标：** 所有 `Command::output`、`Config::load`、`discover_worktrees` 等阻塞调用均不出现在主线程同步路径。

### Task 5.1：Config::load 缓存与异步

**文件：**
- 修改：`src/config.rs`（若存在）、以及所有调用 `Config::load()` 的站点

**方案：**
- 应用启动时在后台 `blocking::unblock` 中加载 config，结果缓存到 `Arc<Mutex<Option<Config>>>`
- 同步 `Config::load()` 改为先读缓存，若未就绪则返回 default 或等待（根据业务决定）
- 避免在点击回调中同步执行 `Config::load()`

---

### Task 5.2：tmux 命令优化

**文件：**
- 修改：`src/runtime/backends/tmux.rs`

**现状：** `ensure_session_and_window` 串行执行多个 `tmux` 命令。

**方案：**
- 合并可合并的逻辑，减少 `Command::output` 次数
- 将 `ensure_session_and_window` 的调用方改为在 `blocking::unblock` 中执行（Phase 2 已把 `create_runtime_from_env` 放入 unblock，tmux  backend 会间接受益）
- 若有重复的 `list-windows` 等查询，可加短期内存缓存（例如 1 秒内相同 session 不重复查询）

---

## 验证清单

完成各 Phase 后，逐项验证：

- [ ] 点击 worktree：侧边栏选中态立即更新，terminal 区域显示 loading，1~2 秒内完成切换
- [ ] 点击 new branch：对话框在 \<100ms 内弹出
- [ ] 点击通知 icon：面板在 \<100ms 内展开/收起
- [ ] 使用 `strace` 或 `eprintln` 确认：任意 UI 交互不再触发 `git worktree list`（除非显式 refresh）
- [ ] **terminal 快速输入**：字符 echo 无明显延迟（Task 4.3 完成后）
- [ ] `cargo test` 全绿
- [ ] 多 worktree、多 workspace 场景下功能正常

---

## 执行顺序建议

1. **Phase 1** 必做，收益最大，风险最低。
2. **Phase 2** 必做，解决 worktree 切换卡顿。
3. **Phase 3** 在 Phase 1 完成后通常已有明显改善；若仍不足再深化。
4. **Phase 4** 作为性能优化，可与 Phase 1~2 穿插。**Task 4.3（terminal 条件 notify）** 与 Phase 1 配合可显著改善输入流畅度，建议优先做。
5. **Phase 5** 视整体体验决定优先级。

---

## 参考

- `src/ui/app_root.rs`：主 UI 与 render 逻辑
- `src/worktree.rs`：`discover_worktrees` 实现
- `src/runtime/backends/mod.rs`、`tmux.rs`、`local_pty.rs`：runtime 创建
- `handle_add_workspace` 中的 `cx.spawn` 用法：异步任务范式
