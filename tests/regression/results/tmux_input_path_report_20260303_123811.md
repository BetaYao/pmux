# tmux Input Path Benchmark Report

**Date:** Tue Mar  3 12:38:41 CST 2026  
**Session:** `__pmux_perf_32499_64115` (ephemeral)  
**Iterations:** 160 (warmup: 20)  
**Timeout:** 250ms

## Results

| Metric | Value | Threshold | Status |
|---|---:|---:|---|
| tmux single p50 | 0.278 ms | <= 8.0 ms | PASS |
| tmux single p95 | 0.856 ms | <= 20.0 ms | PASS |
| tmux burst per-char p50 | 0.005 ms | <= 3.0 ms | PASS |
| local PTY single p50 | 0.008 ms | baseline | INFO |
| ratio p50 (tmux/local) | 34.724 x | <= 4.0 x | WARN |
| total timeouts | 0 | 0 | PASS |

## Distribution Detail

- tmux single: min=0.158ms mean=0.381ms p50=0.278ms p90=0.713ms p95=0.856ms p99=1.058ms max=1.239ms
- tmux burst per-char: mean=0.006ms p50=0.005ms p95=0.007ms
- local single: p50=0.008ms p95=0.009ms
- ratio: p50=34.724x p95=99.677x
- pmux UI E2E (automation-included): count=16 timeout=0 min=158.278ms mean=195.114ms p50=175.504ms p95=215.292ms max=460.997ms

## Verdict

**WARN**

## Notes

- pmux UI E2E 指标包含 AppleScript 自动化开销与 shell 执行时间，主要用于趋势观测（同机同环境对比）。
- tmux control-mode 指标不含 GUI 自动化，主要用于锁定输入注入链路回归。
