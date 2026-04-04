# Reflex

CPU pipeline trace visualizer built with GPUI (Zed's UI framework). Displays instruction execution timelines, queue occupancy, and pipeline stage progression.

## Build & Run

```bash
git clone --recurse-submodules <repo-url>  # or: git submodule update --init --recursive
cargo build          # debug build
cargo build --release
cargo run -- path/to/trace.uscope   # open a trace file
cargo fmt            # always run before committing
```

No test suite yet. Verify changes by building and visually testing with a trace file.

## Architecture

### Three-Layer Architecture

```
Reflex (GUI)  ←  uscope-cpu (protocol)  ←  uscope (transport)
```

- **uscope** (`uscope/crates/uscope/`): Format-only transport layer. Reader, writer, schema, checkpoints, delta replay, mipmaps. No protocol knowledge.
- **uscope-cpu** (`uscope/crates/uscope-cpu/`): CPU protocol library. `CpuTrace` provides instruction lifecycle, counters, buffers, lazy loading, performance analysis. Shared by Reflex, uscope-cli, and uscope-mcp.
- **uscope-cli** (`uscope/crates/uscope-cli/`): CLI for trace inspection (info, state, timeline, counters, buffers).
- **uscope-mcp** (`uscope/crates/uscope-mcp/`): MCP server for AI-assisted trace debugging with Claude.
- **Reflex**: GUI consumer — renders data from uscope-cpu. Currently has its own protocol logic in `uscope_source.rs` (migration to uscope-cpu planned).

### Core Data Flow

`TraceState` (Entity) is the shared state observed by all panels. It holds the `PipelineTrace` (parsed trace data), viewport state, cursor positions, and selection. Panels read from it during render; scroll/zoom handlers update it via `state.update(cx, ...)` and call `cx.notify()`.

### Module Layout

- `src/main.rs` - App bootstrap, window creation, theme setup
- `src/app.rs` - `AppView` root view: tab management, DockArea setup, action handlers, panel lifecycle
- `src/trace/model.rs` - `PipelineTrace`, `Instruction`, `StageSpan`, `QueueOccupancy` - all trace data structures
- `src/trace/konata.rs` - Konata format parser
- `src/trace/uscope_source.rs` - uscope binary format parser (to be migrated to `uscope-cpu`)
- `src/trace/generator.rs` - Synthetic trace generator for testing
- `src/views/pipeline_panel.rs` - Splits into label pane (left) + timeline pane (right) with resizable splitter
- `src/views/timeline_pane.rs` - Canvas-based pipeline stage rendering, custom scroll/zoom via `on_scroll_wheel`
- `src/views/label_pane.rs` - Row labels (addresses + disassembly), synced scroll with timeline
- `src/views/queue_panel.rs` - `QueuePanel` with `QueueKind` enum (Retire/Dispatch/Issue), one entity per queue type
- `src/views/counter_panel.rs` - Performance counter display (detail sparklines + heatmap overview modes)
- `src/views/minimap_view.rs` - Full-trace minimap with counter trendline, draggable range handles, pipeline indicator
- `src/config.rs` - TOML config file parsing for counter presets
- `src/session.rs` - Session persistence: auto-save/restore UI state (tabs, viewport, cursors, counter modes)
- `src/interaction/actions.rs` - All GPUI actions (keybindable commands)
- `src/interaction/keybindings.rs` - Key binding registration
- `src/theme/` - Dark color constants (`colors::BG_PRIMARY`, `colors::TEXT_PRIMARY`, etc.)

### DockArea Integration (gpui-component)

The layout uses `gpui_component::dock::DockArea`:
- **Center**: `DockItem::tabs([pipeline_panel, counter_panel])` - pipeline viewer + counter panel as tabs
- **Bottom/Left/Right dock**: `DockItem::h_split` or `v_split` of three `DockItem::tab` queue panels
- Layout presets switchable via `Alt+Cmd+1/2/3` (bottom/left/right)

**Key gotchas with DockArea + TabPanel:**
- TabPanel wraps content in `cached(absolute().size_full())` which skips re-rendering unless the panel entity is notified. Both `PipelinePanel` and `QueuePanel` use `cx.observe(&state, ...)` to self-notify when `TraceState` changes.
- TabPanel wraps content in `overflow_y_scroll()`. The timeline/label panes use custom `on_scroll_wheel` handlers with `cx.stop_propagation()` to prevent TabPanel from stealing scroll events.
- GPUI scroll containers require an element `.id()` to track scroll state. Without an ID, `overflow_y_scroll()` silently does nothing.
- Left/right docks render as empty `div()` when collapsed (no header). Set non-collapsible for those placements.
- `DockItem::tab()` creates single-panel TabPanels where `is_locked()` returns true (no `stack_panel`). Drag-and-drop only works within `h_split`/`v_split` groups that create a `StackPanel` parent.

### Panel Trait Requirements

To work inside DockArea, panels must implement:
- `Render` - standard GPUI render
- `Focusable` - needs `FocusHandle` field, created in constructor
- `EventEmitter<PanelEvent>` - empty impl is fine
- `gpui_component::dock::Panel` - `panel_name()`, `title()`, `closable()`, `inner_padding()`, `dump()`

### Theme

gpui-component's theme is set to `ThemeMode::Dark` in `main.rs`, then fine-tuned to match the app's custom dark palette. The `Theme` struct derefs to `ThemeColor` so you can set fields like `theme.background`, `theme.tab_bar`, etc. directly.

### Session Persistence

UI state is automatically saved/restored across app restarts:
- **Save**: on Cmd+Q, tab close, tab switch, and layout change — session state is written to a `.filename.session` JSON file alongside the trace (falls back to `~/.config/reflex/sessions/` if the directory is read-only)
- **Restore**: when opening a trace, checks for an existing session file and restores viewport position, cursor positions, counter display modes, full DockArea layout (panel arrangement, split ratios, buffer panel positions), and active tab
- **DockArea round-trip**: uses `DockArea::dump()`/`DockArea::load()` via `PanelRegistry` — all panels registered in `register_panels()` (`src/app.rs`). `BufferPanel::dump()` includes `buffer_idx` in `PanelInfo` for correct reconstruction.
- **Graceful degradation**: corrupt/missing session files are silently ignored — the app opens with defaults
- Session file format is versioned (currently v1) for forward compatibility

## Conventions

- Use `saturating_sub` for u32 cycle arithmetic to avoid panics on malformed trace data
- All cycle values are `u32`
- Stage names are interned via `StageNameIdx` (u32 index into `PipelineTrace::stage_names`)
- Trace files: `.uscope` (binary, via uscope crate) or Konata text format
