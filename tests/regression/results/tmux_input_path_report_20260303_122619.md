# tmux Input Path Benchmark Report

**Date:** Tue Mar  3 12:26:29 CST 2026  
**Session:** `__pmux_perf_26199_41552` (ephemeral)  
**Iterations:** 180 (warmup: 20)  
**Timeout:** 250ms

## Results

| Metric | Value | Threshold | Status |
|---|---:|---:|---|
| tmux single p50 | 0.609 ms | <= 8.0 ms | PASS |
| tmux single p95 | 0.994 ms | <= 20.0 ms | PASS |
| tmux burst per-char p50 | 0.006 ms | <= 3.0 ms | PASS |
| local PTY single p50 | 0.008 ms | baseline | INFO |
| ratio p50 (tmux/local) | 78.602 x | <= 4.0 x | WARN |
| total timeouts | 0 | 0 | PASS |

## Distribution Detail

- tmux single: min=0.125ms mean=0.554ms p50=0.609ms p90=0.939ms p95=0.994ms p99=1.161ms max=1.668ms
- tmux burst per-char: mean=0.006ms p50=0.006ms p95=0.007ms
- local single: p50=0.008ms p95=0.009ms
- ratio: p50=78.602x p95=116.361x

## Verdict

**WARN**
