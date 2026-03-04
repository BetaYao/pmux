# Workspace Input Latency Comparison

**Date**: 2026-03-03 18:21:36
**tmux version**: tmux 3.6a
**Iterations**: 50

## Workspaces

| | WS1 | WS2 |
|---|---|---|
| Path | `okena` | `saas-mono` |
| Pane size | 80x24 | 80x24 |
| Pane TTY | /dev/ttys031 | /dev/ttys040 |

## Test 1: send-keys -H round-trip (ms)

This is pmux's actual input path. Measures: send hex → tmux processes → shell executes → capture-pane sees output.

| Metric | okena | saas-mono |
|---|---|---|
| Stats | N=0 min=0 max=0 avg=0 p50=0 p95=0 p99=0 | N=0 min=0 max=0 avg=0 p50=0 p95=0 p99=0 |

## Test 2: Direct TTY write round-trip (ms)

Bypasses tmux command parsing. Writes directly to pane TTY.

| Metric | okena | saas-mono |
|---|---|---|
| Stats | N=50 min=6 max=14 avg=8.6 p50=8 p95=13 p99=14 | N=50 min=6 max=62 avg=14.3 p50=8 p95=48 p99=62 |

## Test 3: Shell startup time

Time for `zsh -i -c exit` in each directory (ms).

| okena | saas-mono |
|---|---|
| 421ms | 84ms |

## Test 4: git status overhead

| okena | saas-mono |
|---|---|
| 0m0.015s | 0m0.058s |

## Raw Data

### send-keys -H latencies (ms)

#### okena
```
 
```

#### saas-mono
```
 
```

### Direct TTY write latencies (ms)

#### okena
```
7 6 6 6 7 8 9 7 6 9 6 7 7 6 9 9 8 7 7 8 7 9 7 7 6 7 6 9 7 7 10 12 14 13 11 11 10 10 10 11 9 11 12 8 8 7 7 14 13 12 
```

#### saas-mono
```
14 13 48 24 53 40 21 19 35 22 23 18 19 21 62 19 23 11 15 6 8 6 6 9 6 8 7 7 7 9 7 6 6 6 8 6 9 6 6 6 6 7 6 7 11 7 6 6 8 6 
```
