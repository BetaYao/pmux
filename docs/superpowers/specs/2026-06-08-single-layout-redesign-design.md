# AMUX 单一布局改版设计

**日期**: 2026-06-08
**状态**: 设计待评审

## 背景与目标

把现有 4 布局(Grid / LeftRight / TopSmall / TopLarge)的仪表盘收敛成**单一左右布局**(左焦点面板 + 右竖排卡片栏),并围绕它重做顶栏、卡片、底部状态栏和 worktree 创建入口。目标是更稳定的信息层级:顶栏聚焦"当前 worktree",底栏承载全局用量与快捷键,侧栏卡片信息精简到一眼可读。

参考线框图(用户提供):顶部长胶囊(标题 + token)、左大焦点面板、右侧竖排精简卡片、右下角内联创建输入、底部全局 status bar。

## 改动总览(6 块)

| # | 模块 | 性质 | 主要文件 |
|---|------|------|----------|
| 1 | 收敛成单一 LeftRight 布局 | 删代码(大头) | `DashboardViewController`、`TitleBarView`、`Config`、`DashboardLayout` |
| 2 | 顶栏胶囊重定义为 worktree 标题 + token | 改 + 净新增(读 session 标题) | `TitleBarView`、新建 session 标题读取器 |
| 3 | 底部全局 status bar | 净新增 | `MainWindowController`、新建 `StatusBarView` |
| 4 | 小卡片缩成固定 3–4 行 | 重构 | `MiniCardView` |
| 5 | 右侧栏底部内联创建 worktree | 改 + 替换弹窗 | 新建内联输入视图、`WorktreeCreator`、`StackedMiniCardContainerView` |
| 6 | "复用当前 worktree 环境"开关 | 净新增 | `WorktreeCreator` |

---

## 1. 收敛成单一布局

放弃 `Grid / TopSmall / TopLarge`,只保留 **LeftRight**:左焦点面板(`FocusPanelView` 承载 `SplitContainerView`)+ 右竖排可滚动卡片栏。

**改动**:
- `TitleBarView`:移除 `gridLayoutButton / leftLayoutButton / topSmallLayoutButton / topLargeLayoutButton` 及 `layoutButtons`、`titleBarDidSelectLayout` 委托方法。
- `DashboardViewController`(~54KB):移除 Grid / TopSmall / TopLarge 三套排布代码路径,只保留 LeftRight 的布局函数;`DashboardLayout` 枚举与切换逻辑删除或退化为常量。
- `Config`:移除 `layout` 字段的读写(用 `decodeIfPresent` 向后兼容旧配置——读到旧字段直接忽略)。
- 清理仅服务于布局切换的快捷键。

**风险**:这是删代码的大头,回归风险集中于此(焦点面板/侧栏的尺寸约束、持久化、selectedWorktree 恢复)。需保证现有 LeftRight 行为零回归。

## 2. 顶栏胶囊重定义

现有左侧轮播用量胶囊(`primaryCapsuleStack` 一系列 label + 进度条)重定义为:

- **左**:当前聚焦 worktree 的**标题**。
- **右**:该 worktree 的 **token 用量**。

**标题数据源(净新增读取器)**:
- Claude:读取 `~/.claude/projects/<encoded-cwd>/<sessionId>.jsonl`,解析其中 `type == "summary"` 的记录取 `summary` 字段(Claude 自动生成的会话标题)。`<encoded-cwd>` 为 worktree 路径按 Claude 约定编码(`/` → `-`)。
- Codex:无等价 summary,回退到现有 `AgentInfo.lastUserPrompt`(已有 `CodexSessionPromptLookup` 基础设施)。
- 取不到任何标题时回退到 worktree/分支名。
- 新建 `SessionTitleLookup`(放 `Sources/Core/`),与 `CodexSessionPromptLookup` 同构,按 sessionId/worktreePath 查标题;在后台队列读取,主线程更新顶栏。

**token 用量**:per-worktree 用量目前**无法获取**(`UsageSummaryStore` 只有 app 全局 session %)。本期**占位**:右侧显示 `—` 或灰态 placeholder,预留接口待后续接入按 session 的 token 统计。

**右侧按钮**:
- 删除"添加 repo"(`addProjectButton`)、"添加 worktree"(`newWorktreeButton`)——入口移到第 5 块的内联输入。
- **保留**主题切换(`themeButton`)、隐藏侧边栏(`collapseSidebarButton`)——即线框图右上角两个方块。

## 3. 底部全局 status bar(净新增)

窗口底部新增常驻一条 bar(新建 `StatusBarView`,由 `MainWindowController` 在内容容器下方布局,占固定高度):

- **左/中**:Claude / Codex **套餐余量**——复用现有 `UsageSummaryStore` / `UsageSummaryFormatter` 的全局用量数据(从顶栏挪下来)。
- **右**:高频快捷键提示。
- **通知**:原顶栏的通知角标/未读提示挪到此 bar(`updateNotificationSummary` 的展示目标改为 status bar)。

`MainWindowController` 现有布局把 `contentContainer` 贴到窗口底边(`bottomAnchor`),需改为给 status bar 让出固定高度。

## 4. 小卡片缩成固定 3–4 行

`MiniCardView` 重构,放弃内部多行滚动内容:

- **第 1–2 行**:标题(同第 2 块数据源,最多折 2 行,尾部截断)。
- **第 3 行**:**状态 + 时长**(复用现有 status dots/text 与 duration)。
- **第 4 行**:**repo + worktree 名**。
- 移除现有的 user prompt / message / task / activity 多行区域。
- 卡片**高度随标题 1 或 2 行变化**(放弃 16:9 / 128pt 固定缩略图尺寸);侧栏改为按内容高度竖排 + 整体滚动。

## 5. 右侧栏底部内联创建 worktree(替代弹窗)

侧栏底部 **sticky** 一个创建输入(新建视图,嵌入 `StackedMiniCardContainerView` 或其容器底部):

- **默认 1 行**:worktree 名输入框(校验无空格,同现有 `NewBranchDialog` 规则)。
- **聚焦后展开 2 行**,第二行:
  - **repo 下拉**(数据源 `config.workspacePaths`,同现有弹窗)。
  - **"复用当前 worktree 环境"开关**(见第 6 块)。
- **base 分支**:不提供选择,默认 `main`/`master`(`WorktreeCreator.listBranches` 已有默认逻辑)。
- **回车提交** → 复用 `WorktreeCreator.createWorktree()`,后续走现有 `TabCoordinator.handleNewBranch` → `integrateNewWorktrees` → 聚焦新 worktree。
- 失焦且无输入时收回到 1 行。
- **移除 `NewBranchDialog` 弹窗入口**(Cmd+N 改为聚焦内联输入,或后续决定;弹窗代码可保留备用或删除——实现时确认)。

## 6. "复用当前 worktree 环境"开关(净新增)

现状:`WorktreeCreator.createWorktree()` 只做 `git worktree add -b`,无任何文件拷贝。

**本期定义**:开关打开时,从**当前聚焦 worktree**(`TabCoordinator.selectedAgent?.worktreePath`)拷贝**环境文件**到新建 worktree:
- 拷贝匹配 `.env`、`.env.*`、`.envrc` 的文件(根目录层级;是否递归子目录在实现时按最简单可用的方案,默认仅根目录)。
- **不**拷 `node_modules` / build 产物。
- 拷贝在 `git worktree add` 成功后、集成进 UI 前执行;拷贝失败不阻断 worktree 创建(记录警告)。

---

## 数据流

```
聚焦切换 → TabCoordinator.selectedAgent.worktreePath
  → SessionTitleLookup(后台读 jsonl summary / codex prompt)
  → 顶栏胶囊标题更新(主线程)；token 区占位

UsageSummaryStore(已有,2s 轮询)
  → 顶栏 ✗(移除)→ 底部 StatusBarView(套餐余量)

AgentHead / WorktreeStatusAggregator(已有)
  → MiniCardView(标题/状态/时长/repo+worktree)

内联输入回车
  → WorktreeCreator.createWorktree(name, repo, base=main/master)
  → [开关开] 拷贝 .env* / .envrc from selectedAgent.worktreePath
  → TabCoordinator.handleNewBranch → 聚焦新 worktree
```

## 单元边界

- `SessionTitleLookup`:输入 sessionId/worktreePath/provider,输出标题字符串(可空)。纯读取,无 UI 依赖,可独立测试。
- `StatusBarView`:输入用量帧 + 通知摘要,纯展示。
- `MiniCardView`:输入 `AgentDisplayInfo`,输出固定 3–4 行卡片。
- 内联创建视图:输入 repoPaths + 当前 worktree path,输出 (name, repo, reuseEnv) 创建请求。
- 环境拷贝:`WorktreeCreator` 内新增方法,输入源/目标路径,拷贝匹配文件。

## 测试

- `SessionTitleLookup`:构造临时 jsonl(含/不含 summary 记录),验证解析与回退顺序(summary → lastUserPrompt → 分支名)。
- 环境拷贝:临时目录放 `.env` / `.envrc` / `node_modules`,验证只拷环境文件、不拷 node_modules、缺失文件不报错。
- `MiniCardView`:不同标题长度(0/1/2 行)下的高度与截断。
- 内联输入:聚焦展开/收回、空名校验、提交回调参数正确。
- 布局收敛:LeftRight 现有行为回归(尺寸约束、selectedWorktree 恢复、隐藏侧边栏)。

## 风险与开放项

1. **布局删除的回归面**最大——优先保证 LeftRight 零回归。
2. **per-worktree token** 本期仅占位,无真实数据。
3. **Claude jsonl 路径编码**:需确认 `<encoded-cwd>` 规则(`/`→`-`)在当前 Claude 版本下成立;读取要容错(文件不存在/格式变化)。
4. **环境拷贝递归范围**:默认仅根目录 `.env*`/`.envrc`;若项目把环境文件放子目录,本期不覆盖。
5. **Cmd+N 行为**:弹窗移除后 Cmd+N 改为聚焦内联输入(实现时确认是否保留弹窗代码)。
