# Navigation & Cursors

## Cursors

Reflex supports multiple cursors for measuring cycle distances between pipeline events.

### Placing a Cursor

Click on the timeline to place the active cursor at that cycle. The cursor appears as a vertical line with a labeled head showing the cycle number.

### Multicursor

| Key | Action |
|-----|--------|
| Cmd+M | Add a new cursor at the current position |
| Cmd+Shift+M | Remove the active cursor |
| \[ | Switch to previous cursor |
| \] | Switch to next cursor |

Each cursor has a distinct color from a built-in palette. When multiple cursors are placed, the header shows delta values between consecutive cursors.

### Cursor Undo/Redo

All cursor operations (moving, adding, removing) are recorded in a history stack:

| Key | Action |
|-----|--------|
| Cmd+Z | Undo last cursor change |
| Cmd+Shift+Z | Redo |
| Cmd+Y | Redo (alternative) |

The history stores complete cursor state snapshots, so undoing an "add cursor" operation removes the cursor entirely, and undoing a "remove" restores it.

History is capped at 100 entries. New cursor movements clear the redo stack.

## Search

Press **Cmd+F** to open the search bar. Search matches against instruction disassembly text (addresses and mnemonics).

## Go to Cycle

Press **Cmd+L** to open the "Go to Cycle" bar. Enter a cycle number to jump the viewport to that position.

## Tab Navigation

| Key | Action |
|-----|--------|
| Ctrl+Tab | Next tab |
| Ctrl+Shift+Tab | Previous tab |
| Cmd+W | Close current tab |

Each tab has its own independent trace, viewport state, and cursor positions.
