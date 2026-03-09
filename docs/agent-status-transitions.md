# Agent 状态变化规则

## 状态定义

| 状态 | 图标 | 含义 |
|------|------|------|
| **Running** | ● 绿 | Agent 正在输出 token 或执行工具 |
| **Waiting** | ◐ 黄 | Agent 阻塞，需要人工确认才能继续 |
| **Idle** | ○ 灰 | 任务完成，等待下一条指令 |
| **Error** | ✕ 红 | 出错（非零退出码或明确错误输出） |
| **Exited** | ✓ 蓝 | 进程已退出（exit 0） |
| **Unknown** | ? 紫 | 无法判断（初始状态 / 无数据） |

---

## 状态转换图

```
                ┌──────────────────────────────────────┐
                │                                      │
                │         ┌─────────┐                  │
                │    ┌───▶│ Running │───┐              │
                │    │    └─────────┘   │              │
                │    │         │        │              │
                │    │         │        ▼              │
  ┌─────────┐   │    │         │   ┌─────────┐   ┌─────────┐
  │ Unknown │───┼────┤         │   │  Error  │   │ Exited  │
  └─────────┘   │    │         │   └─────────┘   └─────────┘
                │    │         │        ▲
                │    │         ▼        │
                │    │    ┌─────────┐   │
                │    └────│  Idle   │───┘
                │         └─────────┘
                │              ▲
                │              │
                │         ┌─────────┐
                └────────▶│ Waiting │
                          └─────────┘
```

### 合法转换路径

| 从 \ 到 | Running | Waiting | Idle | Error | Exited |
|---------|---------|---------|------|-------|--------|
| **Unknown** | ✓ 首次检测到活动 | ✓ 首次就在确认 | ✓ 首次就在 prompt | ✓ 启动即出错 | ✓ 启动即退出 |
| **Running** | — | ✓ 执行中弹出确认 | ✓ 任务完成 | ✓ 执行出错 | ✓ 进程退出 |
| **Waiting** | ✓ 用户确认后恢复 | — | ✓ 用户取消/跳过 | ✓ 确认过程出错 | ✓ 进程退出 |
| **Idle** | ✓ 用户下达新指令 | ✓ 新指令触发确认 | — | ✓ 出现错误 | ✓ 进程退出 |
| **Error** | ✓ 用户重试 | ✓ 重试触发确认 | ✓ 回到 prompt | — | ✓ 进程退出 |
| **Exited** | ✗ 终态 | ✗ 终态 | ✗ 终态 | ✗ 终态 | — |

> **Exited 是终态**，进程已退出不可恢复。需要用户重新启动。

---

## 通知规则

### 何时触发通知

| 转换 | 是否通知 | 通知类型 | 说明 |
|------|---------|---------|------|
| Running → **Idle** | ✅ 通知 | Info | 任务完成 |
| Running → **Error** | ✅ 通知 | Error | 任务失败 |
| * → **Waiting** | ✅ 立即通知 | Waiting | 需要人工介入 |
| * → **Exited** | ✅ 通知 | Info | 进程退出 |
| 其他转换 | ❌ 不通知 | — | 正常流转 |

### 不触发通知的转换

- **Idle → Running**：用户主动下达指令，不需要通知
- **Unknown → Running**：首次启动检测到活动，不需要通知
- **Waiting → Running**：用户刚确认完，不需要通知
- **Error → Running**：用户重试中，不需要通知
- **Error → Idle**：错误恢复回到 prompt，不需要通知

### 通知去重（Debounce）

状态检测使用 `DebouncedStatusTracker` 防止抖动：

- **Error / Exited / Waiting**：立即生效，跳过 debounce（紧急状态）
- **Running / Idle / Unknown**：需要连续检测到 2 次相同状态才确认变化

这避免了以下误报：
- Agent 输出中短暂出现 "error" 文本但实际仍在运行
- Agent 完成任务后立即开始下一步，短暂闪现 Idle

---

## 检测优先级

状态检测分三层，高层覆盖低层：

```
优先级 1：进程生命周期（最高）
    进程退出 exit 0  → Exited
    进程退出 exit≠0  → Error

优先级 2：OSC 133 Shell 标记
    ShellPhase::Running  → Running（命令执行中）
    ShellPhase::Input    → Idle（shell prompt 可见）
    ShellPhase::Prompt   → Idle（shell prompt 可见）
    ShellPhase::Output + exit≠0  → Error  ⚠️ 当前未生效，exit code 始终为 None
    ShellPhase::Output + exit=0  → Idle   ⚠️ 同上，fallthrough 到文本检测

优先级 3：文本模式匹配（fallback）
    CONFIRM_PATTERNS  → Waiting（最高文本优先级）
    IDLE_PATTERNS     → Idle（prompt 在最后 3 行）
    ERROR_PATTERNS    → Error
    WAITING_PATTERNS  → Waiting
    RUNNING_PATTERNS  → Running
    非空文本无匹配    → Idle（fallback）
    空文本            → Unknown
```

### 文本优先级说明

- **Confirm > Idle-Prompt**：确认对话框中可能包含 `>` 等 prompt 字符，confirm 必须先检查
- **Idle-Prompt > Error/Running**：终端底部有 shell prompt 说明命令已结束，即使上方还有旧的 "thinking" 或 "error" 文本
- **Error > Waiting > Running**：错误信号最强，其次是阻塞信号

### 数据流

```
Terminal PTY bytes
    │
    ▼
ContentExtractor.feed()
    │  OSC 133 → ShellPhaseInfo
    │  visible text → content (增量/快照)
    ▼
StatusPublisher.check_status()
    │  process_status (进程退出码)
    │  shell_phase (OSC 133)
    │  content (文本内容)
    │  process_ctx (process_active, alt_screen)
    ▼
StatusDetector.detect()
    │  优先级 1 → 2 → 3
    ▼
AgentStatus (Running / Waiting / Idle / Error / Exited / Unknown)
    │
    ▼
EventBus → AppRoot → Sidebar / TopBar / Notification
```

---

## 各 Agent 的典型状态流转

### Claude Code

```
Unknown → Running（用户输入指令，agent 开始思考）
    ↓
Running → Running（spinner: "✱ Reticulating…" / "✶ Pondering…", "esc to interrupt"）
    ↓
Running → Waiting（弹出权限确认: "Do you want to overwrite X?"）
    ↓
Waiting → Running（用户按 Yes，agent 继续执行）
    ↓
Running → Idle（任务完成，显示 "❯ " prompt）
```

**Waiting 触发条件**（CONFIRM_PATTERNS）：
- `Allow bash command?`
- `Do you want to overwrite/make this edit/run/create/delete...?`
- `Esc to cancel`（确认 UI 底部，区别于流式的 `esc to interrupt`）
- `allow once / allow always / No, deny`

### Aider

```
Unknown → Running（"Thinking..."）
    ↓
Running → Idle（显示 "ask>" / "code>" / "architect>" prompt）
```

**无 Waiting 状态**：Aider 没有权限确认机制。所有 mode prompt 都是 Idle。

### Gemini CLI

```
Unknown → Running（spinner 动画）
    ↓
Running → Waiting（"Yes, allow once / Yes, allow always / No"）
    ↓
Waiting → Running（用户确认后继续）
    ↓
Running → Idle（显示 "gemini>" prompt）
```

### Cursor Agent

```
Unknown → Running（"● Generating..." / "○ Reading" / "○ Globbing..." / "○ Running.." 等）
    ↓
Running → Waiting（"Run this command?" + "Waiting for approval..."）
    ↓
Waiting → Running（用户按 y / Run (once)）
    ↓
Running → Idle（显示 "→ " prompt）
```

**Waiting 触发条件**（CONFIRM_PATTERNS）：
- `Run this command?`（确认标题）
- `Waiting for approval...`（明确等待文本）
- `Run (once) (y)` / `Skip (esc or n)`（选项 + 快捷键）
- `Add Shell(xxx) to allowlist?`（白名单选项）

### Codex

```
Unknown → Running（"Reasoning..."）
    ↓
Running → Waiting（"Approve Once / Approve This Session / Deny"）
    ↓
Waiting → Running → Idle
```

### Opencode

```
Unknown → Running（底部 spinner 动画 + "esc interrupt" / "esc again to interrupt"）
    ↓
Running → Waiting（"Run bash command? ❯ Yes  No  Always allow"）
    ↓
Waiting → Running → Idle
```

---

## 边界情况

### 1. 进程退出覆盖一切

无论 OSC 133 或文本显示什么，进程已退出就是终态：
- 进程 exit 0 → Exited（即使屏幕还显示 "thinking"）
- 进程 exit≠0 → Error（即使屏幕显示 prompt）

### 2. OSC 133 覆盖文本（当前行为，方案 2 提议改变）

当前实现中，OSC 133 信息可用时，`ShellPhase::Running` 直接返回 Running，文本模式匹配不执行。这是 Bug 1/2/5 的根因——shell 子进程（Claude Code 等）的内部状态变化（如弹出确认对话框）无法被检测到。Output 阶段无 exit code 时会 fallthrough 到文本检测。

### 3. 确认 UI 中的干扰字符

Claude Code 确认对话框包含 `>` 和 `$` 等字符：
```
Do you want to overwrite CLAUDE.md?
› 1. Yes
  2. Yes, allow all edits during this session (shift+tab)
  3. No
```
`›` 和 `>` 可能匹配 IDLE_PATTERNS。因此 **CONFIRM 检测必须在 IDLE 之前**。

### 4. `esc` + 动词 决定状态

`esc` 后面跟的动词是区分 Running 和 Waiting 的通用信号：

| 文本 | 含义 | 状态 | Agent |
|------|------|------|-------|
| `esc interrupt` | 可中断正在执行的任务 | Running | OpenCode |
| `esc to interrupt` | 可中断流式输出 | Running | Claude Code |
| `esc again to interrupt` | 再按一次中断 | Running | OpenCode |
| `esc dismiss` | 可关闭对话框/选择 UI | Waiting | OpenCode |
| `Esc to cancel` | 可取消确认操作 | Waiting | Claude Code |
| `enter submit` | 可提交选择 | Waiting | OpenCode |

核心规则：**`esc` + `interrupt` = Running，`esc` + `dismiss`/`cancel` = Waiting**。

### 5. 屏幕滚动导致旧文本残留

检测窗口限制为最后 15 行。超出窗口的旧 "thinking" 或 "error" 文本不影响当前状态判断。如果底部 3 行内有 prompt，直接判定 Idle。

---

## 已知 Bug 与根因分析

### Bug 1：Claude Code 编辑确认 → 误判为 Idle

**现象**：Claude Code 弹出编辑确认对话框 `"Do you want to make this edit to CLAUDE.md?"`，UI 显示 `○ 1 Idle`，正确状态应为 `Waiting`。

**终端屏幕内容**（还原）：
```
Do you want to make this edit to CLAUDE.md?
› 1. Yes
  2. Yes, allow all edits during this session (shift+tab)
  3. No
```

**根因分析**：

存在三层根因，由浅入深：

#### 层 1：文本检测 — CONFIRM_PATTERNS 覆盖不足

当时的 CONFIRM_PATTERNS 没有包含 `Do you want to make this edit` 这类确认语句。Claude Code 的编辑确认不使用 `Allow bash command?` 格式，而是用 `Do you want to <verb>...?` 格式，该模式未被覆盖。

#### 层 2：文本检测 — IDLE 优先级高于 CONFIRM

当时的 `detect_from_text_detailed()` 先检查 IDLE_PATTERNS（最后 3 行有 `>` 或 `›` 等 prompt 字符），再检查 CONFIRM_PATTERNS。确认对话框中包含 `› 1. Yes` 和 `3. No`，其中 `›` 匹配了 IDLE_PATTERNS 的 shell prompt 检测，导致直接返回 Idle，**CONFIRM_PATTERNS 从未被执行**。

#### 层 3（架构根因）：OSC 133 Running 短路了文本检测

这是最深层的根因。在 `status_detector.rs` 第 190 行：

```rust
ShellPhase::Running => return AgentStatus::Running,
```

当用户的 shell 启用了 OSC 133，shell 执行 `claude` 命令时会发出 PreExec（`\x1b]133;C\x07`），将 `ShellPhase` 设为 `Running`。此后 Claude Code 作为 shell 的子进程运行，**shell 不会再发出任何 OSC 133 标记**（直到 Claude Code 退出后 shell 发出 PostExec `D`）。

这意味着：
1. OSC 133 `ShellPhase::Running` 在 Claude Code 整个运行期间保持不变
2. `detect()` 在第 190 行立即 `return AgentStatus::Running`
3. **文本检测（第 207-212 行）永远不会被执行**
4. 无论 CONFIRM_PATTERNS 写得多完美，都不会被检查

在用户 shell 未启用 OSC 133 的情况下，`ShellPhase` 为 `Unknown`，会 fallthrough 到文本检测，此时层 1 和层 2 的 bug 才会暴露。**但启用 OSC 133 时，第 190 行的短路才是真正阻止 Waiting 检测的原因。**

**修复状态**：层 1 和层 2 已通过以下修改缓解（仅对 OSC 133 未启用时有效）：
- 添加 CONFIRM_PATTERNS：`Do you want to \w+.*\?`
- 将 CONFIRM 检查移到 IDLE-Prompt 检查之前

**未修复**：层 3（OSC 133 短路）尚未解决。

---

### Bug 2：Claude Code 覆盖确认 → 误判为 Running

**现象**：Claude Code 弹出覆盖确认对话框 `"Do you want to overwrite CLAUDE.md?"`，UI 显示 `● 1 Running`，正确状态应为 `Waiting`。

**终端屏幕内容**（还原）：
```
Do you want to overwrite CLAUDE.md?
› 1. Yes
  2. Yes, allow all edits during this session (shift+tab)
  3. No
Esc to cancel  ·  Tab to amend
```

**根因分析**：

同样存在三层根因：

#### 层 1：文本检测 — CONFIRM_PATTERNS 未匹配 "overwrite"

当时的 CONFIRM_PATTERNS 使用固定动词列表 `Do you want to (make this edit|run|execute|create|delete|apply)`，不包含 `overwrite`。因此确认对话框未被 CONFIRM 命中。

#### 层 2：文本检测 — RUNNING_PATTERNS 误匹配 "Esc to cancel"

当时的 RUNNING_PATTERNS 包含 `esc to (interrupt|cancel)`。确认对话框底部的 `Esc to cancel` 匹配了这条规则，导致返回 Running。

实际上 `esc to interrupt` 和 `Esc to cancel` 的语义完全不同：

| 文本 | 出现场景 | 语义 | 正确状态 |
|------|---------|------|---------|
| `esc to interrupt` | 流式输出中（spinner 旁） | 用户可中断正在生成的输出 | Running |
| `Esc to cancel` | 确认对话框底部 | 用户可取消当前确认 | Waiting |

将两者合并在同一个 pattern 中是错误的。

#### 层 3（架构根因）：同 Bug 1 — OSC 133 Running 短路

与 Bug 1 相同，当 OSC 133 启用时，`ShellPhase::Running` 在第 190 行直接返回 Running，文本检测不会执行。**层 1 和层 2 的问题在 OSC 133 未启用时才会暴露。**

但这里有一个微妙的差异：Bug 2 的结果恰好是 Running（与 OSC 133 短路的结果一致），所以无论 OSC 133 是否启用，用户看到的都是 Running。而 Bug 1 在 OSC 133 启用时也会显示 Running（而非 Idle），因为 OSC 133 短路在文本检测之前。

**修复状态**：层 1 和层 2 已通过以下修改缓解（仅对 OSC 133 未启用时有效）：
- CONFIRM_PATTERNS：`Do you want to (make this edit|...)` → `Do you want to \w+.*\?`（通配动词）
- RUNNING_PATTERNS：`esc to (interrupt|cancel)` → `esc to interrupt`（移除 cancel）
- CONFIRM_PATTERNS 新增：`Esc to cancel`

**未修复**：层 3（OSC 133 短路）尚未解决。

---

### Bug 3：OpenCode 输入命令未执行 → 误判为 Running

**现象**：OpenCode 中用户输入 `/init` 触发自动补全菜单，命令尚未执行，UI 显示 `● Running`，正确状态应为 `Idle`。

**终端屏幕内容**（还原）：
```
/init                create/update AGENTS.md
/review              review changes [commit]...
/opsx-continue       Continue working on a c...
/opsx-onboard        Guided onboarding — wal...
/opsx-verify         Verify implementation m...
/opsx-archive        Archive a completed cha...
/opsx-new            Start a new change usin...
/opsx-explore        Enter explore mode — th...
/opsx-apply          Implement tasks from an...
/opsx-ff             Create a change and gen...

/init

Build  GLM-5 Model Studio Coding Plan
```

**根因分析**：

这是一种全新的 bug 类型——**UI 文案导致的误报（false positive）**。前两个 Bug 是 "该检测到但没检测到"（漏检），这个是 "不该检测到但误检到了"（误报）。

#### 层 1：文本检测 — RUNNING_PATTERNS 过于宽泛，匹配了 UI 描述文字

OpenCode 的自动补全菜单中，每个命令都有描述文字。这些描述文字混入了终端可见文本的最后 15 行窗口。关键匹配：

| 自动补全描述文字 | 匹配的 RUNNING_PATTERNS |
|-----------------|----------------------|
| `"Continue working on a c..."` | `(?i)in progress\|working on\|busy` — 命中 `working on` |
| `"Create a change and gen..."` | 如果完整文本是 `"...and generate..."` → `(?i)writing\|generating\|creating` 命中 `generating` |

这些词汇在描述文字中的语义是 "帮你继续做某事" "创建并生成某物"，但被 RUNNING_PATTERNS 理解为 "Agent 正在执行任务"。

#### 层 2：文本检测 — IDLE_PATTERNS 未命中 OpenCode 的 TUI 界面

在 `detect_from_text_detailed()` 中，CONFIRM 检查首先执行（不匹配），然后 IDLE 检查（`last_lines_match_idle`）查看最后 3 行：

```
/init
                                      ← 可能有空行
Build  GLM-5 Model Studio Coding Plan
```

这些行都不匹配当前的 IDLE_PATTERNS：
- `^\s*[❯➜→\$%#>]\s*$` — 裸 prompt 字符
- `^\s*(ask|architect|code|help|multi)>\s*$` — aider prompt
- `^\s*gemini>\s*$` — gemini prompt
- 等等

**OpenCode 没有传统意义上的 "prompt"**。它是一个 TUI 应用（基于 bubbletea），用户通过 UI 控件输入，不像 CLI 工具那样在底部显示 `>` prompt。因此 IDLE_PATTERNS 无法通过最后 3 行检测到 Idle。

结果：IDLE 未命中 → ERROR 未命中 → WAITING 未命中 → RUNNING 命中（`working on`）→ 返回 Running。

#### 层 3（架构根因 A）：OSC 133 Running 短路

如果用户 shell 启用了 OSC 133，`opencode` 作为 shell 子进程启动时，`ShellPhase` 从 PreExec 起一直为 `Running`。此时 `detect()` 在第 190 行直接返回 Running，文本检测不会执行。**状态从启动就锁定为 Running**，与 Bug 1/2 相同。

但用户描述 "输入命令后状态变成了 Running"，暗示之前不是 Running，说明此处 OSC 133 可能未生效或 `ShellPhase` 为 `Unknown`，走的是文本检测路径。

#### 层 3（架构根因 B）：缺乏 "UI 文案" vs "Agent 输出" 的区分能力

这是本 Bug 独有的深层问题。当前的文本检测架构假设：

> 终端可见文本 = Agent 活动的直接反映

但对于 TUI 型 Agent（OpenCode、可能还有未来的 Cursor TUI），这个假设不成立：

| 文本来源 | 是否反映 Agent 活动 | 示例 |
|---------|-------------------|------|
| Agent 思考/输出流 | ✅ 是 | `"Thinking..."`, `"Running bash command..."` |
| 确认对话框 | ⚠️ 间接（表示等待确认） | `"Do you want to overwrite?"` |
| UI 菜单/描述文字 | ❌ 否 | `"Continue working on a change"` |
| 帮助文本/快捷键提示 | ❌ 否 | `"Press Enter to execute"` |

终端可见文本是一个 **扁平化的字符流**，丢失了 UI 结构信息（菜单 vs 输出区 vs 状态栏）。StatusDetector 对所有可见文本一视同仁地做模式匹配，无法区分 "这是 Agent 在告诉你它在做什么" 还是 "这是 Agent 的 UI 装饰文字"。

#### 加剧因素：双次 check_status 跳过 debounce

在 `app_root.rs` 第 920-929 行，OpenCode 作为 TUI 应用会触发 `alt_screen = true`，导致 `process_active = true`，进而在同一轮循环中调用两次 `check_status()`：

```rust
// app_root.rs 第 919-929 行
if process_active {
    let _ = pub_.check_status(
        &status_key_clone,
        ProcessStatus::Running,
        Some(shell_info),
        content_for_status,
        process_ctx,
    );
}
```

`DebouncedStatusTracker` 要求连续 2 次检测到相同状态才确认变更。双次调用直接满足了 debounce 阈值，误报的 Running 状态被**立即确认并发布**，没有任何缓冲窗口来等待更稳定的检测结果。

**修复状态**：未修复。→ 见[统一修复方案](#统一修复方案)

---

### Bug 4：OpenCode 执行过程中状态在 Unknown 和 Running 之间反复跳动

**现象**：OpenCode 正在执行任务（Thinking、执行工具调用、Build 等），sidebar 状态在 `? Unknown` 和 `● Running` 之间反复切换，不稳定。预期应保持 `● Running`。

**终端屏幕内容**（还原）：
```
Thinking: The user wants me to analyze the codebase and create/improve
an AGENTS.md file. Let me check what already exists...

I'll analyze the codebase and improve the existing AGENTS.md. Let me
start by examining the current state and relevant configuration files.

→ Read AGENTS.md
* Glob ".cursor/rules/*" (10 matches)
* Glob ".cursorrules" (2 matches)
* Glob ".github/copilot-instructions.md"
→ Read package.json
* Glob "apps/*/package.json" (8 matches)

■ Build · glm-5

█

Build  GLM-5 Model Studio Coding Plan

.......                esc interrupt     tab agents    ctrl+p comman...
```

**根因分析**：

这是一个**多因子叠加**的问题，涉及内容提取机制、模式覆盖、TUI 兼容性三个层面。

#### 层 1（核心根因）：`take_content()` 是增量式的，不是屏幕快照

`ContentExtractor.take_content()` 的实现（`content_extractor.rs` 第 119-123 行）：

```rust
pub fn take_content(&mut self) -> (String, ()) {
    let s = String::from_utf8_lossy(&self.text_buf).into_owned();
    self.text_buf.clear();  // ← 清空缓冲区
    (s, ())
}
```

**`text_buf` 积累的是自上次 take 以来的增量 PTY 输出**，不是当前终端屏幕的完整快照。每次 `take_content()` 调用后缓冲区被清空。

状态检查每 200ms 执行一次（`status_interval = Duration::from_millis(200)`）。在两次检查之间：

| 场景 | text_buf 内容 | 检测结果 |
|------|-------------|---------|
| Agent 正在输出 Thinking 文本 | `"Thinking: The user wants..."` | Running ✅ |
| Agent 执行工具调用（网络 I/O、文件读取） | 可能为空或仅有工具结果 | Unknown ❌ 或 Idle ❌ |
| TUI 部分重绘（光标闪烁、进度条动画） | 仅含 CSI 序列（被过滤）→ 可见文本为空 | Unknown ❌ |
| 工具结果返回 | `"→ Read AGENTS.md\n* Glob ..."` | 无 Running 模式匹配 → Idle ❌ |

关键问题：**Agent "正在执行" 不意味着 PTY 时刻有输出**。工具调用期间（如读文件、网络请求）可能有数百毫秒的静默期。在静默期内 `take_content()` 返回空字符串，检测结果为 Unknown。

#### 层 2：OpenCode 的 `esc interrupt` 不匹配 RUNNING_PATTERNS

截图底部显示 OpenCode 的状态栏：`esc interrupt`（没有 "to"）

当前 RUNNING_PATTERNS（第 17 行）：
```rust
Regex::new(r"(?i)esc to interrupt").unwrap(),
```

要求 `esc to interrupt`（三个词），但 OpenCode 显示 `esc interrupt`（两个词，没有 "to"）。

**这是最稳定的 Running 信号**——TUI 应用的底部状态栏在整个执行期间持续显示，不随内容滚动消失。但因为少了一个 "to"，这个信号被完全忽略了。

如果该模式能命中，即使增量内容大部分是工具输出文本，只要 TUI 的每次屏幕重绘包含底部状态栏，`esc interrupt` 就会持续出现在 text_buf 中，让 Running 检测保持稳定。

#### 层 3：工具调用输出不包含 Running 关键词

OpenCode 执行工具时的输出格式：
```
→ Read AGENTS.md
* Glob ".cursor/rules/*" (10 matches)
■ Build · glm-5
```

逐一检查 RUNNING_PATTERNS：
- `(?i)thinking|analyzing|processing` — 不匹配
- `(?i)reasoning|streaming` — 不匹配
- `(?i)writing|generating|creating` — 不匹配
- `(?i)running (tool|bash|command)|executing|performing` — 不匹配
- `(?i)loading|downloading|uploading` — 不匹配
- `(?i)in progress|working on|busy` — 不匹配
- `(?i)esc to interrupt` — 不匹配（如层 2 所述）

只有 `"Thinking:"` 能匹配 `thinking`，但它只出现在输出开始阶段，很快被滚出 15 行检测窗口，或更关键的——在增量模式下被前一次 `take_content()` 消费掉了。

#### 层 4：TUI 屏幕重绘的文本提取不可靠

OpenCode 是基于 bubbletea 的 TUI 应用（alt_screen 模式）。TUI 使用 CSI 光标定位序列（如 `ESC[10;1H`）而非 `\n` 来布局文本。`ContentExtractor` 过滤掉所有 CSI 序列，只保留可打印字符和 `\n\r\t`。

结果：
- TUI 每次重绘产生的 text_buf **可能缺少行分隔符**（如果 TUI 用光标定位而非换行）
- 多次重绘的文本会**累积拼接**：同一行文本在 text_buf 中出现多次
- `preprocess()` 取最后 15 "行"，但如果没有 `\n`，可能是一整个巨大字符串
- 行结构混乱导致 `last_lines_match_idle()` 和模式匹配都不可靠

#### 振荡机制总结

将以上因素串联，得到状态振荡的完整时序：

```
时间    PTY 输出                    take_content()            检测结果    Debounce
─────────────────────────────────────────────────────────────────────────────────
t0      "Thinking: ..."              (有内容)                  Running     pending=1
t+200ms TUI 重绘(含 Thinking)        重绘文本(含 thinking)     Running     pending=2 → 确认 ✅
t+400ms 工具调用中(网络 I/O)         ""(空)                    Unknown     pending_new=1
t+600ms 仍在 I/O                     ""(空)                    Unknown     pending=2 → 确认 ❌
t+800ms 工具结果返回                  "→ Read AGENTS.md"       Idle        pending_new=1
t+1000ms TUI 重绘(含结果)            重绘文本(无 Running 词)   Idle        pending=2 → 确认 ❌
t+1200ms 新 Thinking 开始            "Thinking: ..."          Running     pending_new=1
t+1400ms TUI 重绘                    重绘文本(含 thinking)     Running     pending=2 → 确认 ✅
...
```

用户看到的就是状态在 Unknown → Running → Unknown → Idle → Running 之间快速跳动。

**修复状态**：未修复。→ 见[统一修复方案](#统一修复方案)

---

### Bug 5：Cursor Agent 输入斜杠命令未执行 → 误判为 Running

**现象**：Cursor Agent 中用户输入 `/status-indicators` 斜杠命令，命令尚未执行（还在输入框中），UI 状态变为 `● Running`，正确状态应为 `Idle`。

**终端屏幕内容**（还原）：
```
Cursor Agent v2026.02.27-e7d2ef6
~/workspace/saas-mono · main

→ /status-indicators █

Auto
/ commands · @ files · ! shell
```

**根因分析**：

#### 层 1（主要根因）：OSC 133 Running 短路

Cursor Agent 通过 shell 命令 `agent` 启动。用户 shell 显示 `→ saas-mono git:(main) ✗`（典型的 zsh + oh-my-zsh 主题），很可能启用了 OSC 133。

时序：
```
t0  用户在 shell 输入 "agent" 并回车
t1  Shell 发出 PreExec(C) → ShellPhase = Running
t2  Cursor Agent 启动，显示版本信息和 prompt
t3  用户在 Cursor Agent 内输入 "/status-indicators"
    此时 ShellPhase 仍为 Running（shell 只知道 "agent 命令还在运行"）
t4  detect() 第 190 行直接返回 Running
```

**文本检测从未执行**。即使 Cursor Agent 的输入区域内容不匹配任何 RUNNING_PATTERNS，OSC 133 的 `ShellPhase::Running` 在第 190 行短路了一切。

#### 层 2（次要，如果 OSC 133 未启用）：文本检测的兜底行为

假设 OSC 133 未启用，`ShellPhase` 为 `Unknown`，走文本检测路径：

Cursor Agent 界面底部的最后几行：
```
→ /status-indicators █        ← 有内容在 → 后面，不匹配 IDLE ^\s*[→]\s*$
                               ← 空行
Auto                          ← 不匹配任何模式
/ commands · @ files · ! shell ← 不匹配任何模式
```

- CONFIRM：不匹配
- IDLE（last 3 lines）：`→ /status-indicators` 中 `→` 后有文字，不满足 `^\s*[❯➜→\$%#>]\s*$`（要求行内只有 prompt 字符）
- ERROR：不匹配
- WAITING：不匹配
- RUNNING：不匹配
- 非空文本 → fallback **Idle** ✅

所以在非 OSC 133 情况下，文本检测的 fallback 行为反而是正确的（Idle）。**本 Bug 的根因完全是 OSC 133 短路**。

#### 与 Bug 3（OpenCode 输入命令）的对比

| 维度 | Bug 3（OpenCode） | Bug 5（Cursor Agent） |
|------|------------------|---------------------|
| Agent 类型 | TUI (alt_screen) | CLI (normal terminal) |
| 误判原因 | 文本检测误匹配 UI 描述文字 | OSC 133 Running 短路 |
| OSC 133 是否为根因 | 是（并存根因之一） | 是（唯一根因） |
| 文本检测是否正确 | ❌（`working on` 误匹配） | ✅（fallback Idle 正确） |

**修复状态**：未修复。需要解决 OSC 133 Running 短路的架构问题。

---

### Bug 6：Cursor Agent 持续输出过程中状态来回跳动

**现象**：Cursor Agent 执行 prompt 后正在持续输出（Reading、Globbing、Generating tokens、输出文本），sidebar 状态在多个状态之间反复跳动，不稳定。预期在整个输出过程中应保持 `● Running`。

**终端屏幕内容**（还原）：
```
The user is asking how to use this repository. I should look at t...
Checking the repo docs for setup and usage.

○ Reading README.md
● Read README.md
● Read 2 files
    Read README.md
    Read package.json

● Reading.    2.78k tokens

○ Globbing "apps/*/package.json" in .
○ Globbing, reading 1 glob, 1 file
● Globbed, read 1 glob, 1 file
    Globbed "apps/*/package.json" in .
    Read CLAUDE.md

● Reading    4.08k tokens
● Reading..  4.08k tokens

Here's how to use this repo.
────────────────────
What this repo is
saas-mono is a pnpm monorepo for a multi-tenant fitness/climbing...

[table: App | Tech | Purpose | ...]

○ Generating..  4.23k tokens

→ Add a follow-up

? Ask (shift+tab to cycle)
```

**根因分析**：

与 Bug 4（OpenCode 执行过程中跳动）根因高度相似，但 Cursor Agent 是 CLI 模式（非 TUI），暴露的问题略有不同。

#### 层 1（核心根因）：增量内容 + 模式覆盖不足 → 检测结果不稳定

`take_content()` 的增量特性意味着每 200ms 检测看到的是不同的文本片段。Cursor Agent 的输出包含多种类型的文本，只有部分匹配 Running：

| Cursor Agent 输出文本 | 匹配的模式 | 检测结果 |
|---------------------|-----------|---------|
| `"Generating.. 4.23k tokens"` | `(?i)generating` → RUNNING | Running ✅ |
| `"Reading 4.08k tokens"` | 无匹配（`reading` 不在 RUNNING） | Idle (fallback) ❌ |
| `"Reading README.md"` | 无匹配 | Idle (fallback) ❌ |
| `"Globbing ..."` | 无匹配 | Idle (fallback) ❌ |
| `"Globbed, read 1 glob"` | 无匹配 | Idle (fallback) ❌ |
| `"Here's how to use this repo."` | 无匹配 | Idle (fallback) ❌ |
| `"? Ask (shift+tab to cycle)"` | `^\?\s` → IDLE | Idle ✅ |
| （空缓冲区） | 空 | Unknown ❌ |

注意关键遗漏：**`Reading`、`Globbing` 不在 RUNNING_PATTERNS 中**。RUNNING_PATTERNS 有 `loading|downloading|uploading` 但没有 `reading`。有 `writing|generating|creating` 但没有 `globbing`。这些是 Cursor Agent 特有的工具活动指示词，当前模式未覆盖。

#### 层 2：IDLE_PATTERNS 误匹配 Cursor Agent 的输入提示

Cursor Agent 输出底部固定显示：
```
→ Add a follow-up
? Ask (shift+tab to cycle)
```

`? Ask (shift+tab to cycle)` 匹配 IDLE_PATTERNS 的 `^\?\s`（以 `?` + 空格开头）。

这个 `?` 在 Cursor Agent 中表示 "当前模式是 Ask"，不是 shell prompt。但在 `detect_from_text_detailed()` 中，IDLE 检查在 RUNNING 之前执行（因为 IDLE-Prompt 优先级高于 RUNNING）。因此：

- 即使同一段内容中有 `"Generating.."` 匹配 Running
- 只要最后 3 行包含 `? Ask...`，就会先命中 IDLE 返回 Idle

这导致了一个矛盾：Agent 正在生成内容（应该 Running），但底部的模式提示被误认为 shell prompt（判定 Idle）。

#### 层 3：`take_content()` 增量性 → 底部提示的可见性不稳定

由于 `take_content()` 是增量式的：
- 当一轮增量刚好包含 TUI 重绘（含底部 `? Ask`）→ IDLE 命中 → Idle
- 当一轮增量只有内容输出（`"Here's how to use this repo."`）→ 无 `? Ask` → 走到 RUNNING 检查 → 可能匹配也可能不匹配
- 当增量为空 → Unknown

结果：状态在 Idle / Running / Unknown 之间振荡。

#### 层 4（并发根因）：OSC 133 Running 短路

如果 OSC 133 启用，`ShellPhase::Running` 会让 `detect()` 在第 190 行直接返回 Running，**所有文本检测不执行**。此时状态应该锁定在 Running，不会跳动。

但用户报告状态 "来回跳"，说明 OSC 133 可能未启用或 phase 为 Unknown。也有另一种可能：OSC 133 的 Running phase 被某些 PTY 输出意外改变（如 ContentExtractor 解析到了错误的 OSC 序列）。

#### 振荡机制总结

```
时间    增量内容                         检测结果    原因
─────────────────────────────────────────────────────────────────
t0      "Checking the repo docs..."      Idle        无 Running 词，fallback Idle
t+200ms "○ Reading README.md..."          Idle        "Reading" 不匹配 Running
t+400ms ""(空)                           Unknown      工具 I/O 期间无输出
t+600ms "● Read 2 files\n..."            Idle        "Read" 不匹配 Running
t+800ms "● Reading. 2.78k tokens\n..."   Idle        "Reading" 不匹配 Running
t+1000ms "○ Generating.. 4.23k\n→ ...\n? Ask..."  Idle  ? Ask 先匹配 IDLE
t+1200ms "Here's how to use..."          Idle        纯文本，fallback Idle
t+1400ms "○ Generating.. 4.23k tokens"   Running     "Generating" 匹配！但仅此一刻
t+1600ms "→ Add a follow-up\n? Ask..."   Idle        ? Ask 匹配 IDLE
...
```

几乎所有增量都被判定为 Idle 或 Unknown，只有偶尔的 `"Generating"` 能触发 Running。用户看到的是：大部分时间 Idle，偶尔闪现 Running，然后又回到 Idle。

**修复状态**：未修复。→ 见[统一修复方案](#统一修复方案)

---

### Bug 7：Cursor Agent `/exit` 退出时触发了不必要的通知

**现象**：用户在 Cursor Agent 中输入 `/exit` 主动退出 agent，pmux 弹出通知。用户期望不要通知——因为退出是自己主动操作的，不需要被提醒。

**根因分析**：

#### 层 1（主要根因）：通知规则无法区分 "任务完成" 和 "用户主动退出"

`status_publisher.rs` 第 113-119 行的通知规则：

```rust
let should_notify = matches!(
    (prev_status, current_status),
    (AgentStatus::Running, AgentStatus::Idle)     // ← 会触发
        | (_, AgentStatus::Waiting)
        | (_, AgentStatus::Error)
        | (_, AgentStatus::Exited)                 // ← 也会触发
);
```

当用户输入 `/exit` 时，状态转换链：

```
Agent Running → (用户输入 /exit) → Cursor Agent 退出
    → Shell 显示 prompt → detect: Idle
    → Running → Idle → 触发通知（规则 1）
```

或者：
```
Agent Running → Cursor Agent 进程退出
    → ProcessStatus::Exited → AgentStatus::Exited
    → 触发通知（规则 4: * → Exited）
```

两条路径都会触发通知。通知系统只关心 **状态转换的方向**（Running → Idle 或 * → Exited），不关心 **转换的原因**（任务完成 vs 用户主动退出）。

#### 层 2（并发根因）：状态振荡导致误报的 Running 基线

由于 Bug 6 描述的状态振荡问题，即使用户在 Cursor Agent 的空闲 prompt 上输入 `/exit`，agent 的最后确认状态也可能恰好是 Running（在振荡中偶然确认的）。这意味着即使 "退出前是 Idle" 应该不触发通知，实际上因为状态不稳定，退出前的基线状态可能是错误的 Running。

时序：
```
t0      Cursor Agent 在 prompt 等待         状态振荡中...
t-400ms 某次增量含 "Generating" → Running   Running 被确认
t-200ms 用户输入 /exit                      状态仍为 Running（200ms 内未翻转）
t0      Agent 退出 → Idle                   Running → Idle → 通知！
```

如果状态稳定为 Idle，转换就是 Idle → Idle（无变化）或 Idle → Exited，只有后者会通知。但由于振荡，Running → Idle 的误报通知概率很高。

#### 两种场景对比

| 场景 | 转换 | 应该通知？ | 实际通知？ |
|------|------|-----------|-----------|
| Agent 执行完任务，回到 prompt | Running → Idle | ✅ 是 | ✅ 是 |
| 用户主动 `/exit` 退出 | Running → Idle | ❌ 否 | ✅ 是 |
| Agent 崩溃退出 | * → Error/Exited | ✅ 是 | ✅ 是 |
| 用户主动关闭终端 | * → Exited | ❌ 否 | ✅ 是 |

核心矛盾：Running → Idle 既表示 "任务完成"（需要通知），也表示 "用户主动退出"（不需要通知），当前架构无法区分这两种情况。

**修复状态**：未修复。→ 见[统一修复方案](#统一修复方案)

---

### Bug 8：`brew install node` 命令状态不跟踪 Running，中途误报 Idle 并发通知

**现象**：在 tmux 终端中执行 `brew install node`，状态变化不正确：
1. 命令启动后状态不是 Running（显示 Idle）
2. 中间某些时刻变成 Running（可能是 brew 输出了 "Downloading..." 等文字）
3. 命令还没结束就变回 Idle
4. Running → Idle 转换触发了通知

用户期望：
1. 命令启动后立即变成 Running
2. 命令真正结束（shell prompt 出现）才变回 Idle
3. 如果窗口有焦点（用户正在看），不发任何通知

**终端屏幕内容**（还原）：
```
→ saas-mono git:(main) ✗ brew install node
==> Auto-updating Homebrew...
Adjust how often this is run with `$HOMEBREW_AUTO_UPDATE_SECS` or disable with
`$HOMEBREW_NO_AUTO_UPDATE=1`. Hide these hints with `$HOMEBREW_NO_ENV_HINTS=1` (see `man brew`).
█
```

Sidebar 显示：`○ main Just now Idle`

**根因分析**：

这是第一个涉及**纯 shell 命令**（非 AI Agent）的 bug，暴露了检测架构的一个根本缺陷。

#### 层 1（核心根因）：`ProcessContext.process_active` 被完全忽略

在 `app_root.rs` 第 880-895 行，系统已经**正确检测**到非 shell 进程正在运行：

```rust
let process_active = if phase == ShellPhase::Unknown {
    if alt_screen { true }
    else if pane_target_clone.starts_with('%') {
        // tmux display-message -p -t %0 "#{pane_current_command}"
        // → 返回 "brew"（非 shell 命令）→ process_active = true
        let cmd = ...;
        !matches!(cmd.as_str(), "zsh" | "bash" | "fish" | "sh" | "dash" | "ksh")
    } else { false }
} else { false };
```

当 `brew` 运行时：
- `#{pane_current_command}` 返回 `"brew"`
- `"brew"` 不在 shell 列表中 → `process_active = true`
- `ProcessContext { process_active: true, alt_screen: false }` 被构建并传入 `detect()`

但在 `status_detector.rs` 第 211 行：
```rust
let _ = process_ctx;  // ← 完全忽略
```

**系统已知 brew 在运行，但选择不使用这个信息**。这是所有 Bug 中 `ProcessContext` 被浪费的最直接例证。

如果 `detect()` 使用 `process_active`：
```
process_active = true → 非 shell 进程在运行 → 返回 Running
process_active = false → shell 在运行（命令已结束）→ fallthrough 到文本检测
```

这就能完美解决 `brew install node` 的状态跟踪问题。

#### 层 2：文本检测无法识别任意 shell 命令的输出

`brew` 的输出格式不包含任何 RUNNING_PATTERNS 关键词：

| brew 输出文本 | 匹配 RUNNING？ |
|-------------|-------------|
| `==> Auto-updating Homebrew...` | ❌ `updating` 不在模式中 |
| `Adjust how often this is run...` | ❌ |
| `==> Downloading https://...` | ✅ `downloading` 匹配！ |
| `==> Installing node...` | ❌ `installing` 不在模式中 |
| `==> Pouring node--...` | ❌ |

只有 `Downloading` 偶尔命中，其余所有输出都不匹配。这导致状态在大部分时间是 Idle（fallback），偶尔闪现 Running。

**但这不是模式覆盖的问题**——你无法为所有可能的 shell 命令添加 Running 模式。`apt-get`、`pip install`、`cargo build`、`make`、`docker pull` 等各有完全不同的输出格式。文本模式匹配从架构上就不适合检测任意 shell 命令的运行状态。

#### 层 3：OSC 133 未启用（推测）

如果 OSC 133 正常工作：
```
t0  brew install node → Shell PreExec(C) → ShellPhase::Running
t1  detect() 第 190 行 → return Running（正确）
t2  brew 运行期间，ShellPhase 持续 Running → 状态稳定 ✅
t3  brew 结束 → Shell PostExec(D) → ShellPhase::Output → Idle
```

这是 OSC 133 设计的正确用途——**追踪 shell 命令的生命周期**。但用户此处遇到状态波动，强烈暗示 OSC 133 未启用（`ShellPhase` 始终为 `Unknown`），因此走了文本检测路径。

结合之前 Bug 4/6 中观察到的状态振荡现象（同样暗示 OSC 133 未启用），用户很可能没有配置 shell integration。

#### 层 4：窗口焦点不影响通知

用户明确提出：**"如果窗口没有失去焦点，就不报任何通知"**。

当前通知链路（`app_root.rs` 第 1800 行）：
```rust
system_notifier::notify("pmux", &message, notif_type);
```

`system_notifier::notify()` 直接发出系统通知，没有检查窗口焦点状态。`notification_manager.add_labeled()` 做了一些去重，但也不考虑焦点。

通知的核心价值是 "用户不在旁边时提醒"。如果用户正在看着 pmux 窗口，sidebar 的状态变化已经足够，系统通知是冗余且干扰的。

#### 三层检测信号对比

| 信号来源 | brew 运行中的值 | 是否可用于判定 Running | 当前是否使用 |
|---------|-------------|-------------------|-----------|
| OSC 133 ShellPhase | Running（如启用） | ✅ 完美 | ✅ 使用（但用户可能未启用） |
| ProcessContext.process_active | `true`（tmux 查 pane_current_command） | ✅ 可靠 | ❌ **被 `let _` 忽略** |
| 文本模式匹配 | 偶尔匹配 `downloading` | ❌ 不可靠 | ✅ 使用（作为 fallback） |

**优先级应该是**：OSC 133 > ProcessContext > 文本匹配。但当前实现是：OSC 133 > ~~ProcessContext~~ > 文本匹配，中间层被跳过了。

**修复状态**：未修复。→ 见[统一修复方案](#统一修复方案)

---

### 架构根因总结：跨 Bug 共性问题

八个 Bug 暴露了六个架构层面的共性问题。

#### 共性问题 1：OSC 133 Running 短路（Bug 1, 2, 3, 5, 8）

**问题代码**（`status_detector.rs` 第 187-205 行）：

```rust
// Priority 2: OSC 133 markers
if let Some(info) = shell_info {
    match info.phase {
        ShellPhase::Running => return AgentStatus::Running,  // ← 短路
        ShellPhase::Input | ShellPhase::Prompt => return AgentStatus::Idle,
        ShellPhase::Output => { /* check exit code, fallthrough if None */ }
        ShellPhase::Unknown => { /* fallthrough */ }
    }
}
```

**时序分析**：

```
时间线      Shell 事件              ShellPhase     实际 Agent 状态
───────────────────────────────────────────────────────────────
t0   用户在 shell 输入 "claude"     Prompt         —
t1   按回车，shell 发出 PreExec(C)  Running        Running（正确）
t2   Claude Code 开始思考           Running        Running（正确）
t3   Claude Code 弹出确认对话框     Running        Waiting（错误！）
t4   用户按 Yes，继续执行           Running        Running（正确）
t5   Claude Code 完成任务           Running        Idle（错误！）
t6   Claude Code 退出               Running        —
t7   Shell 发出 PostExec(D)         Output         Idle / Error
t8   Shell 发出 PromptStart(A)      Prompt         Idle（正确）
```

从 t1 到 t6，`ShellPhase` 始终为 `Running`，因为 OSC 133 是 **shell 级别**的标记——shell 只知道「我启动了一个命令，它还没结束」，不知道子进程（Claude Code）内部的状态变化。

**影响范围**：

| Agent | 受影响？ | 原因 |
|-------|---------|------|
| Claude Code | ✅ 受影响 | 作为 shell 子进程运行，OSC 133 无法追踪其内部状态 |
| Aider | ✅ 受影响 | 同上 |
| Gemini CLI | ✅ 受影响 | 同上 |
| Codex | ✅ 受影响 | 同上 |
| Cursor Agent | ✅ 受影响 | 同上 |
| Opencode | ✅ 受影响 | 通过 shell 启动，同上 |

#### 共性问题 2：`take_content()` 增量式设计 → 状态振荡（Bug 4, 6, 8）

`ContentExtractor.take_content()` 每次调用清空缓冲区，返回增量数据而非屏幕快照。状态检测只能看到最近 200ms 的 PTY 输出碎片，导致：
- 工具 I/O 静默期 → 空内容 → Unknown
- 工具结果文本无 Running 关键词 → fallback Idle
- 偶尔出现 Running 关键词（"Thinking"、"Generating"）→ Running

检测结果随每次增量的内容而波动，永远无法稳定。

#### 共性问题 3：RUNNING_PATTERNS 覆盖面问题（Bug 3, 4, 6）

| 问题类型 | 具体表现 | 涉及 Bug |
|---------|---------|---------|
| 过于宽泛 | `working on` 匹配 UI 菜单描述文字 | Bug 3 |
| 覆盖不足 | `esc interrupt`（无 "to"）不匹配 | Bug 4 |
| 覆盖不足 | `Reading`、`Globbing` 不在模式中 | Bug 4, 6 |
| 模式冲突 | `? Ask` 匹配 IDLE，覆盖了同时存在的 Running 信号 | Bug 6 |

Running 模式来源于对 Claude Code 的观察，未充分覆盖其他 Agent 的输出格式。

#### 共性问题 4：未使用的 `ProcessContext`（所有 Bug，尤其 Bug 8）

`status_detector.rs` 第 211 行 `let _ = process_ctx;`。`ProcessContext`（`process_active`、`alt_screen`）被传入但完全忽略，没有参与任何检测逻辑。

Bug 8（`brew install node`）是最直接的例证：系统通过 tmux `#{pane_current_command}` 已确认 `brew` 在运行（`process_active = true`），但 `detect()` 完全忽略这个信息，只靠文本模式匹配（不包含 brew 输出关键词）→ Idle。

如果能利用 `ProcessContext`：
- `process_active = true` → 非 shell 进程在运行 → 直接返回 Running（**最高优先级的可靠信号**）
- `alt_screen = true` → TUI 模式，可降低 RUNNING 误报权重，或启用 TUI 专用检测逻辑
- 双重 `check_status` 调用（app_root.rs 920-929）跳过 debounce 的问题也可在此处解决

#### 共性问题 5：通知规则不区分转换原因（Bug 7, 8）

`should_notify` 只依据 `(prev_status, current_status)` 的组合，不考虑转换原因。Running → Idle 既可能是 "Agent 完成了任务"（应通知），也可能是 "用户主动退出 Agent"（不应通知）。需要引入转换原因上下文（如用户输入活动、窗口焦点状态）来过滤不必要的通知。

#### 共性问题 6：通知不感知窗口焦点（Bug 7, 8）

`system_notifier::notify()` 在 `app_root.rs` 第 1800 行无条件发出系统通知，不检查 pmux 窗口是否有焦点。用户明确提出：**如果窗口有焦点（用户正在看），不应发送系统通知**。

通知的核心价值是 "用户不在旁边时提醒"。如果用户正盯着 pmux 窗口，sidebar 的状态变化已经是足够的视觉反馈，系统弹窗是冗余且干扰的。需要在发送通知前检查 GPUI 的窗口焦点状态。

**其他相关问题**：

- **`last_post_exec_exit_code` 始终为 None**：`app_root.rs` 第 903 行 — exit code 从未传递给 `ShellPhaseInfo`，OSC 133 PostExec exit code 采集未完成。

→ 各共性问题的修复方案见下文[统一修复方案](#统一修复方案)

---

## 统一修复方案

基于 8 个 Bug 的根因分析和各 Agent 官方终端特征，提出 6 个修复方案，按优先级排列。

---

### 各 Agent 终端特征参考

修复方案依赖对各 Agent 终端输出的准确理解。以下为官方文档/源码确认的特征：

| Agent | 屏幕模式 | Running 信号 | Idle 信号 | Waiting 信号 |
|-------|---------|-------------|----------|-------------|
| **Claude Code** | Normal | `esc to interrupt`、spinner 字符行 `✱✳✶✻✽✢·` + 动词（`Pondering`、`Reticulating`、`Simmering` 等，动词不可穷举） | `❯ ` prompt | `Do you want to`、`Esc to cancel`、`allow once` |
| **Aider** | Normal | "Thinking" spinner、流式输出 | `> `、`ask> `、`code> `、`architect> ` | `(Y)es/(N)o`、`[Yes]:` |
| **Gemini CLI** | Normal | spinner、工具名大写如 `GoogleSearch`、标题栏 `✦` | `>` prompt、标题栏 `◇` | `Approve? (y/n/always)`、标题栏 `✋` |
| **Cursor Agent** | Normal | `●/○` + 活动词（`Reading`、`Generating...`、`Globbing`、`Running..`） | `→ ` prompt（空输入）| `Run this command?`、`Waiting for approval...`、`Run (once) (y)`、`Skip (esc or n)` |
| **OpenCode** | Alt Screen (TUI) | 状态栏 `esc interrupt` / `esc again to interrupt`、底部 spinner 动画（`··██··` 点阵闪动）、流式输出 | TUI 输入区活跃、无 spinner 动画 | 权限对话框、选择 UI（`↑↓ select  enter submit  esc dismiss`）|
| **Codex CLI** | Alt Screen (TUI) | 流式输出、工具反馈 | Composer 活跃 | 审批提示（Suggest 模式）|

---

### 方案 1：启用 ProcessContext（P0 · 解决 Bug 8，改善全部）

**解决的 Bug**：Bug 8（brew）  **间接改善**：Bug 1-6（所有 Agent——process_active 提供 Running 兜底，文本检测仍可覆盖 Waiting）

**改动文件**：`status_detector.rs`

**思路**：`ProcessContext { process_active, alt_screen }` 已在 `app_root.rs` 正确计算（tmux `#{pane_current_command}` 判断非 shell 进程），但 `detect()` 用 `let _` 忽略了。启用它作为检测优先级 1.5（进程退出 > ProcessContext > OSC 133 > 文本）：

```
detect(process_status, shell_info, content, process_ctx):
    // 优先级 1: 进程退出
    if process_status == Exited → Exited
    if process_status == Error → Error

    // 优先级 1.5: ProcessContext（新增）
    if process_ctx.process_active && shell_info 无 PostExec:
        // 非 shell 进程在运行（brew、claude、agent 等）
        // 仍做文本检测以识别 Waiting
        text_result = detect_from_text(content)
        if text_result == Waiting → Waiting
        else → Running

    // 优先级 2: OSC 133（改造，见方案 2）
    // 优先级 3: 文本检测
```

**效果**：
- `brew install node`：`process_active = true` → 直接 Running，命令结束后 shell 回到前台 → `process_active = false` → Idle ✅
- Agent 运行中：`process_active = true` → Running（OSC 133 不再需要短路），文本检测仍可覆盖 Waiting ✅

---

### 方案 2：OSC 133 Running 文本回补（P0 · 解决 Bug 1, 2, 5）

**解决的 Bug**：Bug 1, 2（Claude Code 确认），Bug 5（Cursor Agent 输入）

**改动文件**：`status_detector.rs`

**思路**：`ShellPhase::Running` 不再直接 `return Running`，而是将 Running 作为 fallback，先执行文本检测允许 Waiting/Error 覆盖：

```rust
ShellPhase::Running => {
    // 不再短路！先做文本检测
    let text_status = self.detect_from_text(content);
    match text_status {
        AgentStatus::Waiting => return AgentStatus::Waiting,  // 确认对话框
        AgentStatus::Error => return AgentStatus::Error,      // 错误输出
        _ => return AgentStatus::Running,                     // OSC 133 兜底
    }
}
```

**效果**：
- Claude Code 弹出 `Do you want to overwrite?` → 文本检测到 CONFIRM → Waiting ✅
- Cursor Agent 在 `→` prompt 输入命令 → 文本 fallback Idle，但 OSC 133 兜底 → Running（可接受，配合方案 1 的 ProcessContext 更佳）

**注意**：方案 1 和 2 配合使用效果最佳。方案 1 提供进程级 Running 信号，方案 2 保留 OSC 133 中的文本覆盖能力。

---

### 方案 3：Agent 终端模式更新（P1 · 解决 Bug 3, 4, 6）

**解决的 Bug**：Bug 3（OpenCode 菜单误报），Bug 4（OpenCode 闪烁），Bug 6（Cursor Agent 闪烁）

**改动文件**：`status_detector.rs`

基于各 Agent 官方终端特征，修正三类模式：

#### 3a. RUNNING_PATTERNS 修正

```rust
// 删除过于宽泛的
- r"(?i)in progress|working on|busy"
+ r"(?i)in progress|busy"  // 移除 "working on"

// 放宽 esc interrupt（覆盖 OpenCode 的三种形式）
- r"(?i)esc to interrupt"
+ r"(?i)esc\s+(again\s+to\s+|to\s+)?interrupt"  // "esc interrupt" / "esc to interrupt" / "esc again to interrupt"

// Cursor Agent：●/○ 前缀 + 活动词（覆盖所有工具状态）
+ r"[○●]\s*(Reading|Generating|Globbing|Running|Searching|Editing|Writing)"  // "● Generating..." "○ Reading"

// Claude Code spinner 字符行（字符固定，动词不可穷举，匹配字符比匹配动词更健壮）
// 终端显示: "✱ Reticulating…" / "✶ Pondering…" / "· Computing…"
+ r"[✱✳✶✻✽✢·]\s+\S+…"  // spinner字符 + 空格 + 动词 + "…"

// OpenCode 底部 spinner 动画（点阵 + esc interrupt 同行）
+ r"[·•█▪]{2,}.*esc\s+(again\s+to\s+|to\s+)?interrupt"  // "██····  esc interrupt" 动画行
```

#### 3b. IDLE_PATTERNS 修正

```rust
// 修正 ? 开头的误匹配（排除 Cursor Agent 的 "? Ask ..."）
- r"^\?\s"
+ r"^\?\s*$"  // 只匹配孤立的 "?" 后接空白到行尾

// 新增 Cursor Agent 空 prompt
+ r"^\s*→\s*$"  // Cursor Agent 空输入行
```

#### 3c. CONFIRM_PATTERNS 新增

```rust
// 现有模式泛化（"Run bash command?" → 覆盖 Cursor 的 "Run this command?"）
- r"(?i)Run bash command\?|Allow this tool"
+ r"(?i)Run (bash command|this command)\?|Allow this tool"

// Gemini CLI
+ r"(?i)Approve\?\s*\(y/n"  // "Approve? (y/n/always)"

// Cursor Agent
+ r"(?i)Waiting for approval"               // "Waiting for approval..." 明确等待文本
+ r"(?i)Run \(once\)\s*\(y\)"              // "Run (once) (y)" 审批选项
+ r"(?i)Skip \(esc or n\)"                 // "Skip (esc or n)" 拒绝选项
+ r"(?i)Add Shell\(.+\) to allowlist"      // "Add Shell(echo) to allowlist?" 白名单选项

// Aider
+ r"\(Y\)es/\(N\)o"  // "(Y)es/(N)o [Yes]:"

// TUI 通用选择 UI（OpenCode 等）
+ r"enter\s+submit.*esc\s+dismiss"  // "↑↓ select  enter submit  esc dismiss"
+ r"esc\s+dismiss"                  // 简化版：只要有 "esc dismiss" 就是等待用户操作
```

#### 3d. `esc` 语义规则（跨 Agent 通用）

`esc` + 后续动词 是区分 Running 和 Waiting 的关键信号：

| 模式 | 状态 | 匹配的 Agent |
|------|------|-------------|
| `esc interrupt` / `esc to interrupt` / `esc again to interrupt` | **Running** | Claude Code, OpenCode |
| `esc dismiss` / `esc cancel` / `Esc to cancel` | **Waiting** | OpenCode, Claude Code |
| `enter submit` | **Waiting** | OpenCode（选择 UI） |

---

### 方案 4：屏幕快照替代增量内容（P1 · 解决 Bug 4, 6）

**解决的 Bug**：Bug 4（OpenCode 振荡），Bug 6（Cursor Agent 振荡）

**改动文件**：`content_extractor.rs`、`app_root.rs`

**思路**：`take_content()` 不再清空 `text_buf`，改为保留滑动窗口（最后 8KB），或直接从 gpui-terminal 的 `Term` 读取当前可见屏幕内容。

方案 A（最小改动）：`text_buf` 保留最后 8KB，不清空。
方案 B（架构级）：从 `terminal.term().lock()` 读取可见行，类似 capture-pane 但无需 tmux。

**效果**：每次状态检测看到完整终端内容，消除因增量空白导致的 Unknown 振荡。

**注意**：如果方案 1（ProcessContext）实施后振荡已可接受，此方案可延后。

---

### 方案 5：状态惯性与 Unknown 降权（P1 · 改善 Bug 4, 6）

**改善的 Bug**：Bug 4（OpenCode 振荡），Bug 6（Cursor Agent 振荡）

**改动文件**：`status_detector.rs`（`DebouncedStatusTracker`）

**思路**：
- **Unknown 不更新状态**：Unknown = "没数据"，不应触发状态变更，保持上一状态
- **Running → Idle 需要更高阈值**：从 2 次提高到 3 次连续确认
- **Waiting/Error/Exited 仍立即生效**

```rust
fn update_with_status(&mut self, new: AgentStatus) -> bool {
    if new == Unknown { return false; }  // 忽略 Unknown
    if matches!(new, Error | Exited | Waiting) { /* 立即生效 */ }
    if self.current == Running && new == Idle { self.threshold = 3; }
    // ...正常 debounce
}
```

---

### 方案 6：通知焦点感知（P2 · 解决 Bug 7, 8）

**解决的 Bug**：Bug 7（/exit 退出通知），Bug 8（brew 误通知）

**改动文件**：`app_root.rs`、`status_publisher.rs`

#### 6a. 窗口焦点过滤

```rust
// 仅窗口失焦时发系统通知
if !window.is_active() {
    system_notifier::notify("pmux", &n.message, n.notif_type);
}
// NotificationPanel 始终记录
```

#### 6b. 用户输入抑制

```rust
// Running → Idle：如果最近 2 秒内有用户键盘输入，抑制通知
if prev == Running && current == Idle && last_input.elapsed() < 2s {
    should_notify = false;
}
```

---

### 方案优先级与 Bug 覆盖矩阵

| 方案 | 优先级 | Bug 1 | Bug 2 | Bug 3 | Bug 4 | Bug 5 | Bug 6 | Bug 7 | Bug 8 |
|------|--------|-------|-------|-------|-------|-------|-------|-------|-------|
| 1. ProcessContext | P0 | ○ | ○ | ◐ | ◐ | ○ | ◐ | — | ● |
| 2. OSC 133 回补 | P0 | ● | ● | ○ | — | ● | — | — | — |
| 3. 模式更新 | P1 | — | — | ● | ● | — | ● | — | — |
| 4. 屏幕快照 | P1 | — | — | ○ | ● | — | ● | — | ○ |
| 5. 状态惯性 | P1 | — | — | — | ● | — | ● | ○ | ○ |
| 6. 通知焦点 | P2 | — | — | — | — | — | — | ● | ● |

● = 直接解决 · ◐ = 改善 · ○ = 间接受益 · — = 不相关

**推荐实施顺序**：方案 1+2（P0，一起做）→ 方案 3 → 方案 5 → 方案 6 → 方案 4（如果仍需要）

方案 1+2 改动集中在 `detect()` 函数，能解决 5 个核心 Bug。方案 3 是模式补充。方案 4 是架构级改动，方案 1+5 实施后如振荡可接受则可推迟。方案 6 独立于检测逻辑，可随时实施。
