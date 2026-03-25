use gpui::*;

use crate::app::TraceState;
use crate::theme::colors;
use crate::trace::model::CounterDisplayMode;

/// Default rate computation window in cycles.
const RATE_WINDOW: u32 = 64;
/// Sparkline strip height in pixels.
const SPARKLINE_HEIGHT: f32 = 40.0;
/// Heatmap row height in pixels.
const HEATMAP_ROW_HEIGHT: f32 = 6.0;

/// Accent color for sparkline fill — brighter and more opaque for visibility.
const SPARKLINE_COLOR: Hsla = Hsla {
    h: 210.0 / 360.0,
    s: 0.55,
    l: 0.50,
    a: 0.55,
};

fn px_val(p: Pixels) -> f32 {
    f32::from(p)
}

/// Counter panel view mode.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ViewMode {
    /// Per-counter sparklines with numeric values.
    Detail,
    /// Compact heatmap showing all counters at once.
    Heatmap,
}

/// A panel displaying performance counter values at the current cursor cycle.
pub struct CounterPanel {
    state: Entity<TraceState>,
    focus_handle: FocusHandle,
    /// Per-counter display mode overrides (None = use default from schema).
    display_modes: Vec<Option<CounterDisplayMode>>,
    /// Current view mode (detail sparklines vs heatmap overview).
    view_mode: ViewMode,
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
            view_mode: ViewMode::Detail,
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

        // Use counter range (independent from pipeline viewport) for sparklines.
        let (vis_start, vis_end) = ts.effective_counter_range();

        // Build counter rows with sparklines
        let mut rows: Vec<AnyElement> = Vec::with_capacity(counters.len() * 2);
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

            // Counter header row
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
                    )
                    .into_any_element(),
            );

            // Sparkline strip below the counter row
            let state = self.state.clone();
            rows.push(
                div()
                    .id(("sparkline", idx))
                    .w_full()
                    .h(px(SPARKLINE_HEIGHT))
                    .mx_2()
                    .mb_1()
                    .child(
                        canvas(move |bounds, _window, _cx| bounds, {
                            let state = state.clone();
                            move |bounds, _bounds_data, window, cx| {
                                let ts = state.read(cx);
                                let width = px_val(bounds.size.width);
                                let height = px_val(bounds.size.height);
                                if width < 2.0 || vis_end <= vis_start {
                                    return;
                                }

                                // Cap buckets at cycle range to avoid sparse bars
                                let cycle_range = (vis_end - vis_start) as usize;
                                let bucket_count = (width as usize).min(cycle_range).max(1);
                                let data =
                                    ts.counter_downsample(idx, vis_start, vis_end, bucket_count);
                                let global_max =
                                    data.iter().map(|(_, mx)| *mx).max().unwrap_or(1).max(1);

                                // Paint contiguous filled bars spanning the full width
                                let n = data.len() as f32;
                                let bar_w = (width / n).ceil().max(1.0);
                                for (i, (_min_d, max_d)) in data.iter().enumerate() {
                                    let bar_top = *max_d as f32 / global_max as f32;
                                    if bar_top < 0.001 {
                                        continue;
                                    }
                                    let bar_h = (bar_top * height).max(2.0);
                                    let y_top = height - bar_h;
                                    let x = (i as f32 / n * width).floor();

                                    window.paint_quad(fill(
                                        Bounds::new(
                                            point(
                                                bounds.origin.x + px(x),
                                                bounds.origin.y + px(y_top),
                                            ),
                                            size(px(bar_w), px(bar_h)),
                                        ),
                                        SPARKLINE_COLOR,
                                    ));
                                }

                                // Paint cursor line
                                let cursor_cycle =
                                    ts.cursor_state.cursors[ts.cursor_state.active_idx]
                                        .cycle
                                        .round() as u32;
                                if cursor_cycle >= vis_start && cursor_cycle <= vis_end {
                                    let cursor_frac = (cursor_cycle - vis_start) as f32
                                        / (vis_end - vis_start) as f32;
                                    let cursor_x = cursor_frac * width;
                                    let cursor_color = colors::cursor_color(
                                        ts.cursor_state.cursors[ts.cursor_state.active_idx]
                                            .color_idx,
                                    );
                                    window.paint_quad(fill(
                                        Bounds::new(
                                            point(bounds.origin.x + px(cursor_x), bounds.origin.y),
                                            size(px(1.0), px(height)),
                                        ),
                                        cursor_color,
                                    ));
                                }
                            }
                        })
                        .size_full(),
                    )
                    .into_any_element(),
            );
        }

        // Header with view mode toggle
        let mode_label = match self.view_mode {
            ViewMode::Detail => "Detail",
            ViewMode::Heatmap => "Heatmap",
        };
        let header = div()
            .px_2()
            .py_1()
            .flex()
            .items_center()
            .border_b_1()
            .border_color(colors::GRID_LINE)
            .child(
                div()
                    .flex_1()
                    .text_color(colors::TEXT_DIMMED)
                    .child(format!(
                        "Performance Counters ({}) @ cycle {}",
                        counters.len(),
                        cursor_cycle
                    )),
            )
            .child(
                div()
                    .id("view-mode-toggle")
                    .px_2()
                    .rounded(px(3.0))
                    .cursor_pointer()
                    .text_color(colors::TEXT_DIMMED)
                    .hover(|s| s.text_color(colors::TEXT_PRIMARY).bg(colors::GRID_LINE))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.view_mode = match this.view_mode {
                            ViewMode::Detail => ViewMode::Heatmap,
                            ViewMode::Heatmap => ViewMode::Detail,
                        };
                        cx.notify();
                    }))
                    .child(mode_label),
            );

        // Content: either detail rows or heatmap canvas
        let content = if self.view_mode == ViewMode::Heatmap {
            // Heatmap: one canvas covering all counters
            let state = self.state.clone();
            let num_counters = counters.len();
            div().id("counter-heatmap").flex_1().child(
                canvas(move |bounds, _window, _cx| bounds, {
                    let state = state.clone();
                    move |bounds, _bounds_data, window, cx| {
                        let width = px_val(bounds.size.width);
                        let height = px_val(bounds.size.height);
                        if width < 2.0 || num_counters == 0 {
                            return;
                        }

                        // Pre-compute all heatmap data while holding the borrow.
                        let ts = state.read(cx);
                        let (vis_start, vis_end) = ts.effective_counter_range();
                        if vis_end <= vis_start {
                            return;
                        }
                        let cycle_range = (vis_end - vis_start) as usize;
                        let bucket_count = (width as usize).min(cycle_range).max(1);
                        let row_h = (height / num_counters as f32).clamp(2.0, HEATMAP_ROW_HEIGHT);

                        // Collect per-counter data + local_max + name.
                        type HeatmapRow = (Vec<(u64, u64)>, u64, String);
                        let heatmap_data: Vec<HeatmapRow> = ts
                            .trace
                            .counters
                            .iter()
                            .enumerate()
                            .map(|(ci, c)| {
                                let data =
                                    ts.counter_downsample(ci, vis_start, vis_end, bucket_count);
                                let local_max =
                                    data.iter().map(|(_, mx)| *mx).max().unwrap_or(1).max(1);
                                (data, local_max, c.name.clone())
                            })
                            .collect();
                        // ts borrow ends here (heatmap_data owns all needed data).

                        window.with_content_mask(Some(ContentMask { bounds }), |window| {
                            let n_f = bucket_count as f32;
                            let bar_w = (width / n_f).ceil().max(1.0);

                            for (ci, (data, local_max, _name)) in heatmap_data.iter().enumerate() {
                                let row_y = ci as f32 * row_h;
                                for (bi, (_min_d, max_d)) in data.iter().enumerate() {
                                    let intensity = *max_d as f32 / *local_max as f32;
                                    if intensity < 0.001 {
                                        continue;
                                    }
                                    let x = (bi as f32 / n_f * width).floor();
                                    let color = Hsla {
                                        h: 210.0 / 360.0,
                                        s: 0.6,
                                        l: 0.15 + intensity * 0.45,
                                        a: 0.4 + intensity * 0.6,
                                    };
                                    window.paint_quad(fill(
                                        Bounds::new(
                                            point(
                                                bounds.origin.x + px(x),
                                                bounds.origin.y + px(row_y),
                                            ),
                                            size(px(bar_w), px(row_h - 1.0)),
                                        ),
                                        color,
                                    ));
                                }
                            }

                            // Counter name labels
                            if row_h >= 10.0 {
                                for (ci, (_data, _local_max, name)) in
                                    heatmap_data.iter().enumerate()
                                {
                                    let row_y = ci as f32 * row_h;
                                    let run = TextRun {
                                        len: name.len(),
                                        font: Font {
                                            family: "Menlo".into(),
                                            features: Default::default(),
                                            fallbacks: None,
                                            weight: FontWeight::NORMAL,
                                            style: FontStyle::Normal,
                                        },
                                        color: colors::TEXT_DIMMED,
                                        background_color: None,
                                        underline: None,
                                        strikethrough: None,
                                    };
                                    let line = window.text_system().shape_line(
                                        name.clone().into(),
                                        px(9.0),
                                        &[run],
                                        None,
                                    );
                                    let _ = line.paint(
                                        point(
                                            bounds.origin.x + px(4.0),
                                            bounds.origin.y + px(row_y),
                                        ),
                                        px(9.0),
                                        window,
                                        cx,
                                    );
                                }
                            }
                        });
                    }
                })
                .size_full(),
            )
        } else {
            div()
                .id("counter-scroll")
                .flex_1()
                .overflow_y_scroll()
                .children(rows)
        };

        div()
            .id("counter-panel")
            .size_full()
            .bg(colors::BG_PRIMARY)
            .text_size(px(11.0))
            .font_family("Menlo")
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(header)
            .child(content)
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
