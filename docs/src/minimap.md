# Minimap

The minimap is a persistent strip above the main view showing the full trace duration with a counter trendline and navigation controls.

## Layout

The minimap shows:
- **Counter trendline** — filled bars showing one counter's per-cycle deltas across the entire trace
- **Counter range handles** — draggable pill-shaped handles controlling which range the counter panel displays
- **Dimmed regions** — areas outside the counter range are darkened
- **Pipeline indicator** — subtle yellow bar at the bottom showing where the pipeline viewport is positioned
- **Cursor marker** — vertical line at the active cursor position

## Counter Range vs Pipeline Viewport

The minimap manages two independent concepts:

| Element | Controls | Visual |
|---------|----------|--------|
| **Handles** (blue pills) | Counter panel range | Blue border rectangle with draggable edges |
| **Yellow bar** (bottom) | Pipeline viewport position | Read-only indicator |

The counter range defaults to the full trace. Use the handles to narrow it. The pipeline viewport is controlled by scrolling/zooming in the pipeline view.

## Interactions

### Dragging

- **Drag handle body** — Pan the counter range (preserves width)
- **Drag left/right handle** — Resize the counter range
- **Scroll wheel** — Zoom the counter range in/out (centered on mouse position)

### Clicking

- **Click anywhere** (not on a handle) — Centers the pipeline viewport on the clicked cycle and scrolls vertically to show instructions active at that cycle
- Clicking does NOT move the cursor — cursors are only moved by clicking in the pipeline view

### Click vs Drag Detection

The minimap distinguishes between clicks (< 4px movement) and drags (>= 4px movement). Dragging the handles does not trigger a pipeline jump.

## Trendline

The minimap displays one counter as a trendline. By default, it shows the first counter in the trace (typically `committed_insns`). The trendline uses min-max envelope downsampling:
- Data is cached and only recomputed when the counter, trace, or canvas width changes
- Inside the counter range: brighter fill
- Outside the counter range: dimmer fill
