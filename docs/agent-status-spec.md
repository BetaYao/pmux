# Agent 状态规范 (Agent Status Specification)

> 基于需求澄清与 opencode / Claude Code 调研

## 1. 状态定义与映射

| 场景 | 目标状态 | 当前 AgentStatus | 说明 |
|------|----------|------------------|------|
| 1. 初始化 terminal | Wait | Idle / Unknown | ✓ 已支持 |
| 2. 长时间任务执行中 | In Progress | Running | ✓ 已支持 (OSC 133 C 或文本) |
| 3. 启动 claude code / opencode | Wait | Idle | agent 启动中，等待 ready |
| 4. agent 执行 prompt 中 | In Progress | Running | 依赖文本模式 |
| 5. **需要人工确认/批准** | **Wait Confirm** | **新增** | 权限/确认请求，需高优先级通知 |
| 6. prompt 执行结束 | Wait | Idle | ✓ 已支持 (OSC 133 D;0) |
| 7. 遇到错误 | Error | Error | ✓ 已支持 |

---

## 2. 新增状态：WaitingConfirm

**动机**：agent 请求权限或确认时，用户必须操作，需要与普通「等待输入」区分。

| 属性 | 值 |
|------|-----|
| 优先级 | 5（高于 Waiting=4，低于 Error=6） |
| is_urgent | true（触发通知） |
| 图标 | ◐ 或 ▲（建议与 Waiting 区分） |
| 颜色 | 橙色/琥珀色（介于 yellow 与 red 之间） |
| display_text | "Waiting for confirmation" |

---

## 3. 文本模式 (Text Patterns)

### 3.1 In Progress (Running) — 已有 + 扩展

**当前已有**（`status_detector.rs`）：
- `thinking|analyzing|processing`
- `writing|generating|creating`
- `running tool|executing|performing`
- `loading|downloading|uploading`
- `in progress|working on|busy`
- `esc to interrupt|^\s*>`

**建议扩展**（来自 opencode / Claude Code 调研）：
- `reasoning|streaming` — 模型推理中
- `Running tool:` — opencode 工具执行
- `process_list|process_stream` — opencode 后台进程相关（可选）
- `tool result|tool execution` — 工具结果输出中

### 3.2 Wait Confirm — 新增

**Claude Code**（[permissions docs](https://code.claude.com/docs/en/permissions)）：
- `requires approval|needs approval`
- `Accept|Reject|Edit|Cancel`（审批按钮）
- `Allow|Deny`（权限选项）
- `This command requires`
- `don't ask again`
- `permission to`

**OpenCode**（[permissions docs](https://opencode.ai/docs/permissions)）：
- `Always allow|Always deny`
- `allow|deny|ask`
- `permission`
- `approve|approval`
- `Run without asking`

**建议正则**（`confirm_patterns`，独立于 waiting_patterns）：
```regex
(?i)(requires approval|needs approval|permission to|don't ask again)
(?i)(Accept|Reject|Allow|Deny)\s+(all|this)
(?i)Always allow|Always deny
(?i)This command requires
(?i)approve|approval required
(?i)Run without asking
```

### 3.3 Waiting（普通等待）— 已有

- `^\?\s`、`^>\s`
- `human:|user:|awaiting input`
- `waiting for|ready for`
- `your turn|input required`

**区分**：Confirm 模式优先于 Waiting；若同时匹配，取 WaitingConfirm。

---

## 4. 检测优先级（更新后）

```
1. ProcessStatus (Exited/Error)     → 直接返回
2. confirm_patterns                 → WaitingConfirm (新增，最高文本优先级)
3. error_patterns                   → Error
4. OSC 133 shell_info               → Running / Waiting / Error
5. waiting_patterns                 → Waiting
6. running_patterns                 → Running
7. 非空内容                         → Idle
8. 空                               → Unknown
```

---

## 5. 实施清单

- [x] **agent_status.rs**：新增 `WaitingConfirm` 变体
- [x] **agent_status.rs**：更新 `StatusCounts`、`color()`、`icon()`、`display_text()`、`priority()`、`is_urgent()`
- [x] **status_detector.rs**：新增 `confirm_patterns: Vec<Regex>`
- [x] **status_detector.rs**：在 `detect_from_text()` 中，confirm 优先于 error（最高文本优先级）
- [x] **status_detector.rs**：添加 confirm 相关测试
- [x] **sidebar.rs / status_bar.rs**：为 WaitingConfirm 添加 UI（图标 ▲、颜色 #ff9800）
- [x] **notification**：WaitingConfirm 触发 `is_urgent` 通知

---

## 6. 参考资料

- [OpenCode Permissions](https://opencode.ai/docs/permissions) — allow/deny/ask
- [Claude Code Permissions](https://code.claude.com/docs/en/permissions) — approval flow
- [OpenCode TUI](https://opencode.ai/docs/tui) — /thinking, tool output
- [Claude Code awaiting input](https://github.com/anthropics/claude-code/issues/21238) — 即时通知需求
