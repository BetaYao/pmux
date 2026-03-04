# tmux Input Path Benchmark Report

**Date:** Tue Mar  3 12:40:25 CST 2026  
**Session:** `__pmux_perf_6481_68057` (ephemeral)  
**Iterations:** 80 (warmup: 10)  
**Timeout:** 250ms  
**Threshold Profile:** `relaxed`

## Results

| Metric | Value | Threshold | Status |
|---|---:|---:|---|
| tmux single p50 | 0.619 ms | <= 8.0 ms | PASS |
| tmux single p95 | 0.975 ms | <= 20.0 ms | PASS |
| tmux burst per-char p50 | 0.005 ms | <= 3.0 ms | PASS |
| local PTY single p50 | 0.008 ms | baseline | INFO |
| ratio p50 (tmux/local) | 82.068 x | <= 60.0 x | WARN |
| total timeouts | 0 | 0 | PASS |

## Distribution Detail

- tmux single: min=0.130ms mean=0.556ms p50=0.619ms p90=0.908ms p95=0.975ms p99=1.027ms max=1.106ms
- tmux burst per-char: mean=0.007ms p50=0.005ms p95=0.017ms
- local single: p50=0.008ms p95=0.009ms
- ratio: p50=82.068x p95=112.019x
- pmux UI E2E (automation-included): count=8 timeout=0 min=169.910ms mean=218.903ms p50=184.534ms p95=237.501ms max=385.509ms

## Verdict

**WARN**

## Notes

- pmux UI E2E 指标包含 AppleScript 自动化开销与 shell 执行时间，主要用于趋势观测（同机同环境对比）。
- tmux control-mode 指标不含 GUI 自动化，主要用于锁定输入注入链路回归。
