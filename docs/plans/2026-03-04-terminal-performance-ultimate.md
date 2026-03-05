# 终端性能终极优化

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 消除 saas-mono 等大型 monorepo workspace 的终端输入延迟和输出卡顿。

**根因:** sidebar 在每次 render 时为每个 worktree 同步 spawn `git diff --shortstat` 子进程 (`sidebar.rs:640`)，而 AppRoot ~95 处 `cx.notify()` 每次触发含 sidebar 的全量重渲染 → **N × git-subprocess × 帧率** 灾难性放大。

**Tech Stack:** Rust, GPUI, `std::sync::LazyLock`, `blocking::unblock`

---

## Phase 1: 从 sidebar render 彻底移除 diff stats

diff stats 从主链路完全去掉。sidebar 列表不再显示 +N/-N 数字，用户通过右键菜单 "View Diff" 按需查看。

### Task 1.1: 删除 sidebar render 中的 get_diff_stats 调用

**Files:**
- Modify: `src/ui/sidebar.rs`

**Step 1: 删除 import**

删除 line 5 的 `get_diff_stats` import：

```rust
// BEFORE:
use crate::worktree::{WorktreeInfo, get_diff_stats};

// AFTER:
use crate::worktree::WorktreeInfo;
```

**Step 2: 删除 render 中的 diff stats 调用和 UI 行** (line 640-654)

```rust
// 删除这两行 (line 640-641):
let (add, del, files) = get_diff_stats(&item.info.path).unwrap_or((0, 0, 0));
let _diff_str = format_diff_stats(add, del, files);

// 替换 diff stats 行 (line 651-654):
// BEFORE:
.child(
    div().pl(px(17.)).flex().flex_row().items_center().justify_between().gap(px(4.))
        .child(Self::render_diff_stats(add, del, files, meta_color))
        .child(div().text_size(px(10.)).text_color(meta_color).flex_shrink_0().child(last_time))
)

// AFTER: 仅保留时间
.child(
    div().pl(px(17.)).flex().flex_row().items_center().justify_end()
        .child(div().text_size(px(10.)).text_color(meta_color).flex_shrink_0().child(last_time))
)
```

### Task 1.2: 删除不再使用的辅助函数

**Files:**
- Modify: `src/ui/sidebar.rs`

删除 `fn render_diff_stats()` (line 278-298) 和 `fn format_diff_stats()` (line 565-585)，前提是没有其他调用点。用 grep 确认后删除。

### Task 1.3: 回归验证

```bash
RUSTUP_TOOLCHAIN=stable cargo test
RUSTUP_TOOLCHAIN=stable cargo check
```

确认编译通过、测试全过。

---

## Phase 2: StatusDetector regex 静态化

### Task 2.1: 将所有 regex 改为 LazyLock 静态编译

**Files:**
- Modify: `src/status_detector.rs`

**Step 1: 添加 LazyLock import 和静态 regex**

在文件顶部添加：

```rust
use std::sync::LazyLock;

static ANSI_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\x1b\[[0-9;]*m").unwrap());

static RUNNING_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| vec![
    Regex::new(r"(?i)thinking|analyzing|processing").unwrap(),
    Regex::new(r"(?i)reasoning|streaming").unwrap(),
    Regex::new(r"(?i)writing|generating|creating").unwrap(),
    Regex::new(r"(?i)running tool|executing|performing").unwrap(),
    Regex::new(r"(?i)loading|downloading|uploading").unwrap(),
    Regex::new(r"(?i)in progress|working on|busy").unwrap(),
    Regex::new(r"(?i)esc to interrupt|^\s*>").unwrap(),
]);

static WAITING_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| vec![
    Regex::new(r"^\?\s").unwrap(),
    Regex::new(r"^>\s").unwrap(),
    Regex::new(r"(?i)human:|user:|awaiting input").unwrap(),
    Regex::new(r"(?i)press enter|hit enter|continue\\?").unwrap(),
    Regex::new(r"(?i)waiting for|ready for").unwrap(),
    Regex::new(r"(?i)your turn|input required").unwrap(),
]);

static CONFIRM_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| vec![
    Regex::new(r"(?i)(requires approval|needs approval|permission to|don't ask again)").unwrap(),
    Regex::new(r"(?i)(Accept|Reject|Allow|Deny)\s+(all|this)").unwrap(),
    Regex::new(r"(?i)Always allow|Always deny").unwrap(),
    Regex::new(r"(?i)This command requires").unwrap(),
    Regex::new(r"(?i)approval required|approve\s").unwrap(),
    Regex::new(r"(?i)Run without asking").unwrap(),
]);

static ERROR_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| vec![
    Regex::new(r"(?i)error|exception|failure|failed").unwrap(),
    Regex::new(r"(?i)panic|abort|crash").unwrap(),
    Regex::new(r"(?i)traceback|stack trace").unwrap(),
    Regex::new(r"(?i)syntax error|compile error").unwrap(),
    Regex::new(r"(?i)command not found|exit code [1-9]").unwrap(),
]);
```

**Step 2: StatusDetector 字段改为静态引用**

```rust
pub struct StatusDetector {
    running_patterns: &'static [Regex],
    waiting_patterns: &'static [Regex],
    confirm_patterns: &'static [Regex],
    error_patterns: &'static [Regex],
    check_line_count: usize,
}

impl StatusDetector {
    pub fn new() -> Self {
        Self {
            running_patterns: &RUNNING_PATTERNS,
            waiting_patterns: &WAITING_PATTERNS,
            confirm_patterns: &CONFIRM_PATTERNS,
            error_patterns: &ERROR_PATTERNS,
            check_line_count: 50,
        }
    }
}
```

**Step 3: preprocess() 使用静态 ANSI_REGEX** (line 192)

```rust
// BEFORE:
let ansi_regex = Regex::new(r"\x1b\[[0-9;]*m").unwrap();
let without_ansi = ansi_regex.replace_all(content, "");

// AFTER:
let without_ansi = ANSI_REGEX.replace_all(content, "");
```

**Step 4: 调整 add_running_pattern / add_waiting_pattern 等方法**

这些方法需要用 `Vec` 包装以支持自定义 pattern：
- 如果确认只在测试中使用，可以保留但改为存储额外的 `custom_patterns: Vec<Regex>`
- 或删除（如果未使用）

### Task 2.2: DebouncedStatusTracker 移除冗余 detector

**Files:**
- Modify: `src/status_detector.rs`

`DebouncedStatusTracker` (line 267-273) 内部的 `detector` 字段在 `StatusPublisher` 中未被使用（`StatusPublisher` 使用自己的 detector）。

检查 `DebouncedStatusTracker.update()` 和 `update_from_text()` 是否被外部调用。如果仅在测试中使用，可让它们接受 `&StatusDetector` 参数而非持有。

### Task 2.3: 回归验证

```bash
RUSTUP_TOOLCHAIN=stable cargo test
RUSTUP_TOOLCHAIN=stable cargo check
```

---

## Phase 3: 减少不必要的 AppRoot 全量重渲染

### Task 3.1: Status 变化 fallback 路径不 notify AppRoot

**Files:**
- Modify: `src/ui/app_root.rs` (line 1490-1494)

```rust
// BEFORE:
if updated {
    let _ = entity.update(cx, |this, cx| {
        this.update_status_counts();
        cx.notify();  // ← 触发全量重渲染
    });
}

// AFTER:
if updated {
    let _ = entity.update(cx, |this, _cx| {
        this.update_status_counts();
        // 不 notify AppRoot — sidebar 的 status icon 已通过 pane_statuses Arc<Mutex> 共享
    });
}
```

### Task 3.2: Notification 事件移除无条件 AppRoot notify

**Files:**
- Modify: `src/ui/app_root.rs` (line 1523)

```rust
// BEFORE:
let _ = entity.update(cx, |_, cx| cx.notify());

// AFTER: 删除这行。NotificationPanelModel 已在 line 1518-1521 独立通知。
```

### Task 3.3: 回归验证

```bash
RUSTUP_TOOLCHAIN=stable cargo test
RUSTUP_TOOLCHAIN=stable cargo check
```

手动验证：状态变化时 sidebar icon 仍正确更新、notification badge 仍显示。

---

## Phase 4: 自适应输出批处理

### Task 4.1: 根据背压动态调整 batch size

**Files:**
- Modify: `src/ui/app_root.rs` (line 40-42, 882-891)

```rust
// BEFORE (line 42):
const TERMINAL_OUTPUT_MAX_CHUNKS_PER_BATCH: usize = 24;

// AFTER:
const TERMINAL_OUTPUT_MIN_BATCH: usize = 24;
const TERMINAL_OUTPUT_MAX_BATCH: usize = 256;
```

Output loop 修改 (line 882):

```rust
// BEFORE:
let mut batch_count = 1usize;
while batch_count < TERMINAL_OUTPUT_MAX_CHUNKS_PER_BATCH {

// AFTER:
let pending = rx.len();
let batch_limit = if pending > 100 {
    TERMINAL_OUTPUT_MAX_BATCH
} else {
    TERMINAL_OUTPUT_MIN_BATCH
};
let mut batch_count = 1usize;
while batch_count < batch_limit {
```

### Task 4.2: 回归验证

```bash
RUSTUP_TOOLCHAIN=stable cargo test
RUSTUP_TOOLCHAIN=stable cargo check
```

---

## 验证清单

1. `RUSTUP_TOOLCHAIN=stable cargo test` — 全部通过
2. `RUSTUP_TOOLCHAIN=stable cargo run` — 打开 saas-mono workspace
3. 终端输入即时响应，输出流畅无卡顿
4. sidebar 不再显示 +N/-N diff stats 数字
5. 右键菜单 "View Diff" 仍然可用
6. 快速 `cat` 大文件时 UI 不卡顿
7. agent status icon 正常切换
8. notification badge 正常显示
