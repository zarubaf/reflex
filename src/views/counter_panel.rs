use gpui::*;

use crate::app::{CursorState, TraceState};
use crate::theme::colors;
use crate::trace::model::CounterDisplayMode;

/// Paint vertical cursor lines within a counter canvas area.
fn paint_cursor_lines(
    cursor_state: &CursorState,
    vis_start: u32,
    vis_end: u32,
    bounds: &Bounds<Pixels>,
    width: f32,
    height: f32,
    window: &mut Window,
) {
    if vis_end <= vis_start {
        return;
    }
    let range = (vis_end - vis_start) as f32;
    for cursor in &cursor_state.cursors {
        let cc = cursor.cycle.round() as u32;
        if cc >= vis_start && cc <= vis_end {
            let frac = (cc - vis_start) as f32 / range;
            let x = frac * width;
            window.paint_quad(fill(
                Bounds::new(
                    point(bounds.origin.x + px(x), bounds.origin.y),
                    size(px(1.0), px(height)),
                ),
                colors::cursor_color(cursor.color_idx),
            ));
        }
    }
}

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

/// Cached heatmap quad data — reused across frames when only cursor changes.
struct HeatmapCache {
    /// Pre-computed (x, y, w, h, color) for each heatmap cell.
    quads: Vec<(f32, f32, f32, f32, Hsla)>,
    /// Pre-computed (name, row_y) for labels.
    labels: Vec<(String, f32)>,
    /// Key: invalidate when these change.
    counter_range: (u32, u32),
    canvas_width: f32,
    canvas_height: f32,
    num_counters: usize,
}

/// A panel displaying performance counter values at the current cursor cycle.
pub struct CounterPanel {
    state: Entity<TraceState>,
    focus_handle: FocusHandle,
    /// Per-counter display mode overrides (None = use default from schema).
    display_modes: Vec<Option<CounterDisplayMode>>,
    /// Current view mode (detail sparklines vs heatmap overview).
    view_mode: ViewMode,
    /// Cached canvas origin for click-to-cycle mapping.
    canvas_origin: Point<Pixels>,
    /// Cached canvas width.
    canvas_width: f32,
    /// Cached heatmap quads — avoids recomputing on cursor-only changes.
    heatmap_cache: Option<HeatmapCache>,
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
            canvas_origin: Point::default(),
            canvas_width: 0.0,
            heatmap_cache: None,
        }
    }

    /// Export display modes for session persistence.
    pub fn session_display_modes(&self) -> Vec<Option<String>> {
        self.display_modes
            .iter()
            .map(|m| {
                m.map(|mode| match mode {
                    CounterDisplayMode::Total => "Total".to_string(),
                    CounterDisplayMode::Rate => "Rate".to_string(),
                    CounterDisplayMode::Delta => "Delta".to_string(),
                })
            })
            .collect()
    }

    /// Export view mode for session persistence.
    pub fn session_view_mode(&self) -> String {
        match self.view_mode {
            ViewMode::Detail => "Detail".to_string(),
            ViewMode::Heatmap => "Heatmap".to_string(),
        }
    }

    /// Restore counter panel state from a session snapshot.
    pub fn restore_session(&mut self, snap: &crate::session::CounterPanelSnapshot) {
        for (i, mode_str) in snap.display_modes.iter().enumerate() {
            if i < self.display_modes.len() {
                self.display_modes[i] = mode_str.as_deref().and_then(|s| match s {
                    "Total" => Some(CounterDisplayMode::Total),
                    "Rate" => Some(CounterDisplayMode::Rate),
                    "Delta" => Some(CounterDisplayMode::Delta),
                    _ => None,
                });
            }
        }
        self.view_mode = match snap.view_mode.as_str() {
            "Heatmap" => ViewMode::Heatmap,
            _ => ViewMode::Detail,
        };
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
                    let val = ts.counter_value_at(idx, cursor_cycle);
                    (Self::format_value(val), "")
                }
                CounterDisplayMode::Rate => {
                    let rate = ts.counter_rate_at(idx, cursor_cycle, RATE_WINDOW);
                    (Self::format_rate(rate), "/cy")
                }
                CounterDisplayMode::Delta => {
                    let delta = ts.counter_delta_at(idx, cursor_cycle);
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

            // Sparkline strip below the counter row — click to set cursor.
            let state = self.state.clone();
            let entity = cx.entity().clone();
            let sparkline_mode = mode;
            rows.push(
                div()
                    .id(("sparkline", idx))
                    .w_full()
                    .h(px(SPARKLINE_HEIGHT))
                    .mx_2()
                    .mb_1()
                    .cursor_pointer()
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(move |this, event: &MouseUpEvent, _window, cx| {
                            let local_x = px_val(event.position.x) - px_val(this.canvas_origin.x);
                            if this.canvas_width > 0.0 {
                                let cycle = {
                                    let ts = this.state.read(cx);
                                    let (vs, ve) = ts.effective_counter_range();
                                    if ve <= vs {
                                        return;
                                    }
                                    let frac = (local_x / this.canvas_width).clamp(0.0, 1.0);
                                    vs as f64 + frac as f64 * (ve - vs) as f64
                                };
                                this.state.update(cx, |ts, cx| {
                                    ts.cursor_state.move_cursor(cycle.round());
                                    cx.notify();
                                });
                            }
                        }),
                    )
                    .child(
                        canvas(
                            {
                                let entity = entity.clone();
                                move |bounds, _window, cx| {
                                    entity.update(cx, |panel: &mut Self, _cx| {
                                        panel.canvas_origin = bounds.origin;
                                        panel.canvas_width = px_val(bounds.size.width);
                                    });
                                    bounds
                                }
                            },
                            {
                                let state = state.clone();
                                move |bounds, _bounds_data, window, cx| {
                                    let ts = state.read(cx);
                                    let width = px_val(bounds.size.width);
                                    let height = px_val(bounds.size.height);
                                    if width < 2.0 || vis_end <= vis_start {
                                        return;
                                    }

                                    let data = if sparkline_mode == CounterDisplayMode::Delta {
                                        // Delta mode: one bucket per cycle for clean bars.
                                        let cycle_range = (vis_end - vis_start) as usize;
                                        let bucket_count = (width as usize).min(cycle_range).max(1);
                                        let cpb =
                                            (vis_end - vis_start) as f64 / bucket_count as f64;
                                        (0..bucket_count)
                                            .map(|b| {
                                                let c0 = (vis_start as f64 + b as f64 * cpb) as u32;
                                                let c1 = (vis_start as f64 + (b + 1) as f64 * cpb)
                                                    .ceil()
                                                    as u32;
                                                let v0 = ts.counter_value_at(idx, c0);
                                                let v1 = ts.counter_value_at(idx, c1.min(vis_end));
                                                let d = v1.saturating_sub(v0);
                                                (d, d)
                                            })
                                            .collect::<Vec<_>>()
                                    } else {
                                        let bucket_count = (width as usize).max(1);
                                        ts.counter_downsample(idx, vis_start, vis_end, bucket_count)
                                    };
                                    crate::views::render_util::paint_bars(
                                        &data,
                                        &bounds,
                                        width,
                                        height,
                                        SPARKLINE_COLOR,
                                        window,
                                    );

                                    // Paint all cursor lines
                                    paint_cursor_lines(
                                        &ts.cursor_state,
                                        vis_start,
                                        vis_end,
                                        &bounds,
                                        width,
                                        height,
                                        window,
                                    );
                                }
                            },
                        )
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
            let entity = cx.entity().clone();
            let num_counters = counters.len();
            div()
                .id("counter-heatmap")
                .flex_1()
                .cursor_pointer()
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseUpEvent, _window, cx| {
                        let local_x = px_val(event.position.x) - px_val(this.canvas_origin.x);
                        if this.canvas_width > 0.0 {
                            let ts = this.state.read(cx);
                            let (vs, ve) = ts.effective_counter_range();
                            if ve > vs {
                                let frac = (local_x / this.canvas_width).clamp(0.0, 1.0);
                                let cycle = vs as f64 + frac as f64 * (ve - vs) as f64;
                                this.state.update(cx, |ts, cx| {
                                    ts.cursor_state.move_cursor(cycle.round());
                                    cx.notify();
                                });
                            }
                        }
                    }),
                )
                .child(
                    canvas(
                        {
                            let entity = entity.clone();
                            move |bounds, _window, cx| {
                                entity.update(cx, |panel: &mut Self, _cx| {
                                    panel.canvas_origin = bounds.origin;
                                    panel.canvas_width = px_val(bounds.size.width);
                                });
                                bounds
                            }
                        },
                        {
                            let state = state.clone();
                            let entity = entity.clone();
                            move |bounds, _bounds_data, window, cx| {
                                let width = px_val(bounds.size.width);
                                let height = px_val(bounds.size.height);
                                if width < 2.0 || num_counters == 0 {
                                    return;
                                }

                                let ts = state.read(cx);
                                let (vis_start, vis_end) = ts.effective_counter_range();
                                if vis_end <= vis_start {
                                    return;
                                }

                                // Check if cache is valid.
                                let cache_key = ((vis_start, vis_end), width, height, num_counters);
                                let need_rebuild = {
                                    let panel = entity.read(cx);
                                    match &panel.heatmap_cache {
                                        Some(c) => {
                                            c.counter_range != cache_key.0
                                                || c.canvas_width != cache_key.1
                                                || c.canvas_height != cache_key.2
                                                || c.num_counters != cache_key.3
                                        }
                                        None => true,
                                    }
                                };

                                if need_rebuild {
                                    let cycle_range = (vis_end - vis_start) as usize;
                                    let max_buckets = (width as usize / 4).max(1);
                                    let bucket_count = max_buckets.min(cycle_range).max(1);
                                    let row_h = (height / num_counters as f32)
                                        .clamp(2.0, HEATMAP_ROW_HEIGHT);
                                    let n_f = bucket_count as f32;

                                    let mut quads = Vec::new();
                                    let mut labels = Vec::new();

                                    for (ci, c) in ts.trace.counters.iter().enumerate() {
                                        let data = ts.counter_downsample(
                                            ci,
                                            vis_start,
                                            vis_end,
                                            bucket_count,
                                        );
                                        let local_max = data
                                            .iter()
                                            .map(|(_, mx)| *mx)
                                            .max()
                                            .unwrap_or(1)
                                            .max(1);
                                        let row_y = ci as f32 * row_h;
                                        labels.push((c.name.clone(), row_y));

                                        // RLE merge.
                                        let mut run_start = 0usize;
                                        let mut run_level: i32 = -1;
                                        for bi in 0..=data.len() {
                                            let level = if bi < data.len() {
                                                let raw = data[bi].1 as f32 / local_max as f32;
                                                if raw < 0.001 {
                                                    0
                                                } else {
                                                    (raw * 8.0).round() as i32
                                                }
                                            } else {
                                                -1
                                            };
                                            if level != run_level {
                                                if run_level > 0 {
                                                    let x =
                                                        (run_start as f32 / n_f * width).floor();
                                                    let x_end = (bi as f32 / n_f * width).floor();
                                                    let qi = run_level as f32 / 8.0;
                                                    quads.push((
                                                        x,
                                                        row_y,
                                                        (x_end - x).max(1.0),
                                                        row_h - 1.0,
                                                        Hsla {
                                                            h: 210.0 / 360.0,
                                                            s: 0.6,
                                                            l: 0.15 + qi * 0.45,
                                                            a: 0.4 + qi * 0.6,
                                                        },
                                                    ));
                                                }
                                                run_start = bi;
                                                run_level = level;
                                            }
                                        }
                                    }

                                    entity.update(cx, |panel: &mut Self, _cx| {
                                        panel.heatmap_cache = Some(HeatmapCache {
                                            quads,
                                            labels,
                                            counter_range: cache_key.0,
                                            canvas_width: cache_key.1,
                                            canvas_height: cache_key.2,
                                            num_counters: cache_key.3,
                                        });
                                    });
                                    // Re-borrow for painting below.
                                }

                                // Extract cached data to avoid borrow conflicts.
                                let cursor_state = state.read(cx).cursor_state.clone();
                                let (quads, labels) = {
                                    let panel = entity.read(cx);
                                    let c = panel.heatmap_cache.as_ref().unwrap();
                                    (c.quads.clone(), c.labels.clone())
                                };
                                let row_h =
                                    (height / num_counters as f32).clamp(2.0, HEATMAP_ROW_HEIGHT);

                                window.with_content_mask(Some(ContentMask { bounds }), |window| {
                                    for &(x, y, w, h, color) in &quads {
                                        window.paint_quad(fill(
                                            Bounds::new(
                                                point(
                                                    bounds.origin.x + px(x),
                                                    bounds.origin.y + px(y),
                                                ),
                                                size(px(w), px(h)),
                                            ),
                                            color,
                                        ));
                                    }

                                    // Cursor lines
                                    paint_cursor_lines(
                                        &cursor_state,
                                        vis_start,
                                        vis_end,
                                        &bounds,
                                        width,
                                        height,
                                        window,
                                    );

                                    // Counter name labels
                                    if row_h >= 10.0 {
                                        for (name, row_y) in &labels {
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
                                                    bounds.origin.y + px(*row_y),
                                                ),
                                                px(9.0),
                                                window,
                                                cx,
                                            );
                                        }
                                    }
                                });
                            }
                        },
                    )
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
