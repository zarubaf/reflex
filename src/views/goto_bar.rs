use gpui::*;

use crate::app::TraceState;
use crate::theme::colors;

pub struct GotoBar {
    state: Entity<TraceState>,
    input: String,
    visible: bool,
    focus_handle: FocusHandle,
    parent_focus: FocusHandle,
    error: Option<String>,
}

impl GotoBar {
    pub fn new(
        state: Entity<TraceState>,
        parent_focus: FocusHandle,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            state,
            input: String::new(),
            visible: false,
            focus_handle: cx.focus_handle(),
            parent_focus,
            error: None,
        }
    }

    pub fn toggle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.visible = !self.visible;
        if self.visible {
            self.input.clear();
            self.error = None;
            window.focus(&self.focus_handle);
        } else {
            window.focus(&self.parent_focus);
        }
        cx.notify();
    }

    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.visible = false;
        window.focus(&self.parent_focus);
        cx.notify();
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let trimmed = self.input.trim();
        if trimmed.is_empty() {
            return;
        }

        // Parse as cycle number (decimal or hex with 0x prefix)
        let cycle = if let Some(hex) = trimmed.strip_prefix("0x") {
            u32::from_str_radix(hex, 16).ok()
        } else {
            trimmed.parse::<u32>().ok()
        };

        match cycle {
            Some(c) => {
                self.state.update(cx, |ts, cx| {
                    // Center horizontally on the target cycle
                    let visible_cycles =
                        ts.viewport.view_width as f64 / ts.viewport.pixels_per_cycle as f64;
                    ts.viewport.scroll_cycle = (c as f64 - visible_cycles / 2.0).max(0.0);

                    // Find the first instruction active at this cycle, or
                    // the nearest one if none spans it exactly
                    let target_row = ts
                        .trace
                        .instructions
                        .iter()
                        .position(|instr| instr.first_cycle <= c && c <= instr.last_cycle)
                        .or_else(|| {
                            // Find nearest instruction by first_cycle
                            ts.trace
                                .instructions
                                .iter()
                                .enumerate()
                                .min_by_key(|(_, instr)| {
                                    (instr.first_cycle as i64 - c as i64).unsigned_abs()
                                })
                                .map(|(i, _)| i)
                        });

                    if let Some(row) = target_row {
                        let visible_rows =
                            ts.viewport.view_height as f64 / ts.viewport.row_height as f64;
                        ts.viewport.scroll_row = (row as f64 - visible_rows / 2.0).max(0.0);
                        ts.selected_row = Some(row);
                    }

                    ts.viewport.clamp();
                    cx.notify();
                });
                self.close(window, cx);
            }
            None => {
                self.error = Some("Invalid cycle number".into());
                cx.notify();
            }
        }
    }
}

impl Render for GotoBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.visible {
            return div().id("goto-bar-hidden");
        }

        let hint = if let Some(ref err) = self.error {
            err.clone()
        } else {
            "Enter cycle (decimal or 0x hex)".to_string()
        };

        let hint_color = colors::TEXT_DIMMED;

        div()
            .id("goto-bar")
            .track_focus(&self.focus_handle)
            .key_context("GotoBar")
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                let keystroke = &event.keystroke;
                if keystroke.key == "escape" {
                    this.close(window, cx);
                } else if keystroke.key == "enter" {
                    this.submit(window, cx);
                } else if keystroke.key == "backspace" {
                    this.input.pop();
                    this.error = None;
                    cx.notify();
                } else if !keystroke.modifiers.platform
                    && !keystroke.modifiers.control
                    && keystroke.key_char.is_some()
                {
                    if let Some(ref ch) = keystroke.key_char {
                        this.input.push_str(ch);
                        this.error = None;
                        cx.notify();
                    }
                }
            }))
            .absolute()
            .top(px(4.0))
            .right(px(4.0))
            .w(px(300.0))
            .h(px(28.0))
            .bg(colors::BG_PRIMARY)
            .border_1()
            .border_color(colors::GRID_LINE_MAJOR)
            .rounded(px(4.0))
            .flex()
            .items_center()
            .px_2()
            .gap_2()
            .text_size(px(12.0))
            .text_color(colors::TEXT_PRIMARY)
            .child(
                div()
                    .flex_shrink_0()
                    .text_color(colors::TEXT_DIMMED)
                    .text_size(px(10.0))
                    .child("Go to:"),
            )
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .child(if self.input.is_empty() {
                        div().text_color(colors::TEXT_DIMMED).child("cycle...")
                    } else {
                        div().child(self.input.clone())
                    }),
            )
            .child(
                div()
                    .text_color(hint_color)
                    .text_size(px(10.0))
                    .flex_shrink_0()
                    .child(hint),
            )
    }
}
