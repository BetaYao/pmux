# Workspace Input Latency Comparison

**Date**: 2026-03-13 19:17:55
**tmux version**: tmux 3.6a
**Iterations**: 50

## Workspaces

| | WS1 | WS2 |
|---|---|---|
| Path | `pmux` | `pmux` |
| Pane size | 80x24 | 80x24 |
| Pane TTY | /dev/ttys181 | /dev/ttys181 |

## Test 1: send-keys -H round-trip (ms)

This is pmux's actual input path. Measures: send hex → tmux processes → shell executes → capture-pane sees output.

| Metric | pmux | pmux |
|---|---|---|
| Stats | N=0 min=0 max=0 avg=0 p50=0 p95=0 p99=0 | N=0 min=0 max=0 avg=0 p50=0 p95=0 p99=0 |

## Test 2: Direct TTY write round-trip (ms)

Bypasses tmux command parsing. Writes directly to pane TTY.

| Metric | pmux | pmux |
|---|---|---|
| Stats | N=50 min=6 max=17 avg=8.5 p50=9 p95=10 p99=17 | N=50 min=6 max=27 avg=8.6 p50=8 p95=11 p99=27 |

## Test 3: Shell startup time

Time for `zsh -i -c exit` in each directory (ms).

| pmux | pmux |
|---|---|
| 431ms | 83ms |

## Test 4: git status overhead

| pmux | pmux |
|---|---|
| 0m0.024s | 0m0.022s |

## Raw Data

### send-keys -H latencies (ms)

#### pmux
```
 
```

#### pmux
```
 
```

### Direct TTY write latencies (ms)

#### pmux
```
17 12 8 6 8 7 6 9 9 10 9 7 8 10 9 7 10 9 10 10 10 10 9 9 10 9 9 8 10 7 7 8 10 9 6 7 7 7 8 6 6 9 10 7 7 7 7 8 8 9 
```

#### pmux
```
19 11 9 7 10 7 9 6 7 7 7 8 8 7 7 7 27 7 7 6 8 9 7 10 10 9 10 8 9 10 10 10 6 7 8 8 8 7 6 7 9 11 6 7 7 7 9 8 6 8 
```
