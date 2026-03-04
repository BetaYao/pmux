# tmux Input Path Benchmark Report

**Date:** Tue Mar  3 12:36:30 CST 2026  
**Session:** `__pmux_perf_2623_53917` (ephemeral)  
**Iterations:** 160 (warmup: 20)  
**Timeout:** 250ms

## Results

| Metric | Value | Threshold | Status |
|---|---:|---:|---|
| tmux single p50 | 0.321 ms | <= 8.0 ms | PASS |
| tmux single p95 | 0.907 ms | <= 20.0 ms | PASS |
| tmux burst per-char p50 | 0.006 ms | <= 3.0 ms | PASS |
| local PTY single p50 | 0.007 ms | baseline | INFO |
| ratio p50 (tmux/local) | 43.030 x | <= 4.0 x | WARN |
| total timeouts | 0 | 0 | PASS |

## Distribution Detail

- tmux single: min=0.138ms mean=0.530ms p50=0.321ms p90=0.814ms p95=0.907ms p99=1.010ms max=9.108ms
- tmux burst per-char: mean=0.006ms p50=0.006ms p95=0.006ms
- local single: p50=0.007ms p95=0.009ms
- ratio: p50=43.030x p95=103.174x
- pmux UI E2E (automation-included): count=0 timeout=16 min=0.000ms mean=0.000ms p50=0.000ms p95=0.000ms max=0.000ms

## Verdict

**WARN**

## Notes

- pmux UI E2E 指标包含 AppleScript 自动化开销与 shell 执行时间，主要用于趋势观测（同机同环境对比）。
- tmux control-mode 指标不含 GUI 自动化，主要用于锁定输入注入链路回归。
