use gpui::*;

use crate::app::TraceState;
use crate::theme::colors;

/// Bottom status bar showing cycle, row, zoom, FPS info.
pub struct StatusBar {
    state: Entity<TraceState>,
}

impl StatusBar {
    pub fn new(state: Entity<TraceState>) -> Self {
        Self { state }
    }
}

impl Render for StatusBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ts = self.state.read(cx);
        let vp = &ts.viewport;

        let total = format!(
            "{} instrs, {} max cycle",
            ts.trace.row_count(),
            ts.trace.max_cycle()
        );

        let cycle_info = format!(
            "Cycles: {:.0}-{:.0}",
            vp.scroll_cycle,
            vp.scroll_cycle + vp.view_width as f64 / vp.pixels_per_cycle as f64
        );

        let (row_start, row_end) = vp.visible_row_range();
        let row_info = format!("Rows: {}-{}", row_start, row_end);
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
                    "Cursor: {:.0} [{}/{}]",
                    active.cycle,
                    ts.cursor_state.active_idx + 1,
                    ts.cursor_state.cursors.len()
                )
            } else {
                format!("Cursor: {:.0}", active.cycle)
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
            .child(div().flex().items_center().gap_2().child(fps_info))
    }
}
