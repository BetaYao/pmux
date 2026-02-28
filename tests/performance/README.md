# pmux 性能测试

本目录包含 GUI 级性能基准脚本；终端渲染的 Rust 基准在 `crates/terminal_bench`。

## 当前覆盖

| 类型 | 脚本 | 指标 |
|------|------|------|
| 启动 | `startup_benchmark.sh` | 冷启动 / 热启动时间 |
| 渲染 | `framerate_benchmark.sh` | 输入响应延迟、大输出下帧率 |
| 输入 | `typing_benchmark.sh` | 单字延迟、吞吐、大粘贴、快捷键 |

## 运行方式

```bash
# 从项目根目录运行完整性能套件（会先编译）
cd /path/to/pmux
tests/performance/run_all.sh

# 跳过编译
tests/performance/run_all.sh --skip-build

# 建立基线模式（会写 baseline 相关输出）
tests/performance/run_all.sh --baseline
```

**注意**：脚本依赖 macOS（`osascript`、`screencapture`），且需先 `cargo build` 得到 `target/debug/pmux`。从 `run_all.sh` 调用时会在项目根目录编译并运行；单独运行某个 benchmark 时需在项目根目录执行，或设置 `PMUX_ROOT`。

## Rust 终端渲染基准

- **Benchmarks**：`crates/terminal_bench`，Criterion 基准（display_iter、segment 数量、viewport culling、row cache）。
- **运行**：`RUSTUP_TOOLCHAIN=stable cargo bench -p terminal_bench`（建议用 stable，避免 gpui 相关 SIGBUS）。
- **性能测试**：`cargo test -p terminal_bench terminal_rendering_perf`，校验 80x24 单帧处理 < 1ms。

## 已知限制与完善建议

1. **帧率测量**：无法直接读 GPUI 帧计数，framerate 脚本用「输入→响应」延迟近似；若需真实 FPS 需在应用内打点或接 profiler。
2. **报告目录**：`REPORT_DIR` 由 `init_report` 在 test_utils 中设置；输出为 `tests/regression/results/report_*.md`。
3. **综合套件**：`tests/regression/run_performance_suite.sh` 会构建 release、运行上述 3 个 benchmark（通过 `PMUX_BIN` 使用 release 二进制），并额外测量内存与 TUI 响应，生成 `performance_report.md`。
4. **可选扩展**：内存占用、TUI 响应（如 vim 方向键）在 `run_performance_suite.sh` 中有一次性问题；若需长期维护可拆成独立脚本并纳入 `run_all.sh`。
