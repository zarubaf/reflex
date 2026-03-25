# Configuration

## Counter Presets

Counter display preferences can be saved in a TOML configuration file. Reflex searches for `counters.toml` in the following order:

1. Next to the trace file (e.g., `trace.counters.toml`)
2. In the same directory as the trace file (`counters.toml`)
3. User config directory:
   - macOS: `~/Library/Application Support/reflex/counters.toml`
   - Linux: `~/.config/reflex/counters.toml`

If no config file is found, Reflex uses defaults (show all counters, no overlay).

### File Format

```toml
[presets.performance]
name = "Overall Performance"
counters = ["committed_insns", "cycles"]
overlay = ["committed_insns"]

[presets.performance.display_modes]
committed_insns = "rate"
cycles = "total"

[presets.cache]
name = "Cache Analysis"
counters = ["dcache_misses", "icache_misses"]
overlay = []

[presets.cache.display_modes]
dcache_misses = "delta"
icache_misses = "delta"

[presets.branch]
name = "Branch Prediction"
counters = ["mispredicts", "committed_insns"]
overlay = ["mispredicts"]

[presets.branch.display_modes]
mispredicts = "rate"
```

### Preset Fields

| Field | Type | Description |
|-------|------|-------------|
| `name` | String | Human-readable preset name |
| `counters` | Array | Counter names to show (empty = show all) |
| `display_modes` | Table | Per-counter display mode: `"total"`, `"rate"`, or `"delta"` |
| `overlay` | Array | Counter names to show as timeline overlay (empty = no overlay) |

### Error Handling

If the config file contains syntax errors, Reflex logs a warning to stderr and falls back to defaults. The application never crashes due to a malformed config file.

## Layout Presets

The queue panel layout is switchable via keyboard shortcuts:

| Key | Layout |
|-----|--------|
| Alt+Cmd+1 | Bottom dock |
| Alt+Cmd+2 | Left dock |
| Alt+Cmd+3 | Right dock |

Layout state is not persisted between sessions.
