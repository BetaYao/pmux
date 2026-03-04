# tmux Input Path Benchmark Report

**Date:** Tue Mar  3 13:15:16 CST 2026  
**Session:** `__pmux_perf_15460_36784` (ephemeral)  
**Iterations:** 220 (warmup: 20)  
**Timeout:** 250ms  
**Threshold Profile:** `relaxed`

## Results

| Metric | Value | Threshold | Status |
|---|---:|---:|---|
| tmux single p50 | 0.664 ms | <= 8.0 ms | PASS |
| tmux single p95 | 0.841 ms | <= 20.0 ms | PASS |
| tmux burst per-char p50 | 0.006 ms | <= 3.0 ms | PASS |
| local PTY single p50 | 0.008 ms | baseline | INFO |
| ratio p50 (tmux/local) | 79.288 x | <= 60.0 x | WARN |
| total timeouts | 0 | 0 | PASS |

## Distribution Detail

- tmux single: min=0.297ms mean=0.670ms p50=0.664ms p90=0.776ms p95=0.841ms p99=0.936ms max=1.006ms
- tmux burst per-char: mean=0.006ms p50=0.006ms p95=0.008ms
- local single: p50=0.008ms p95=0.009ms
- ratio: p50=79.288x p95=92.160x
- pmux UI E2E (automation-included): count=20 timeout=0 min=157.412ms mean=190.079ms p50=183.251ms p95=199.912ms max=383.545ms

## Verdict

**WARN**

## Notes

- pmux UI E2E 指标包含 AppleScript 自动化开销与 shell 执行时间，主要用于趋势观测（同机同环境对比）。
- tmux control-mode 指标不含 GUI 自动化，主要用于锁定输入注入链路回归。
