# JSONL Session Scanner — 补充层状态检测

## 目标

在现有终端流状态检测（OSC 133 + 文本模式匹配）的基础上，增加 JSONL 会话文件扫描作为补充信号源，提供更精确的结构化状态信息。

## 核心设计

### Pane ↔ Session 绑定

维护一个 `pane_id → session_id` 的显式映射关系：

```
pane_id: "local:/Users/matt/workspace/repo/.worktrees/feature-auth"
    ↓ 路径转换
project_dir: ~/.claude/projects/-Users-matt-workspace-repo--worktrees-feature-auth/
    ↓ fsnotify 监听
active JSONL: 70984a90-6fdd-4602-85d9-7f935ef890b1.jsonl
    ↓ 提取 sessionId
session_id: "70984a90-6fdd-4602-85d9-7f935ef890b1"
```

绑定建立时机：
- 当 pane 的 project_dir 下有新 JSONL 文件**被创建**（新 session）→ 绑定
- 当 pane 的 project_dir 下有 JSONL 文件**被写入**（--continue / --resume）→ 更新绑定
- 一个 pane 始终只绑定最新活跃的那个 session

### 路径映射规则

从 Claude Code 的存储规则反推：
```
worktree_path.replace("/", "-").replace(".", "-")
```

示例：
```
/Users/matt.chow/workspace/pmux
  → -Users-matt-chow-workspace-pmux

/Users/matt.chow/workspace/ganwork/.worktrees/feature-117
  → -Users-matt-chow-workspace-ganwork--worktrees-feature-117
```

### JSONL 消息类型（需要关注的）

| type | 说明 | 对状态检测的价值 |
|---|---|---|
| `queue-operation` (enqueue) | 用户发送了新消息 | 标记一轮对话开始 |
| `user` | 用户消息，含 `cwd`, `sessionId` | 确认 session 归属 |
| `assistant` | Claude 回复，含 `content[]` | 判断 thinking/tool_use/text |
| `assistant` + `tool_use` | 工具调用 | 精确知道在调哪个工具 |
| `assistant` + `thinking` | 推理中 | 确认 Running 状态 |
| `system` (subtype: stop_hook_summary) | 一轮结束 | 确认 Idle/回到等待 |

---

## 实现方案

### 文件结构

```
src/
  session_scanner/
    mod.rs            -- pub mod, SessionScanner 对外接口
    path_mapper.rs    -- worktree path → Claude project dir 转换
    file_watcher.rs   -- fsnotify 监听 + JSONL 增量读取
    message_parser.rs -- JSONL 行解析，提取结构化事件
    binding.rs        -- pane_id ↔ session_id 绑定管理
```

### Task 1: path_mapper.rs — 路径映射

将 worktree 路径转换为 Claude Code 的项目目录路径。

```rust
use std::path::{Path, PathBuf};

/// Convert a worktree path to Claude Code's project directory path.
/// Rule: replace '/' with '-', '.' with '-'
/// Example: /Users/matt/workspace/pmux → ~/.claude/projects/-Users-matt-workspace-pmux/
pub fn worktree_to_claude_project_dir(worktree_path: &Path) -> PathBuf {
    let claude_base = dirs::home_dir()
        .expect("home dir")
        .join(".claude")
        .join("projects");

    let sanitized = worktree_path
        .to_string_lossy()
        .replace('/', "-")
        .replace('.', "-");

    claude_base.join(sanitized)
}

/// Check if the project directory exists (Claude Code may not have been used in this worktree)
pub fn claude_project_dir_exists(worktree_path: &Path) -> bool {
    worktree_to_claude_project_dir(worktree_path).is_dir()
}
```

### Task 2: message_parser.rs — JSONL 消息解析

解析 JSONL 行，提取我们关心的结构化事件。

```rust
use serde::Deserialize;

/// Events we extract from JSONL messages
#[derive(Debug, Clone)]
pub enum SessionEvent {
    /// User sent a new message (enqueue)
    UserInput { session_id: String, timestamp: String },
    /// Assistant is thinking (has thinking block in content)
    Thinking { session_id: String },
    /// Assistant is using a tool
    ToolUse { session_id: String, tool_name: String, tool_id: String },
    /// Tool result received
    ToolResult { session_id: String, tool_id: String, is_error: bool },
    /// Assistant produced text output
    TextOutput { session_id: String },
    /// Turn ended (system stop message)
    TurnEnd { session_id: String, timestamp: String },
}

#[derive(Deserialize)]
struct RawMessage {
    #[serde(rename = "type")]
    msg_type: Option<String>,
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
    operation: Option<String>,
    timestamp: Option<String>,
    subtype: Option<String>,
    message: Option<RawInnerMessage>,
}

#[derive(Deserialize)]
struct RawInnerMessage {
    role: Option<String>,
    content: Option<serde_json::Value>,
}

pub fn parse_jsonl_line(line: &str) -> Option<(String, SessionEvent)> {
    let raw: RawMessage = serde_json::from_str(line).ok()?;
    let session_id = raw.session_id?;

    match raw.msg_type.as_deref()? {
        "queue-operation" if raw.operation.as_deref() == Some("enqueue") => {
            Some((session_id.clone(), SessionEvent::UserInput {
                session_id,
                timestamp: raw.timestamp.unwrap_or_default(),
            }))
        }
        "system" if raw.subtype.as_deref() == Some("stop_hook_summary") => {
            Some((session_id.clone(), SessionEvent::TurnEnd {
                session_id,
                timestamp: raw.timestamp.unwrap_or_default(),
            }))
        }
        "assistant" => {
            let msg = raw.message?;
            if msg.role.as_deref() != Some("assistant") { return None; }
            let content = msg.content.as_ref()?.as_array()?;

            // Check content blocks for thinking, tool_use, text
            for block in content {
                let block_type = block.get("type")?.as_str()?;
                match block_type {
                    "thinking" => {
                        return Some((session_id.clone(), SessionEvent::Thinking {
                            session_id,
                        }));
                    }
                    "tool_use" => {
                        let name = block.get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown")
                            .to_string();
                        let id = block.get("id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        return Some((session_id.clone(), SessionEvent::ToolUse {
                            session_id, tool_name: name, tool_id: id,
                        }));
                    }
                    "text" => {
                        return Some((session_id.clone(), SessionEvent::TextOutput {
                            session_id,
                        }));
                    }
                    _ => {}
                }
            }
            None
        }
        "user" => {
            // tool_result is inside user messages
            let msg = raw.message?;
            if msg.role.as_deref() != Some("user") { return None; }
            let content = msg.content.as_ref()?.as_array()?;
            for block in content {
                if block.get("type").and_then(|v| v.as_str()) == Some("tool_result") {
                    let tool_id = block.get("tool_use_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let is_error = block.get("is_error")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    return Some((session_id.clone(), SessionEvent::ToolResult {
                        session_id, tool_id, is_error,
                    }));
                }
            }
            None
        }
        _ => None,
    }
}
```

### Task 3: file_watcher.rs — 文件监听与增量读取

监听 Claude 项目目录，增量读取 JSONL 文件的新内容。

```rust
use notify::{Watcher, RecursiveMode, Event, EventKind};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::fs::File;
use std::path::{Path, PathBuf};

pub struct JsonlFileWatcher {
    /// Track read position per file
    file_positions: HashMap<PathBuf, u64>,
    /// The project directory we're watching
    project_dir: PathBuf,
}

impl JsonlFileWatcher {
    pub fn new(project_dir: PathBuf) -> Self {
        Self {
            file_positions: HashMap::new(),
            project_dir,
        }
    }

    /// Read new lines from a JSONL file since last read position.
    /// Returns (session_id_from_filename, new_lines).
    pub fn read_new_lines(&mut self, file_path: &Path) -> Vec<String> {
        let pos = self.file_positions.get(file_path).copied().unwrap_or(0);
        let mut file = match File::open(file_path) {
            Ok(f) => f,
            Err(_) => return vec![],
        };

        if file.seek(SeekFrom::Start(pos)).is_err() {
            return vec![];
        }

        let reader = BufReader::new(&file);
        let mut lines = Vec::new();
        let mut new_pos = pos;

        for line in reader.lines() {
            match line {
                Ok(l) => {
                    new_pos += l.len() as u64 + 1; // +1 for newline
                    if !l.is_empty() {
                        lines.push(l);
                    }
                }
                Err(_) => break,
            }
        }

        self.file_positions.insert(file_path.to_path_buf(), new_pos);
        lines
    }

    /// Extract session ID from JSONL filename (UUID format)
    pub fn session_id_from_path(path: &Path) -> Option<String> {
        path.file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
    }

    /// Initialize: skip all existing content (only process new writes)
    pub fn skip_existing(&mut self, file_path: &Path) {
        if let Ok(metadata) = std::fs::metadata(file_path) {
            self.file_positions.insert(file_path.to_path_buf(), metadata.len());
        }
    }
}
```

### Task 4: binding.rs — Pane ↔ Session 绑定管理

```rust
use std::collections::HashMap;
use std::path::PathBuf;

pub struct SessionBinding {
    /// pane_id → (session_id, jsonl_file_path)
    bindings: HashMap<String, BoundSession>,
}

pub struct BoundSession {
    pub session_id: String,
    pub jsonl_path: PathBuf,
    pub project_dir: PathBuf,
}

impl SessionBinding {
    pub fn new() -> Self {
        Self { bindings: HashMap::new() }
    }

    /// Bind or update a pane's session
    pub fn bind(&mut self, pane_id: &str, session_id: String, jsonl_path: PathBuf, project_dir: PathBuf) {
        self.bindings.insert(pane_id.to_string(), BoundSession {
            session_id,
            jsonl_path,
            project_dir,
        });
    }

    /// Get the bound session for a pane
    pub fn get(&self, pane_id: &str) -> Option<&BoundSession> {
        self.bindings.get(pane_id)
    }

    /// Remove binding when pane is closed
    pub fn unbind(&mut self, pane_id: &str) {
        self.bindings.remove(pane_id);
    }

    /// Check if session changed (different JSONL file is now active)
    pub fn session_changed(&self, pane_id: &str, new_session_id: &str) -> bool {
        self.bindings.get(pane_id)
            .map(|b| b.session_id != new_session_id)
            .unwrap_or(true)
    }
}
```

### Task 5: mod.rs — SessionScanner 主逻辑

协调所有组件，对外提供统一接口。

```rust
use gpui::*;
use std::path::Path;

pub struct SessionScanner {
    binding: SessionBinding,
    watchers: HashMap<String, JsonlFileWatcher>,  // pane_id → watcher
    // notify::RecommendedWatcher instances managed here
}

impl SessionScanner {
    /// Start scanning for a pane's worktree.
    /// Called when a pane starts running an agent.
    pub fn start_watching(&mut self, pane_id: &str, worktree_path: &Path) {
        let project_dir = path_mapper::worktree_to_claude_project_dir(worktree_path);
        if !project_dir.is_dir() {
            return; // Claude Code hasn't been used in this worktree
        }

        let mut watcher = JsonlFileWatcher::new(project_dir.clone());

        // Skip existing content in all JSONL files (only care about new writes)
        if let Ok(entries) = std::fs::read_dir(&project_dir) {
            for entry in entries.flatten() {
                if entry.path().extension().map(|e| e == "jsonl").unwrap_or(false) {
                    watcher.skip_existing(&entry.path());
                }
            }
        }

        self.watchers.insert(pane_id.to_string(), watcher);

        // Set up fsnotify watcher on project_dir
        // On file MODIFY event for *.jsonl:
        //   1. Read new lines
        //   2. Parse events
        //   3. Update binding if session_id changes
        //   4. Emit SessionEvents for StatusPublisher
    }

    /// Stop watching when pane is closed
    pub fn stop_watching(&mut self, pane_id: &str) {
        self.watchers.remove(pane_id);
        self.binding.unbind(pane_id);
    }

    /// Get current binding for a pane
    pub fn get_session(&self, pane_id: &str) -> Option<&BoundSession> {
        self.binding.get(pane_id)
    }
}
```

### Task 6: 集成到 StatusPublisher

在 `StatusPublisher.check_status()` 中加入 JSONL 信号源：

```rust
// 现有的 check_status 签名扩展
pub fn check_status(
    &mut self,
    pane_id: &str,
    process_status: ProcessStatus,
    shell_info: Option<&ShellPhaseInfo>,
    content: &str,
    skip_patterns: &[String],
    jsonl_event: Option<&SessionEvent>,  // 新增：JSONL 事件
) -> bool {
    // 优先级：ProcessStatus > OSC 133 > JSONL > Text Pattern > Unknown
    //
    // JSONL 事件的状态映射：
    //   Thinking / ToolUse / TextOutput → Running
    //   UserInput (enqueue) → Running (刚开始处理)
    //   TurnEnd → Idle (回到等待)
    //   ToolResult(is_error=true) → Error
}
```

### Task 7: 集成到 AppRoot 和 runtime 流程

在 agent runtime 创建时启动 SessionScanner，在关闭时停止：

```rust
// In runtime creation flow:
session_scanner.start_watching(&pane_id, &worktree_path);

// In runtime teardown:
session_scanner.stop_watching(&pane_id);
```

---

## 优先级和阶段

### Phase 1（MVP）
- [x] Task 1: path_mapper — 路径映射
- [x] Task 2: message_parser — JSONL 解析
- [x] Task 3: file_watcher — 文件监听
- [x] Task 4: binding — Pane ↔ Session 绑定

### Phase 2（集成）
- [x] Task 5: SessionScanner 主模块
- [x] Task 6: 集成到 StatusPublisher（via EventBus direct publish, no notify crate needed）
- [x] Task 7: 集成到 AppRoot/runtime 流程

### Phase 3（增强，未来）
- [ ] 在 UI 中展示 JSONL 提取的工具调用信息（比如 sidebar 显示 "Running: Bash"）
- [ ] 子 agent 状态跟踪（读取 subagents/ 目录）

---

## 依赖

- `notify` crate（文件系统监听）— 检查 Cargo.toml 是否已有
- `serde_json`（JSONL 解析）— 已有
- `dirs` crate（获取 home 目录）— 检查是否已有

## 风险和注意事项

1. **Claude Code 未安装或未使用**：project_dir 不存在时静默跳过，不影响现有功能
2. **JSONL 格式变更**：serde 解析用 Option 字段，未知字段忽略，向前兼容
3. **性能**：只读增量内容（seek to last position），不会重复扫描整个文件
4. **macOS fsnotify**：使用 kqueue，可靠且低延迟
