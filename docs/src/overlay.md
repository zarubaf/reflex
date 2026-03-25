# Timeline Overlay

The timeline overlay shows a selected counter's sparkline directly above the pipeline stages, synchronized with the pipeline view's scroll and zoom.

## Toggle

Press **Cmd+Shift+O** to toggle the overlay on/off.

When enabled, a 30px strip appears between the pipeline header and the stage content. The pipeline stages shift down to accommodate it.

## Display

The overlay shows:
- Min-max envelope bars of the selected counter's per-cycle deltas
- Data synchronized with the pipeline viewport (same visible cycle range)
- Subtle background distinguishing it from the pipeline header
- Rates recompute automatically at the current zoom level

## Use Case

The overlay lets you correlate counter behavior with pipeline events without switching to the Counters tab. For example:
- Watch IPC drop while inspecting a cache miss stall in the pipeline
- See branch misprediction spikes aligned with flush events
- Monitor dispatch queue pressure alongside instruction flow

## Counter Selection

The overlay shows the first counter by default. The selected counter can be configured via the `overlay_counter` state (currently toggled via the action, which cycles the first counter on/off).
