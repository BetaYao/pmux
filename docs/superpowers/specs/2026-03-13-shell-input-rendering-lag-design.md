# Shell 模式输入渲染延迟修复

**日期**: 2026-03-13
**状态**: 设计已批准

## 问题

在普通 shell（zsh）下快速输入或连续删除时，出现两个视觉问题：

1. **空字符闪烁**：快速连续输入时，偶尔出现空白字符，随后才补上正确内容
2. **光标超前**：连续按退格键时，光标位置先于字符消失，字符过一会才被擦除

### 根本原因

`coalesce_and_process_output()` 收到第一个 `%output` 块后**固定等待 4ms** 再统一处理。
zsh 的回显/重绘会被拆成多个 `%output` 事件，4ms 的切割点与按键边界不对齐，
导致部分回显序列在中间状态被渲染——光标先动了但字符没出来，或字符出来了但行尾清理没到。

### 当前数据流

```
%output 到达 → 固定等 4ms → process_output() + cx.notify() → 立即渲染
```

当前 shell 模式：每次 `coalesce_and_process_output()` 返回后立即触发渲染（没有
deferred/pending 机制——那套只在 alt screen TUI 模式下生效）。4ms 固定窗口在快速
输入时切割不准，导致中间状态被渲染。

## 设计

### 核心思路：处理与渲染分离

```
%output 到达 → 立刻 process_output()（VTE grid 始终最新）
                                              ↘
                               渲染定时器（16ms）→ cx.notify() → 画面更新
```

- **处理零等待**：数据到达立刻喂给 VTE parser，grid 状态始终准确
- **渲染定频**：16ms（60fps）触发一次 UI 重绘，16ms 窗口内的所有 `%output` 合并为一帧
- **TUI 模式不变**：alt screen 下继续使用现有完整的输出循环逻辑不变（包括
  `coalesce_and_process_output()` 50ms 合并、`pending_notify`、`RENDER_GAP`、
  `MAX_RENDER_DELAY` 等机制）

### 输出任务循环结构

两个输出任务循环（`setup_local_terminal` ~line 1335 和 `setup_pane_terminal_output`
~line 1736）都需要改动。两者结构相同，下面以一个为例。

循环顶部根据 `terminal.mode().contains(ALT_SCREEN)` 分流：

```rust
// 共享状态（在 loop 外部声明）：
let mut dirty = false;                     // Shell 路径用的脏标志

loop {
    let alt_screen = terminal_for_output.mode()
        .contains(alacritty_terminal::term::TermMode::ALT_SCREEN);

    if alt_screen {
        // ── TUI 路径：现有逻辑整体搬入此分支 ──
        // 包括：idle_timeout 计算、coalesce_and_process_output() 调用、
        // pending_notify/RENDER_GAP/MAX_RENDER_DELAY 渲染策略、
        // status detection、take_dirty() 等。
        // 代码与当前完全相同，只是缩进进了 if 块。
        //
        // 进入 TUI 路径时重置 Shell 状态：
        dirty = false;
    } else {
        // ── Shell 路径：新的零等待处理 + 定频渲染 ──
        // 用 futures_util::future::select 嵌套实现三路等待
        //
        // 进入 Shell 路径时重置 TUI 状态：
        pending_notify = false;
        first_pending_time = None;
    }
}
```

**TUI 路径重构说明**：现有循环体（从 idle_timeout 计算到渲染策略）整体搬入
`if alt_screen` 分支内。代码逻辑零修改，仅增加一层缩进。具体包括：
1. idle_timeout 计算（基于 `pending_notify` 和 `last_output_time`）
2. `coalesce_and_process_output()` 调用
3. `!got_output` 分支（pending_notify gap-render + take_dirty idle + agent status）
4. `got_output` 分支（status detection + 渲染策略 pending_notify/MAX_RENDER_DELAY）

注意：现有渲染策略有三个分支（modal_open / alt_screen / else-shell），搬入 TUI 路径后
`else`（shell 立即渲染）分支变为死代码（因为已在 `if alt_screen` 块内）。实现时应
删除该 else 分支以保持代码清晰。

**模式切换状态管理**：

共享状态变量分两类：
- **路径专属**（切换时需重置）：
  - Shell→TUI：`dirty = false`
  - TUI→Shell：`pending_notify = false; first_pending_time = None`
- **跨路径持久**（切换时不重置，自然延续）：
  - `last_output_time`：上次收到数据的时间戳
  - `last_status_check`：上次 status detection 时间戳
  - `last_phase`：上次 shell phase
  - `last_alt_screen`：上次 alt screen 状态（TUI 路径用于检测模式变化）
  - `agent_override`：当前检测到的 agent 定义

### Shell 路径伪代码

```rust
// ── Shell 路径 ──
// 使用 futures_util::future::select + pin_mut!（与项目现有用法一致）

// 渲染定时器：每次迭代创建新的一次性 16ms timer
let render_tick = cx.background_executor().timer(Duration::from_millis(16));

// idle 超时：基于 last_output_time 的距离
let idle_timeout = if Instant::now().duration_since(last_output_time) < Duration::from_secs(2) {
    Duration::from_millis(300)
} else {
    Duration::from_secs(2)
};
let idle_tick = cx.background_executor().timer(idle_timeout);

// 三路等待：用嵌套 select 实现
// select(rx.recv_async(), select(render_tick, idle_tick))
let recv = rx.recv_async();
let timers = select(render_tick, idle_tick);
pin_mut!(recv);
pin_mut!(timers);

match select(recv, timers).await {
    Either::Left((Ok(chunk), _)) => {
        // ── 数据到达：立刻处理，零等待 ──
        terminal.process_output(&chunk);
        ext.feed(&chunk);
        // 排空所有已到的数据
        while let Ok(next) = rx.try_recv() {
            terminal.process_output(&next);
            ext.feed(&next);
        }
        dirty = true;
        last_output_time = Instant::now();

        // ── Status detection（与现有 got_output=true 分支完全相同）──
        // 节流：200ms 间隔或 phase 变化时运行
        // 注意：现有代码还检查 alt_screen != last_alt_screen，但在 Shell 路径中
        // alt_screen 始终为 false（否则不会进入此分支），因此省略该条件。
        let now = Instant::now();
        let phase = ext.shell_phase();
        if phase != last_phase || now.duration_since(last_status_check) >= status_interval {
            last_status_check = now;
            last_phase = phase;
            // agent_override detection + status_publisher.check_status()/force_status()
            // （代码与现有 got_output=true 分支中的 status detection 完全相同，
            //  但省略 alt_screen 相关分支——Shell 路径只需处理 agent_override 和
            //  normal shell 两种情况）
        }

        // ── 模式再次检查：处理期间可能进入 alt screen ──
        let recheck = terminal_for_output.mode()
            .contains(alacritty_terminal::term::TermMode::ALT_SCREEN);
        if recheck {
            // 程序刚启动进入 TUI，不在此渲染，让下次迭代走 TUI 路径
            dirty = false;
            continue;
        }
    }
    Either::Left((Err(_), _)) => {
        // ── channel 关闭：退出循环 ──
        break;
    }
    // 嵌套 select 结果映射：
    //   Either::Left          = recv（数据到达或 channel 关闭）
    //   Either::Right(Left)   = render_tick（16ms 渲染定时器到期）
    //   Either::Right(Right)  = idle_tick（idle 超时到期）
    Either::Right((Either::Left((_, _)), _)) => {
        // ── render_tick 到期 ──
        if dirty {
            // 清除 terminal dirty 标志（拾取 background resync 修正）
            terminal_for_output.take_dirty();

            if !modal_open.load(Ordering::Relaxed) {
                if let Some(ref tae) = term_area_entity {
                    let _ = cx.update_entity(tae, |_, cx| cx.notify());
                }
            }
            dirty = false;
        } else {
            // 无输出数据，但 background resync 线程可能标记了 dirty
            if terminal_for_output.take_dirty()
                && !modal_open.load(Ordering::Relaxed)
            {
                if let Some(ref tae) = term_area_entity {
                    let _ = cx.update_entity(tae, |_, cx| cx.notify());
                }
            }
        }
    }
    Either::Right((Either::Right((_, _)), _)) => {
        // ── idle_tick 到期 ──
        // 拾取 background resync 修正（与现有 idle 分支逻辑相同）
        if terminal_for_output.take_dirty()
            && !modal_open.load(Ordering::Relaxed)
        {
            if let Some(ref tae) = term_area_entity {
                let _ = cx.update_entity(tae, |_, cx| cx.notify());
            }
        }

        // agent_override idle status detection（与现有逻辑相同）
        if let Some(ref agent_def) = agent_override {
            let screen_text = terminal_for_output.screen_tail_text(
                terminal_for_output.size().rows as usize,
            );
            let detected = agent_def.detect_status(&screen_text);
            if let Some(ref pub_) = status_publisher {
                let _ = pub_.force_status(
                    &status_key_clone,
                    detected,
                    &screen_text,
                    &agent_def.message_skip_patterns,
                );
            }
        }
    }
}
```

**关键实现细节**：

1. **`select` 实现方式**：使用 `futures_util::future::select` + `pin_mut!`（项目
   已有依赖，见 `app_root.rs` line 21-22）。三路等待用嵌套 select 实现：
   `select(recv, select(render_tick, idle_tick))`。不使用 `select!` 宏——项目中
   没有引入任何提供该宏的 crate。

2. **定时器语义**：`render_tick` 和 `idle_tick` 都是**一次性 timer**（GPUI 的
   `background_executor().timer()` 返回的是 one-shot future），每次循环迭代在
   Shell 分支入口处**重新创建**。这确保每次迭代都有独立的 16ms 渲染窗口和
   独立的 idle 超时。

3. **`take_dirty()` 处理**：在 render tick 和 idle tick 分支中调用
   `terminal_for_output.take_dirty()`。background resync 线程（在 `terminal_core.rs`
   ~line 267 运行）调用 `capture_pane_resync()` 并将结果写入 VTE grid，然后设置
   dirty 标志。Shell 路径通过 `take_dirty()` 拾取这些修正——**不直接调用
   `capture_pane_resync()`**，这与现有架构一致。

4. **模式切换防护**：
   - 循环顶部：每次迭代重新检查 `alt_screen`，自然完成路径切换
   - 数据处理末尾：`recheck` 检查 alt screen，如果处理的数据包含 `CSI ?1049h`
     则 `continue` 跳过渲染，下次迭代走 TUI 路径
   - 切换时重置状态：Shell→TUI 时 `dirty = false`，TUI→Shell 时
     `pending_notify = false; first_pending_time = None`

5. **Status detection 放置**：移到数据处理分支末尾（`dirty = true` 之后），
   复用现有的 throttle 逻辑（200ms 间隔或 phase 变化触发）。idle 分支中保留
   agent_override 的 idle status detection。与现有代码完全相同，仅调用位置不同。

6. **Channel 关闭处理**：`recv_async()` 返回 `Err` 时 `break` 退出循环，
   与现有 `coalesce_and_process_output()` 的 `Err(e) => return Err(e)` → loop break
   行为一致。

7. **防止渲染饥饿**：`render_tick` 是独立的 16ms timer。即使数据持续到达，
   每次 `select` 迭代后 `render_tick` 会在 16ms 后就绪。数据处理分支设置
   `dirty = true` 后立即回到 loop 顶部重新创建 timer 并 select，渲染分支在
   下一个 tick 触发。最坏情况：数据高速到达时，每 16ms 渲染一次（60fps 上限）。

8. **`try_recv` 排空循环**：数据到达分支中 `while let Ok(next) = rx.try_recv()`
   只排空 channel 中**当前已缓冲**的数据（非阻塞），不会无限等待。`process_output()`
   是纯 CPU VTE 解析，单次调用微秒级。即使高吞吐下一次排空几十个 chunk，总耗时
   仍远小于 16ms，不会阻塞 render tick。

### 现有逻辑的处理

| 现有机制 | Shell 路径 | TUI 路径 |
|----------|-----------|----------|
| `coalesce_and_process_output()` | 不使用（零等待 + try_recv 排空替代） | 不变（搬入 if alt_screen 块） |
| `pending_notify` / `RENDER_GAP` | 不使用（16ms render tick 替代） | 不变 |
| `MAX_RENDER_DELAY` | 不需要（16ms tick 保证渲染） | 不变 |
| `modal_open` 检查 | 在 render tick 分支中检查 | 不变 |
| `is_synchronized_output()` | shell 模式下不触发，不检查 | 不变 |
| `take_dirty()` | 在 render tick 和 idle tick 分支中调用（拾取 background resync 修正） | 不变 |
| background resync 线程 | 不变（线程独立运行，Shell 路径通过 take_dirty() 拾取） | 不变 |
| idle resync | 通过 idle tick + take_dirty() 实现（不直接调用 capture_pane_resync） | 不变 |
| status detection | 数据处理分支末尾运行（throttle 逻辑不变，时机更及时） | 不变 |
| agent_override idle detection | idle tick 分支中运行 | 不变 |

### 延迟分析

| 场景 | 当前延迟 | 新延迟 |
|------|---------|--------|
| Shell 单键回显（最坏） | tmux ~10ms + 4ms 合并 + 渲染 ≈ 14ms | tmux ~10ms + 0ms 处理 + 最多 16ms 等渲染 ≈ 26ms |
| Shell 单键回显（最好） | tmux ~10ms + 4ms 合并 ≈ 14ms | tmux ~10ms + 0ms 处理 + ~0ms（下次迭代 tick 立即到期）≈ 10ms |
| Shell 快速连续输入 | 每键 4ms 固定等待，切割不准，中间状态可见 | 零等待处理，16ms 帧内全部合并，无中间状态 |
| TUI 帧渲染 | 50ms 合并（不变） | 50ms 合并（不变） |

**最坏单键延迟分析**：从按键到渲染的完整路径：
1. 输入：按键 → `send_input()` → tmux PTY（~1ms 本地）
2. 回显：tmux PTY 回显 → `%output` 到达 parser（tmux 延迟 ~5-10ms）
3. 处理：`process_output()`（新方案：0ms 等待，立即处理）
4. 渲染：等待下一个 render tick（0-16ms，平均 8ms）

新方案最坏 = 10ms(tmux) + 0ms(处理) + 16ms(等 tick) = **26ms**
当前方案最坏 = 10ms(tmux) + 4ms(合并) + 0ms(立即渲染) = **14ms**

**权衡**：单键最坏延迟增加 12ms（从 14ms 到 26ms，平均增加 ~8ms），仍远低于
人眼感知阈值（~50ms）。但快速输入时**彻底消除中间状态渲染**——这是用户实际遇到
的问题。

## 改动范围

| 文件 | 改动 |
|------|------|
| `src/ui/app_root.rs` ~line 1335 | `setup_local_terminal` 输出循环：添加 alt screen 分流，TUI 路径包裹现有代码，Shell 路径用嵌套 select |
| `src/ui/app_root.rs` ~line 1736 | `setup_pane_terminal_output` 输出循环：同上 |
| `src/ui/app_root.rs` ~line 234 | `coalesce_and_process_output()` 保留不变，仅被 TUI 路径调用 |

每个循环的改动：
- **TUI 路径**：现有循环体（约 180 行）整体搬入 `if alt_screen` 块，仅增加缩进，逻辑零修改
- **Shell 路径**：新增约 80 行（嵌套 select + 数据处理 + render tick + idle tick + status detection）
- **共享状态**：新增 `dirty: bool` 声明（1 行），模式切换重置逻辑（各 2 行）

总改动：每个循环约 85 行新代码 + 约 180 行缩进调整 = 约 265 行 touched。
两个循环合计约 530 行 touched（其中约 170 行是新代码，360 行是缩进调整）。

## 不改动的部分

- 输入路径（`send_input` → writer thread）
- `capture_initial_content()` / `capture_pane_resync()`
- Background resync 线程（`terminal_core.rs` ~line 240-290）
- TUI 模式的输出处理逻辑（代码不变，仅缩进进 if 块）
- IME 处理（50ms Enter 延迟）
- `coalesce_and_process_output()` 函数本身（保留给 TUI 路径）
- Status detection / agent detection 逻辑（代码不变，Shell 路径中调用位置从渲染后移到处理后）

## 验证方法

### 功能验证（手动）

1. **快速输入**：在 shell 中快速输入 `echo abcdefghijklmnop`，观察是否还有空字符闪烁
2. **快速删除**：在 shell 中快速连续按退格键删除长命令，观察光标是否还会超前于字符消失
3. **TUI 模式**：在 shell 中启动 `claude` / `vim` 等 TUI 程序，确认 TUI 渲染不受影响
4. **模式切换**：在 TUI 中退出回到 shell，确认 Shell 路径正常工作；再次启动 TUI 确认 TUI 路径正常
5. **渲染饥饿测试**：运行 `seq 1 100000` 或 `cat` 大文件，观察屏幕是否持续滚动
   更新（非卡住后一次性跳到末尾）。这验证 16ms render tick 在高吞吐下不被饿死。
6. **Background resync**：idle 2s+ 后确认终端内容与 tmux 一致（`take_dirty()` 拾取修正）
7. **Modal dialog**：开启 modal dialog 时确认渲染暂停（`modal_open` 检查生效）
8. **主观流畅度**：对比修改前后的输入体验

### 回归检查

9. **Agent status（Shell 模式）**：在 shell 中运行简单命令，确认 status 从 Idle→Running→Idle 正确切换
10. **Agent status（TUI 模式）**：启动 Claude Code，确认 agent status detection 正常（Running/Waiting/Idle）
11. **Channel 关闭**：关闭 tab / workspace 时确认循环正常退出（`Err(_) => break`），无 panic 或 hang
12. **多次模式切换**：在 shell 和 TUI 之间快速多次切换（启动 vim → :q → vim → :q），
    确认状态变量正确重置，无渲染异常

### 关于自动化测试

本改动的核心是异步输出循环中的渲染时机调整，依赖 GPUI 的 `background_executor().timer()`
和 `cx.update_entity()`。这些是 GPUI 框架的运行时行为，无法在不启动完整 GPUI 应用的
情况下进行单元测试。现有代码库中同类逻辑（`pending_notify`、`RENDER_GAP`、
`MAX_RENDER_DELAY`）也没有自动化测试。因此本改动以手动验证为主。
