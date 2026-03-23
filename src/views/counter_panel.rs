use gpui::*;

use crate::app::TraceState;
use crate::theme::colors;
use crate::trace::model::CounterDisplayMode;

/// Default rate computation window in cycles.
const RATE_WINDOW: u32 = 64;

/// A panel displaying performance counter values at the current cursor cycle.
pub struct CounterPanel {
    state: Entity<TraceState>,
    focus_handle: FocusHandle,
    /// Per-counter display mode overrides (None = use default from schema).
    display_modes: Vec<Option<CounterDisplayMode>>,
}

impl CounterPanel {
    pub fn new(state: Entity<TraceState>, cx: &mut Context<Self>) -> Self {
        let ts = state.read(cx);
        let num_counters = ts.trace.counters.len();

        cx.observe(&state, |_this, _state, cx| {
            cx.notify();
        })
        .detach();

        Self {
            state,
            focus_handle: cx.focus_handle(),
            display_modes: vec![None; num_counters],
        }
    }

    fn effective_mode(&self, idx: usize, default: CounterDisplayMode) -> CounterDisplayMode {
        self.display_modes
            .get(idx)
            .copied()
            .flatten()
            .unwrap_or(default)
    }

    fn cycle_mode(&mut self, idx: usize, current: CounterDisplayMode) {
        let next = match current {
            CounterDisplayMode::Total => CounterDisplayMode::Rate,
            CounterDisplayMode::Rate => CounterDisplayMode::Delta,
            CounterDisplayMode::Delta => CounterDisplayMode::Total,
        };
        if idx < self.display_modes.len() {
            self.display_modes[idx] = Some(next);
        }
    }

    fn format_value(value: u64) -> String {
        if value >= 1_000_000 {
            format!("{:.2}M", value as f64 / 1_000_000.0)
        } else if value >= 1_000 {
            format!("{:.1}k", value as f64 / 1_000.0)
        } else {
            format!("{}", value)
        }
    }

    fn format_rate(rate: f64) -> String {
        if rate >= 100.0 {
            format!("{:.0}", rate)
        } else if rate >= 10.0 {
            format!("{:.1}", rate)
        } else {
            format!("{:.3}", rate)
        }
    }

    fn mode_label(mode: CounterDisplayMode) -> &'static str {
        match mode {
            CounterDisplayMode::Total => "T",
            CounterDisplayMode::Rate => "R",
            CounterDisplayMode::Delta => "Δ",
        }
    }

    fn mode_color(mode: CounterDisplayMode) -> Hsla {
        match mode {
            CounterDisplayMode::Total => colors::TEXT_DIMMED,
            CounterDisplayMode::Rate => Hsla {
                h: 200.0 / 360.0,
                s: 0.6,
                l: 0.55,
                a: 1.0,
            },
            CounterDisplayMode::Delta => Hsla {
                h: 40.0 / 360.0,
                s: 0.7,
                l: 0.55,
                a: 1.0,
            },
        }
    }
}

impl Render for CounterPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ts = self.state.read(cx);
        let cursor_cycle = ts.cursor_state.cursors[ts.cursor_state.active_idx]
            .cycle
            .round() as u32;

        let counters = &ts.trace.counters;

        if counters.is_empty() {
            return div()
                .id("counter-panel")
                .size_full()
                .bg(colors::BG_PRIMARY)
                .text_size(px(11.0))
                .font_family("Menlo")
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .text_color(colors::TEXT_DIMMED)
                        .child("No performance counters in this trace"),
                );
        }

        // Build counter rows
        let mut rows: Vec<Stateful<Div>> = Vec::with_capacity(counters.len());
        for (idx, counter) in counters.iter().enumerate() {
            let mode = self.effective_mode(idx, counter.default_mode);

            let (value_str, value_label) = match mode {
                CounterDisplayMode::Total => {
                    let val = ts.trace.counter_value_at(idx, cursor_cycle);
                    (Self::format_value(val), "")
                }
                CounterDisplayMode::Rate => {
                    let rate = ts.trace.counter_rate_at(idx, cursor_cycle, RATE_WINDOW);
                    (Self::format_rate(rate), "/cy")
                }
                CounterDisplayMode::Delta => {
                    let delta = ts.trace.counter_delta_at(idx, cursor_cycle);
                    (Self::format_value(delta), "")
                }
            };

            let mode_label = Self::mode_label(mode);
            let mode_color = Self::mode_color(mode);

            rows.push(
                div()
                    .id(("counter", idx))
                    .flex()
                    .gap_2()
                    .px_2()
                    .py(px(2.0))
                    .cursor_pointer()
                    .hover(|s| s.bg(colors::GRID_LINE))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let default = this.state.read(cx).trace.counters[idx].default_mode;
                        let current = this.effective_mode(idx, default);
                        this.cycle_mode(idx, current);
                        cx.notify();
                    }))
                    .child(
                        div()
                            .w(px(20.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_color(mode_color)
                            .child(mode_label),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_color(colors::TEXT_PRIMARY)
                            .overflow_x_hidden()
                            .child(counter.name.clone()),
                    )
                    .child(
                        div()
                            .min_w(px(70.0))
                            .text_color(colors::TEXT_PRIMARY)
                            .flex()
                            .justify_end()
                            .child(format!("{}{}", value_str, value_label)),
                    ),
            );
        }

        div()
            .id("counter-panel")
            .size_full()
            .bg(colors::BG_PRIMARY)
            .text_size(px(11.0))
            .font_family("Menlo")
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(
                div()
                    .px_2()
                    .py_1()
                    .text_color(colors::TEXT_DIMMED)
                    .border_b_1()
                    .border_color(colors::GRID_LINE)
                    .child(format!(
                        "Performance Counters ({}) @ cycle {}",
                        counters.len(),
                        cursor_cycle
                    )),
            )
            .child(
                div()
                    .id("counter-scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .children(rows),
            )
    }
}

impl Focusable for CounterPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<gpui_component::dock::PanelEvent> for CounterPanel {}

impl gpui_component::dock::Panel for CounterPanel {
    fn panel_name(&self) -> &'static str {
        "CounterPanel"
    }

    fn title(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        "Counters"
    }

    fn closable(&self, _cx: &App) -> bool {
        false
    }

    fn inner_padding(&self, _cx: &App) -> bool {
        false
    }

    fn dump(&self, _cx: &App) -> gpui_component::dock::PanelState {
        gpui_component::dock::PanelState::new(self)
    }
}
