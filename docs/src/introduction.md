# Reflex

A fast, GPU-accelerated CPU pipeline trace visualizer built with [GPUI](https://gpui.rs) (Zed's rendering framework).

Reflex displays instruction execution timelines, queue occupancy, performance counters, and pipeline stage progression from CPU simulator traces.

## Features

- **Pipeline visualization** — GPU-rendered instruction timeline with smooth panning, zooming, and scrolling at 60fps
- **Performance counters** — Sparklines, heatmap overview, and numeric display for 200+ hardware counters
- **Minimap** — Full-trace overview with draggable range selection and pipeline position indicator
- **Multicursor** — Place multiple cursors to measure cycle deltas between pipeline events, with undo/redo
- **Queue panels** — Live retire, dispatch, and issue queue occupancy at the cursor position
- **Timeline overlay** — Counter sparkline strip above the pipeline view for correlation
- **Tabbed interface** — Open multiple traces side-by-side with independent viewport state
- **Konata & µScope formats** — Native support for [Konata](https://github.com/shioyadan/Konata) text traces and [µScope](https://github.com/zarubaf/uscope) binary traces
- **macOS & Linux** — Native builds for both platforms

## Quick Start

```bash
git clone --recurse-submodules <repo-url>
cargo run --release -- path/to/trace.uscope
```

Or drag and drop trace files onto the window.
