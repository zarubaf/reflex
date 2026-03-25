# Pipeline Viewer

The pipeline viewer is the main view in Reflex, showing instruction execution timelines as colored stage spans.

## Layout

The pipeline viewer has three areas:

- **Label pane** (left) — Instruction addresses and disassembly, with a resizable splitter
- **Timeline pane** (right) — Canvas-rendered pipeline stages with cycle ruler header
- **Queue panels** (bottom/left/right dock) — Retire, dispatch, and issue queue state

## Stage Rendering

Each instruction occupies one row. Pipeline stages are rendered as colored rectangles:

- Each stage has a unique color based on its position in the pipeline
- Stage names (e.g., `Al`, `Ds`, `Is`, `RdEx`, `Cp`) are shown inside the rectangles when zoomed in
- At low zoom levels, stages are rendered as thin colored bars for performance

## Zoom and Pan

| Action | Effect |
|--------|--------|
| Scroll / Trackpad | Pan horizontally and vertically |
| Ctrl + Scroll | Zoom in/out (both axes, preserving aspect ratio) |
| Cmd + = / Cmd + - | Zoom in / out |
| Cmd + 0 | Zoom to fit (show entire trace) |
| Arrow keys | Pan in the corresponding direction |

Zoom uses a focal-point model: the point under the cursor stays fixed while the view scales around it. Both horizontal (cycles) and vertical (rows) axes zoom together.

## Row Selection

Click on an instruction row to select it. The selected row is highlighted, and its details appear in the queue panels and status bar.

- **j / k** — Select next / previous instruction
- The status bar shows: instruction count, cycle range, zoom level, selected instruction address, and cursor position

## Tooltips

Hover over an instruction to see its annotations as a tooltip. Annotations come from the trace source and typically include operand details, execution metadata, and pipeline events.

## Click vs Drag

- **Click** (press + release without movement) — Selects the instruction row and places the cursor at the clicked cycle
- **Drag** (press + move) — Pans the viewport without moving the cursor

This distinction prevents accidental cursor jumps during navigation.
