# tmux Input Path Benchmark Report

**Date:** Tue Mar  3 12:40:49 CST 2026  
**Session:** `__pmux_perf_9498_69374` (ephemeral)  
**Iterations:** 80 (warmup: 10)  
**Timeout:** 250ms  
**Threshold Profile:** `strict`

## Results

| Metric | Value | Threshold | Status |
|---|---:|---:|---|
| tmux single p50 | 0.320 ms | <= 2.0 ms | PASS |
| tmux single p95 | 0.733 ms | <= 6.0 ms | PASS |
| tmux burst per-char p50 | 0.005 ms | <= 0.5 ms | PASS |
| local PTY single p50 | 0.007 ms | baseline | INFO |
| ratio p50 (tmux/local) | 42.862 x | <= 25.0 x | WARN |
| total timeouts | 0 | 0 | PASS |

## Distribution Detail

- tmux single: min=0.117ms mean=0.487ms p50=0.320ms p90=0.715ms p95=0.733ms p99=3.548ms max=3.580ms
- tmux burst per-char: mean=0.006ms p50=0.005ms p95=0.008ms
- local single: p50=0.007ms p95=0.008ms
- ratio: p50=42.862x p95=89.752x
- pmux UI E2E (automation-included): count=8 timeout=0 min=167.296ms mean=204.853ms p50=182.910ms p95=197.928ms max=363.272ms

## Verdict

**WARN**

## Notes

- pmux UI E2E 指标包含 AppleScript 自动化开销与 shell 执行时间，主要用于趋势观测（同机同环境对比）。
- tmux control-mode 指标不含 GUI 自动化，主要用于锁定输入注入链路回归。
