# 主界面响应性设计

> **Brainstorming 产出**：针对 release 仍卡（hover 阴影延迟、点击迟滞、仅 Select Workspace 正常）的根因分析与方案。

---

## 1. 现象与根因

### 1.1 现象

| 现象 | 描述 |
|------|------|
| Hover 延迟 | worktree button、sidebar item 鼠标悬停阴影半天才出现 |
| 点击迟滞 | 所有 button、sidebar item 点击后明显延迟才触发动作 |
| 启动页正常 | Select Workspace 界面流畅，进入主界面后全面变慢 |

### 1.2 根因：主线程饱和

**Select Workspace 与主界面的本质差异：**

| 维度 | Select Workspace | 主界面 |
|------|------------------|--------|
| 渲染树 | 简单 div + 一个按钮 | Sidebar + TopBar + TerminalArea + 多个 Entity |
| 后台任务 | 无 | 终端输出循环、EventBus 订阅、status 检测 |
| 主线程负载 | 极低 | 高：频繁 update_entity、重 render、同步阻塞 |

**主线程被占用的三个来源：**

1. **点击回调内的同步阻塞**
   - `process_pending_worktree_selection` 内：`refresh_worktrees_for_repo()` → **同步执行 `git worktree list` 子进程**
   - `on_delete`：`refresh_worktrees_for_repo()` 同步
   - `on_close_orphan`：`kill_tmux_window()` 同步
   - 用户点击后，主线程在 git/tmux 命令返回前无法处理后续事件（包括 hover、其他点击）

2. **终端输出驱动的高频更新**
   - 每个 terminal 输出 chunk 触发 `cx.update_entity(terminal_area_entity, \|_, cx\| cx.notify())`
   - 有输出时（shell prompt、进度、甚至 idle 时的少量输出）更新频率可达数十次/秒
   - 每次 notify → TerminalAreaEntity render → `split_tree.clone()`、`terminal_buffers.lock()`、SplitPaneContainer 整树构建

3. **EventBus / status 更新**
   - AgentStateChange 时：`entity.update(app_root)` 或 `update_entity(status_counts_model)`
   - 若 `status_counts_model` 为 None，则更新 AppRoot → 整树重绘
   - 主界面树大，单次 render 成本高

**Hover 延迟的直接原因：** GPUI 在主线程处理输入（含 hover）。主线程忙于 (1) 处理排队中的 update_entity、(2) 执行重 render、(3) 被 sync 回调阻塞时，hover 事件排队，响应变慢。

---

## 2. 方案对比

### 方案 A：最小改动 —— 仅移除点击路径中的同步 I/O

**做法：**
- `process_pending_worktree_selection`：不在此处调用 `refresh_worktrees_for_repo`；改为使用已有 `cached_worktrees`，若 idx 越界则 fallback 到 spawn 中异步 refresh
- `on_delete`：点击时只设 `pending_delete_idx`，下一帧或 spawn 中再 `refresh_worktrees_for_repo` + 弹窗
- `on_close_orphan`：`kill_tmux_window` 放入 `cx.spawn` + `blocking::unblock`

**优点：** 改动小，风险低，立即缓解点击阻塞  
**缺点：** 不解决终端输出驱动的高频更新，hover 可能仍慢

---

### 方案 B：节流终端更新 + 移除同步 I/O（推荐）

**做法：**
- 方案 A 的全部内容
- 终端输出循环：仅在「有实际新内容」时 notify；或加时间节流（如最多 60 次/秒）
- 可选：`advance_bytes` / `process_output` 返回 `bool` 表示是否有新内容，无则跳过 `update_entity`

**优点：** 同时缓解点击阻塞和主线程被终端更新淹没  
**缺点：** 需确认 `process_output` 的语义，避免漏刷新

---

### 方案 C：架构级 —— Zed 风格 Entity 拆分

**做法：**
- 将 Sidebar 升格为独立 Entity，observe WorktreeListModel
- 点击 worktree 时只更新 Model，Sidebar 自身 notify，不触发 AppRoot 整树
- 终端输出仅 notify TerminalAreaEntity（已部分实现），确保不触发 AppRoot
- 方案 A + B 作为前置

**优点：** 从根本上减少整树重绘，与 Zed 一致  
**缺点：** 改动大，需 Model 抽取和订阅关系梳理

---

## 3. 推荐设计（方案 B）

### 3.1 Phase 1：点击路径零阻塞

**目标：** 所有用户点击的回调在 1ms 内返回，不执行任何子进程或阻塞 I/O。

| 回调 | 当前阻塞点 | 改动 |
|------|------------|------|
| `on_select` (worktree) | `refresh_worktrees_for_repo` | 使用 `cached_worktrees`；idx 越界时 spawn 异步 refresh 后重试 |
| `on_delete` | `refresh_worktrees_for_repo` | 先 `pending_delete_idx = Some(idx)` + `cx.notify()`；`on_next_frame` 或 spawn 中 refresh + 弹窗 |
| `on_close_orphan` | `kill_tmux_window` | `cx.spawn` + `blocking::unblock(|| kill_tmux_window(...))`，完成后 `refresh_sidebar` |
| `on_view_diff` | 若有 sync 逻辑 | 检查并移出主线程 |

**验收：** 点击 worktree / delete / orphan 后，主界面在 50ms 内响应（可主观感受或加简单计时）。

---

### 3.2 Phase 2：终端更新节流

**目标：** 终端输出不淹没主线程，hover 和点击保持流畅。

**选项 2a：内容变化才 notify**
- `process_output` 或等价接口返回「是否有新可见内容」
- 仅当有变化时 `update_entity(terminal_area_entity)`

**选项 2b：时间节流**
- 终端输出循环内：距上次 notify 不足 16ms 则跳过（约 60fps 上限）
- 简单，但可能略影响首字符 echo 延迟

**建议：** 先实现 2b（改动最小），若效果不足再考虑 2a。

---

### 3.3 Phase 3（可选）：Render 瘦身

- Sidebar：`pane_statuses.lock().unwrap().clone()` 改为 `Arc::clone` 或只读借用，避免整表 clone
- SplitPaneContainer：`split_tree` 改为 `Arc<SplitNode>` 共享，减少 clone
- 与 `2026-02-28-ui-performance-ultimate.md` Phase 4 对齐

---

## 4. 验证清单

- [ ] 点击 worktree：选中态立即更新，无卡顿感
- [ ] 点击 delete / close orphan：无主线程冻结
- [ ] Hover worktree button：阴影在 50ms 内出现
- [ ] Hover sidebar 其他按钮：同上
- [ ] 终端有输出时：hover、点击仍流畅
- [ ] `cargo run --release` 下全场景验证

---

## 5. 与现有文档的关系

- `2026-02-28-ui-performance-ultimate.md`：Phase 1（worktree 缓存）已做；Phase 2（异步切换）部分做；Phase 4 Task 4.3（终端条件 notify）与本设计 Phase 2 一致
- `2026-02-28-route-b-zed-style-entity-split-brainstorm.md`：长期架构方向，本设计 Phase 3 后可逐步推进

---

## 6. 小结

**根因：** 主线程被 (1) 点击回调内的同步 git/tmux、(2) 终端输出驱动的高频 notify、(3) 重 render 占用，导致 hover 和点击响应慢。

**推荐：** 方案 B —— 点击路径零阻塞 + 终端更新节流。先做 Phase 1 和 Phase 2，验证后再决定是否进入 Phase 3 或 Zed 风格 Entity 拆分。
