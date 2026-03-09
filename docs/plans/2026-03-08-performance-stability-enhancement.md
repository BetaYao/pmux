# pmux 性能与稳定性增强方案

> 基于 WezTerm、Alacritty 架构分析，结合 pmux 现状，制定演进路线。

---

## 一、现状分析

### pmux 当前架构

| 层面 | 现状 | 评级 |
|------|------|------|
| **渲染** | GPUI Element trait，BatchedTextRun 文本合批，LayoutRect 背景合并 | 良好 |
| **终端模拟** | 依赖 alacritty_terminal (via gpui-terminal)，VTE 解析成熟 | 良好 |
| **I/O 管线** | flume channel + adaptive batching (24-256 chunks) | 良好 |
| **内存** | 无额外 scrollback 拷贝，直接读 alacritty grid | 良好 |
| **线程** | 专用 PTY reader/writer 线程 + GPUI async task | 良好 |
| **状态检测** | 80+ regex 线性扫描，200ms 节流，32KB 截断 | 可优化 |
| **损伤追踪** | 无 — 每帧全量重绘 terminal area | **需改进** |
| **字体缓存** | thread-local cell width 缓存，每次 shape_line 调 GPUI text system | 可优化 |

### 与 WezTerm/Alacritty 的关键差距

| 能力 | Alacritty | WezTerm | pmux |
|------|-----------|---------|------|
| **损伤追踪 (Damage Tracking)** | FrameDamage 双缓冲，脏矩形区域 | seqno 单调递增，行级脏标记 | 无 |
| **滚动优化** | Ring Buffer O(1) 旋转 | VecDeque O(1) 两端操作 | 依赖 alacritty_terminal 内部实现 |
| **字形缓存** | HashMap<GlyphKey, Glyph> + Atlas 1024x1024 | Shape Cache → Glyph Cache → Line Quad Cache 四级缓存 | 仅 cell width thread-local 缓存 |
| **批渲染** | 按 texture 分批，GPU instancing，65536 实例/批 | Quad Cache 预计算，DEC Synchronized Output | BatchedTextRun 文本合批 |
| **公平锁** | FairMutex 防写饥饿，64KB 上限释放 | 读写线程分离，socketpair 缓冲 | parking_lot::Mutex，无持锁上限 |
| **PTY 读缓冲** | 1MB read buffer | 1MB socketpair buffer | 4KB read buffer |
| **Cell 内存** | 24 bytes/cell，Arc<CellExtra> 惰性分配 | 24 bytes/cell，TeenyString 内联压缩 | 依赖 alacritty_terminal |

---

## 二、增强方案（按优先级排列）

### Phase 1: 高收益低风险改进（1-2 周）

#### 1.1 增大 PTY Read Buffer

**问题**: 当前 4KB read buffer，在高吞吐场景（`cat large_file.txt`）下需要频繁系统调用。

**对标**: Alacritty 使用 1MB buffer，WezTerm 使用 1MB socketpair。

**方案**:
```rust
// src/runtime/backends/local_pty.rs
// 当前: let mut buf = [0u8; 4096];
// 改为:
const READ_BUFFER_SIZE: usize = 64 * 1024; // 64KB，平衡内存与吞吐
let mut buf = vec![0u8; READ_BUFFER_SIZE];
```

**预期收益**: 大文件输出吞吐提升 5-10x，减少系统调用次数。

**风险**: 极低。仅改变缓冲区大小。

---

#### 1.2 RegexSet 替代线性 Regex 扫描

**问题**: StatusDetector 使用 80+ 编译好的 regex pattern，逐一匹配。

**方案**:
```rust
use regex::RegexSet;

// 构建时（一次性）
lazy_static! {
    static ref STATUS_PATTERNS: RegexSet = RegexSet::new(&[
        // confirm patterns
        r"(?i)(y/n|yes/no|\[Y/n\]|\[y/N\])",
        // idle patterns
        r"^[\$#%>❯→›»\\>] ?$",
        // error patterns
        r"(?i)(error|failed|panic|fatal)",
        // ... 其余 patterns
    ]).unwrap();
}

// 检测时
let matches: Vec<usize> = STATUS_PATTERNS.matches(content).into_iter().collect();
// 按优先级选取第一个命中的分类
```

**预期收益**: regex 匹配性能提升 3-5x（RegexSet 内部使用 Aho-Corasick 多模式匹配）。

**风险**: 低。需要仔细映射 pattern index → status category。

---

#### 1.3 Mutex 持锁时间上限

**问题**: `Terminal::term` 的 parking_lot::Mutex 在 `process_output()` 和 `with_content()` 中持锁，高输出速率时可能造成渲染延迟。

**对标**: Alacritty 限制每次持锁最多处理 64KB (MAX_LOCKED_READ)。

**方案**:
```rust
// 在 process_output 中分批处理
const MAX_PROCESS_PER_LOCK: usize = 64 * 1024; // 64KB

fn process_output_batched(&self, data: &[u8]) {
    let mut offset = 0;
    while offset < data.len() {
        let end = (offset + MAX_PROCESS_PER_LOCK).min(data.len());
        {
            let mut term = self.term.lock();
            // 解析 data[offset..end]
            term.process_output(&data[offset..end]);
        } // 释放锁，让渲染线程有机会获取
        offset = end;
    }
}
```

**预期收益**: 高吞吐场景下渲染帧率更稳定，避免长时间锁住导致 UI 卡顿。

---

### Phase 2: 渲染性能优化（2-4 周）

#### 2.1 行级损伤追踪 (Line-Level Damage Tracking)

**问题**: 当前每帧完整遍历 grid (rows × cols) 构建 BatchedTextRun 和 LayoutRect，即使只有光标闪烁。

**对标**:
- Alacritty: `DamageTracker` 双缓冲帧级脏区域，只重绘变化矩形
- WezTerm: `seqno` 单调递增序列号，行级脏标记

**方案 — 采用 seqno 方式（与 alacritty_terminal 内部一致）**:

```rust
// src/terminal/terminal_element.rs

struct TerminalElementState {
    // 新增: 上次渲染的行状态快照
    last_rendered_lines: Vec<u64>,   // 每行的 hash 值
    last_cursor_pos: (usize, usize),
    cached_text_runs: Vec<Vec<BatchedTextRun>>,  // 按行缓存
    cached_layout_rects: Vec<Vec<LayoutRect>>,
}

impl TerminalElement {
    fn paint(&mut self, ...) {
        let content = terminal.with_content(|term| {
            let mut dirty_lines = Vec::new();
            for (i, row) in term.grid().display_iter().enumerate() {
                let row_hash = hash_row(row);
                if self.state.last_rendered_lines.get(i) != Some(&row_hash) {
                    dirty_lines.push(i);
                    // 重新构建此行的 text runs 和 layout rects
                    self.state.cached_text_runs[i] = build_text_runs(row);
                    self.state.cached_layout_rects[i] = build_layout_rects(row);
                    self.state.last_rendered_lines[i] = row_hash;
                }
            }
        });

        // 绘制: 使用缓存的 runs，只有 dirty 行重新 shape
        for line in 0..visible_rows {
            paint_line_from_cache(
                &self.state.cached_text_runs[line],
                &self.state.cached_layout_rects[line],
            );
        }
    }
}
```

**预期收益**:
- Idle 状态 CPU 使用降低 60-80%（只有光标行重绘）
- 多 pane 场景下优化尤为明显

**风险**: 中等。需要处理窗口 resize、滚动、搜索高亮等使缓存失效的场景。

---

#### 2.2 文本 Shaping 结果缓存

**问题**: 每帧对每个 BatchedTextRun 调用 `window.text_system().shape_line()`，即使内容未变。

**对标**: WezTerm 的 Shape Cache (LFU eviction)。

**方案**:
```rust
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

#[derive(Hash, Eq, PartialEq, Clone)]
struct ShapeCacheKey {
    text: String,
    font_id: usize,       // 字体标识
    font_size: OrderedFloat<f32>,
}

struct ShapeCache {
    cache: HashMap<ShapeCacheKey, ShapedLine>,
    max_entries: usize,    // 例如 4096
}

impl ShapeCache {
    fn get_or_shape(
        &mut self,
        key: ShapeCacheKey,
        window: &mut Window,
        font: Font,
        font_size: Pixels,
    ) -> ShapedLine {
        if let Some(cached) = self.cache.get(&key) {
            return cached.clone();
        }
        let shaped = window.text_system().shape_line(
            key.text.clone().into(),
            font_size,
            &[TextRun { len: key.text.len(), font, color: Default::default(), ... }],
        ).unwrap();
        self.cache.insert(key, shaped.clone());
        shaped
    }
}
```

**预期收益**: 静态内容（提示符、不变文本）零 shaping 开销。典型场景下 shaping 调用减少 70-90%。

---

#### 2.3 光标独立渲染层

**问题**: 光标闪烁导致整行甚至全屏重绘。

**方案**: 光标作为独立 overlay 绘制，不触发行内容重新构建。

```rust
fn paint_cursor_overlay(&self, window: &mut Window, cursor_pos: Point<Pixels>, cell_size: Size<Pixels>) {
    // 直接绘制光标矩形，不参与行内容的脏检测
    let cursor_rect = Bounds::new(cursor_pos, cell_size);
    window.paint_quad(fill(cursor_rect, self.cursor_color));
}
```

---

### Phase 3: I/O 管线增强（2-3 周）

#### 3.1 PTY I/O 与解析分离

**问题**: 当前 PTY reader 直接推 raw bytes 到 flume，AppRoot 的 async task 同时做 VTE 解析 + 状态检测 + UI notify。解析和 UI 更新耦合在一起。

**对标**:
- WezTerm: Reader Thread → socketpair → Parser Thread → Terminal
- Alacritty: Reader Thread → FairMutex<Term> → Wakeup Event

**方案 — 三阶段管线**:
```
[PTY Reader Thread]       [Parser Task]           [UI Task]
  blocking read(64KB)  →  VTE parse + term.lock  →  cx.notify()
  flume::Sender         →  flume::Receiver         (debounced)
                           ContentExtractor
                           StatusPublisher
```

```rust
// 阶段 1: PTY Reader (已有，增大 buffer)
// 阶段 2: Parser Task (新增)
cx.spawn(async move {
    while let Ok(chunk) = output_rx.recv_async().await {
        // 批量收集
        let mut batch = vec![chunk];
        while let Ok(more) = output_rx.try_recv() {
            batch.push(more);
            if batch.len() >= 256 { break; }
        }

        // VTE 解析 + 状态检测（不在 UI 线程）
        for chunk in &batch {
            terminal.process_output(chunk);
            extractor.feed(chunk);
        }
        status_publisher.check_status(&extractor);

        // 通知 UI（合并为单次 notify）
        ui_notify_tx.send(()).ok();
    }
});

// 阶段 3: UI Task
cx.spawn(async move {
    while let Ok(()) = ui_notify_rx.recv_async().await {
        // drain 多余通知
        while ui_notify_rx.try_recv().is_ok() {}
        cx.update_entity(terminal_area_entity, |_, cx| cx.notify());
    }
});
```

**预期收益**: UI 线程不再阻塞在 VTE 解析上，渲染帧率更稳定。

---

#### 3.2 DEC Synchronized Update 支持 (CSI 2026)

**对标**: Alacritty 和 WezTerm 均支持。

**说明**: 当应用发送 `CSI ? 2026 h`（开始同步更新），终端缓存所有后续输出，直到收到 `CSI ? 2026 l`（结束同步更新）或超时（通常 150ms），才一次性应用并触发渲染。

**收益**: 消除全屏 TUI 应用（如 vim、htop、lazygit）的画面撕裂。

**方案**: 这应该在 alacritty_terminal / gpui-terminal 层面支持。检查 gpui-terminal 是否已支持此模式。若未支持，向上游提 PR 或在 fork 中添加。

---

### Phase 4: 内存优化（1-2 周）

#### 4.1 Scrollback 内存策略

**当前**: 依赖 alacritty_terminal 默认配置（通常 10000 行）。

**建议**:
- 提供用户可配置的 scrollback 行数（config.json）
- 对于 pmux 的多 pane 场景，默认值应适中（例如 5000 行/pane）
- 考虑总内存上限：`max_scrollback_memory = 50MB`，动态调整各 pane 行数

```rust
// config.rs
pub struct TerminalConfig {
    pub scrollback_lines: usize,        // 默认 5000
    pub max_scrollback_memory_mb: usize, // 默认 50
}
```

---

#### 4.2 ContentExtractor 文本缓冲优化

**问题**: `text_buf` 持续累积可见文本，仅在 `take_content()` 时清空。

**方案**:
```rust
// 使用固定大小环形缓冲
struct ContentExtractor {
    text_ring: RingBuffer<u8>,  // 固定 64KB
    // ...
}
```

这避免了在高输出场景下 text_buf 无限增长。

---

### Phase 5: 稳定性增强（持续）

#### 5.1 PTY I/O 错误恢复

**问题**: PTY read 错误导致 pane 静默死亡。

**方案**:
```rust
// reader thread
loop {
    match master.read(&mut buf) {
        Ok(0) => {
            // PTY 关闭 — 发送 Exited 事件
            event_bus.publish(RuntimeEvent::PaneExited { pane_id });
            break;
        }
        Ok(n) => output_tx.send(buf[..n].to_vec()).ok(),
        Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
        Err(e) => {
            log::error!("PTY read error for pane {}: {}", pane_id, e);
            event_bus.publish(RuntimeEvent::PaneError {
                pane_id,
                error: e.to_string(),
            });
            // 可选: 短暂退避后重试
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}
```

---

#### 5.2 Channel 背压处理

**问题**: flume unbounded channel 在极端吞吐下可能内存暴涨。

**方案**: 改用 bounded channel + 背压策略。

```rust
// 使用 bounded channel，容量 = 1024 chunks
let (output_tx, output_rx) = flume::bounded(1024);

// writer 端: 背压时丢弃或合并
match output_tx.try_send(chunk) {
    Ok(()) => {},
    Err(flume::TrySendError::Full(_)) => {
        // 策略 1: 阻塞等待（PTY reader 自然减速）
        output_tx.send(chunk).ok();
        // 策略 2: 或者合并到上一个 chunk
    }
}
```

---

#### 5.3 tmux 查询节流

**问题**: tmux `display-message` 每帧调用来检测活跃进程。

**方案**:
```rust
// 缓存 tmux 查询结果，1 秒刷新一次
struct TmuxQueryCache {
    pane_pid: Option<u32>,
    pane_current_command: Option<String>,
    last_query: Instant,
    ttl: Duration, // 1s
}

impl TmuxQueryCache {
    fn get_or_refresh(&mut self) -> &str {
        if self.last_query.elapsed() > self.ttl {
            self.pane_current_command = tmux_display_message(...);
            self.last_query = Instant::now();
        }
        self.pane_current_command.as_deref().unwrap_or("")
    }
}
```

---

## 三、优先级路线图

```
时间线（大致）:

Week 1-2   Phase 1 — 低风险高收益
           ├─ 1.1 增大 PTY Read Buffer (0.5d)
           ├─ 1.2 RegexSet 优化 (1-2d)
           └─ 1.3 Mutex 持锁上限 (1d)

Week 3-6   Phase 2 — 渲染优化（核心收益）
           ├─ 2.1 行级损伤追踪 (1w)
           ├─ 2.2 Shape 缓存 (3-4d)
           └─ 2.3 光标独立渲染 (1d)

Week 5-7   Phase 3 — I/O 管线重构
           ├─ 3.1 解析与 UI 分离 (1w)
           └─ 3.2 DEC Synchronized Update (2-3d)

Week 7-8   Phase 4 — 内存优化
           ├─ 4.1 Scrollback 配置化 (2d)
           └─ 4.2 ContentExtractor 环形缓冲 (1d)

持续       Phase 5 — 稳定性
           ├─ 5.1 PTY 错误恢复 (1d)
           ├─ 5.2 Channel 背压 (1d)
           └─ 5.3 tmux 查询缓存 (0.5d)
```

---

## 四、基准测试方案

实施任何优化前，先建立性能基线:

### 测试场景

| 场景 | 命令 | 测量指标 |
|------|------|----------|
| **大文件输出** | `cat /dev/urandom \| base64 \| head -100000` | 吞吐量 (MB/s)、CPU% |
| **高频刷新 TUI** | `htop` / `top` 运行 60 秒 | 平均帧率、CPU% |
| **Idle 状态** | 终端空闲 60 秒 | CPU%（应趋近 0%） |
| **多 Pane** | 4 pane 同时 `cat large_file` | 总 CPU%、内存峰值 |
| **滚动** | 快速滚动 10000 行历史 | 帧率、延迟 |
| **输入延迟** | 键入字符到回显 | 延迟 (ms) |

### 测量工具

```bash
# CPU 和内存 profiling
cargo instruments -t "Time Profiler" -- cargo run --release
cargo instruments -t "Allocations" -- cargo run --release

# 帧率测量
# 在 TerminalElement::paint 中加入计时:
let start = Instant::now();
// ... paint logic ...
let frame_time = start.elapsed();
FRAME_TIMES.lock().push(frame_time);

# 吞吐量测试
time cat /dev/urandom | base64 | head -n 100000 > /dev/null
# 在 pmux 中运行同样命令，对比完成时间
```

---

## 五、非目标（当前阶段不做）

| 项目 | 原因 |
|------|------|
| 自建 VTE 解析器 | alacritty_terminal 已足够成熟，维护成本高 |
| 自建 GPU 渲染器 | GPUI 提供了足够好的 GPU 加速渲染 |
| OpenGL shader 直接操作 | GPUI 抽象层已封装，直接操作破坏兼容性 |
| 字形 Atlas 管理 | GPUI text system 内部处理，无需重复 |
| FairMutex 实现 | parking_lot 已足够，除非实测发现写饥饿 |
| 多 GPU backend 支持 | GPUI 已处理 Metal/Vulkan 选择 |

---

## 六、总结

pmux 的终端底层（alacritty_terminal + GPUI）已具备良好基础。主要的优化空间在于:

1. **渲染层**: 引入损伤追踪和 shape 缓存，消除不必要的重绘和重复 shaping — 这是**最大收益点**
2. **I/O 层**: 增大读缓冲、分离解析与渲染线程 — 提升高吞吐稳定性
3. **状态检测**: RegexSet 替代线性扫描 — 降低 CPU 开销
4. **稳定性**: 错误恢复、背压控制、tmux 缓存 — 提升长时间运行可靠性

这些改进可以让 pmux 在保持当前功能特色（多 worktree agent 管理）的同时，达到接近专业终端模拟器的性能水平。

---

## 七、文本检测、格式处理与窗口 Resize 的 Bug 分析

> 基于 WezTerm/Alacritty 的成熟实现，对比 pmux 现有代码，发现以下问题。

### 7.1 文本内容检测 (ContentExtractor) 的问题

#### BUG-1: `is_printable()` 丢弃所有非 ASCII 文本 [严重]

**文件**: `src/terminal/content_extractor.rs:108-109`

```rust
fn is_printable(b: u8) -> bool {
    matches!(b, 0x20..=0x7e | b'\n' | b'\r' | b'\t')
}
```

**问题**: 只接受 ASCII (0x20-0x7E)，所有 >= 0x80 的字节被丢弃。这意味着：
- 中文/日文/韩文全部丢失（"Error: 编译失败" → "Error: "）
- Emoji 丢失（"✓ Done" → " Done"）
- 带重音符的拉丁字符丢失（"café" → "caf"）

**对标**:
- **Alacritty**: VTE parser 的 `Handler::input(c: char)` 直接操作 Unicode char，完全不存在这个问题。Cell 存储的是 `char` 类型。
- **WezTerm**: `Action::Print(char)` 和 `Action::PrintString(String)` 在解析层就完成了 byte → char 转换。`flush_print()` 还做了 Unicode NFC 归一化 + grapheme clustering。

**修复方案**:

ContentExtractor 操作的是 raw bytes（来自 PTY output），而 UTF-8 是多字节编码。不能逐字节判断 printable。

```rust
pub struct ContentExtractor {
    osc133: Osc133Parser,
    phase: ShellPhase,
    text_buf: Vec<u8>,
    text_state: TextState,
    utf8_buf: [u8; 4],      // 新增: UTF-8 多字节累积
    utf8_expected: usize,    // 新增: 还需要的字节数
    utf8_len: usize,         // 新增: 已累积的字节数
}

fn advance_text(&mut self, b: u8) {
    match self.text_state {
        TextState::Normal => {
            if b == ESC {
                self.text_state = TextState::AfterEsc;
            } else if self.utf8_expected > 0 {
                // 正在累积 UTF-8 多字节序列
                self.utf8_buf[self.utf8_len] = b;
                self.utf8_len += 1;
                self.utf8_expected -= 1;
                if self.utf8_expected == 0 {
                    // UTF-8 序列完整，写入 text_buf
                    self.text_buf.extend_from_slice(&self.utf8_buf[..self.utf8_len]);
                    self.utf8_len = 0;
                }
            } else if b >= 0xC0 && b <= 0xF7 {
                // UTF-8 多字节起始字节
                self.utf8_buf[0] = b;
                self.utf8_len = 1;
                self.utf8_expected = if b < 0xE0 { 1 }
                    else if b < 0xF0 { 2 }
                    else { 3 };
            } else if b >= 0x20 && b <= 0x7E || b == b'\n' || b == b'\r' || b == b'\t' {
                self.text_buf.push(b);
            }
            // 0x80-0xBF 单独出现 = 无效 UTF-8，丢弃
            // 0x00-0x1F (除 \n\r\t) = 控制字符，丢弃
        }
        // ... 其余 state 不变
    }
}
```

---

#### BUG-2: `extract_last_line()` 字符串切片可能 panic [严重]

**文件**: `src/terminal/content_extractor.rs:146-147`

```rust
if l.len() > max_len {
    format!("{}...", &l[..max_len])
}
```

**问题**: `l.len()` 返回的是**字节数**，`&l[..max_len]` 按**字节**切片。如果 `max_len` 落在多字节 UTF-8 字符的中间，会 panic：

```
thread panicked at 'byte index 3 is not a char boundary; it is inside 'é' (bytes 2..4) of `café`'
```

**对标**:
- **Alacritty**: 从不按字节切片 String。grid 操作使用 `Column` 索引，每个 Cell 存储完整 `char`。
- **WezTerm**: `Line::wrap()` 按 cell 宽度切分，`visible_cells()` 迭代器返回完整 grapheme。

**修复方案**:
```rust
pub fn extract_last_line(content: &str, max_len: usize) -> String {
    content
        .lines()
        .rev()
        .map(|l| l.trim())
        .find(|l| {
            !l.is_empty()
                && l.len() > 1
                && !l.chars().all(|c| matches!(c, '-' | '=' | '*' | '_' | '~' | '.' | ' '))
        })
        .map(|l| {
            // 按字符数截断，不按字节
            let char_count = l.chars().count();
            if char_count > max_len {
                let truncated: String = l.chars().take(max_len).collect();
                format!("{}...", truncated)
            } else {
                l.to_string()
            }
        })
        .unwrap_or_default()
}
```

---

#### BUG-3: ANSI_REGEX 只剥离 SGR，遗漏其他 CSI 序列 [中等]

**文件**: `src/status_detector.rs:8`

```rust
static ANSI_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\x1b\[[0-9;]*m").unwrap());
```

**问题**: 只匹配 `ESC[...m`（SGR 颜色/样式），遗漏：
- 光标移动: `ESC[H`, `ESC[5A`, `ESC[10;20H`
- 清屏/清行: `ESC[2J`, `ESC[K`
- 带 `?` 的私有序列: `ESC[?25h`（显示光标）
- 带字母后缀的其他 CSI: `ESC[5C`（光标前移）

这些遗留在 status detector 检查的文本中，可能导致误匹配。

**对标**:
- **Alacritty**: Handler trait 方法完全消费所有 CSI 序列，Cell 中只存储字符，不存在需要剥离的问题。
- **WezTerm**: 同理，`Action::Print` 只包含可见字符。

**修复方案**:
```rust
// 匹配所有 CSI 序列: ESC[ (参数字节) (中间字节) (终结字节)
static ANSI_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\x1b\[[0-9;?]*[A-Za-z]").unwrap());

// 更全面: 也处理 OSC (ESC]...BEL 或 ESC]...ESC\)
static ANSI_FULL_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(
        r"(\x1b\[[0-9;?]*[A-Za-z]|\x1b\][^\x07]*(?:\x07|\x1b\\)|\x1b[()][0-9A-Za-z])"
    ).unwrap());
```

---

### 7.2 文本格式渲染的问题

#### BUG-4: Wide Character (CJK) 背景色覆盖不完整

**文件**: `src/terminal/terminal_element.rs:374-378`

```rust
if cell.flags.contains(Flags::WIDE_CHAR) {
    if let Some(ref mut rect) = current_bg {
        rect.extend();
    }
}
```

**问题**: 只在 `current_bg` 已存在时扩展。如果 wide char 的背景是默认色（`is_default_bg` 为 true），但紧接在它后面是一个非默认色 cell，这种情况处理正确。但如果 wide char 本身有非默认背景色且是行首第一个非默认色 cell，`current_bg` 已经在前面的 `if !is_default_bg` 分支中创建了，所以这里的扩展是对的。

但考虑以下情况：wide char 有非默认 bg，但它前面紧邻的是不同颜色的 bg。此时 `current_bg` 被替换为新的 rect（1 cell 宽），然后 extend 使它变成 2 cells，这是正确的。

**实际风险**: 低，但不如 Alacritty 的处理清晰。

**对标 Alacritty**:
- `WIDE_CHAR` 占 2 列，`WIDE_CHAR_SPACER` 在渲染时被 skip
- Renderer 对 `RenderableCell` 直接知道宽度是 2，统一处理
- 位置计算和背景绘制在同一处完成，不会遗漏

---

#### BUG-5: DIM (Faint) 属性未渲染

**文件**: `src/terminal/terminal_element.rs:384-388`

```rust
let font = match (cell.flags.contains(Flags::BOLD), cell.flags.contains(Flags::ITALIC)) {
    (true, true) => state.font_bold_italic.clone(),
    (true, false) => state.font_bold.clone(),
    (false, true) => state.font_italic.clone(),
    (false, false) => state.font.clone(),
};
```

**问题**: 检查了 BOLD 和 ITALIC，但 **DIM (Flags::DIM)** 完全被忽略。DIM 文本应该降低亮度/透明度。

**对标**:
- **Alacritty** (`display/content.rs`): `compute_fg_rgb()` 检查 `Flags::DIM` 并将前景色亮度减半。
- **WezTerm** (`CellAttributes`): `intensity` 字段有三个值: Normal, Bold, Half (dim)，渲染时对 Half 降低亮度。

**修复方案**:
```rust
let mut fg_hsla = self.palette.resolve(fg, colors);

// 处理 DIM: 降低前景色亮度
if cell.flags.contains(Flags::DIM) {
    fg_hsla.l *= 0.5;  // 亮度减半
    // 或者: fg_hsla.a *= 0.5; (降低透明度)
}
```

---

#### BUG-6: HIDDEN (Invisible) 属性未渲染

**问题**: `Flags::HIDDEN` 意味着文本应该不可见（前景色 = 背景色），但 pmux 没有处理这个 flag。

**对标**:
- **Alacritty**: `Flags::HIDDEN` 导致前景色设为背景色（或跳过渲染）
- **WezTerm**: `invisible` bit 使文本不绘制

**修复方案**:
```rust
if cell.flags.contains(Flags::HIDDEN) {
    fg_hsla = bg_hsla; // 前景色等于背景色，文本不可见
}
```

---

#### BUG-7: 多种下划线样式未区分

**文件**: `src/terminal/terminal_element.rs:401-407`

```rust
underline: if cell.flags.contains(Flags::UNDERLINE) {
    Some(UnderlineStyle {
        thickness: px(1.0),
        color: Some(fg_hsla),
        wavy: false,
    })
} else {
    None
},
```

**问题**: 只检查了 `Flags::UNDERLINE`，未处理 `DOUBLE_UNDERLINE`、`UNDERCURL`（波浪线）、`DOTTED_UNDERLINE`、`DASHED_UNDERLINE`。

**对标**:
- **Alacritty** Cell Flags 中有 5 种下划线: `UNDERLINE`, `DOUBLE_UNDERLINE`, `UNDERCURL`, `DOTTED_UNDERLINE`, `DASHED_UNDERLINE`
- **WezTerm** `CellAttributes::underline` 是枚举: `None`, `Single`, `Double`, `Curly`, `Dotted`, `Dashed`

**修复方案**:
```rust
// 检查所有下划线变体
let underline = if cell.flags.contains(Flags::UNDERCURL) {
    Some(UnderlineStyle { thickness: px(1.0), color: Some(fg_hsla), wavy: true })
} else if cell.flags.intersects(
    Flags::UNDERLINE | Flags::DOUBLE_UNDERLINE | Flags::DOTTED_UNDERLINE | Flags::DASHED_UNDERLINE
) {
    Some(UnderlineStyle { thickness: px(1.0), color: Some(fg_hsla), wavy: false })
    // TODO: GPUI 目前可能不支持 double/dotted/dashed 样式，但至少渲染为单下划线
} else {
    None
};
```

---

### 7.3 窗口 Resize 的问题

#### BUG-8: tmux ioctl 返回值被忽略 [中等]

**文件**: `src/runtime/backends/tmux_control_mode.rs:877`

```rust
unsafe {
    let ws = libc::winsize { ws_col: client_cols, ws_row: client_rows, ... };
    libc::ioctl(self.pty_master_fd, libc::TIOCSWINSZ, &ws);
    //          ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    //          返回值被忽略！如果 fd 无效，resize 静默失败
}
```

**对标**:
- **Alacritty** (`tty/unix.rs:406-437`): 检查 ioctl 返回值，失败时 die!
  ```rust
  let res = unsafe { libc::ioctl(self.file.as_raw_fd(), libc::TIOCSWINSZ, &win) };
  if res < 0 { die!("ioctl TIOCSWINSZ failed: {}", Error::last_os_error()); }
  ```
- **WezTerm**: PTY resize 通过 `portable-pty` 抽象层，内部检查错误

**修复方案**:
```rust
unsafe {
    let ws = libc::winsize { ws_col: client_cols, ws_row: client_rows, ws_xpixel: 0, ws_ypixel: 0 };
    let ret = libc::ioctl(self.pty_master_fd, libc::TIOCSWINSZ, &ws);
    if ret < 0 {
        log::warn!(
            "TIOCSWINSZ ioctl failed for fd {}: {}",
            self.pty_master_fd,
            std::io::Error::last_os_error()
        );
    }
}
```

---

#### BUG-9: Resize 在 prepaint 中触发，与渲染同帧 [中等]

**文件**: `src/terminal/terminal_element.rs:236-248`

```rust
// In prepaint():
let current_size = self.terminal.size();
if current_size.cols as usize != cols || current_size.rows as usize != rows {
    self.terminal.resize(new_size);      // 1. 立即 resize 终端 grid
    if let Some(ref cb) = self.on_resize {
        cb(cols as u16, rows as u16);    // 2. 回调通知 runtime 做 PTY resize
    }
}
// ... 然后紧接着执行 paint()
```

**问题**: `terminal.resize()` 触发 alacritty_terminal 内部的 grid reflow（行重排），然后同帧内的 `paint()` 读取 grid。如果 reflow 导致行数变化（scrollback 交互），paint 使用的 `state.rows`/`state.cols` 是 prepaint 计算的新值，但 grid 内部状态可能还有不一致。

此外，PTY resize callback 是异步的（通过 runtime.resize → ioctl TIOCSWINSZ），进程收到 SIGWINCH 后会重绘。这个重绘的输出会在后续帧到达，但在当前帧，grid 内容可能是旧宽度的。

**对标**:
- **Alacritty**: resize 通过 event channel（`Msg::Resize`）发送，event loop 先处理所有 resize，**然后**才处理 PTY 读取。渲染在单独帧。
  ```rust
  // drain_recv_channel 先处理 resize
  Msg::Resize(window_size) => self.pty.on_resize(window_size),
  // 然后才处理 PTY I/O
  ```
- **WezTerm**: `LocalPane::resize()` 先 resize PTY，再 resize Terminal model。渲染在下一帧。

**修复方案**:
考虑将 resize 延迟到下一帧，或者至少保证 paint 使用 resize 后的正确维度：
```rust
// 在 prepaint 中记录需要 resize，但不立即执行
// 在下一个 notify cycle 中执行 resize
// 或者: 在 paint 开始时重新查询 grid 维度而不是用 prepaint 缓存的值
```

实际上当前实现在大多数情况下工作正常，因为 alacritty_terminal 的 `resize()` 是同步的。主要风险是闪烁和一帧的视觉不一致。

---

#### BUG-10: Resize 后 Selection 坐标未失效 [低]

**文件**: `src/terminal/terminal_element.rs:461-500`

Selection range 在 resize 后可能指向无效的 grid 坐标（旧的行列数）。虽然不会 panic（Alacritty grid 的 Index 实现会 wrap），但可能导致选区视觉位置不正确。

**对标**:
- **Alacritty**: resize 时清除或调整 selection
- **WezTerm**: resize 时 selection 坐标随 reflow 一起调整

**修复**: resize 时清除 selection。

---

#### BUG-11: 非 tmux 后端 resize 不通知 PTY [低]

**文件**: `src/runtime/backends/local_pty.rs:197-215` (LocalPtyAgent)

```rust
fn resize(&self, pane_id: &PaneId, cols: u16, rows: u16) -> Result<(), RuntimeError> {
    pane.cols.store(cols, Ordering::SeqCst);
    pane.rows.store(rows, Ordering::SeqCst);
    guard.resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
}
```

这里 `pixel_width` 和 `pixel_height` 总是 0。一些现代终端应用（如 kitty graphics）依赖像素尺寸来正确渲染图片。

**对标**:
- **Alacritty**: `ToWinsize` 实现计算 `ws_xpixel = ws_col * cell_width`, `ws_ypixel = ws_row * cell_height`
- **WezTerm**: `PtySize` 包含 `pixel_width` 和 `pixel_height`

**修复**: 从 TerminalElement 传递 cell 像素尺寸到 resize callback。

---

### 7.4 借鉴总结

#### 从 Alacritty 借鉴

| 实践 | pmux 当前 | Alacritty 做法 | 建议 |
|------|-----------|---------------|------|
| **文本存储** | ContentExtractor 逐字节 FSM | Cell 存 `char`，VTE handler 消费所有序列 | 修复 UTF-8 多字节处理 |
| **属性完整性** | 仅 BOLD/ITALIC/UNDERLINE/STRIKEOUT/INVERSE | 15 个 flag + CellExtra | 补充 DIM/HIDDEN/多种下划线 |
| **Wide char** | WIDE_CHAR_SPACER skip + 背景扩展 | 两 cell 系统 + 完整 reflow | 基本一致，已可用 |
| **Resize 序列** | prepaint 中 resize + 同帧 paint | event channel 先 resize 后 paint | 考虑分帧 |
| **Resize reflow** | 依赖 alacritty_terminal 内部 | 完整的 grow/shrink + cursor 调整 | 已继承，无需改动 |
| **Pixel size** | resize 时传 0 | 传 cell_width * cols | 补充像素尺寸 |

#### 从 WezTerm 借鉴

| 实践 | pmux 当前 | WezTerm 做法 | 建议 |
|------|-----------|-------------|------|
| **Unicode 处理** | ASCII-only is_printable | NFC 归一化 + grapheme clustering | 至少修复 UTF-8 多字节 |
| **属性存储** | 依赖 alacritty Cell | 32-bit bitfield + FatAttributes | 已通过 alacritty 处理 |
| **Semantic zones** | OSC 133 → ShellPhase | SemanticType (Prompt/Input/Output) per cell | 可扩展语义区域标记 |
| **Resize rewrap** | 依赖 alacritty | 完整 rewrap 含 cursor 追踪 + scrollback 裁剪 | 已通过 alacritty 处理 |
| **ioctl 错误** | 忽略返回值 | 通过 portable-pty 处理 | 必须检查返回值 |
| **ClusteredLine** | 无 | 相同属性的连续 cells 合并存储，节省 90% 内存 | 非目标（alacritty 内部管理） |

---

### 7.5 修复优先级

```
立即修复（会导致 crash 或功能缺失）:
  ├─ BUG-1: is_printable() UTF-8 支持         [影响所有非 ASCII 用户]
  ├─ BUG-2: extract_last_line() 字节切片 panic [任何非 ASCII 内容可触发]
  └─ BUG-3: ANSI_REGEX 不完整                 [影响状态检测准确性]

近期修复（影响视觉正确性）:
  ├─ BUG-5: DIM 属性未渲染                    [影响 ls --color 等]
  ├─ BUG-6: HIDDEN 属性未渲染                 [影响密码输入等]
  ├─ BUG-7: 多种下划线未区分                   [影响 LSP 错误显示等]
  └─ BUG-8: tmux ioctl 返回值检查              [可能导致 resize 静默失败]

中期改进:
  ├─ BUG-9: resize 同帧渲染                    [偶尔一帧闪烁]
  ├─ BUG-10: resize 后 selection 失效           [选区偶尔错位]
  └─ BUG-11: pixel_width/height 为 0            [影响图片终端协议]
```
