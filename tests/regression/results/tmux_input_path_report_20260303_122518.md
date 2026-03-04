# tmux Input Path Benchmark Report

**Date:** Tue Mar  3 12:25:30 CST 2026  
**Session:** `__pmux_perf_14985_39713` (ephemeral)  
**Iterations:** 220 (warmup: 20)  
**Timeout:** 250ms

## Results

| Metric | Value | Threshold | Status |
|---|---:|---:|---|
| tmux single p50 | 0.233 ms | <= 8.0 ms | PASS |
| tmux single p95 | 0.351 ms | <= 20.0 ms | PASS |
| tmux burst per-char p50 | 0.009 ms | <= 3.0 ms | PASS |
| local PTY single p50 | 0.013 ms | baseline | INFO |
| ratio p50 (tmux/local) | 18.667 x | <= 4.0 x | WARN |
| total timeouts | 0 | 0 | PASS |

## Distribution Detail

- tmux single: min=0.059ms mean=0.230ms p50=0.233ms p90=0.320ms p95=0.351ms p99=0.442ms max=0.499ms
- tmux burst per-char: mean=0.012ms p50=0.009ms p95=0.022ms
- local single: p50=0.013ms p95=0.019ms
- ratio: p50=18.667x p95=18.656x

## Verdict

**WARN**
