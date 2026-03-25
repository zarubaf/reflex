# Performance Counters

Reflex can parse and display performance counters from µScope traces. Counters track metrics like committed instructions, cache misses, branch mispredictions, and stall cycles.

## How Counters Are Detected

Counters are automatically detected from the µScope trace schema. A storage is recognized as a counter if it has:
- Exactly 1 slot (dense, not sparse)
- A single `U64` field
- Values updated via `DA_SLOT_ADD` (incremental additions)

Common counters include `committed_insns`, `cycles`, `retired_insns`, `mispredicts`, `dcache_misses`, `icache_misses`.

## Counter Panel

Switch to the **Counters** tab (next to Pipeline Viewer) to see the counter panel. It has two view modes, toggled by clicking the mode button in the header.

### Detail Mode

Shows each counter as a row with:
- **Mode indicator** (T/R/D) — click to cycle between display modes
- **Counter name**
- **Value at cursor** — numeric value at the current cursor position
- **Sparkline** — min-max envelope visualization below each counter

### Heatmap Mode

Shows all counters as a compact matrix:
- One row per counter (~6px per row)
- Color intensity = per-cycle delta activity
- Per-counter normalization (each row's max maps to full brightness)
- Counter names shown when rows are tall enough

The heatmap is designed for scanning 200+ counters at a glance to spot activity patterns and anomalies.

## Display Modes

Click the mode indicator (T/R/D) on any counter row to cycle through:

| Mode | Label | Description |
|------|-------|-------------|
| Total | T | Raw cumulative value |
| Rate | R | Delta per cycle over a 64-cycle window (e.g., IPC) |
| Delta | D | Single-cycle change |

## Counter Range

The counter panel displays data for the **counter range**, which is independent from the pipeline viewport. By default, the counter range covers the full trace. Use the [minimap](minimap.md) handles to narrow the range to a region of interest.

This independence means you can view full-trace IPC trends in the counter panel while the pipeline view is zoomed into a specific region for detailed inspection.

## Konata Traces

Konata format traces do not include performance counters. The counter panel will show "No performance counters in this trace" for Konata files.
