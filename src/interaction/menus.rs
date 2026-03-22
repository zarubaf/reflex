use gpui::{App, Menu, MenuItem};

use super::actions::*;

pub fn setup(cx: &App) {
    cx.set_menus(vec![
        Menu {
            name: "Reflex".into(),
            items: vec![MenuItem::action("Quit Reflex", Quit)],
        },
        Menu {
            name: "File".into(),
            items: vec![
                MenuItem::action("Open...", OpenFile),
                MenuItem::action("Reload Trace", ReloadTrace),
                MenuItem::action("Generate Trace", GenerateTrace),
                MenuItem::separator(),
                MenuItem::action("Close Tab", CloseTab),
            ],
        },
        Menu {
            name: "View".into(),
            items: vec![
                MenuItem::action("Zoom In", ZoomIn),
                MenuItem::action("Zoom Out", ZoomOut),
                MenuItem::action("Zoom to Fit", ZoomToFit),
                MenuItem::separator(),
                MenuItem::action("Find...", ToggleSearch),
                MenuItem::action("Go to Cycle...", GotoCycle),
                MenuItem::separator(),
                MenuItem::action("Trace Info", ToggleInfo),
                MenuItem::action("Keyboard Shortcuts", ToggleHelp),
            ],
        },
        Menu {
            name: "Navigate".into(),
            items: vec![
                MenuItem::action("Select Next", SelectNext),
                MenuItem::action("Select Previous", SelectPrevious),
                MenuItem::separator(),
                MenuItem::action("Add Cursor", AddCursor),
                MenuItem::action("Remove Cursor", RemoveCursor),
                MenuItem::action("Next Cursor", NextCursor),
                MenuItem::action("Previous Cursor", PrevCursor),
            ],
        },
        Menu {
            name: "Window".into(),
            items: vec![
                MenuItem::action("Next Tab", NextTab),
                MenuItem::action("Previous Tab", PrevTab),
                MenuItem::separator(),
                MenuItem::action("Toggle Queues", ToggleQueues),
                MenuItem::action("Queues Bottom", LayoutBottom),
                MenuItem::action("Queues Left", LayoutLeft),
                MenuItem::action("Queues Right", LayoutRight),
            ],
        },
        Menu {
            name: "Tools".into(),
            items: vec![
                MenuItem::action("Connect to Surfer (WCP)", WcpConnect),
                MenuItem::action("Disconnect from Surfer", WcpDisconnect),
            ],
        },
    ]);
}
