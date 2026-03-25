use gpui::{App, KeyBinding};

use super::actions::*;

pub fn register(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("cmd-=", ZoomIn, None),
        KeyBinding::new("cmd-+", ZoomIn, None),
        KeyBinding::new("cmd--", ZoomOut, None),
        KeyBinding::new("cmd-0", ZoomToFit, None),
        KeyBinding::new("left", PanLeft, None),
        KeyBinding::new("right", PanRight, None),
        KeyBinding::new("up", PanUp, None),
        KeyBinding::new("down", PanDown, None),
        KeyBinding::new("j", SelectNext, None),
        KeyBinding::new("k", SelectPrevious, None),
        KeyBinding::new("cmd-f", ToggleSearch, None),
        KeyBinding::new("cmd-g", GenerateTrace, None),
        KeyBinding::new("cmd-o", OpenFile, None),
        KeyBinding::new("cmd-r", ReloadTrace, None),
        KeyBinding::new("cmd-w", CloseTab, None),
        KeyBinding::new("ctrl-tab", NextTab, None),
        KeyBinding::new("ctrl-shift-tab", PrevTab, None),
        KeyBinding::new("cmd-m", AddCursor, None),
        KeyBinding::new("cmd-shift-m", RemoveCursor, None),
        KeyBinding::new("[", PrevCursor, None),
        KeyBinding::new("]", NextCursor, None),
        KeyBinding::new("cmd-l", GotoCycle, None),
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("cmd-i", ToggleInfo, None),
        KeyBinding::new("cmd-b", ToggleQueues, None),
        KeyBinding::new("alt-cmd-1", LayoutBottom, None),
        KeyBinding::new("alt-cmd-2", LayoutLeft, None),
        KeyBinding::new("alt-cmd-3", LayoutRight, None),
        KeyBinding::new("cmd-z", CursorUndo, None),
        KeyBinding::new("cmd-shift-z", CursorRedo, None),
        KeyBinding::new("cmd-y", CursorRedo, None),
        KeyBinding::new("cmd-shift-o", ToggleOverlay, None),
        KeyBinding::new("alt-cmd-w", WcpConnect, None),
        KeyBinding::new("alt-cmd-shift-w", WcpDisconnect, None),
    ]);
}
