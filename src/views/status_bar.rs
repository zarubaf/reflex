use gpui::prelude::FluentBuilder as _;
use gpui::*;

use crate::app::TraceState;
use crate::theme::colors;

/// Format a number with apostrophe thousand separators (e.g., 1'000'000).
pub fn fmt_num(n: impl std::fmt::Display) -> String {
    let s = n.to_string();
    let mut result = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push('\'');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

/// Bottom status bar showing cycle, row, zoom, FPS info.
pub struct StatusBar {
    state: Entity<TraceState>,
    pub wcp_connected: bool,
}

impl StatusBar {
    pub fn new(state: Entity<TraceState>) -> Self {
        Self {
            state,
            wcp_connected: false,
        }
    }
}

impl Render for StatusBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ts = self.state.read(cx);
        let vp = &ts.viewport;

        let total_instrs = if ts.trace.total_instruction_count > 0 {
            ts.trace.total_instruction_count
        } else {
            ts.trace.row_count()
        };
        let total = format!(
            "{} instrs, {} max cycle",
            fmt_num(total_instrs),
            fmt_num(ts.trace.max_cycle())
        );

        let vis_end = (vp.scroll_cycle + vp.view_width as f64 / vp.pixels_per_cycle as f64) as u64;
        let cycle_info = format!(
            "Cycles: {}-{}",
            fmt_num(vp.scroll_cycle as u64),
            fmt_num(vis_end)
        );

        let (row_start, row_end) = vp.visible_row_range();
        // Show global instruction indices via density mipmap if available.
        let row_info = if let Some(summary) = ts.trace_summary() {
            let vis_start_cycle = vp.scroll_cycle as u32;
            let vis_end_cycle =
                (vp.scroll_cycle + vp.view_width as f64 / vp.pixels_per_cycle as f64) as u32;
            format!(
                "Instrs: {}-{}",
                fmt_num(summary.cycle_to_row(vis_start_cycle)),
                fmt_num(summary.cycle_to_row(vis_end_cycle))
            )
        } else {
            format!("Rows: {}-{}", fmt_num(row_start), fmt_num(row_end))
        };
        let zoom_info = format!(
            "Zoom: {:.1}px/cyc, {:.1}px/row",
            vp.pixels_per_cycle, vp.row_height
        );

        let fps_info = format!("{:.0} fps", ts.fps);

        let sel_info = if let Some(row) = ts.selected_row {
            if row < ts.trace.row_count() {
                format!("Sel: {} - {}", row, &ts.trace.instructions[row].disasm)
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        let cursor_info = if !ts.cursor_state.cursors.is_empty() {
            let active = &ts.cursor_state.cursors[ts.cursor_state.active_idx];
            if ts.cursor_state.cursors.len() > 1 {
                format!(
                    "Cursor: {} [{}/{}]",
                    fmt_num(active.cycle as u64),
                    ts.cursor_state.active_idx + 1,
                    ts.cursor_state.cursors.len()
                )
            } else {
                format!("Cursor: {}", fmt_num(active.cycle as u64))
            }
        } else {
            String::new()
        };

        div()
            .id("status-bar")
            .h(px(24.0))
            .w_full()
            .bg(colors::STATUS_BAR_BG)
            .flex()
            .items_center()
            .justify_between()
            .px_2()
            .text_size(px(11.0))
            .text_color(colors::TEXT_DIMMED)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_4()
                    .child(total)
                    .child(cycle_info)
                    .child(row_info)
                    .child(zoom_info)
                    .child(sel_info)
                    .child(cursor_info),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_4()
                    .when(self.wcp_connected, |el| {
                        el.child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .child(div().w(px(6.0)).h(px(6.0)).rounded(px(3.0)).bg(
                                    gpui::Hsla {
                                        h: 120.0 / 360.0,
                                        s: 0.7,
                                        l: 0.45,
                                        a: 1.0,
                                    },
                                ))
                                .child("WCP"),
                        )
                    })
                    .child(fps_info),
            )
    }
}
