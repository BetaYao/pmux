# Coordinator Agent & Mobile Remote Control Design

## 背景

pmux 是一个 macOS 原生终端多路复用器，能同时管理多个项目的 AI coding agent（Claude Code、Codex、OpenCode、Gemini CLI 等）。目前每个 agent 独立运行，互不感知。本设计引入一个协调 Agent（Coordinator Agent），统领所有子 Agent，并通过 MQTT 将数据透传到手机 APP，实现远程监控和指挥。

## 核心理念

**人 → 协调 Agent → 各项目 Agent**

人只跟协调 Agent 对话，不再直接管理每个项目的 Agent。协调 Agent 负责把意图翻译成具体指令，分发下去，监控进度，汇报结果。逐步实现自动化。

## 架构总览

```
人（Mac 或手机）
    ↓
协调 Agent（pmux 内右上角 AI 面板）
    ↓ 结构化数据（Hooks / ACP）
┌───┼───┬───┬───┐
v   v   v   v   v
A1  A2  A3  A4  A5   ← 各项目的 coding agent（终端 UI 保留不变）
    ↓ MQTT
手机 APP（远程仪表盘 + 遥控器）
```

## 设计决策

### 1. 协调 Agent 的位置

**决策：运行在 pmux 内部，复用现有的右上角 AI 面板。**

不需要新建面板或外部进程。协调 Agent 本身也是一个终端会话，只是角色不同。

### 2. 保留各 Agent 原生终端 UI

**决策：不自己画 UI，保留各厂家的原生终端 UI。**

pmux 继续用 Ghostty + tmux 渲染各个 Agent 的界面。协调 Agent 只在后台抓取结构化数据，不干扰用户对子 Agent 的直接操作。

### 3. 数据获取方式

**决策：Hooks 优先起步，ACP 作为演进方向。**

#### 方案一：Hooks（推荐先行）

Agent 正常跑在终端里，Hooks 在生命周期事件触发时推送结构化 JSON 给协调 Agent。

- Claude Code：18 种 hook 事件（PreToolUse、PostToolUse、Stop、SessionStart、SessionEnd 等）
- Codex CLI：hooks 系统（PR 中）
- Cline：PreToolUse / PostToolUse hooks
- 其他 Agent：各自的 hook 或 event 机制

优点：
- 不改现有架构，终端渲染完全不动
- 数据已经是结构化 JSON
- 现在就能用

缺点：
- 每个 Agent 的 Hook 机制不同，需要写不同的 hook 脚本
- 不是所有 Agent 都有完善的 hook 支持

#### 方案二：ACP（演进方向）

ACP（Agent Client Protocol）是 Zed 发起的开放标准，基于 JSON-RPC over stdio，已成为行业趋势。

主流 Agent 的 ACP 支持情况：

| 工具 | ACP 支持 |
|------|---------|
| Claude Code | ✅ Registry |
| Codex CLI | ✅ Registry |
| Gemini CLI | ✅ Registry |
| OpenCode | ✅ 原生 |
| Goose | ✅ 迁移中 |
| Cline | ✅ |
| Amp | ✅ adapter |
| Cursor | ✅ Registry |

ACP 的 `use_terminal` 模式允许 Agent 以 headless 方式运行，同时通过 `terminal/create` 请求 Client 分配嵌入式终端。这样结构化数据和终端 UI 可以共存。

推荐演进路径：
1. **阶段一**：用 Hooks 抓数据，快速验证协调 Agent 的核心逻辑
2. **阶段二**：实现 ACP Client，支持 ACP 的 Agent 走 ACP 通道，不支持的继续用 Hooks 兜底

### 4. 协调 Agent 获取的信息

协调 Agent 需要掌握每个子 Agent 的**全量消息**，而非仅状态摘要。

通过 Hooks / ACP 获取的结构化数据包括：
- **对话轮次**：用户输入、Agent 回复、思考过程
- **工具调用**：文件编辑、命令执行、搜索等
- **状态变化**：Running / Idle / Waiting / Error / Exited
- **阻塞信息**：等待用户确认、报错详情
- **代码 diff**：Agent 产出的代码修改

**架构影响**：协调 Agent 成为唯一的信息中心后，现有的 `StatusPublisher`（2 秒轮询）、`StatusDetector`（文本匹配）、`lastMessage` 汇总逻辑可以逐步移除，改由协调 Agent 统一提供状态信息。

### 5. 远程通信：MQTT

**决策：使用 MQTT 协议实现手机 APP 与 pmux 之间的实时通信。**

- pmux 作为 MQTT publisher，推送 Agent 状态、事件、对话
- 手机 APP 作为 subscriber，实时接收更新
- 手机也可以 publish，向协调 Agent 发送指令
- 云端 MQTT Server 作为中转

MQTT 特性优势：
- **Retained message**：手机重连立即获取最新状态
- **QoS 级别**：状态更新用 QoS 0，关键指令用 QoS 1
- **Last Will**：pmux 掉线手机立刻感知

Topic 设计示例：`pmux/{machine}/agents/{project}/status`

网络策略：**局域网优先，云端兜底**。局域网内低延迟直连，外网通过云端 MQTT Server 中转。

### 6. 手机 APP 定位

手机 APP 是**远程遥控器 + 仪表盘**，核心功能：
- 查看所有 Agent 的实时状态
- 给协调 Agent 下达高层指令
- 接收关键事件通知（Agent 卡住、任务完成等）

不需要在手机上写代码或看完整终端。

## 实施路径

### 第一步：协调 Agent（纯本地）
在 pmux 右上角 AI 面板运行协调 Agent，通过 Hooks 获取各子 Agent 的结构化数据，实现监控和指挥。这一步不涉及网络，坐在 Mac 前就能用。

### 第二步：MQTT 透传
pmux 接入 MQTT client，把协调 Agent 掌握的信息推到云端 MQTT Server。手机 APP 订阅 topic，实现远程查看和操控。

### 第三步：ACP 演进
实现 pmux ACP Client，支持 ACP 的 Agent 迁移到 ACP 通道获取更丰富的结构化数据。不支持 ACP 的 Agent 继续用 Hooks 兜底。

## 技术备注

- 目前没有官方 Swift ACP SDK，但协议是 JSON-RPC over stdio，Swift 实现不复杂
- MCP 已有 [Swift SDK](https://github.com/modelcontextprotocol/swift-sdk)，ACP 复用了很多 MCP 的消息格式定义
- pmux 从纯本地桌面应用演变为带服务端能力的系统（MQTT client）
- 云端 MQTT Server 可用现成服务，不需要自建
