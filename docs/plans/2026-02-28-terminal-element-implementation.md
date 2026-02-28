# TerminalElement 完全重构实施计划（Zed 参考）

> **For Claude:** Use TDD when implementing. Consider `subagent-driven-development` for parallel tasks.

**Goal:** 终端渲染达到 Zed/Ghostty 级性能，cursor/resize 与 Zed 一致；TUI 模式光标（vim、Claude、OpenCode）完全支持。

**Architecture:** 1 个 TerminalElement 代替 60+ div/span；`paint_quad` + `shape_line().paint()` 直接 GPU 绘制；**ShapedLine 缓存**、**RowCache** 在 `terminal_renderer/` 子模块；TerminalElement 仅为 front-end。光标、resize、viewport culling 参考 Zed。

**Tech Stack:** Rust, GPUI, alacritty_terminal

**Runtime Pipeline（Zed 三层）：**

```
PTY/TMUX
   ↓
TerminalEngine (state)
   ↓ snapshot()
RenderableSnapshot   ⭐
   ↓
layout_grid + RowCache
   ↓
RenderableGrid   ⭐ (background_regions, text_runs, cursor_layout)
   ↓
TerminalElement (pure renderer, ONLY paint)
   ↓
GPU paint
```

**⚠️ Element 不应做 terminal logic。** Zed 实际是：Terminal (state) → RenderableContent (snapshot) → TerminalElement (pure renderer)。`layout_grid` 必须在 engine/render layer，TerminalElement **仅 paint**。

**TerminalElement 责任（最小化）：**

| 只负责 | 绝不 |
|--------|------|
| viewport clip | resize |
| paint_quad | engine mutation |
| cursor paint | shaping / layout_grid / cache 管理 |
| | 任何 terminal logic |

**参考:** `docs/plans/2026-02-28-terminal-element-brainstorm.md`

---

## 数据流（关键约束）

**⚠️ 必须引入 RenderableSnapshot 层，否则会遭遇 cursor jitter、layout/render 不一致、resize race。**

| 架构 | 数据流 |
|------|--------|
| **Zed** | `Terminal (model)` → snapshot → `RenderableContent` → `TerminalElement`（无状态 render） |
| **pmux 必须** | `TerminalEngine` → snapshot() → `RenderableSnapshot` → layout_grid + RowCache → `RenderableGrid` → `TerminalElement` (ONLY paint) → GPU paint |

**原则：** 每帧只 lock 一次 engine，取出 snapshot；Renderer 产出 **RenderableGrid**；TerminalElement **仅 paint**，不做任何 terminal logic。Zed 每帧只 render snapshot，不碰 engine。

**RenderableGrid（解耦 Element 的关键）：**

```rust
pub struct RenderableGrid {
    pub background_regions: Vec<LayoutRect>,
    pub text_runs: Vec<BatchedTextRun>,
    pub cursor_layout: CursorLayout,
}
```

规则：layout_grid → engine/render layer；TerminalElement ONLY paint。否则 resize、scrollback、GPU cache 会互相耦合。

**Renderer 子模块：**

```
src/ui/terminal_renderer/
   mod.rs
   shaped_line_cache.rs
   row_cache.rs
   layout_grid.rs
   renderable_grid.rs   // 产出 RenderableGrid
```

**由谁取 snapshot：** TerminalView 调用 `engine.try_renderable_content()` 一次，构建 `RenderableSnapshot`，经 `terminal_renderer::build_frame()` 产出 `RenderableGrid`，传入 `TerminalElement::new(grid)`。TerminalElement 只做 viewport clip、paint_quad、cursor paint。

**⚠️ layout_grid 不能每帧全量重建：** 真实 terminal 约 95% 行不变。Ghostty + Zed 共识：**RowCache**，`hash(row content)` → unchanged 复用 runs，changed 才 rebuild。否则大文件 scroll 会炸 CPU，GPU 再快也没用。见 Task 1.4b。

**⚠️ tmux backend + viewport culling → cursor 错位：** tmux cursor 是 **pane-relative**，非 screen-relative。viewport culling 后 `display_offset != tmux scroll position`，cursor 会错。**Runtime 必须维护三种坐标：** LogicalCursor（engine）、VisualCursor（viewport 调整后）、TmuxCursor（pane relative）。否则 vim 模式一定错位。见 Task 2.5。

---

## Phase 0：API 确认（可跳过）

### Task 0.1：确认 shape_line().paint() API

**方式：** 直接读 Zed 源码，**务必按当前 GPUI 版本验证**（gpui 已升级，API 可能有变）。

**Zed 实际签名**（terminal_element.rs:136-153）：
```rust
window.text_system().shape_line(
    self.text.clone().into(),
    self.font_size.to_pixels(window.rem_size()),  // 需 rem_size 转换
    std::slice::from_ref(&self.style),
    Some(dimensions.cell_width),                  // cell_width 需包在 Some()
).paint(pos, dimensions.line_height, gpui::TextAlign::Left, None, window, cx);
```

**结论：** 实现前在本仓库 GPUI 版本下确认 exact signature；若有出入，以 Zed 源码为准。

**⚠️ shape_line 不能高频调用：** `shape_line()` 内部是 Harfbuzz shaping，成本高。vim 滚动时每帧 200~400 次 shape_line = CPU 杀手。**Zed 做法：ShapedLine 缓存，key = (text, font, size, style)，非 optional，是必须项。** 计划中 BatchedTextRun::paint 必须走 cache，不能每 run 直接调用 shape_line（见 Task 1.2b）。

**Error Handling（shape_line fallback 优先级）：** `shape_line()` 可能失败（字体缺失、**GPU 上下文丢失**等）。**不能**仅 "failure → skip run"；GPU context 丢失时 shape_line 会**连续失败**，整个 terminal 消失（macOS GPU reset 会直接黑屏）。**正确策略：**

1. **retry next frame**（首选）
2. **fallback monospace raster cache**（预渲染 ASCII 或简单字形）
3. **最后才 skip** run

否则 macOS GPU reset 会直接黑屏。

---

### Task 0.2：RenderableSnapshot（数据流关键）

**目标：** 定义 immutable snapshot，供 TerminalElement 使用；确保 layout 与 paint 看到同一帧数据。

**Files:**
- Create: `src/terminal/renderable_snapshot.rs` 或 `src/ui/terminal_renderer/snapshot.rs`

**Snapshot 结构（从 `RenderableContent` 提取，单次 lock 内完成）：**

```rust
pub struct RenderableSnapshot {
    pub cells: Vec<...>,           // 或保留 display_iter 的产出
    pub cursor: Cursor,
    pub display_offset: usize,
    pub screen_lines: usize,
    pub colors: Rgb,
    pub modes: TermMode,
    // ...
}
```

**构建入口：** 必须在 **单次 lock 内** 完成。由 TerminalView 调用 `engine.try_renderable_content(|content, display_iter, screen_lines| { RenderableSnapshot::from(...) })`；闭包内从 content/display_iter 拷贝 grid、cursor、display_offset 等到 snapshot，闭包返回后 lock 释放，snapshot 供本帧 layout/paint 使用。

**Step:** 在 Phase 1 开始前或 Task 1.1 中一并实现；Task 1.5 中 TerminalView 每帧调用一次并传入 TerminalElement。

---

## Phase 1：TerminalElement 骨架与 BatchedTextRun（3–4 天）

### Task 1.1：TerminalElement 骨架（request_layout 固定尺寸）

**引用类型：** TerminalElement **不持有** `Arc<TerminalEngine>`。持有 `RenderableGrid`（Phase 1.5 后；Phase 1.1 骨架可先占位）；仅 paint，无 terminal logic。

**ContentMode：** Zed 的 `request_layout` 根据 ContentMode 返回不同高度：
- Inline → `displayed_lines * line_height`
- Scrollable → `relative(1.)`（填满可用空间）
本计划仅使用 **固定尺寸模式**（cols*cell_w, rows*cell_h），适用于 pmux 终端无内滚动的场景。

**Files:**
- Create: `src/ui/terminal_element.rs`
- Create: `src/ui/terminal_renderer/renderable_grid.rs`（Task 1.1 中先建 minimal impl，供 skeleton 测试用）
- Modify: `src/ui/mod.rs`（添加 `mod terminal_element`、`mod terminal_renderer`）
- Test: `src/ui/terminal_element.rs`（inline `#[cfg(test)]`）

**⚠️ Phase 1.1 时 RenderableGrid 尚未在 Phase 1.5 完善：** 在 Task 1.1 中先于 `renderable_grid.rs` 实现 `RenderableGrid::empty(cols, rows)` 的 minimal 占位（空 vec、空 cursor_layout），供 skeleton 测试依赖。Phase 1.5 再完善。

**Step 1: Write the failing test**

⚠️ 不能只验证数学计算；须验证 `Element::request_layout` 实际返回正确的 LayoutId 和尺寸。需 GPUI 测试上下文：

```rust
// src/ui/terminal_element.rs 末尾
#[cfg(test)]
mod tests {
    use super::*;
    use gpui::px;

    #[gpui::test]
    fn test_terminal_element_request_layout_returns_correct_size(cx: &mut gpui::TestAppContext) {
        let grid = RenderableGrid::empty(80, 24);  // Task 1.1 中先建 minimal impl
        let cell_w = px(8.0);
        let cell_h = px(16.0);
        let elem = TerminalElement::new(grid, 80, 24, cell_w, cell_h);
        cx.update(|cx| {
            let (layout_id, _) = elem.request_layout(
                None,
                cx,
                |req| req.max_size(AvailableSpace::Definite(size(px(1000.), px(600.)))),
            );
            let layout = cx.layout(layout_id);
            assert_eq!(layout.size.width, px(640.));  // 80 * 8
            assert_eq!(layout.size.height, px(384.)); // 24 * 16
        });
    }
}
```

若 `#[gpui::test]` 或 `request_layout` 签名与上述不同，先写 `test_terminal_bounds_compute_size()` 验证尺寸计算，Phase 1 完成时补充集成测试验证 request_layout 返回正确 LayoutId/尺寸。

**Step 2: Run test**

Run: `RUSTUP_TOOLCHAIN=stable cargo test terminal_element::tests::`
Expected: 编译失败（terminal_element 不存在）→ 创建模块后 PASS

**Step 3: Implement minimal skeleton**

```rust
// src/ui/terminal_element.rs
use gpui::prelude::*;
use gpui::*;

/// Zed-style terminal element: 1 element, direct paint_quad + shape_line.
/// 持有 RenderableGrid（由 renderer 产出），不持有 engine；TerminalElement ONLY paint。
pub struct TerminalElement {
    grid: RenderableGrid,
    cols: u16,
    rows: u16,
    cell_width: Pixels,
    cell_height: Pixels,
    // resize 由 Window 事件 + debounce 驱动，TerminalElement 不参与
}

impl Element for TerminalElement {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> { None }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> { None }

    fn request_layout(/* ... */) -> (LayoutId, Self::RequestLayoutState) {
        // 返回固定尺寸，参考 Zed
        todo!("implement request_layout")
    }
    fn prepaint(/* ... */) -> Self::PrepaintState { () }
    fn paint(/* ... */) { /* 占位 */ }
}

impl IntoElement for TerminalElement {
    type Element = Self;
    fn into_element(self) -> Self::Element { self }
}
```

**Step 4: Run test**

Run: `cargo test terminal_element::`
Expected: test 通过（或 todo! 未触发时的编译通过）

**Step 5: Commit**

`git add src/ui/terminal_element.rs src/ui/mod.rs && git commit -m "feat(terminal): add TerminalElement skeleton"`

---

### Task 1.2：BatchedTextRun 与 can_append / append_char

**StyledSegment vs BatchedTextRun 关系：**

| 字段 | StyledSegment（现有） | BatchedTextRun（Zed） |
|------|------------------------|------------------------|
| text | ✓ | ✓ |
| fg/bg | fg, bg (Rgb) | style: TextRun (color, background_color) |
| flags | Flags | 编码到 TextRun (underline, strikethrough) |
| 位置 | 无 | start_point, cell_count |
| 绘制 | 无（用于 div） | paint() → shape_line().paint() |

**决策：** 新建 `BatchedTextRun`，不复用 StyledSegment。原因：
- BatchedTextRun 需要 `start_point`、`cell_count` 用于 paint 定位
- TextRun 是 GPUI 原生类型，与 shape_line 接口匹配
- layout_grid 可复用 `group_cells_into_segments` 的**合并逻辑**，但输出结构改为 BatchedTextRun（从 StyledSegment 转换，或直接产出）

**Files:**
- Modify: `src/ui/terminal_element.rs`
- Test: `src/ui/terminal_element.rs`（inline）

**Step 1: Write the failing test**

**注意：** `can_append` 必须**完整**比较 style，否则 vim + italic theme 会随机断 run。除 font、fg/bg 外，还需：
- `font_features`
- `ligature state`
- `italic/bold synthetic flags`
- `underline style`

```rust
#[cfg(test)]
mod tests {
    // ...
    #[test]
    fn test_batched_text_run_can_append_same_style() {
        let run = BatchedTextRun::new_from_char(/* point */, 'a', style_a, font_size);
        assert!(run.can_append(&style_a));  // 同 font + style
    }
    #[test]
    fn test_batched_text_run_cannot_append_different_font() {
        let run = BatchedTextRun::new_from_char(/* point */, 'a', style_a, font_size);
        let mut style_b = style_a.clone();
        style_b.font = other_font;  // 仅 font 不同
        assert!(!run.can_append(&style_b));  // font 必须相等
    }
    #[test]
    fn test_batched_text_run_cannot_append_different_style() {
        let run = BatchedTextRun::new_from_char(/* point */, 'a', style_a, font_size);
        assert!(!run.can_append(&style_b));  // 不同 color/background
    }
    #[test]
    fn test_batched_text_run_append_char_increments_cell_count() {
        let mut run = BatchedTextRun::new_from_char(/* point */, 'a', style, font_size);
        run.append_char('b');
        assert_eq!(run.cell_count, 2);
        assert_eq!(run.text, "ab");
    }
}
```

**Step 2: Run test**

Run: `cargo test batched_text_run`
Expected: FAIL（BatchedTextRun 未实现）

**Step 3: Implement BatchedTextRun**

参考 Zed `terminal_element.rs` 的 `BatchedTextRun` 结构，实现：

```rust
fn can_append(&self, other_style: &TextRun) -> bool {
    self.style.font == other_style.font
        && self.style.color == other_style.color
        && self.style.background_color == other_style.background_color
        && self.style.font_features == other_style.font_features
        && self.style.underline == other_style.underline
        && self.style.strikethrough == other_style.strikethrough
        // italic/bold synthetic, ligature state
}
```

`paint()` **不得**每 run 直接调用 `shape_line()`。必须配合 ShapedLine cache（见 Task 1.2b）。cache miss 时调用 shape_line 并写入 cache；fallback：失败时跳过该 run 或绘制占位（Phase 0 Error Handling）。

**Step 4: Run test**

Run: `cargo test batched_text_run`
Expected: PASS

**Step 5: Commit**

`git add src/ui/terminal_element.rs && git commit -m "feat(terminal): add BatchedTextRun with can_append/append_char"`

---

### Task 1.2b：ShapedLine 缓存（必须项）

**问题：** `shape_line()` = Harfbuzz shaping，每帧 200~400 次（vim scroll）会拖垮 CPU。Zed 缓存 ShapedLine，非可选优化。

**归属：** `terminal_renderer/` 子模块，**不是** TerminalElement 责任。

**Files:**
- Create: `src/ui/terminal_renderer/shaped_line_cache.rs`
- Test: `src/ui/terminal_element.rs`

**实现：**
- **Cache key：** `(text, font, font_size, style)`（或 `(text, font, size, color, bg, flags)`）
- **Cache value：** `ShapedLine` 或 GPUI 返回的 shaped 结果
- **使用：** BatchedTextRun::paint 前查 cache；hit 则直接用，miss 则 shape_line + 写入 cache
- **失效：** 字体切换、DPI 变化、窗口缩放时清空 cache

**Step 1: Write the failing test**

```rust
#[test]
fn test_shaped_line_cache_hit_avoids_shape_call() {
    let mut cache = ShapedLineCache::new(1000);
    let key = CacheKey::new("hello", &font, px(14.), &style);
    let shaped = cache.get_or_insert(&key, |k| shape_line_once(k));
    let shaped2 = cache.get_or_insert(&key, |_| panic!("should not call"));
    assert!(std::ptr::eq(shaped.as_ref(), shaped2.as_ref()));
}
```

**Step 2–4:** 实现 ShapedLineCache。Renderer 在 paint 前查 cache；BatchedTextRun::paint 由 renderer 调用，走 cache 而非直接 shape_line。

**Step 5: Commit**

`git add src/ui/terminal_element.rs && git commit -m "feat(terminal): ShapedLine cache (required for vim/scroll perf)"`

---

### Task 1.3：LayoutRect、BackgroundRegion 合并与 paint_quad 背景

**Zed 优化：** Zed 有 `BackgroundRegion` + `merge_background_regions()` 合并**相邻**背景矩形。实现时应对 rects 做 merge：**同行**相邻且同色的 rect 合并。**必须禁止跨 wrap 行 merge**：长行自动换行时，跨行合并会产生大矩形，selection/cursor 覆盖错误。

**装饰字符（可选）：** Zed 有 `is_decorative_character()` 跳过 Powerline 分隔符等的对比度调整。若需支持 Powerline 字体，可加入；否则 Phase 1 可跳过。

**Files:**
- Modify: `src/ui/terminal_element.rs`
- Test: `src/ui/terminal_element.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn test_layout_rect_creates_valid_bounds() {
    let rect = LayoutRect::new(AlacPoint::new(0, 0), 5, Hsla::default());
    assert_eq!(rect.num_of_cells, 5);
}
```

**Step 2–4:** Implement LayoutRect，验证 paint 逻辑（可在集成测试中验证）

**Step 5: Commit**

`git add src/ui/terminal_element.rs && git commit -m "feat(terminal): add LayoutRect for background paint_quad"`

---

### Task 1.4：layout_grid 产出 (rects, batched_runs)

**归属：** `terminal_renderer/` 子模块，**不是** TerminalElement 责任。TerminalElement 只消费其输出。

**输入来源：** layout_grid 的 cells 来自 **RenderableSnapshot**（display_iter 的产出），不直接从 engine 读取。

**⚠️ 不每帧全量调用：** layout_grid 必须配合 RowCache（Task 1.4b）；每行先 `hash(row_content)`，cache hit 则复用已有 runs，miss 才调用 layout_grid 并写入 cache。

**Zero-width chars：** Zed 处理 `cell.zerowidth()` 用于 emoji 变体序列。必须两步：
1. zerowidth cell **append** 到前一个 run（不单独起新 run，不占列宽）
2. **inherit cluster index**（关键）：否则 emoji skin-tone sequence 时 cursor 会落在字符中间。90% 自写 terminal 会踩的坑。

**Files:**
- Create: `src/ui/terminal_renderer/layout_grid.rs`
- Modify: `src/ui/mod.rs`（添加 `mod terminal_renderer`）
- Test: `src/ui/terminal_renderer/layout_grid.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn test_layout_grid_empty_cells_returns_empty() {
    let (rects, runs) = layout_grid(
        std::iter::empty(),
        0,
        &text_style,
        None,
        1.0,
        /* cx */);
    assert!(rects.is_empty());
    assert!(runs.is_empty());
}
#[test]
fn test_layout_grid_merges_same_style_cells_into_batch() {
    // 输入 5 个同 style cells，输出 1 个 BatchedTextRun
    let cells = /* ... */;
    let (rects, runs) = layout_grid(cells, 0, &text_style, None, 1.0, cx);
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].text, "hello");
}
#[test]
fn test_layout_grid_different_backgrounds_create_rects() {
    // 两行不同 bg，应产生 LayoutRect
    // ...
}
#[test]
fn test_zerowidth_cluster_index_inherited() {
    // emoji + skin-tone modifier（zerowidth）→ zerowidth 必须 append 到前一 run 并 inherit cluster index
    // 否则 cursor 会落在 emoji 中间，而非开头。90% 自写 terminal 会踩的坑。
    // 输入：如 "👋" + U+FE0F (emoji style) 或 "👍" + U+1F3FF (skin-tone)
    // 验证：shaped_line 的 cluster index 正确继承；cursor_position 在 grapheme 边界
}
```

**Step 2–4:** Implement layout_grid：复用 `group_cells_into_segments` 思路，处理 zerowidth 附加到前一 run **并 inherit cluster index**；产出 rects 后调用 `merge_background_regions()` 合并相邻矩形；输出 `(Vec<LayoutRect>, Vec<BatchedTextRun>)`

**Step 5: Commit**

`git add src/ui/terminal_renderer/layout_grid.rs src/ui/mod.rs && git commit -m "feat(terminal): add layout_grid in terminal_renderer"`

---

### Task 1.4b：RowCache（必须项）

**问题：** 若 prepaint 每帧对每行调用 layout_grid，大文件 scroll 会炸 CPU。真实 terminal 约 **95% 行未变**。

**归属：** `terminal_renderer/` 子模块，**不是** TerminalElement 责任。

**Ghostty + Zed 共识：** RowCache，key = `hash(row content)`，value = `(Vec<LayoutRect>, Vec<BatchedTextRun>)`。

- **unchanged** → 复用 cache 中的 runs，跳过 layout_grid
- **changed** → 调用 layout_grid，写入 cache

**Files:**
- Create: `src/ui/terminal_renderer/row_cache.rs`
- Test: `src/ui/terminal_renderer/row_cache.rs`

**实现：** 可复用现有 `hash_row_content`（terminal_rendering.rs）；cache 用 `LruCache<u64, (Vec<LayoutRect>, Vec<BatchedTextRun>)>` 或等价结构。

**⚠️ Cache 跨帧持久位置（明确）：** RowCache 和 ShapedLineCache 必须是 **TerminalView 的字段**，不作为每次 render 的局部变量或传参创建。否则每帧重建，cache 无效。

```rust
pub struct TerminalView {
    // ... 现有字段 ...
    row_cache: RowCache,
    shaped_line_cache: ShapedLineCache,
}
```

Renderer 内**按行**：`hash = hash_row_content(cells)` → `cache.get(hash)` → hit 则用缓存，miss 则 layout_grid 并 `cache.put(hash, result)`。

**Step 1: Write the failing test**

```rust
#[test]
fn test_row_cache_reuses_unchanged_rows() {
    let mut cache = RowCache::new(200);
    let cells_a = make_cells("hello");
    let hash_a = hash_row_content(&cells_a);
    let (rects1, runs1) = cache.get_or_build(hash_a, || layout_grid(cells_a));
    let (rects2, runs2) = cache.get_or_build(hash_a, || panic!("should not rebuild"));
    assert_eq!(runs1.len(), runs2.len());
    // 或 ptr eq
}
```

**Step 5: Commit**

`git add src/ui/terminal_renderer/row_cache.rs src/ui/mod.rs && git commit -m "feat(terminal): RowCache in terminal_renderer (required for scroll perf)"`

---

### Task 1.5：TerminalView 使用 TerminalElement 替代 div

**Snapshot 取用（核心）：** TerminalView 持有 `Arc<TerminalEngine>`。在 `render` 中：
1. 调用 `engine.try_renderable_content(|content, display_iter, screen_lines| { ... })` **一次**
2. 在闭包内构建 `RenderableSnapshot`（拷贝 grid、cursor、display_offset 等）
3. 调用 `terminal_renderer::build_frame(snapshot, &mut self.row_cache, &mut self.shaped_line_cache)` → RenderableGrid（row_cache、shaped_line_cache 为 TerminalView 字段，跨帧持久）
4. 将 `grid` 传入 `TerminalElement::new(grid, ...).into_element()`
5. TerminalElement 的 request_layout、prepaint、paint 只做 layout、viewport clip、paint_quad、cursor paint；**不管理 cache**，**不直接访问 engine**

**RowCache 必须：** 在 `terminal_renderer/` 中，见 Task 1.4b。Renderer 负责 `hash(row)` → cache hit 复用，miss 才 layout_grid。当前 `TerminalBuffer::Term` 的 `LruCache<u64, Vec<StyledSegment>>` 可迁移为 `terminal_renderer/row_cache.rs`。

**重构时移除调试日志：** 当前 `terminal_view.rs`、`terminal_rendering.rs` 中有写入 `~/.cursor/debug-*.log` 的代码；TerminalElement 重构时应删除。

**Files:**
- Modify: `src/ui/terminal_view.rs`
- Modify: `Cargo.toml`（dev-dependencies 添加 `gpui = { ..., features = ["test-support"] }` 若需 GPUI 测试）
- Test: `tests/terminal_rendering.rs`（现有测试应仍通过）
- Test: `tests/terminal_element_integration.rs`（新建）

**GPUI Context 设置：** 需要 Window/App 的测试使用 `#[gpui::test]` 或 `App::production().run(|cx| {...})`。参考 Zed `terminal_view` 的 test-support。

**Step 1a: 无需 GPUI 的间接验证（可先做）**

```rust
// tests/terminal_element_integration.rs
#[test]
fn test_terminal_buffer_content_unchanged_after_terminal_element_switch() {
    // 不调用 render，仅验证 TerminalBuffer 数据路径
    let engine = make_engine_with_bytes(b"hello\r\n");
    let buf = TerminalBuffer::new_term_with_cache_size(engine.clone(), 200);
    let content = buf.content_for_status_detection();
    assert!(content.unwrap().contains("hello"));
}
```

**Step 1b: 需要 GPUI context 的渲染验证**

```rust
#[gpui::test]
async fn test_terminal_view_renders_with_terminal_element(cx: &mut gpui::TestAppContext) {
    let engine = make_engine_with_bytes(b"hello\r\n");
    let buf = TerminalBuffer::new_term_with_cache_size(engine, 200);
    // 在 cx 中创建 Window，挂载 TerminalView，触发 render
    // 验证 TerminalElement 被 layout/paint 且无 panic
    cx.update(|cx| {
        // 参考现有 GPUI 测试：open_window, 获取 view, 调用 render
    }).unwrap();
}
```

若 `#[gpui::test]` 不可用，可先依赖 `test_terminal_buffer_content_unchanged_*` 与 `tests/regression/run_all.sh` 的自动化视觉验证。

**Step 2: Modify TerminalView::render**

```rust
// 伪代码
let snapshot = self.engine.try_renderable_content(|content, display_iter, screen_lines| {
    RenderableSnapshot::from(content, display_iter, screen_lines)
}).unwrap_or_else(|| RenderableSnapshot::empty(cols, rows));

let grid = terminal_renderer::build_frame(&snapshot, &mut self.row_cache, &mut self.shaped_line_cache);
let elem = TerminalElement::new(grid, cols, rows, cell_w, cell_h);
elem.into_element()
```

将 `div().children(line_elements)` 替换为上述逻辑。`terminal_renderer::build_frame()` 产出 `RenderableGrid`；TerminalElement 只接收 grid 并 paint。

**Step 3: Run tests**

Run: `cargo test terminal_rendering:: 2>&1`
Run: `cargo test terminal_element_integration 2>&1`
Run: `cargo test terminal_view:: 2>&1`
Expected: 全部 PASS

**Step 4: Run regression**

Run: `tests/regression/run_all.sh --skip-build 2>&1`
Expected: Sidebar、cursor、colors 等回归通过

**Step 5: Commit**

`git add src/ui/terminal_view.rs tests/terminal_element_integration.rs && git commit -m "feat(terminal): TerminalView uses TerminalElement instead of div tree"`

---

## Phase 1.5：RenderableGrid 职责分离 ⭐⭐⭐

**目的：** 解耦 Element，避免 Phase 3（resize）时结构性返工。**否则 resize、scrollback、GPU cache 会互相耦合。**

**Task 1.5a：引入 RenderableGrid**

```rust
pub struct RenderableGrid {
    pub background_regions: Vec<LayoutRect>,
    pub text_runs: Vec<BatchedTextRun>,
    pub cursor_layout: CursorLayout,
}

impl RenderableGrid {
    pub fn empty(cols: u16, rows: u16) -> Self { ... }
}
```

- `terminal_renderer::build_frame(snapshot, row_cache, shaped_line_cache)` 产出 `RenderableGrid`
- TerminalElement 构造改为 `TerminalElement::new(grid)`，**仅**接收 grid，**仅** paint
- 移除 TerminalElement 内任何 layout_grid、cache 逻辑

**验收：** TerminalElement 无 terminal logic；所有计算在 renderer 层完成。

---

## Phase 2：光标与 CursorLayout（1–2 天）

### Task 2.1：DisplayCursor 与 cursor_position

**三种 cursor 坐标（tmux backend 必须区分）：**

| 类型 | 坐标系 | 来源/用途 |
|------|--------|-----------|
| **LogicalCursor** | engine 内部，含 scrollback | alacritty `renderable_content().cursor` |
| **VisualCursor** | viewport 调整后，用于 paint | LogicalCursor + display_offset → 可见区 pixel 坐标 |
| **TmuxCursor** | pane-relative | tmux `capture-pane` 等；与 LogicalCursor 可能不同（display_offset ≠ tmux scroll） |

Local PTY 下 Logical = Tmux。Tmux backend + viewport culling 时需做坐标转换；否则 vim 模式错位。见 Task 2.5。

**Files:**
- Modify: `src/ui/terminal_element.rs`
- Test: `src/ui/terminal_element.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn test_display_cursor_from_point_and_offset() {
    let point = AlacPoint::new(0, 5);
    let dc = DisplayCursor::from(point, 0);
    assert_eq!(dc.line(), 0);
    assert_eq!(dc.col(), 5);
}
#[test]
fn test_cursor_position_with_display_offset() {
    // scrollback 时：cursor.point.line 可为负，display_offset 为正
    // DisplayCursor.line = point.line + display_offset
    let point = AlacPoint::new(-3, 10); // 历史行
    let dc = DisplayCursor::from(point, 5);
    assert_eq!(dc.line(), 2); // -3 + 5
    assert_eq!(dc.col(), 10);
}
#[test]
fn test_cursor_position_pixel_coords() {
    let dims = TerminalBounds::new(px(16.), px(8.), bounds);
    let pos = TerminalElement::cursor_position(DisplayCursor { line: 1, col: 10 }, dims);
    assert!(pos.is_some());
    assert_eq!(pos.unwrap().x, 80.); // 10 * 8
    assert_eq!(pos.unwrap().y, 16.); // 1 * 16
}
#[test]
fn test_cursor_width_for_wide_char() {
    // emoji/CJK：cursor_width = shaped_text.width.max(cell_width)
    // 空白字符：cursor_width = cell_width
    // 验证 cursor_width 计算逻辑
}
#[test]
fn test_cursor_width_whitespace_vs_non_whitespace() {
    // Zed 逻辑：whitespace → cell_width；非空白 → shaped_width.max(cell_width)
}
#[test]
fn test_cursor_width_grapheme_fallback() {
    // combining marks：shaped_width == 0 时用 cell_width，否则 invisible cursor
}
```

**Step 2–4:** Implement DisplayCursor、cursor_position、**cursor_width**。cursor_width 必须含 **grapheme fallback**：`if shaped_width == 0 { cell_width } else { shaped_width.max(cell_width) }`，否则 combining marks 会产生 invisible cursor。

**Step 5: Commit**

`git add src/ui/terminal_element.rs && git commit -m "feat(terminal): add DisplayCursor and cursor_position"`

---

### Task 2.2：Cursor Shape（Block/Beam/Underline/Hollow/HollowBlock）

**命名：** Zed 的 `Beam` 即本计划中的 Bar（竖线光标）；实现时与 AlacCursorShape 映射一致。

**Files:**
- Modify: `src/ui/terminal_element.rs`
- Test: `src/ui/terminal_element.rs`、`tests/terminal_cursor.rs`

**Step 1: Write the failing test**

```rust
// tests/terminal_cursor.rs
#[test]
fn test_cursor_shape_block_parsed_from_decscusr() {
    let (tx, rx) = flume::unbounded();
    let engine = Arc::new(TerminalEngine::new(80, 24, rx));
    tx.send(b"\x1b[1 q".to_vec()).unwrap(); // blinking block
    engine.advance_bytes();
    // 验证 renderable_content().cursor.shape == Block
}
#[test]
fn test_cursor_shape_bar_from_decscusr() {
    tx.send(b"\x1b[5 q".to_vec()).unwrap(); // blinking bar
    // ...
}
#[test]
fn test_cursor_shape_hollow_block() {
    tx.send(b"\x1b[2 q".to_vec()).unwrap(); // steady block → HollowBlock
    // 验证 AlacCursorShape::HollowBlock 被正确处理（空心框，不显示字符）
}
```

**Step 2–4:** 实现 CursorLayout，按 AlacCursorShape 绘制 Block/Bar(=Beam)/Underline/Hollow

**Step 5: Run integration**

Run: `cargo test terminal_cursor`
Run: `tests/regression/test_cursor_position_auto.sh`
Expected: PASS

**Step 6: Commit**

`git add src/ui/terminal_element.rs tests/terminal_cursor.rs && git commit -m "feat(terminal): cursor shape Block/Bar/Underline/Hollow per Zed"`

---

### Task 2.3：Cursor Visibility（DECTCEM）

**Files:**
- Modify: `src/ui/terminal_element.rs`
- Test: `tests/terminal_cursor.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn test_cursor_hidden_when_dectcem_l() {
    tx.send(b"\x1b[?25l".to_vec()).unwrap(); // hide cursor
    engine.advance_bytes();
    // 验证 cursor 不绘制
}
#[test]
fn test_cursor_visible_when_dectcem_h() {
    tx.send(b"\x1b[?25l\x1b[?25h".to_vec()).unwrap();
    // 验证 cursor 绘制
}
```

**Step 2–4:** 在 paint 中检查 cursor.shape == Hidden 或 DECTCEM 状态，不调用 cursor.paint()

**Step 5: Commit**

`git add src/ui/terminal_element.rs tests/terminal_cursor.rs && git commit -m "feat(terminal): cursor visibility per DECTCEM"`

---

### Task 2.4：TUI 光标回归测试

**Files:**
- Modify: `tests/regression/test_cursor_position_auto.sh` 或新建 `test_cursor_tui_auto.sh`
- Modify: `tests/functional/tui/vim_compatibility_test.sh`（若有）

**Step 1: Add regression case**

在 `test_cursor_position_auto.sh` 中增加：启动 vim，发送 `\x1b[5 q`（bar），验证光标形状/位置。

**Step 2: Run**

Run: `tests/regression/run_all.sh`
Expected: 全部通过

**Step 3: Commit**

`git add tests/regression/ && git commit -m "test(regression): add TUI cursor shape/visibility regression"`

---

### Task 2.5：Tmux cursor 坐标转换（tmux backend + viewport culling）

**问题：** tmux cursor 为 pane-relative；engine 的 display_offset 与 tmux scroll 可能不一致；viewport culling 后 cursor 会错位，vim 模式一定错。

**Runtime 必须维护：**

- **LogicalCursor**：engine 内 cursor（含 scrollback 行号）
- **VisualCursor**：viewport 内 pixel 坐标，用于 paint
- **TmuxCursor**：pane-relative，用于 tmux 命令（如 `send-keys` 点击定位）

**实现：** 在 tmux runtime/StatusPublisher 中，将 LogicalCursor 转换为 TmuxCursor（考虑 display_offset 与 tmux scroll 的映射）；或由 tmux 同步 scroll 与 engine display_offset。Phase 4 viewport culling 启用后必须验证 tmux + vim 光标无错位。

**Files:**
- Modify: `src/runtime/`（tmux backend、或 cursor 转换层）
- Test: `tests/functional/tui/vim_compatibility_test.sh`（tmux + vim + scroll）

---

## Phase 3：Resize 与 TerminalBounds（1–2 天）

### Task 3.0：ResizeController（Window bounds observer）

**⚠️ resize 不属于 UI。** 不要在 `request_layout`、`prepaint`、`paint` 中检测 bounds 并触发 resize。

**为何 request_layout 危险：**
- GPUI layout 可能被**多次 speculative 调用**
- 结果：`layout → resize → terminal redraw → layout → resize …` = resize storm
- bounds 浮点误差：`640.0` → `639.999` → `640.0` → 疯狂 resize

**✅ Zed 做法：** resize 在 **window bounds observer** 中触发，**不是** element lifecycle。

**新增 ResizeController：**

```
WindowBoundsObserver
   ↓ debounce (16ms)
   ↓
resize channel → engine.resize(cols, rows)
```

TerminalElement **只读取尺寸**，不决定 resize。

**Files:**
- Modify: `src/ui/app_root.rs`（或新建 `src/ui/terminal_controller.rs`）
- 需订阅 GPUI 的 Window resize 回调（如 `window.on_resize`、`observe_window_bounds` 等，以 GPUI API 为准）

**实现要点：**
1. 在 Window resize 时收到事件，**不要**在 render/layout 中轮询
2. **Debounce 16–32ms**：连续 resize 时合并为一次 `engine.resize`
3. 计算 `(cols, rows)` 后调用 `engine.resize()` 或 runtime `resize_pane`
4. TerminalElement、TerminalView **不持有** last_bounds、resize_tx；不参与 resize 检测

**迁移说明：** 当前 `app_root.rs` 在 `render` 中通过 `window.window_bounds()` 检测尺寸变化并调用 resize；需改为 Window resize event + debounce，避免与 layout 耦合及浮点抖动。

**Test:**
```rust
#[test]
fn test_resize_debounced() {
    // 快速连续 5 次 resize 事件，只应触发 1 次 engine.resize
}
#[test]
fn test_resize_float_tolerance() {
    // 639.999 vs 640.0 不应触发 resize（或 debounce 后稳定为 640）
}
```

---

### Task 3.1：TerminalBounds 与 num_lines/num_columns

**位置：** Zed 将 TerminalBounds 放在 terminal crate（`crates/terminal/src/terminal.rs`），作为 terminal 核心概念。建议放在 `src/terminal/`（例如 `src/terminal/bounds.rs` 或 `term_bridge.rs` 内），而非 `src/ui/terminal_element.rs`。

**Files:**
- Create: `src/terminal/bounds.rs` 或 Modify: `src/terminal/term_bridge.rs`
- Test: `src/terminal/`

**Step 1: Write the failing test**

```rust
#[test]
fn test_terminal_bounds_num_lines_columns() {
    let bounds = TerminalBounds::new(px(16.), px(8.), Bounds::new(Point::zero(), size(px(640.), px(384.))));
    assert_eq!(bounds.num_lines(), 24);
    assert_eq!(bounds.num_columns(), 80);
}
```

**Step 2–4:** 实现 TerminalBounds（参考 Zed）

**Step 5: Commit**

`git add src/... && git commit -m "feat(terminal): add TerminalBounds with num_lines/num_columns"`

---

### Task 3.2：Local PTY Resize（SIGWINCH）

**Files:**
- Modify: `src/runtime/backends/local_pty.rs`
- Test: `src/runtime/backends/local_pty.rs`、`tests/functional/terminal/resize_test.sh`

**Step 1: Write the failing test**

```rust
// local_pty.rs #[cfg(test)]
#[test]
fn test_resize_sends_sigwinch_to_child() {
    // 启动 local pty，resize 后验证子进程收到 SIGWINCH（或通过 term 尺寸变化间接验证）
}
```

**Step 2–4:** 实现 resize 路径：由 Task 3.0（Window resize event + debounce）调用；本 Task 负责 `engine.resize()` → SIGWINCH

**Step 5: Integration test**

新建 `tests/functional/terminal/resize_test.sh`：启动 pmux，resize 窗口，验证终端行列变化。

**Step 5b: 并发 resize + 大输出测试**（详见 Task 5.6）

**Step 6: Commit**

`git add src/runtime/backends/local_pty.rs tests/functional/terminal/resize_test.sh && git commit -m "feat(terminal): local PTY resize via SIGWINCH"`

---

### Task 3.3：Tmux Resize（resize-pane）

**Files:**
- Modify: `src/runtime/backends/tmux.rs`、调用方
- Test: `src/runtime/backends/tmux.rs`、`tests/functional/terminal/tmux_resize_test.sh`

**Step 1: Write the failing test**

```rust
#[test]
fn test_tmux_resize_pane_command_format() {
    let cmd = build_resize_pane_command("%pane123", 100, 30);
    assert!(cmd.contains("resize-pane"));
    assert!(cmd.contains("-x"));
    assert!(cmd.contains("-y"));
}
```

**Step 2–4:** 实现 tmux `resize-pane -t %pane_id -x cols -y rows`；由 Task 3.0（Window resize event + debounce）调用

**Step 5: Commit**

`git add src/runtime/backends/tmux.rs tests/functional/terminal/tmux_resize_test.sh && git commit -m "feat(terminal): tmux resize via resize-pane"`

---

## Phase 4：Viewport Culling（1 天）

### Task 4.1：content_mask 求交与可见行/列过滤

**Zed 实现：** `window.content_mask().bounds.intersect(&bounds)` 求交，按可见区域过滤 cells。

**visible_line ≠ screen_line：** 必须 `visible_line = grid_line - display_offset`，否则 tmux scrollback 时 cursor 漂移。

**⚠️ tmux + viewport culling：** 启用后 `display_offset` 与 tmux pane scroll 可能不同，cursor 会错。必须在 Task 2.5 完成 LogicalCursor / VisualCursor / TmuxCursor 转换，否则 vim 模式错位。

**宽终端水平裁剪：** 不仅按**行**过滤，还需按**列**过滤。宽终端（如 200+ 列）或水平滚动时，viewport 可能只显示部分列；需计算 `visible_col_start..visible_col_end`，只 layout/paint 可见列内的 cells，避免宽屏场景下的多余绘制。

**Files:**
- Modify: `src/ui/terminal_element.rs`
- Test: `src/ui/terminal_element.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn test_visible_bounds_empty_intersection_returns_no_cells() {
    // bounds 完全在 viewport 外 → layout_grid 输入为空
}
#[test]
fn test_visible_bounds_partial_intersection_filters_rows() {
    // 仅部分行可见 → 只 layout 可见行
}
#[test]
fn test_visible_bounds_horizontal_culling_for_wide_terminal() {
    // 宽终端 200 列，viewport 只显示 col 50..150
    let visible = visible_bounds_from_intersection(
        &full_bounds, &viewport_bounds, 200, 24, cell_w, cell_h);
    assert_eq!(visible.start_col, 50);
    assert_eq!(visible.end_col, 150);
    // layout_grid 输入应只包含这些列对应的 cells
    let (_, runs) = layout_grid(visible_cells_iter, ...);
    assert!(runs.iter().all(|r| r.start_point.column >= 50 && r.end_col() <= 150));
}
```

**Step 2–4:** 实现 `window.content_mask().bounds` 与 bounds 求交，按可见**行和列**过滤 cells（参考 Zed 8.5）

**Step 5: Commit**

`git add src/ui/terminal_element.rs && git commit -m "feat(terminal): viewport culling for layout_grid"`

---

## Phase 5：回归与全量测试

### Task 5.1：全量单元/集成测试

**Commands:**

```bash
RUSTUP_TOOLCHAIN=stable cargo test 2>&1
```

Expected: 所有 `terminal_element`、`terminal_rendering`、`terminal_view`、`terminal_engine`、`status_detector` 等测试 PASS。

---

### Task 5.2：回归测试套件

**Commands:**

```bash
tests/regression/run_all.sh
```

Expected:
- Sidebar 状态颜色
- 终端光标位置
- ANSI 颜色显示
- （新增）TUI 光标形状/可见性

---

### Task 5.3：功能测试

**Commands:**

```bash
tests/functional/run_all.sh
```

Expected: terminal、tui、render、status 等子集通过。

---

### Task 5.4：E2E 与手动验收

**Commands:**

```bash
tests/e2e/run_all.sh
```

手动验证：
- vim：normal/insert 模式光标切换
- Claude/OpenCode：终端内光标
- 快速输入无延迟
- resize 后终端正确响应

---

### Task 5.5：性能基准测量

**目标：** 验证「达到 Zed/Ghostty 级性能」。

**测量指标：**

| 指标 | 目标 | 方法 |
|------|------|------|
| 元素数量 | 1（vs 当前 ~60） | 在 render 路径打印或 assert 元素树深度/数量 |
| 帧时间 | < 16ms（60fps） | `Instant::now()` 记录每帧 paint 耗时 |
| FPS | ≥ 60（快速滚动时） | 统计 1 秒内 frame 数 |

**实现：**

1. **元素数量**：在 `TerminalView::render` 返回前，验证只返回 1 个 TerminalElement（无子 div/span 树）
2. **帧时间**：在 `TerminalElement::paint` 内：
   ```rust
   let start = std::time::Instant::now();
   // ... paint ...
   log::debug!("terminal paint: {:?}", start.elapsed());
   ```
3. **场景**：运行 `vim` 或 `cat large_file`，快速滚动/输入，观察 `--log debug` 输出
4. **对比**：与 Zed 内建终端做相同操作（手动或脚本），对比帧时间

**Files:**
- Modify: `src/ui/terminal_element.rs`（临时或可选 debug 日志）
- Create: `crates/terminal_bench/` 或 `tests/performance/terminal_paint_bench.rs`（可选，规范化 benchmark）

**验收标准：**
- 元素数量 = 1 ✓
- 快速输入时无肉眼卡顿 ✓
- 帧时间 < 16ms（P95）✓

---

### Task 5.6：并发 resize + 大输出集成测试

**目标：** 覆盖并发 resize 与大量终端输出场景，验证鲁棒性。

```rust
// tests/terminal_element_integration.rs 或 tests/functional/
#[test]
fn test_concurrent_resize_heavy_output() {
    // 1. 启动 PTY + TerminalEngine
    // 2. 后台线程：快速连续 resize（模拟用户拖拽）
    // 3. 主线程：cat large_file 或 yes 大量输出
    // 4. 验证：无 panic、无 deadlock、最终内容正确
}
```

或对应 shell 脚本：启动 pmux，同时 resize 窗口 + 在终端执行 `cat /usr/share/dict/words`。

**PTY flood + GPU stall 测试（必须）**

模拟 `yes | pv > /dev/null`，暴露 paint backlog；否则 paint backlog 不会暴露：

```rust
#[test]
fn test_pty_flood_gpu_stall() {
    // 1. PTY 快速输出（yes 或大量行）
    // 2. 模拟 GPU stall（如短暂 sleep 或 mock）
    // 3. 验证：无 panic、无 backlog 导致内容错乱
}
```

---

### Task 5.7：ShapedLine 缓存验收

**已在 Task 1.2b 实现**（必须项，非可选）。Phase 5 验收：
- vim 快速滚动时每帧 shape_line 调用数 << 未缓存时的 200~400
- 通过 cache hit 率或 profile 验证

---

## 测试清单汇总

| 层级 | 文件/命令 | 覆盖 |
|------|-----------|------|
| Unit | `src/terminal/renderable_snapshot.rs` 或 `terminal_element.rs` | RenderableSnapshot::from, empty |
| Unit | `src/ui/terminal_renderer/` | ShapedLineCache, RowCache, layout_grid，**test_zerowidth_cluster_index_inherited** |
| Unit | `src/ui/terminal_element.rs` #[cfg(test)] | BatchedTextRun, LayoutRect, DisplayCursor, cursor_position, TerminalBounds, viewport clip |
| Unit | `src/ui/terminal_rendering.rs` | group_cells_into_segments（已有） |
| Unit | `src/terminal/engine.rs` | advance_bytes, renderable_content（已有） |
| Integration | `tests/terminal_element_integration.rs` | TerminalView + TerminalElement，并发 resize + 大输出，**PTY flood + GPU stall** |
| Integration | `tests/terminal_cursor.rs` | Cursor shape, visibility |
| Integration | tmux + vim + scroll | LogicalCursor/VisualCursor/TmuxCursor 转换（Task 2.5） |
| Integration | `tests/terminal_rendering.rs` | 已有 VT、alternate screen、status |
| Integration | `tests/terminal_engine_osc133.rs` | 已有 |
| Regression | `tests/regression/run_all.sh` | Sidebar, cursor, colors, TUI cursor |
| Functional | `tests/functional/terminal/resize_test.sh` | Local resize |
| Functional | `tests/functional/terminal/tmux_resize_test.sh` | Tmux resize |
| Functional | `tests/functional/tui/vim_compatibility_test.sh` | vim 光标 |
| E2E | `tests/e2e/` | 用户场景 |
| Performance | Task 5.5：帧时间、FPS、元素数量 | Zed/Ghostty 级验收 |

---

## 执行建议

**阶段顺序（重要）：** Phase 1 → **Phase 1.5（RenderableGrid split）** → Phase 2 → Phase 3 → Phase 4。否则 debug 会非常痛苦。

1. **TDD**：每 Task 先写失败测试，再实现，再验证通过。
2. **Subagent**：Phase 1 内 Task 1.2–1.4 可并行；Task 1.2b、Task 1.4b 必须在 Task 1.5 前完成；**Phase 1.5 必须在 Phase 2 前完成**。
3. **Commit**：每 Task 完成后独立 commit，便于回滚。
4. **重构清单**：迁移时移除 `terminal_view.rs`、`terminal_rendering.rs` 中写入 `~/.cursor/debug-*.log` 的调试代码。

---

## 高收益建议（按需）

**[建议 A] FrameArena：** Zed 的隐藏性能来源，frame-local allocation。否则 `Vec<TextRun>` 每帧 alloc/free。

**[建议 B] Row Hash 升级为 Render Hash：** 不要只缓存 StyledSegment；缓存 `(style + glyph cluster + width)`，否则 shaping 成本仍在。

**[建议 C] Cursor 单独 Layer：** 不要在 text pass 画 cursor。cursor repaint 频率 >> text repaint。

---

## 建议优先修改项（P0/P1/P2）

| 优先级 | 项 |
|--------|-----|
| **P0** | 引入 RenderableGrid（解耦 Element）；resize 移出 request_layout |
| **P1** | 完整 style equality；grapheme/zerowidth cluster 处理 |
| **P2** | FrameArena；Cursor Layer |

---

## 参考

- `docs/plans/2026-02-28-terminal-element-brainstorm.md`
- Zed `crates/terminal_view/src/terminal_element.rs`
- `src/ui/terminal_rendering.rs`、`src/ui/terminal_view.rs`
