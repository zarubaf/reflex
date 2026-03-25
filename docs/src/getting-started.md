# Getting Started

## Prerequisites

- **Rust** (stable toolchain)
- **macOS** or **Linux** (GPUI supports both)
- Linux requires additional system libraries (see [Building from Source](building.md))

## Clone and Build

```bash
git clone --recurse-submodules https://github.com/zarubaf/reflex.git
cd reflex
cargo build --release
```

The `--recurse-submodules` flag is required because the [µScope](https://github.com/zarubaf/uscope) crate is included as a git submodule.

## Opening a Trace

```bash
# Open a trace file directly
cargo run --release -- path/to/trace.uscope

# Start with an empty window
cargo run --release
```

You can also:
- **Drag and drop** trace files onto the Reflex window
- Use **Cmd+O** to open a file dialog
- Use **Cmd+G** to generate a random test trace

## First Steps

1. Open a trace file — the pipeline view shows instruction execution timelines
2. **Scroll** to pan, **Ctrl+Scroll** to zoom
3. **Click** on the timeline to place the cursor and select an instruction
4. Press **Cmd+I** to see trace metadata (format, clock, duration)
5. Switch to the **Counters** tab to see performance counter data
6. Press **?** to see the full keyboard shortcut reference
