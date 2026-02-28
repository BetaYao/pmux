# Terminal 性能终极方案：Zed/Ghostty 级渲染 Brainstorm

> **目标**：达到 Zed、Ghostty 级别的终端输入/滚动流畅度。  
> **现状**：style-run 批处理已将元素从 O(cells) 降到 O(style-runs)（约 97% 减少），但仍是 div/span 元素树。  
> **结论**：需实现方案 A —— 自定义 TerminalElement 直接 GPU 绘制。

---

## 1. 现状 vs 目标

### 1.1 当前渲染路径

```
Grid (alacritty_terminal)
    → display_iter() → StyledSegment
    → render_batch_row() → div().children([span, span, ...])
    → GPUI 布局 (Taffy) + 每元素 paint
```

- **元素数量**：~60 个/帧（80×24 终端，每行 1 flex + 若干 span）
- **每帧成本**：60× request_layout、60× prepaint、60× paint、60× Taffy 节点
- **已做优化**：Phase 1–4（worktree 缓存、异步切换、terminal 条件 notify、Arc 优化）—— 均未改渲染方式

### 1.2 Zed / Ghostty 的差异

| 方面 | pmux 当前 | Zed (GPUI) | Ghostty |
|------|-----------|------------|---------|
| 元素树 | 60+ div/span | 1 个 TerminalElement | 无 GPUI，直接 Metal/OpenGL |
| 布局 | Taffy 对每元素 | 1 个固定尺寸元素 | 无 |
| 绘制 | 每元素各自 paint | paint() 内批量 paint_quad + paint_glyph | 直接 GPU 命令 |
| 文字 | span 走 GPUI 文本 | ShapedLine::paint / paint_glyph | 自有 glyph atlas |

---

## 2. GPUI 底层绘制 API（已验证）

从 Zed 的 `gpui/src/window.rs` 可见，`Window` 在 paint 阶段提供：

```rust
// 背景、边框、圆角矩形
pub fn paint_quad(&mut self, quad: PaintQuad)

// 单字形（文档建议优先用 ShapedLine::paint 批量）
pub fn paint_glyph(&mut self, origin: Point, font_id: FontId, glyph_id: GlyphId, font_size: Pixels, color: Hsla) -> Result<()>

// 文字装饰
pub fn paint_underline(...)
pub fn paint_strikethrough(...)

// 路径、SVG、图片等
pub fn paint_path(...)
pub fn paint_svg(...)
pub fn paint_image(...)
```

- `paint_quad`：插入 `Quad` 到 `next_frame.scene`，用于背景、光标
- `paint_glyph`：通过 `text_system().rasterize_glyph()` 光栅化，放入 `sprite_atlas`，插入 `MonochromeSprite` 或 `SubpixelSprite`
- 文档建议：**优先使用 `ShapedLine::paint` 或 `WrappedLine::paint`** 做批量文字

---

## 3. 方案 A：Zed 式 TerminalElement（推荐）

### 3.1 思路

新建 `terminal_element.rs`，实现 `Element` trait：

1. **request_layout**：返回固定尺寸 `(cols * cell_w, rows * cell_h)`，无需 Taffy 子节点
2. **prepaint**：最简，仅提交 bounds
3. **paint**：在 `paint()` 内直接调用 `window.paint_quad`、`window.paint_glyph`（或 `ShapedLine::paint`）

**核心**：**1 个 Element** 代替 60+ 个 div/span，所有绘制在单一 `paint()` 中完成。

### 3.2 数据流（保留现有 pipeline）

```
TerminalEngine / renderable_content()
    → display_iter() → group_cells_into_segments() → Vec<StyledSegment> per row
    → TerminalElement::new(segments, cursor, colors, ...)
    → Element::paint() 内：
        for row in visible_rows:
            for seg in row:
                window.paint_quad(bg_quad)      // 背景
                window.shape_and_paint(seg.text)  // 或遍历 glyph
            if cursor_in_row:
                window.paint_quad(cursor_quad)
```

- **可复用**：`StyledSegment`、`group_cells_into_segments`、row cache、viewport culling 全部保留
- **变化**：不再 `render_batch_row() -> div().children()`，改为 `TerminalElement` 持有 `Vec<Vec<StyledSegment>>` 并在 `paint` 中绘制

### 3.3 技术要点

| 项目 | 说明 |
|------|------|
| **FontId / TextSystem** | 从 `window.text_system()` 获取；需配置终端字体（如 Menlo 12pt） |
| **字形批量** | 优先用 `ShapedLine::paint` 绘制整段文字；若无，则对每字符 `paint_glyph` |
| **光标** | `paint_quad` 画一矩形，与当前实现逻辑一致 |
| **Bold / Underline** | Bold 用 `FontWeight::BOLD` 再 shape；Underline 用 `paint_underline` |
| **CJK / 宽字符** | 依赖 TextSystem 的 shaping；需验证 double-width 与 cell 对齐 |

### 3.4 预估复杂度

- **Spike**：1–2 天，验证 `ShapedLine::paint` / `paint_glyph` 是否能满足终端需求
- **实现**：3–5 天，含 request_layout/prepaint/paint、字体配置、光标与样式
- **风险**：GPUI 版本 `rev = "269b03f4"` 的 TextSystem API 可能和 main 有差异，需对照源码

---

## 4. 其他方案（对比）

### 4.1 方案 B：GPUI TextRun / 高层文本 API

- **思路**：若 GPUI 提供类似 `TextRun` 的多样式文本，可尝试 1 个 `TextRun` 代替多 span
- **问题**：当前实现已是「每段一个 span」，TextRun 可能仍会生成多内部元素；且需确认是否支持固定宽字体、终端布局
- **结论**：可作为 Spike 的一部分探索，不作为主路径

### 4.2 方案 C：Texture Atlas 预渲染

- **思路**：预渲染 256×N 的 ASCII/常用字符到纹理，每帧按 cell 索引 blit
- **优点**：draw call 极少，适合超大数据量
- **缺点**：需要自建 atlas、字体回退、样式组合爆炸（fg×bg×flags），工程量大
- **结论**：pmux 规模下不必要，方案 A 更合适

### 4.3 方案 D：脱离 GPUI，自建渲染（Ghostty 模式）

- **思路**：终端区域不用 GPUI，用 Metal/OpenGL 直接画
- **缺点**：与 GPUI 窗口/布局/事件系统割裂，需处理 overlay、焦点、IME 等
- **结论**：仅在 GPUI 成为明显瓶颈时考虑，当前不采纳

---

## 5.  incremental 实施路径

### Phase 0：Spike（必做）

1. 在 Zed repo 中定位 `ShapedLine`、`WrappedLine`、`TextSystem` 的用法
2. 写最小 `Element`，在 `paint()` 中调用 `paint_quad`、`paint_glyph` 或 `ShapedLine::paint`
3. 验证字体、颜色、baseline 是否满足终端需求

### Phase 1：TerminalElement 骨架

1. 新建 `src/ui/terminal_element.rs`
2. 实现 `Element`（request_layout 返回固定尺寸，prepaint 简单，paint 占位）
3. 在 `TerminalView::render` 中先用 `TerminalElement` 替代现有 `div().children(line_elements)`，确保布局正确

### Phase 2：paint 实现

1. 在 `paint()` 中遍历 `StyledSegment`，对每段调用 `paint_quad`（背景）+ 文字绘制
2. 实现光标 `paint_quad`
3. 处理 Bold、Underline（若 TextSystem 支持则用，否则先简化）

### Phase 3：性能与正确性

1. 保留 row cache、viewport culling，避免重复计算
2. 回归测试：颜色、光标、CJK、滚动
3. 与 Phase 4 的条件 notify 配合，观察输入延迟

---

## 6. 依赖与前置条件

| 依赖 | 状态 |
|------|------|
| GPUI `paint_quad` / `paint_glyph` | ✅ 已确认存在 |
| `ShapedLine::paint` 批量文本 | 需 Spike 验证 |
| `TextSystem`、`FontId` 获取方式 | 需查 GPUI 文档或 Zed 用法 |
| 字体配置（Menlo/等宽） | 需在 Window/App 层配置 |
| Phase 4 条件 notify | 建议先完成，减少无效重绘 |

---

## 7. 成功标准

- [ ] 终端输入无肉眼延迟，与 Zed 内建终端接近
- [ ] 快速滚动 1000+ 行历史无卡顿
- [ ] 元素数量从 ~60 降为 1
- [ ] `cargo test` 全过，视觉与当前一致（颜色、光标、CJK）

---

## 8. Zed 终端实现调研（完全重构参考）

> 目标：terminal 光标、resize、性能都和 Zed 一样，**参考 Zed 完全重构**。

### 8.1 Zed 架构概览

Zed 将终端拆成两个 crate：

- **`crates/terminal`**：核心 Terminal（alacritty_terminal、PTY、事件循环、Resize）
- **`crates/terminal_view`**：UI（TerminalView、TerminalElement、TerminalPanel）

```
terminal/
  terminal.rs        - Terminal Entity, InternalEvent::Resize, term.resize()
  pty_info.rs        - PTY 信息

terminal_view/
  terminal_view.rs   - TerminalView (Entity, 焦点、blink、scroll)
  terminal_element.rs - TerminalElement 实现 Element trait
  terminal_panel.rs  - 面板布局
```

### 8.2 TerminalElement 核心结构（Zed 源码）

**BatchedTextRun**（style-run 批处理，对应 pmux 的 StyledSegment）：

```rust
pub struct BatchedTextRun {
    pub start_point: AlacPoint,
    pub text: String,
    pub cell_count: usize,
    pub style: TextRun,      // GPUI TextRun（font, color, background_color, underline）
    pub font_size: AbsoluteLength,
}

impl BatchedTextRun {
    fn can_append(&self, other_style: &TextRun) -> bool { ... }
    fn append_char(&mut self, c: char) { ... }
    pub fn paint(&self, origin, dimensions, window, cx) {
        let pos = Point::new(...);
        window.text_system().shape_line(...).paint(pos, line_height, ...);
    }
}
```

**LayoutRect**（背景矩形）：

```rust
pub fn paint(&self, origin, dimensions, window) {
    window.paint_quad(fill(Bounds::new(position, size), self.color));
}
```

**layout_grid**：单遍遍历 cells → 合并背景区域（BackgroundRegion）→ 转为 LayoutRect；合并同 style 的 cells → BatchedTextRun。与 pmux 的 `group_cells_into_segments` 思路一致，但输出为 `(rects, batched_runs)` 直接用于 paint。

**paint 流程**：

1. `window.paint_quad(fill(bounds, background_color))` 画整体背景
2. `for rect in rects { rect.paint(...) }` 画背景区域
3. `for batch in batched_text_runs { batch.paint(...) }` 画文字（shape_line + paint）
4. 若有 IME 选区：`window.paint_quad(...)` 擦掉光标区背景
5. `cursor.paint(origin, window, cx)` 画光标
6. `block_below_cursor_element.paint(...)` 画块元素

### 8.3 光标实现（Zed）

**TUI 模式光标（vim、Claude、OpenCode 等）—— 1、2、3 均参考 Zed 实现：**

1. **Cursor Position（位置）**：使用 `cursor.point` + `display_offset` → `DisplayCursor` → `cursor_position()` 像素坐标。不假设「行尾」，按终端实际报告绘制。
2. **Cursor Shape（形状）**：`AlacCursorShape` → `CursorLayout`，支持 Block、Bar、Underline、Hollow。DECSCUSR 由 alacritty_terminal 解析。
3. **Cursor Visibility（可见性）**：`cursor.shape == Hidden` 或 DECTCEM 隐藏时不绘制。`cursor_visible` 与 BlinkManager 结合，focused 且 visible 时才 `cursor.paint()`。

其他细节（与 Zed 一致）：
- **DisplayCursor**：`line, col`，由 `cursor.point` + `display_offset` 转换
- **cursor_position**：`(col * cell_width, line * line_height)` 像素坐标
- **CursorLayout**：Block 时显示字符，其他为线/框
- **光标宽度**：空白用 cell_width；否则 `cursor_text.width.max(cell_width)`，兼容 emoji

### 8.4 Resize 实现（Zed）

```rust
// terminal.rs
enum InternalEvent { Resize(TerminalBounds), ... }
// 事件循环收到 Resize 时：
term.resize(new_bounds);
pty_tx.send(Msg::Resize(new_bounds.into()));  // 通知 PTY

// TerminalBounds: cell_width, line_height, bounds (Bounds)
impl TerminalBounds {
    fn num_lines(&self) -> usize { (height / line_height).floor() }
    fn num_columns(&self) -> usize { (width / cell_width).floor() }
}
```

- **触发**：面板尺寸变化 → `events.push_back(InternalEvent::Resize(new_bounds))`
- **PTY**：`pty_tx.send(Msg::Resize)` 使 PTY 进程收到 SIGWINCH
- **pmux 现状**：tmux 接管 PTY，resize 可能需通过 tmux `resize-pane` 传递；local_pty 需直接发 SIGWINCH

### 8.5 Viewport Culling（Zed 关键优化）

```rust
let visible_bounds = window.content_mask().bounds;
let intersection = visible_bounds.intersect(&bounds);

if intersection.size.height <= 0 || intersection.size.width <= 0 {
    (Vec::new(), Vec::new())  // 完全不可见，跳过所有 cell 处理
} else if intersection == bounds {
    // Fast path: 完全可见，直接 layout_grid(cells)
} else {
    // 按像素计算可见行：rows_above_viewport, visible_row_count
    // layout_grid(cells.skip().take().flat_map())
}
```

仅对可见行做 layout，避免大 scrollback 时的无用计算。对应 pmux 的 viewport culling。

### 8.6 pmux vs Zed 差异

| 方面 | pmux | Zed |
|------|------|-----|
| Terminal 所有权 | TerminalEngine (Arc+Mutex) | Terminal Entity，事件驱动 |
| 渲染 | div/span 树 | TerminalElement，paint_quad + shape_line.paint |
| 批处理输出 | StyledSegment → div | BatchedTextRun + LayoutRect → 直接 paint |
| 光标 | 嵌入 span 的 div | CursorLayout.paint() |
| Resize | 需确认 local_pty/tmux 路径 | InternalEvent::Resize → term.resize + pty |
| 依赖 | alacritty_terminal 直接 | terminal crate 封装 |

---

## 9. 完全重构路线图（参考 Zed）

### 策略：**复制架构模式，不直接依赖 Zed crate**

- Zed 的 `terminal`、`terminal_view` 为 GPL，直接依赖会带来许可证约束
- pmux 场景不同（tmux pane、worktree、agent status），只需借鉴模式

### Phase 0：Spike（1–2 天）

1. 验证 pmux 当前 GPUI 版本是否有 `shape_line().paint()`
2. 最小 TerminalElement：request_layout 固定尺寸，paint 内 `paint_quad` + 一段 `shape_line().paint()`
3. 确认字体、baseline、颜色可用

### Phase 1：TerminalElement 与 BatchedTextRun（3–4 天）

1. 新建 `terminal_element.rs`，实现 `Element` trait
2. 实现 `BatchedTextRun`（或改造现有 StyledSegment 为可 paint 结构）：`can_append`、`append_char`、`paint`（shape_line + paint）
3. 实现 `LayoutRect`（或等效）：背景 `paint_quad`
4. `layout_grid`：cells → (rects, batched_runs)，复用现有 segment 合并逻辑
5. TerminalView::render 返回 `TerminalElement.into_element()`，不再 `div().children()`

### Phase 2：光标与 CursorLayout（1–2 天）

**完全参考 Zed 8.3**，实现 TUI 模式光标支持（vim、Claude、OpenCode）：

1. Cursor Position：`cursor.point` → DisplayCursor → 像素坐标
2. Cursor Shape：Block/Bar/Underline/Hollow，按 `AlacCursorShape`
3. Cursor Visibility：Hidden 或 DECTCEM 时不绘制；blink 与 focused 逻辑
4. 宽字符光标宽度：`cursor_text.width.max(cell_width)`

### Phase 3：Resize 与 TerminalBounds（1–2 天）

1. 定义 `TerminalBounds { cell_width, line_height, bounds }`
2. 容器尺寸变化 → 计算 new_bounds → `engine.resize(new_bounds)`
3. local_pty：发 SIGWINCH；tmux：调用 `resize-pane`
4. `term.resize()` 更新 alacritty 的 Dimensions

### Phase 4：Viewport Culling 与性能（1 天）

1. `window.content_mask().bounds` 与终端 bounds 求交
2. 仅对可见行做 layout_grid，与 Zed 一致
3. 保留 row cache（可选），评估与 culling 的叠加收益

### Phase 5：回归与打磨

1. 颜色、CJK、block/box 字符（Powerline、Unicode 制表符）
2. 可选：decorative character 不做 contrast 调整（Zed #34234）
3. 全量测试、性能对比

---

## 10. 参考

- **Zed 源码**：[zed-industries/zed](https://github.com/zed-industries/zed) — `crates/terminal/`, `crates/terminal_view/`
- **terminal_element.rs**：BatchedTextRun、LayoutRect、layout_grid、CursorLayout、paint 流程
- **terminal.rs**：InternalEvent::Resize、TerminalBounds、term.resize、pty_tx
- pmux：`src/ui/terminal_rendering.rs`、`src/ui/terminal_view.rs`、`src/terminal/engine.rs`
- `docs/plans/2026-02-28-ui-performance-ultimate.md`：Phase 1–4（非渲染）
- [GPUI: Zed's 120fps GPU-Accelerated UI](https://kaelan.fyi/research/gpui-zed-renderer/)
