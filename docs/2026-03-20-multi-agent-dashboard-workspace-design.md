# 多 Agent 协作工具交互设计（Dashboard + Project Workspace）

日期：2026-03-20  
阶段：低保真 IA + **HTML 高保真单文件原型**（作为 **macOS / Windows 原生 App** 的实现基线）  
范围：`Dashboard` 总控台、`Project Workspace` 工作区；视觉与交互细节以 `prototypes/dashboard-hifi-prototype.html` 为准，本文档已与该文件同步。

## 1. 设计目标

本设计用于一个类似视频会议软件视图切换逻辑的多 Agent 协作工具，第一版聚焦个人多线程开发场景，目标是：

- 在一个总控视图内管理多个 `project / thread / agent`
- 快速定位并处理 `Waiting` 状态 Agent
- 在放大视图中直接执行命令（真实 shell 直连）
- 在项目工作区中以 `worktree` 为核心切换并持续执行

## 2. 数据模型定义

### 2.1 Project

- `name`
- `path`（通常目录名可等于 name）
- `avatar`（可选）
- `color`（可选）

### 2.2 Thread（对应 worktree/branch）

- `name`（例如 `feature/123`）
- `path`

### 2.3 Agent

- `project`
- `thread`
- `status`：`Idle | Running | Waiting`
- `lastMessage`
- `totalDuration`
- `roundDuration`

## 3. 产品范围与定位

### 3.1 用户与场景

- 第一版目标用户：个人开发者
- 核心场景：一个人并行管理多个项目与多个 worktree 线程

### 3.2 MVP 范围

- `Dashboard`：跨项目总控与调度
- `Project Workspace`：单项目内 worktree 执行界面
- `Agent` 状态流转与动作联动
- 放大卡片中的内嵌 Terminal（真实 shell）

### 3.3 非目标（本版不做）

- 团队权限与多人协作机制
- 多 Agent 编排策略引擎
- 高保真品牌视觉系统
- 单 thread 多 terminal tab

## 4. 信息架构（IA）

- 一级页面：
  - `Dashboard`（跨项目总控）
  - `Project Workspace`（单项目深入操作）
- 路由关系：
  - 在 Dashboard 通过 `project` 入口进入 Workspace
  - 在 Workspace 可返回 Dashboard（保留当前布局与焦点上下文）

## 5. Dashboard 设计

## 5.1 页面目标

- 按状态组织 Agent，优先处理 `Waiting`
- 快速切换焦点 Agent 并直接执行操作
- 支持多种布局以适配不同调度偏好

## 5.2 顶层结构

- 视觉规范：
  - 交互风格对齐 Codex 工作台体验（极简、工具型、低干扰）
  - 完整遵循 macOS 平台习惯（traffic lights、紧凑标题栏、系统字体、克制边框与层级）
  - 版式采用沉浸式简洁风（减少卡片边框与分割线，增加留白，突出主任务区）
  - 保留 agent card 信息密度，但避免 “card 套 card” 的层级堆叠
  - 支持深色/浅色模式（系统跟随 + 手动切换）
- Title 栏（Zoom 风格）：
  - 左侧 traffic lights
  - traffic 后：`Dashboard` 为**常驻入口**（胶囊形 + 网格图标，无关闭），与可关闭的 `project` tab 用竖线分隔；其后为多个 `project` tab 及末尾 `+`
  - 每个 project tab：
    - 左侧状态点：绿色 `running`、蓝色 `waiting`、红色 `error`、灰色 `idle`
    - 右侧关闭 icon（`×`）用于关闭该 project tab
    - 点击关闭时弹窗二次确认，并提示“将停止该 Project 下所有进行中的任务”
  - 点击 `+`：弹窗输入系统目录路径，确认后新增 project tab
  - 右侧工具区（自左向右建议顺序）：`New Thread`（仅 Project 视图）、**视图**（布局菜单）、**通知**（打开右侧通知面板）、**AI 助手**（右侧侧滑聊天面板）、**主题**
    - 通知：点击打开自右侧滑入的面板，与 AI 面板共用半透明遮罩；可展示系统/Agent 消息列表
    - AI 助手：与通知同宽的右侧侧滑面板，内含可滚动消息区 + 底部输入区（Enter 发送，Shift+Enter 换行）；与通知**互斥**（同时只开一个），与其它浮层互斥（打开时收起布局菜单等）
    - Dashboard：视图 icon 弹出布局 popover
    - Project：`New Thread` 弹窗创建 thread
- 主区：
  - 渲染当前布局（4 选 1）
- 右侧或焦点详情能力（按布局承载）：
  - 默认会话优先
  - 上下文信息可切换查看
- 底部统计栏（建议保留）：
  - 展示当前状态信息（当前视图、焦点对象、当前 thread）
  - 展示快捷键提示（如布局切换、新建 thread、弹窗确认）
  - 作为全局状态栏，在 Dashboard 与 Project 视图均常驻显示
- **HTML 原型已实现的全部交互、尺寸、Token、浮层规则**：见 **§11**；**macOS / Windows 原生实现**：见 **§12**。

## 5.3 布局切换（手动）

Dashboard 提供 4 种布局，用户手动切换。入口为 Title 栏右侧的视图 icon，点击后打开 **popover 菜单**（类 macOS 菜单：项与项之间无独立描边，hover/选中为浅色底；整体仅外框 + 柔和阴影）：

1. Grid
2. 左边大 + 右边竖列小卡片
3. 上面小卡片 + 下面大卡片
4. 上面大卡片 + 下面小卡片

默认布局：`左边大 + 右边竖列小卡片`（减少同屏卡片数量，提升沉浸感）

约束：

- 切换只改变排布，不改变信息字段与交互语义
- 切换后保留：
  - 当前布局状态
  - 当前选中 Agent
  - 当前会话上下文状态

## 5.4 Agent 卡片规范（全布局统一）

- 主信息：
  - 标题统一为 `project.name - thread.name`
  - `status` 状态点
- 会话信息：
  - `lastMessage`（2-3 行显示，超出省略）
- 时间信息：
  - 紧凑展示：`Σ <totalDuration>` 与 `⟳ <roundDuration>`
- 动作：
  - 无卡片内动作按钮（卡片仅用于聚焦切换）

## 5.5 状态视觉与排序规则

### 视觉规则（与 HTML 原型 CSS token 一致）

- `Running`：绿色（`--running`）
- `Waiting`：蓝色（`--waiting`），列表中优先展示
- `Idle`：中性灰（`--idle`）
- `Error`：红色（`--error` / `--danger`）
- 当前焦点卡片 / 选中项：强调色混合边框 + 浅底（`--accent` 混合）

### 排序规则

- **产品目标**（供最终实现）：一级 `Waiting > Running > Idle`；二级在同状态内按等待时长 / 最近消息 / 最近活跃时间降序（见上文）。
- **HTML 原型当前实现**：一级 `Waiting > Running > Idle`；**同状态内不细分时间**，仅保持数据数组顺序经简单排序后的结果。

## 5.6 放大卡片（焦点卡）= Live Terminal Workspace

当卡片进入放大态时，主区域直接显示可操作 Terminal，而不是只读信息。

### 结构

- 顶栏：Agent 基本信息 + 状态 + 时长 + 连接状态；右上角放置 `进入 Project` icon 按钮（macOS 紧凑高度）
- 主区：Terminal 输出流（实时）
- 不单独提供底部输入栏；输入光标直接在 terminal 内容区末行

### Terminal 行为

- 真实 shell 直连
- 会话 `cwd` 绑定到当前 `thread.path`
- 单 Terminal 会话（不做 tab）
- 输出实时回显

### 会话隔离

- 每个 Agent（或其绑定 thread）保持独立终端上下文
- 切换焦点不会串改其他 thread 的命令历史或目录状态

## 5.7 Dashboard 关键交互流

### 流程 A：处理 Waiting

1. 用户在 Dashboard 发现 Waiting 卡片
2. 在 `Grid` 布局下点击卡片，直接进入对应 `project + thread`
3. 在其他布局下先进入焦点态，再通过右上角 `进入 Project` icon 跳转
4. 进入 Project 视图后在沉浸式 terminal 中继续处理

### 流程 B：多布局调度

1. 用户根据当前任务手动切换布局
2. 保持当前焦点 Agent 与布局上下文
3. 在新布局继续同一会话与命令执行

## 6. Project Workspace 设计

## 6.1 页面目标

- 在单项目维度内，以 worktree 为核心进行持续执行
- 减少信息干扰，强化命令操作效率

## 6.2 页面结构

- 独立 Project 视图（通过 Title 栏 project tab 进入）
- 左侧边栏：`Worktree List`（thread 列表）
- 右侧主区：`Immersive Terminal`（主内容区全部为 terminal）
- Project 视图不显示 `PMUX · <project> Workspace` 标题文案

## 6.3 左侧 Worktree List 规范

每项展示：

- `thread.name`（branch 名）
- `status`（Idle/Running/Waiting）
- `lastMessage`（最近消息，双行显示）
- 最近活跃时间（可选）

## 6.4 右侧 Terminal 主区规范

- 选中某个 worktree 后，右侧显示该 worktree 对应 Terminal
- Terminal 为真实 shell，`cwd = thread.path`
- 单会话，不支持多 tab（MVP）
- 右侧不显示额外标题栏（project/thread 信息由左侧列表承担）
- Project terminal 的视觉样式与 Dashboard terminal 完全一致（同一套颜色/字体/间距 token）

## 6.5 Worktree 切换规则

- 切换左侧 worktree 时：
  - 保留并恢复各自 Terminal 会话状态
  - 不重建新 shell
- 每个 worktree 的命令历史独立维护
- 输出滚动位置与上下文尽量恢复
- 从 Dashboard 点 `进入 Project` 时，自动定位到对应 project 视图和当前 thread
- 在 Project 视图点击 `New Thread` 后：
  - 弹窗为多行文本输入（每行一个 thread，如 `feature/123`）
  - 点击确认后批量加入左侧 thread 列表并自动选中首个新增项

## 7. 状态机与动作矩阵

## 7.1 状态机（简化）

- `Idle -> Running`：用户执行命令或触发继续
- `Running -> Waiting`：Agent 需要用户输入或确认
- `Waiting -> Running`：用户在工作区继续执行命令或消息交互
- `Running -> Idle`：任务结束且无活动

## 7.2 动作矩阵

- Idle：
  - 可执行命令
  - 可切换焦点
- Running：
  - 可继续观察输出
  - 可追加命令（按策略允许）
- Waiting：
  - 进入 Project（主动作）
  - 在工作区 terminal 中继续执行

## 8. 交互一致性原则

- 字段一致：四种布局卡片字段完全一致
- 语义一致：同名操作在所有布局行为一致
- 上下文一致：切换布局与切换页面尽可能保持用户当前任务上下文
- 状态优先：Waiting 在视觉与排序上始终优先

## 9. 后续实现建议（供前端开发）

- 先实现统一卡片数据协议，再做布局容器切换
- Terminal 区域抽象为独立组件，输入/输出/连接状态解耦
- 将状态流转逻辑放入统一状态机层，避免布局逻辑重复实现
- MVP 完成后再进入高保真视觉与动效阶段

## 10. Dashboard 四种布局线框草图说明（实现级）

本节用于指导前端按固定区域比例实现四种布局。所有布局共享同一组组件：Title 栏、卡片、焦点大卡（含 terminal）与统一动作区。

### 10.1 统一画布与基础尺寸

- 设计基准宽度：`1440px`
- 页面基础分区：
  - Title 栏：`40px`（紧凑，贴近 macOS 工具栏）
  - 主区：`calc(100vh - 40px - 32px)`（底部统计栏开启时）
  - 底部统计栏：`32px`
- 卡片最小宽度：`280px`
- 小卡建议高度：`150px`
- 焦点大卡最小高度：`520px`
- 区块间距：`12px`（容器内边距 `16px`）

### 10.2 布局 1：Grid（宫格）

#### 区域比例

- 主区宽度 `100%`
- 使用响应式网格：
  - `>=1600px`：4 列
  - `1200px~1599px`：3 列
  - `768px~1199px`：2 列
  - `<768px`：1 列

#### 组件摆放

- 每张 Agent 卡片等宽等高
- 每卡包含：
  - 头部（状态 + 标题）
  - 中部（project/thread + lastMessage）
  - 底部（时长）
- 点击卡片后：
  - 直接进入对应 `project` 视图并定位到该 `thread`

#### 适用场景

- 需要一次查看大量 agent 分布
- 更偏监控而非深度操作

### 10.3 布局 2：左大 + 右竖列小卡片（默认推荐）

#### 区域比例

- 主区左右分栏：
  - 左侧焦点区：`78%`
  - 右侧列表区：`22%`

#### 组件摆放

- 左侧焦点区：
  - 顶栏信息条：`48px`
  - 快捷动作栏：`40px`
  - Terminal 输出区：自适应填满
  - 命令输入栏：`44px`
- 右侧竖列区：
  - 小卡纵向排列，样式与上下布局小卡一致
  - 小卡固定比例 `16:9`
  - 默认展示 6~8 张，超出滚动
  - Waiting 卡优先置顶

#### 交互规则

- 点击右侧小卡，左侧焦点区切换到该 Agent
- 左侧 terminal 会话按 thread 恢复，不重建

### 10.4 布局 3：上小卡片 + 下大卡片

#### 区域比例

- 主区上下分区：
  - 上区（小卡横条）：`auto`（按小卡内容高度）
  - 下区（焦点大卡）：`1fr`（占满剩余空间）

#### 组件摆放

- 上区：
  - 横向滚动小卡片轨道（类似会议缩略卡）
  - 小卡固定比例 `16:9`
  - 支持鼠标滚轮横向滚动（实现可选）
- 下区：
  - 与布局 2 左侧焦点区结构相同（信息条 + 动作栏 + terminal + 输入）

#### 适用场景

- 会议式心智（上方人像条 + 下方主画面）
- 快速切换与深度执行并重

### 10.5 布局 4：上大卡片 + 下小卡片

#### 区域比例

- 主区上下分区：
  - 上区（焦点大卡）：`1fr`（占满剩余空间）
  - 下区（小卡池）：`auto`（按小卡内容高度）

#### 组件摆放

- 上区：
  - 结构同布局 2 焦点区
  - 强调 terminal 可视高度，建议最小 `500px`
- 下区：
  - 小卡池为横向缩略卡，固定比例 `16:9`
  - 默认按状态分组展示（Waiting 组在前）

#### 适用场景

- 长时间盯住一个主任务
- 次要任务在底部待命

### 10.6 焦点大卡（四布局共用内部线框）

焦点大卡内部统一采用三层结构：

1. 元信息层（`42px`）  
   `Agent 名`、状态点、`project/thread`、时长、连接状态；右上角 `进入 Project` icon
2. Terminal 输出层（弹性高度）  
   实时日志、命令回显、错误提示
3. 输入光标层（嵌入 terminal 末行）  
   不额外占据独立布局高度

**与 HTML 原型对齐说明**：原型中焦点区为 `grid-template-rows: 42px 1fr`（顶栏 + Terminal 填满），**未**单独实现 §10.3 线框中的 `44px` 命令输入栏行；Terminal 末行用闪烁光标块模拟「光标在内容区末行」。原生 App 可按产品线框增加独立输入行，或继续采用沉浸式末行输入。

### 10.7 响应式断点与降级策略

- `>=1200px`：完整布局（四种全可用）
- `900px~1199px`：
  - 布局 2 的右列降为抽屉式侧栏
  - 布局 3/4 保持上下结构
- `<900px`：
  - 强制单焦点模式（仅显示大卡）
  - 小卡列表改为可折叠面板
- `<640px`（移动端预留）：
  - 仅保留核心能力：布局切换 + 焦点 terminal + 进入 Project

### 10.8 组件映射（前端实现建议）

- `LayoutSwitcher`：控制布局枚举值
- `AgentCard`：小卡组件（四布局复用）
- `FocusAgentPanel`：焦点大卡组件（四布局复用）
- `TerminalView`：输出区组件
- `TerminalInput`：输入区组件
- `WorktreeContextBadge`：显示 `project/thread/path` 的轻量标签组

以上映射用于确保四种布局仅更换容器，不复制业务逻辑。

## 11. HTML 原型 → 文档同步（实现基线）

**源文件**：`prototypes/dashboard-hifi-prototype.html`（单文件 HTML + CSS + JS）。  
以下与源码一致，供 **macOS / Windows 原生 App** 复刻交互与视觉；平台差异见 **§12**。

### 11.1 应用外壳与分区

- 根布局：CSS Grid，三行 **`40px`（Title） / `1fr`（主区） / `32px`（底栏）**，`height: 100vh`。
- 主区：`main.main` 内边距 **`10px`**，`overflow: hidden`。
- Dashboard / Project：`section.view` 互斥 `.active` 切换显示。
- **视图相关显隐**：
  - **Dashboard**：`#layout-switch` `visibility: visible`；`#project-actions` `display: none`。
  - **Project**：`#layout-switch` `visibility: hidden`；`#project-actions` `display: flex`（`New Thread` 可见）。

### 11.2 设计 Token（语义色，深浅 + 系统）

原型默认深色 `:root`；`[data-theme="light"]` 强制浅色；**无 `data-theme` 时**浅色由 `@media (prefers-color-scheme: light)` 作用于 `:root:not([data-theme])`（系统跟随）。

| Token | 语义 |
|--------|------|
| `--bg` | 页面背景 |
| `--panel` / `--panel-2` | 面板层级 |
| `--text` / `--muted` | 正文与次要文字 |
| `--line` | 边框、分割线 |
| `--accent` | 强调色（选中 tab、链接感控件） |
| `--running` / `--waiting` / `--idle` | Agent/thread 状态 |
| `--danger` / `--error` | 角标、错误状态 |

**主题按钮循环**（点击 `#theme-toggle`）：当前 **浅色** → 切 **深色**；当前 **深色** → 移除 `data-theme`（**系统**）；无属性 → **浅色**。底栏文案会随切换更新（演示用）。

### 11.3 字体与等宽

- **UI**：`-apple-system, BlinkMacSystemFont, "SF Pro Text", "SF Pro Display", sans-serif`（Windows 原生实现时替换为系统 UI 字体，如 Segoe UI Variable）。
- **Terminal**：`ui-monospace, "SF Mono", Menlo, Monaco, "JetBrains Mono", monospace`。

### 11.4 Title 栏（结构与度量）

- 容器：flex，`padding: 0 10px`，`gap: 12px`，底边 **`1px`** 分割线；背景为 `--panel` 半透明混合 + **`backdrop-filter: blur(14px)`**。
- **左侧 `tb-left`**：`traffic`（三圆点装饰，**非可点**，颜色接近 macOS 红/黄/绿）+ `tabs` 区域。
- **Tabs**：
  - **Dashboard**：class `tab tab-dashboard`，**胶囊圆角**，内含 **16px 网格 SVG** + 文案 `Dashboard`，**无关闭**；`active` 时 accent 混合底与边框。
  - **分隔**：`tab-separator`，宽 `1px`、高 `18px`。
  - **Project tab**：左侧 **`tab-dot`**（`8px` 圆，running / waiting / error / idle）；文案为 project 名；右侧 **`tab-close`**（`×`，点击 **stopPropagation**）。
  - **`+` tab**：`tab add`，用于添加 Project。
- **Project tab 聚合状态**（用于圆点颜色）：该 project 下 threads 若含 `error` → error；否则若含 `waiting` → waiting；否则若含 `running` → running；否则 idle。
- **右侧 `tb-right`**：`margin-left: auto`，flex，`gap: 8px`，**从左到右顺序**：
  1. `New Thread`（`layout-btn`，仅 Project 视图显示）
  2. **视图**（`icon-btn` `32×32`，圆角 `10px`，内 SVG `18×18`）
  3. **通知**（同上 + `icon-badge`：右上 **`14px` 高**圆角条，红底白字，**pointer-events: none**）
  4. **AI 助手**
  5. **主题**（`theme-toggle`，`32×32` 最小点击区）
- **图标按钮**：无边框、透明底；**hover** 为 `--line` **22%** 混合背景。
- **`New Thread`**：带边框 `layout-btn`（与纯图标区分）。

### 11.5 布局菜单（Layout popover）

- 锚点：`layout-switch` `position: relative`；菜单 **`position: absolute`**，`top: calc(100% + 4px)`，`right: 0`。
- 尺寸：宽 **`200px`**，`padding: 4px`，圆角 **`8px`**，外框 + 阴影（与 macOS 菜单风格一致：项间 **`gap: 1px`**，**无行与行之间独立描边**）。
- 四项（`layout-item`）：`1 Grid` / `2 左大右列` / `3 上小下大` / `4 上大下小`，对应 `data-layout`：`grid` / `left-right` / `top-small` / `top-large`。
- **默认选中**：`左大右列`（`left-right`）。
- **交互**：点击一项 → 更新 `active` 样式、关闭菜单、刷新布局；**点视图按钮** → 若侧栏未开则 **toggle** 菜单，且 **关闭通知 + AI**；**document 点击外部**（非菜单、非视图按钮）关闭菜单。
- **与其它浮层**：打开 **通知 / AI / 弹窗** 时会 **remove** 菜单 `open`。

### 11.6 通知与 AI 侧栏（共用遮罩）

- **遮罩** `#notif-backdrop`：`fixed inset:0`，`rgba(0,0,0,0.2)`，**z-index 38**，默认 `display:none`；任一侧栏打开时 `display:block`。
- **面板** `#notif-panel` / `#ai-panel`：`fixed`，**`top: 40px`**，**`right: 0`**，**宽 `min(360px, 100vw)`**，**高 `calc(100vh - 40px - 32px)`**（为底栏留空）；左边框 + 左侧阴影；**z-index 40**；默认 **`transform: translateX(100%)`**，`.open` 时 **`translateX(0)`**，**`transition: transform 0.22s ease`**。
- **互斥**：打开其一会关闭另一；**aria-expanded / aria-hidden** 在按钮与面板上联动。
- **通知**：标题行 **13px 粗体** + 关闭（复用 `notif-panel-close`）；列表 `notif-list` 可滚动，项为 **`notif-item`**（圆角 `8px`，**无边框**，底 `--panel-2`）；每项 **标题 12px 半粗 + meta 11px muted**（原型为静态示例文案）。
- **关闭**：面板内 **×**、**点击遮罩**（同时关两个）、**document 点击**在面板外且不在对应铃铛/AI 包裹内时关闭对应面板。
- **AI**：标题行同通知；**消息区**可滚动，**用户气泡**右对齐 accent 混合底，**助手**左对齐 `--panel-2`；**底部输入行**：`textarea`（**Enter 发送**，**Shift+Enter 换行**）+ **发送**按钮；打开时 **focus** 到 textarea；发送后原型 **~450ms** 追加一条助手气泡（占位逻辑）。

### 11.7 全局弹窗（Modal）

- **容器**：`#input-modal`，`fixed inset:0`，背景 **`rgba(7, 10, 20, 0.72)`**，**z-index 50**，`open` 时 flex 居中。
- **卡片**：`min(560px, 100vw - 32px)`，圆角 `10px`，内边距 `16px`，栅格间距 `12px`；标题 **16px 粗**，副文案 **12px muted**。
- **输入**：单行 `input` 或 **`textarea.modal-input.multiline`**（最小高 `108px`，**可垂直 resize**，等宽字体 JetBrains Mono）。
- **按钮**：取消 + 主按钮（文案可配置，如「确认关闭」「添加」「创建」）。
- **打开弹窗时**：关闭 layout 菜单、通知、AI。
- **键盘**：单行 **`Enter` → 确认**；多行 **`⌘/Ctrl + Enter` → 确认**；**`Escape` → 关闭**（由动态绑定的 keydown 处理）。
- **用途（与文案）**：
  - **关闭 Project**：标题「关闭 Project」，副文说明将停止进行中的任务；确认后从列表移除 project，若当前正在该 project 则回 **Dashboard**。
  - **添加 Project**：副文要求系统目录路径；确认后取路径 **最后一段** 为 project 名（若无则用 `project N`），加入列表并 **进入 Project 视图**。
  - **New Thread**：多行，每行一个 thread；过滤空行、去重已有 thread；批量追加（idle + 占位 lastMessage）；**选中首行对应 thread**（与实现一致：`selectedThreadByProject` 设为 `lines[0]`）。

### 11.8 Dashboard：四种布局（DOM 与比例）

- 布局容器均为 `display: grid`，**仅当前布局 `.active`**。
- **Grid**（`layout-grid`）：**`repeat(3, minmax(0,1fr))`**，`align-content: start`，整体可纵向滚动；**视口 `≤1150px`**（`@media (max-width: 1150px)`）降为 **2 列**。
- **左大右列**（`layout-left-right`）：**`78%` / `22%`**；**`max-width: 1150px`** 时改为 **单列 + 下行 `280px` 列表区**（`grid-template-rows: 1fr 280px`）。
- **上小下大** / **上大下小**：行分别为 **`auto 1fr`** / **`1fr auto`**；小卡轨道 **`small-row`**：`grid-auto-flow: column`，单列宽 **`clamp(180px, 18vw, 260px)`**，横向滚动，`gap: 6px`。
- **小卡（mini-card）**：**`aspect-ratio: 16/9`**，圆角 `9px`，内 **三行栅格**（标题 / 消息区两行 clamp / 底时间）。
- **Agent 卡片（Grid 内 `.card`）**：与列表小卡区分——**实心 `--panel-2` 底 + 更明显边框**；非 Grid 布局中卡片可为透明底、hover 才显底（见源码 `.card` vs `.layout-grid .card`）。
- **卡片内容**：行1 状态点 + 标题 **`{project} - {thread}`**；**`lastMessage` 最多 3 行** clamp；底 **`Σ` 总时长 · `⟳` 轮次时长**（原型用 `compactDuration`：`xhxxm` / `xmxxs` / `xs`）。
- **排序**：`sortedAgents()` — **waiting → running → 其它**。
- **点击行为**：
  - **Grid + Dashboard**：点击卡片 → **直接进入 Project**，project/thread 取自该 Agent，并更新 `selectedThreadByProject`。
  - **其它布局 + Dashboard**：点击卡片/小卡 → 仅 **切换 `selectedAgentId`** 并刷新布局。
- **焦点大卡**：顶栏 **`42px`**（状态点、Agent 名、meta、Total/Round）+ **右上角「进入 Project」** `enter-project-icon`（`28×28`，无边框，hover 同 icon-btn）；**Terminal 区**填满剩余高度；点击 icon **stopPropagation**，进入该 Agent 的 **project** 视图（**不自动改 thread** 为当前 Agent，与 Grid 跳转行为不同——以原型为准）。

### 11.9 Project 工作区

- **栅格** `project-shell`：**`300px` + `1fr`**，`gap: 12px`，高度 `100%`；**`max-width: 900px`** 时改为 **上下栈叠**：首行 **`220px`** thread 区，次行 terminal。
- **左侧 `thread-list`**：`padding: 6px`，子项间距 `4px`，可滚动。
- **Thread 项**：**无列表项粗边框**；`active` 时 accent 混合边框 + 浅底；展示 **thread 名 + 状态点 + `lastMessage` 两行 clamp**（**不展示 path**）。
- **空列表**：文案提示点击标题栏 **New Thread**。
- **右侧**：`terminal-immersive` 包裹与 Dashboard 同类的 **`.terminal`**（全高、同色 token），末行 **闪烁光标**；切换 thread 仅 **替换 terminal 内 mock 内容**，逻辑上表达「会话恢复」。

### 11.10 底栏（Bottom status）

- 高 **`32px`**，顶边分割，**`padding: 0 10px`**，**11px** 字，**muted 混合色**；flex 两端对齐。
- **左侧** `#status-summary`：Dashboard 为 `Status: Dashboard ready · Focus {AgentName}`；Project 为 `Status: {project} active · Thread {thread}`；主题切换等操作会 **临时改写** 该文案（演示）。
- **右侧**：静态 **hint** 胶囊（细边框），文案含 **`V`**（布局）、**`N`**（新建 Thread）、**`⌘+Enter`**（弹窗确认）。**说明**：当前 HTML **未**全局监听 `V`/`N` 键，仅作 **目标快捷键/文案占位**；原生 App 应实现真实快捷键并与此处文案一致。

### 11.11 层级（z-index）与焦点可见

- Layout 菜单：**30**；侧栏遮罩：**38**；侧栏面板：**40**；弹窗：**50**。
- `layout-item:focus-visible`：**2px** outline，accent 混合。

### 11.12 与 §10 线框的差异摘要

| 项目 | §10 线框 | HTML 原型 |
|------|-----------|-----------|
| 焦点区行高 | 信息条 48px、动作栏 40px、输入 44px 等 | **仅 42px 顶栏 + Terminal** |
| Grid 断点与列数 | 多档视口 | **固定 3 列，≤1150px 为 2 列** |
| Agent 排序二级键 | 按时间细分 | **未实现** |
| 底栏快捷键 | 提示 | **未绑定全局热键** |

---

## 12. 原生应用实现注意（macOS / Windows）

- **交互与信息架构**：与 **§11** 保持一致即可视为同一产品；不必再依赖浏览器语义。
- **窗口 chrome**：macOS 使用真实交通灯与原生全尺寸标题栏时，可将 **§11.4** 中装饰性三圆点 **替换为系统控件**；Windows 使用 **最小化/最大化/关闭** 与 **Segoe UI** 体系，**内容区布局（40px 工具条 + 主区 + 32px 状态栏）可保留** 或按平台 HIG 微调高度。
- **字体**：macOS 继续 SF；Windows **Segoe UI Variable** + **Cascadia Mono / Consolas**（Terminal）等。
- **主题**：保留 **浅色 / 深色 / 系统** 三态；Windows 对接 **系统主题变更**（如注册表 `AppsUseLightTheme` / WinRT `UISettings` 等，按技术栈选型）。
- **侧栏与弹窗**：建议仍用 **滑入动画 ~220ms**、**共用遮罩**、**互斥浮层** 规则，以保证与原型一致的手感。
- **可访问性**：侧栏按钮维护 **`aria-expanded` / `aria-controls`**；弹窗迁移到原生后补齐 **焦点陷阱** 与 **ESC 关闭**。

### 12.1 文档与原型维护约定

- 以后若只改 HTML，请 **同步更新 §11**（及必要时 **§12**、**§10 差异表**），避免原生实现与原型漂移。
