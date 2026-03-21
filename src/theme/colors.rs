use gpui::Hsla;

/// Dark background color for the main canvas.
pub const BG_PRIMARY: Hsla = Hsla {
    h: 220.0 / 360.0,
    s: 0.13,
    l: 0.10,
    a: 1.0,
};

/// Slightly lighter background for label pane.
pub const BG_SECONDARY: Hsla = Hsla {
    h: 220.0 / 360.0,
    s: 0.13,
    l: 0.12,
    a: 1.0,
};

/// Grid line color.
pub const GRID_LINE: Hsla = Hsla {
    h: 220.0 / 360.0,
    s: 0.10,
    l: 0.20,
    a: 1.0,
};

/// Grid line color for major lines.
pub const GRID_LINE_MAJOR: Hsla = Hsla {
    h: 220.0 / 360.0,
    s: 0.10,
    l: 0.28,
    a: 1.0,
};

/// Text color.
pub const TEXT_PRIMARY: Hsla = Hsla {
    h: 220.0 / 360.0,
    s: 0.05,
    l: 0.85,
    a: 1.0,
};

/// Dimmed text (flushed instructions).
pub const TEXT_DIMMED: Hsla = Hsla {
    h: 220.0 / 360.0,
    s: 0.05,
    l: 0.45,
    a: 1.0,
};

/// Row number text.
pub const TEXT_ROW_NUMBER: Hsla = Hsla {
    h: 220.0 / 360.0,
    s: 0.05,
    l: 0.40,
    a: 1.0,
};

/// Selected row highlight.
pub const SELECTION_BG: Hsla = Hsla {
    h: 210.0 / 360.0,
    s: 0.50,
    l: 0.25,
    a: 0.5,
};

/// Hover row highlight.
#[allow(dead_code)]
pub const HOVER_BG: Hsla = Hsla {
    h: 210.0 / 360.0,
    s: 0.30,
    l: 0.18,
    a: 0.4,
};

/// Dependency arrow color.
#[allow(dead_code)]
pub const ARROW_COLOR: Hsla = Hsla {
    h: 0.0,
    s: 0.0,
    l: 0.60,
    a: 0.6,
};

/// Status bar background.
pub const STATUS_BAR_BG: Hsla = Hsla {
    h: 220.0 / 360.0,
    s: 0.13,
    l: 0.14,
    a: 1.0,
};

/// Perfetto/Zed-inspired stage color palette.
/// Indexed by stage_name_idx % STAGE_PALETTE.len().
pub const STAGE_PALETTE: &[Hsla] = &[
    // Blue - Fetch
    Hsla {
        h: 210.0 / 360.0,
        s: 0.65,
        l: 0.45,
        a: 1.0,
    },
    // Teal - Decode
    Hsla {
        h: 175.0 / 360.0,
        s: 0.55,
        l: 0.40,
        a: 1.0,
    },
    // Green - Rename
    Hsla {
        h: 140.0 / 360.0,
        s: 0.50,
        l: 0.40,
        a: 1.0,
    },
    // Lime - Dispatch
    Hsla {
        h: 80.0 / 360.0,
        s: 0.50,
        l: 0.40,
        a: 1.0,
    },
    // Yellow - Issue
    Hsla {
        h: 45.0 / 360.0,
        s: 0.65,
        l: 0.45,
        a: 1.0,
    },
    // Orange - Execute
    Hsla {
        h: 25.0 / 360.0,
        s: 0.70,
        l: 0.45,
        a: 1.0,
    },
    // Red - Complete
    Hsla {
        h: 0.0 / 360.0,
        s: 0.60,
        l: 0.45,
        a: 1.0,
    },
    // Purple - Retire
    Hsla {
        h: 280.0 / 360.0,
        s: 0.50,
        l: 0.45,
        a: 1.0,
    },
    // Pink
    Hsla {
        h: 330.0 / 360.0,
        s: 0.55,
        l: 0.45,
        a: 1.0,
    },
    // Indigo
    Hsla {
        h: 240.0 / 360.0,
        s: 0.45,
        l: 0.50,
        a: 1.0,
    },
];

/// Get stage color by name index.
pub fn stage_color(stage_name_idx: u16) -> Hsla {
    STAGE_PALETTE[stage_name_idx as usize % STAGE_PALETTE.len()]
}

/// Cursor color palette — warm/bright colors that contrast with blue-teal stages.
pub const CURSOR_PALETTE: &[Hsla] = &[
    // Amber/Gold
    Hsla {
        h: 42.0 / 360.0,
        s: 0.90,
        l: 0.55,
        a: 1.0,
    },
    // Coral/Salmon
    Hsla {
        h: 12.0 / 360.0,
        s: 0.85,
        l: 0.60,
        a: 1.0,
    },
    // Mint/Cyan
    Hsla {
        h: 165.0 / 360.0,
        s: 0.70,
        l: 0.55,
        a: 1.0,
    },
    // Lavender
    Hsla {
        h: 265.0 / 360.0,
        s: 0.65,
        l: 0.65,
        a: 1.0,
    },
    // Lime
    Hsla {
        h: 90.0 / 360.0,
        s: 0.70,
        l: 0.55,
        a: 1.0,
    },
    // Rose
    Hsla {
        h: 340.0 / 360.0,
        s: 0.80,
        l: 0.60,
        a: 1.0,
    },
];

/// Get cursor color at full opacity (active cursor).
pub fn cursor_color(color_idx: usize) -> Hsla {
    CURSOR_PALETTE[color_idx % CURSOR_PALETTE.len()]
}

/// Get cursor color at reduced opacity (inactive cursor).
pub fn cursor_color_inactive(color_idx: usize) -> Hsla {
    let mut c = cursor_color(color_idx);
    c.a = 0.6;
    c
}

/// Darken a stage color for flushed instructions.
pub fn stage_color_flushed(stage_name_idx: u16) -> Hsla {
    let mut c = stage_color(stage_name_idx);
    c.l *= 0.5;
    c.a = 0.5;
    c
}
