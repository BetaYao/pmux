# Workspace Input Latency Comparison

**Date**: 2026-03-03 18:38:20
**tmux version**: tmux 3.6a
**Iterations**: 50

## Workspaces

| | WS1 | WS2 |
|---|---|---|
| Path | `saas-mono` | `okena` |
| Pane size | 80x24 | 80x24 |
| Pane TTY | /dev/ttys031 | /dev/ttys040 |

## Test 1: send-keys -H round-trip (ms)

This is pmux's actual input path. Measures: send hex → tmux processes → shell executes → capture-pane sees output.

| Metric | saas-mono | okena |
|---|---|---|
| Stats | N=0 min=0 max=0 avg=0 p50=0 p95=0 p99=0 | N=0 min=0 max=0 avg=0 p50=0 p95=0 p99=0 |

## Test 2: Direct TTY write round-trip (ms)

Bypasses tmux command parsing. Writes directly to pane TTY.

| Metric | saas-mono | okena |
|---|---|---|
| Stats | N=50 min=6 max=13 avg=6.2 p50=6 p95=6 p99=13 | N=50 min=5 max=25 avg=6.4 p50=6 p95=14 p99=25 |

## Test 3: Shell startup time

Time for `zsh -i -c exit` in each directory (ms).

| saas-mono | okena |
|---|---|
| 359ms | 75ms |

## Test 4: git status overhead

| saas-mono | okena |
|---|---|
| 0m0.054s | 0m0.014s |

## Raw Data

### send-keys -H latencies (ms)

#### saas-mono
```
 
```

#### okena
```
 
```

### Direct TTY write latencies (ms)

#### saas-mono
```
13 8 6 6 6 6 6 6 6 6 6 6 6 6 6 6 6 6 6 6 6 6 6 6 6 6 6 6 6 6 6 6 6 6 6 6 6 6 6 6 6 6 6 6 6 6 6 6 6 6 
```

#### okena
```
16 6 5 5 5 6 6 5 6 25 14 6 6 6 5 5 5 5 6 5 6 6 5 5 5 5 5 8 5 6 6 6 6 6 6 6 6 6 6 6 6 6 6 6 5 6 6 5 5 5 
```
