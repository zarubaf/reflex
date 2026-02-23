# Reflex

A fast, GPU-accelerated CPU pipeline trace visualizer. Built with [GPUI](https://gpui.rs) (Zed's rendering framework).

![Reflex screenshot](resources/screenshot.png)

## Features

- **Konata/Kanata format** — Drop `.log`, `.kanata`, or `.konata` trace files to visualize
- **GPU-rendered pipeline view** — Smooth panning, zooming, and scrolling at 60fps for traces with thousands of instructions
- **Tabbed interface** — Open multiple traces side-by-side, each with independent viewport state
- **Stage annotations** — Hover over instructions to see Konata lane 1+ annotations as tooltips
- **Keyboard-driven** — Vim-style navigation, search, and zoom

## Getting Started

```
cargo run                           # Start with empty window
cargo run -- path/to/trace.log      # Open a trace file directly
```

Or drag and drop trace files onto the window.

## Keyboard Shortcuts

| Key               | Action                             |
| ----------------- | ---------------------------------- |
| Scroll / Trackpad | Pan                                |
| Ctrl + Scroll     | Zoom in / out                      |
| Cmd + = / Cmd + - | Zoom in / out                      |
| Cmd + 0           | Zoom to fit                        |
| j / k             | Select next / previous instruction |
| Cmd + F           | Search instructions                |
| Cmd + O           | Open trace file                    |
| Cmd + R           | Reload current trace               |
| Cmd + G           | Generate random trace              |
| Cmd + W           | Close tab                          |
| Ctrl + Tab        | Next tab                           |
| ?                 | Toggle help overlay                |

## Trace Format

Reflex supports the [Kanata log format](https://github.com/shioyadan/Konata/blob/master/docs/kanata-log-format.md) used by CPU simulators to record pipeline behavior. This is the same format used by [Konata](https://github.com/shioyadan/Konata).

## Building

Requires Rust and macOS (GPUI currently targets macOS).

```
cargo build --release
```
