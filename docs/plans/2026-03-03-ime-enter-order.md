# IME 回车顺序修复方案

## 现象

- **期望**：中文输入法下输入 `ls` 回车 → 终端显示 `prompt ：ls` 并执行（一行）。
- **实际**：先多出一行空行，再出现 `prompt ：ls`，即「回车」被先发给了 tmux，提交的 `ls` 后到。

## 当前数据流

### 两条输入路径

| 路径 | 触发 | 处理位置 | 发送到 tmux |
|------|------|----------|-------------|
| **Key** | `KeyDownEvent` | `app_root::handle_key_down` → `key_to_bytes()` | 控制键、方向键、**Enter → `\r`** 等 |
| **IME** | `InputHandler::replace_text_in_range(text)` | `TerminalInputHandler` | 提交的字符串（如 `ls`），内部把 `\r`/`\n` 转成 `\r` |

- **Enter**：在 `input.rs` 里对 `enter`/`return`/`kp_enter` 直接返回 `Some(b"\r")`，在 `handle_key_down` 里若 `bytes_opt` 有值就立刻 `runtime.send_input(target, &bytes)`，因此**回车一定从 Key 路径发出**。
- **提交文字**：IME 提交时由系统调用 `replace_text_in_range(replacement_range, "ls")`，只发 `ls`，没有自带「后面跟一个回车」的语义。

### 顺序问题

- 若系统先派发 **KeyDown(Enter)** 再调用 **replace_text_in_range("ls")**：
  - 先发 `\r` → 终端换行（多出一行）。
  - 再发 `ls` → 出现在下一行。
- 因此需要保证：**在「用回车确认的 IME 提交」场景下，先发提交内容，再发一个 `\r`，且不在 Key 路径里单独发一次 Enter。**

## 方案：Pending Enter + 超时

### 思路

- 把「可能是 IME 确认用的 Enter」和「纯换行 Enter」区分开，用「延迟发送 Enter + 与 IME 提交合并」的方式统一成「先文本、后回车」。
- Key 路径：收到 **Enter 且无 Cmd/Alt 等** 时，**不立刻发 `\r`**，而是设 **pending_enter = true**，并启动一个短超时（如 50ms）。
- IME 路径：`replace_text_in_range` 被调用时，先按现有逻辑发 `text`；若 **pending_enter == true**，再发 `\r` 并清掉 pending_enter（并取消超时，若已实现）。
- 超时：若在超时内没有发生 `replace_text_in_range`，则认为这是**单独按回车**，在超时回调里发 `\r` 并清 pending_enter。

这样：

- IME 提交 + 回车：先 commit 出 `replace_text_in_range("ls")`，发 `ls`，发现 pending_enter 再发 `\r` → 一行 `ls` + 执行。
- 单独回车：只触发 KeyDown Enter，设 pending，50ms 内没有 replace_text_in_range，超时发 `\r` → 行为与现在一致。

### 需要改动的点

1. **共享状态**
   - 在「当前焦点终端」能访问到的地方维护：
     - `pending_enter: AtomicBool`（或等价）
     - 可选：`pending_enter_deadline` + 定时器，用于超时发 `\r`。
   - 若用 `cx.spawn` 做 50ms 延迟，需要在 spawn 里能拿到 `runtime + target` 和该共享状态，以便：超时若仍 pending 则 `send_input(..., b"\r")` 并清 pending。

2. **app_root::handle_key_down**
   - 在「转发到终端」分支里，当 `key == "enter" | "return" | "kp_enter"` 且无 Cmd（以及无其他需要特殊处理的修饰键）时：
     - 不调用 `key_to_bytes` 发 `\r`；
     - 设置 pending_enter = true；
     - 启动 50ms 超时任务：到期若 pending_enter 仍 true，则 `send_input(target, b"\r")` 并清 pending_enter。

3. **TerminalInputHandler::replace_text_in_range**
   - 在现有「过滤并编码 text → send_input(bytes)」之后：
     - 若存在「当前 pane 的 pending_enter」且为 true：再 `send_input(..., b"\r")`，然后清 pending_enter；
     - 若有超时任务，可在这里取消（若 GPUI 支持 cancel 或用 generation id 忽略过期回调）。

4. **作用域**
   - pending_enter 必须和「当前获得键盘焦点的终端 pane」绑定，避免多 pane 时 A 的 Enter 和 B 的 replace_text 串在一起。即：要么按 `active_pane_target` 存一个 pending_enter，要么只保留一个「当前焦点 pane」的 pending 状态，在切换焦点时清掉或忽略旧超时。

### 可选细化

- **仅对「可能 IME」的 Enter 做延迟**：若 GPUI/平台能提供「当前是否有未提交的 composition」或「该 KeyDown 是否被 IME 消费」，可以只在这种 Enter 上设 pending，其余 Enter 照常立刻发 `\r`，减少对纯英文/无 IME 场景的影响。当前方案不依赖该能力，实现简单。
- **超时时间**：50ms 是折中；可做成配置或常量，便于后续根据平台调优。

### 不采用的方案（简述）

- **在 Key 路径直接丢弃 Enter**：会破坏「无 IME 时单独回车」。
- **只在 replace_text_in_range 里根据「最后一个键是 Enter」追加 `\r`**：InputHandler 拿不到「刚按了哪个键」，需要额外状态，等价于本方案的 pending_enter。
- **完全依赖平台「先 commit 再 KeyDown」**：顺序因平台/输入法而异，不能假定，所以需要本方案在应用层统一顺序。

## 小结

- **根因**：Enter 在 Key 路径被立即发往 tmux，IME 提交的文本稍后才从 replace_text_in_range 发出，导致先换行再出字。
- **方案**：Enter 时不立刻发 `\r`，设 pending_enter；replace_text_in_range 时只发提交文本并**清掉** pending（不发 `\r`），超时（50ms）未提交则补发 `\r`。
- **行为**：中文 IME 下第一次回车仅「确认组字」、把文字送上命令行，不发送；再按一次回车才发送执行。英文/无 IME 时直接回车仍由 50ms 超时发 `\r`。
- **改动**：共享 pending 状态、`handle_key_down` 里 Enter 分支（设 pending + 50ms 超时）、input 回调里只清 pending 不发 `\r`。
