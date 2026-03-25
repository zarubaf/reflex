use gpui::*;

use crate::app::TraceState;
use crate::theme::colors;

/// Height of the minimap strip in pixels.
const MINIMAP_HEIGHT: f32 = 40.0;
/// Width of the viewport edge grab handles.
const HANDLE_WIDTH: f32 = 8.0;
/// Height of the viewport edge grab handles.
const HANDLE_HEIGHT: f32 = 24.0;
/// Handle corner radius.
const HANDLE_RADIUS: f32 = 4.0;
/// Corner radius of the minimap strip.
const MINIMAP_RADIUS: f32 = 6.0;
/// Minimum viewport width in pixels to prevent collapsing.
const MIN_VIEWPORT_PX: f32 = 16.0;
/// Minimum hit-test width for grabbing the viewport edges.
const HANDLE_HIT_ZONE: f32 = 12.0;

/// Accent color for viewport handles and trendline.
const ACCENT: Hsla = Hsla {
    h: 210.0 / 360.0,
    s: 0.65,
    l: 0.60,
    a: 1.0,
};

/// Trendline fill color — brighter and more opaque than before.
const TRENDLINE_FILL: Hsla = Hsla {
    h: 210.0 / 360.0,
    s: 0.55,
    l: 0.50,
    a: 0.50,
};

/// Dimmed trendline (outside viewport selection).
const TRENDLINE_DIM: Hsla = Hsla {
    h: 210.0 / 360.0,
    s: 0.30,
    l: 0.35,
    a: 0.30,
};

fn px_val(p: Pixels) -> f32 {
    f32::from(p)
}

/// Drag interaction state for the minimap viewport rectangle.
#[derive(Clone, Copy)]
enum MinimapDrag {
    Pan {
        start_mouse_x: f32,
        start_scroll: f64,
        range_width: f64,
    },
    ResizeLeft,
    ResizeRight,
}

/// Cached trendline data to avoid recomputing on every frame.
struct TrendlineCache {
    counter_idx: usize,
    max_cycle: u32,
    bucket_count: usize,
    /// Pre-computed global max for normalization.
    global_max: u64,
    data: Vec<(u64, u64)>,
}

/// A minimap showing the full trace duration as a trendline strip,
/// with a draggable viewport rectangle for navigation.
/// Click detection threshold — if mouse moves less than this during
/// press+release, it's a click (not a drag).
const CLICK_THRESHOLD: f32 = 4.0;

pub struct MinimapView {
    state: Entity<TraceState>,
    focus_handle: FocusHandle,
    selected_counter: Option<usize>,
    drag_state: Option<MinimapDrag>,
    /// Mouse-down position for click vs drag detection.
    click_start: Option<f32>,
    /// Whether the mouse moved enough to be considered a drag.
    did_drag: bool,
    canvas_origin: Point<Pixels>,
    canvas_width: f32,
    /// Cached trendline — recomputed only when counter, trace, or width changes.
    trendline_cache: Option<TrendlineCache>,
}

impl MinimapView {
    pub fn new(state: Entity<TraceState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_this, _state, cx| {
            cx.notify();
        })
        .detach();

        let ts = state.read(cx);
        let selected = if ts.trace.counters.is_empty() {
            None
        } else {
            Some(0)
        };

        Self {
            state,
            focus_handle: cx.focus_handle(),
            selected_counter: selected,
            drag_state: None,
            click_start: None,
            did_drag: false,
            canvas_origin: Point::default(),
            canvas_width: 1.0,
            trendline_cache: None,
        }
    }

    fn pixel_to_cycle(&self, local_px: f32, max_cycle: u32) -> f64 {
        if self.canvas_width <= 0.0 || max_cycle == 0 {
            return 0.0;
        }
        (local_px as f64 / self.canvas_width as f64) * max_cycle as f64
    }

    fn cycle_to_pixel(&self, cycle: f64, max_cycle: u32) -> f32 {
        if max_cycle == 0 {
            return 0.0;
        }
        (cycle / max_cycle as f64) as f32 * self.canvas_width
    }
}

/// Paint the trendline as contiguous filled bars spanning the full width.
fn paint_trendline_cached(
    data: &[(u64, u64)],
    global_max: u64,
    bounds: &Bounds<Pixels>,
    width: f32,
    height: f32,
    color: Hsla,
    window: &mut Window,
) {
    if data.is_empty() || global_max == 0 {
        return;
    }
    let n = data.len() as f32;
    // Each bar fills width/n pixels — ceil to avoid sub-pixel gaps.
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
                point(bounds.origin.x + px(x), bounds.origin.y + px(y_top)),
                size(px(bar_w), px(bar_h)),
            ),
            color,
        ));
    }
}

impl Render for MinimapView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.clone();

        // Read counter range for handle positioning (independent from pipeline viewport).
        let ts = self.state.read(cx);
        let max_cycle = ts.trace.max_cycle();
        let (cr_start, cr_end) = ts.effective_counter_range();
        let left_pct = if max_cycle > 0 {
            cr_start as f32 / max_cycle as f32
        } else {
            0.0
        };
        let right_pct = if max_cycle > 0 {
            (cr_end as f32 / max_cycle as f32).min(1.0)
        } else {
            1.0
        };

        div()
            .id("minimap")
            .h(px(MINIMAP_HEIGHT))
            .mx_2()
            .my_1()
            .rounded(px(MINIMAP_RADIUS))
            .bg(colors::BG_SECONDARY)
            .overflow_hidden()
            .relative()
            .child(
                canvas(
                    {
                        let view = cx.entity().clone();
                        let state = state.clone();
                        move |bounds, _window, cx| {
                            let width = px_val(bounds.size.width);

                            // Read trace data and compute cache update outside entity borrow.
                            let max_cycle = state.read(cx).trace.max_cycle();
                            let selected_counter = view.read(cx).selected_counter;
                            let new_cache = if let Some(counter_idx) = selected_counter {
                                let bucket_count = (width as usize).min(max_cycle as usize).max(1);
                                let needs_update = {
                                    let v = view.read(cx);
                                    match &v.trendline_cache {
                                        Some(c) => {
                                            c.counter_idx != counter_idx
                                                || c.max_cycle != max_cycle
                                                || c.bucket_count != bucket_count
                                        }
                                        None => true,
                                    }
                                };
                                if needs_update {
                                    let ts = state.read(cx);
                                    if counter_idx < ts.trace.counters.len() {
                                        let data = ts.counter_downsample(
                                            counter_idx,
                                            0,
                                            max_cycle,
                                            bucket_count,
                                        );
                                        let global_max = data
                                            .iter()
                                            .map(|(_, mx)| *mx)
                                            .max()
                                            .unwrap_or(1)
                                            .max(1);
                                        Some(TrendlineCache {
                                            counter_idx,
                                            max_cycle,
                                            bucket_count,
                                            global_max,
                                            data,
                                        })
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                }
                            } else {
                                None
                            };

                            view.update(cx, |v, _cx| {
                                v.canvas_origin = bounds.origin;
                                v.canvas_width = width;
                                if let Some(cache) = new_cache {
                                    v.trendline_cache = Some(cache);
                                }
                            });
                            bounds
                        }
                    },
                    {
                        let state = state.clone();
                        let view = cx.entity().clone();
                        move |bounds, _bounds_data, window, cx| {
                            let ts = state.read(cx);
                            let max_cycle = ts.trace.max_cycle();
                            if max_cycle == 0 {
                                return;
                            }
                            let width = px_val(bounds.size.width);
                            let height = px_val(bounds.size.height);

                            // Clip all painting to the canvas bounds.
                            window.with_content_mask(Some(ContentMask { bounds }), |window| {
                                // 1. Paint trendline from cache (computed in layout closure)
                                let v = view.read(cx);
                                if let Some(cache) = &v.trendline_cache {
                                    // Paint dim trendline across full width
                                    paint_trendline_cached(
                                        &cache.data,
                                        cache.global_max,
                                        &bounds,
                                        width,
                                        height,
                                        TRENDLINE_DIM,
                                        window,
                                    );

                                    // Paint bright trendline clipped to viewport
                                    let vp = &ts.viewport;
                                    let (vis_start, vis_end) = vp.visible_cycle_range();
                                    let vp_left_px =
                                        (vis_start as f64 / max_cycle as f64) as f32 * width;
                                    let vp_right_px =
                                        (vis_end as f64 / max_cycle as f64) as f32 * width;
                                    let vp_width_px =
                                        (vp_right_px - vp_left_px).max(MIN_VIEWPORT_PX);

                                    let clip_bounds = Bounds::new(
                                        point(bounds.origin.x + px(vp_left_px), bounds.origin.y),
                                        size(px(vp_width_px), px(height)),
                                    );
                                    window.with_content_mask(
                                        Some(ContentMask {
                                            bounds: clip_bounds,
                                        }),
                                        |window| {
                                            paint_trendline_cached(
                                                &cache.data,
                                                cache.global_max,
                                                &bounds,
                                                width,
                                                height,
                                                TRENDLINE_FILL,
                                                window,
                                            );
                                        },
                                    );
                                }

                                // 2. Compute counter range rectangle (independent from pipeline)
                                let (cr_start, cr_end) = ts.effective_counter_range();
                                let vp_left = ((cr_start as f64 / max_cycle as f64) as f32 * width)
                                    .clamp(0.0, width);
                                let vp_right = ((cr_end as f64 / max_cycle as f64) as f32 * width)
                                    .clamp(0.0, width);
                                let vp_width = (vp_right - vp_left)
                                    .max(MIN_VIEWPORT_PX)
                                    .min(width - vp_left);

                                // 3. Dimmed overlays outside counter range
                                let dim_color = Hsla {
                                    h: 0.0,
                                    s: 0.0,
                                    l: 0.0,
                                    a: 0.45,
                                };
                                if vp_left > 0.0 {
                                    window.paint_quad(fill(
                                        Bounds::new(bounds.origin, size(px(vp_left), px(height))),
                                        dim_color,
                                    ));
                                }
                                let right_start = vp_left + vp_width;
                                if right_start < width {
                                    window.paint_quad(fill(
                                        Bounds::new(
                                            point(
                                                bounds.origin.x + px(right_start),
                                                bounds.origin.y,
                                            ),
                                            size(px(width - right_start), px(height)),
                                        ),
                                        dim_color,
                                    ));
                                }

                                // 4. Viewport border
                                window.paint_quad(PaintQuad {
                                    bounds: Bounds::new(
                                        point(bounds.origin.x + px(vp_left), bounds.origin.y),
                                        size(px(vp_width), px(height)),
                                    ),
                                    corner_radii: Corners::all(px(2.0)),
                                    background: gpui::transparent_black().into(),
                                    border_widths: Edges::all(px(1.0)),
                                    border_color: Hsla { a: 0.5, ..ACCENT },
                                    border_style: BorderStyle::default(),
                                });

                                // 5. Pipeline viewport indicator (yellow bar at bottom)
                                let pvp = &ts.viewport;
                                let (pv_start, pv_end) = pvp.visible_cycle_range();
                                let pv_left = ((pv_start as f64 / max_cycle as f64) as f32 * width)
                                    .clamp(0.0, width);
                                let pv_right = ((pv_end as f64 / max_cycle as f64) as f32 * width)
                                    .clamp(0.0, width);
                                let pv_width = (pv_right - pv_left).max(8.0).min(width - pv_left);
                                let indicator_h = 5.0;
                                window.paint_quad(PaintQuad {
                                    bounds: Bounds::new(
                                        point(
                                            bounds.origin.x + px(pv_left),
                                            bounds.origin.y + px(height - indicator_h),
                                        ),
                                        size(px(pv_width), px(indicator_h)),
                                    ),
                                    corner_radii: Corners::all(px(2.0)),
                                    background: Hsla {
                                        h: 40.0 / 360.0,
                                        s: 0.8,
                                        l: 0.55,
                                        a: 0.9,
                                    }
                                    .into(),
                                    border_widths: Edges::default(),
                                    border_color: gpui::transparent_black(),
                                    border_style: BorderStyle::default(),
                                });

                                // 6. ALL cursor markers (not just active)
                                for cursor in &ts.cursor_state.cursors {
                                    let cursor_x = (cursor.cycle / max_cycle as f64) as f32 * width;
                                    let cursor_color = colors::cursor_color(cursor.color_idx);
                                    window.paint_quad(fill(
                                        Bounds::new(
                                            point(bounds.origin.x + px(cursor_x), bounds.origin.y),
                                            size(px(1.0), px(height - indicator_h)),
                                        ),
                                        cursor_color,
                                    ));
                                }
                            }); // end with_content_mask
                        }
                    },
                )
                .size_full(),
            )
            // Left handle — absolutely positioned, clipped by parent's overflow_hidden.
            // Use percentage-based left position so it scales with parent width.
            // Negative margin centers the handle on the viewport edge.
            .child(
                div()
                    .absolute()
                    .left(relative(left_pct))
                    .ml(px(-HANDLE_WIDTH / 2.0))
                    .top(px((MINIMAP_HEIGHT - HANDLE_HEIGHT) / 2.0))
                    .w(px(HANDLE_WIDTH))
                    .h(px(HANDLE_HEIGHT))
                    .rounded(px(HANDLE_RADIUS))
                    .bg(ACCENT),
            )
            // Right handle
            .child(
                div()
                    .absolute()
                    .left(relative(right_pct))
                    .ml(px(-HANDLE_WIDTH / 2.0))
                    .top(px((MINIMAP_HEIGHT - HANDLE_HEIGHT) / 2.0))
                    .w(px(HANDLE_WIDTH))
                    .h(px(HANDLE_HEIGHT))
                    .rounded(px(HANDLE_RADIUS))
                    .bg(ACCENT),
            )
            // Mouse interactions — use bare closures (not cx.listener) so drag
            // events continue firing even when the mouse leaves the minimap bounds.
            .on_mouse_down(MouseButton::Left, {
                let entity = cx.entity().clone();
                let state = self.state.clone();
                move |ev: &MouseDownEvent, _window, cx| {
                    let ts = state.read(cx);
                    let max_cycle = ts.trace.max_cycle();
                    let (cr_start, cr_end) = ts.effective_counter_range();
                    let cr_start_f = cr_start as f64;

                    entity.update(cx, |this, _cx| {
                        let local_x = px_val(ev.position.x - this.canvas_origin.x);
                        let cr_left = this.cycle_to_pixel(cr_start as f64, max_cycle);
                        let cr_right = this.cycle_to_pixel(cr_end as f64, max_cycle);

                        let near_left = (local_x - cr_left).abs() < HANDLE_HIT_ZONE;
                        let near_right = (local_x - cr_right).abs() < HANDLE_HIT_ZONE;
                        let inside = local_x >= cr_left && local_x <= cr_right;

                        // Record click position for click-vs-drag detection.
                        this.click_start = Some(local_x);
                        this.did_drag = false;

                        if near_left && !near_right {
                            this.drag_state = Some(MinimapDrag::ResizeLeft);
                        } else if near_right {
                            this.drag_state = Some(MinimapDrag::ResizeRight);
                        } else if inside {
                            this.drag_state = Some(MinimapDrag::Pan {
                                start_mouse_x: local_x,
                                start_scroll: cr_start_f,
                                range_width: cr_end as f64 - cr_start_f,
                            });
                        }
                        // Don't jump pipeline here — wait for mouse_up to distinguish
                        // click from drag.
                    });
                }
            })
            .on_mouse_move({
                let entity = cx.entity().clone();
                let state = self.state.clone();
                move |ev: &MouseMoveEvent, _window, cx| {
                    let drag = entity.read(cx).drag_state;
                    let Some(drag) = drag else {
                        return;
                    };

                    // If the mouse button was released outside the minimap,
                    // on_mouse_up never fired. Detect and clear stale drag.
                    if ev.pressed_button.is_none() {
                        entity.update(cx, |v, _| {
                            v.drag_state = None;
                            v.click_start = None;
                        });
                        return;
                    }

                    let (local_x, canvas_width) = {
                        let v = entity.read(cx);
                        (px_val(ev.position.x - v.canvas_origin.x), v.canvas_width)
                    };

                    // Mark as drag if mouse moved beyond threshold.
                    {
                        let v = entity.read(cx);
                        if let Some(start) = v.click_start {
                            if (local_x - start).abs() > CLICK_THRESHOLD {
                                entity.update(cx, |v, _| v.did_drag = true);
                            }
                        }
                    }

                    let ts = state.read(cx);
                    let max_cycle = ts.trace.max_cycle();
                    if max_cycle == 0 || canvas_width <= 0.0 {
                        return;
                    }

                    let mouse_cycle = ((local_x as f64 / canvas_width as f64) * max_cycle as f64)
                        .clamp(0.0, max_cycle as f64);

                    match drag {
                        MinimapDrag::Pan {
                            start_mouse_x,
                            start_scroll,
                            range_width,
                        } => {
                            let start_cycle =
                                (start_mouse_x as f64 / canvas_width as f64) * max_cycle as f64;
                            let dx = mouse_cycle - start_cycle;
                            let max_start = (max_cycle as f64 - range_width).max(0.0);
                            let new_start = (start_scroll + dx).clamp(0.0, max_start);
                            let new_end = (new_start + range_width).min(max_cycle as f64);
                            state.update(cx, |ts, cx| {
                                ts.counter_range = Some((new_start as u32, new_end as u32));
                                cx.notify();
                            });
                        }
                        MinimapDrag::ResizeLeft => {
                            let (_, cr_end) = ts.effective_counter_range();
                            let new_start = mouse_cycle.clamp(0.0, cr_end as f64 - 10.0) as u32;
                            state.update(cx, |ts, cx| {
                                ts.counter_range = Some((new_start, cr_end));
                                cx.notify();
                            });
                        }
                        MinimapDrag::ResizeRight => {
                            let (cr_start, _) = ts.effective_counter_range();
                            let new_end =
                                mouse_cycle.clamp(cr_start as f64 + 10.0, max_cycle as f64) as u32;
                            state.update(cx, |ts, cx| {
                                ts.counter_range = Some((cr_start, new_end));
                                cx.notify();
                            });
                        }
                    }
                }
            })
            .on_mouse_up(MouseButton::Left, {
                let entity = cx.entity().clone();
                let state = self.state.clone();
                move |ev: &MouseUpEvent, _window, cx| {
                    let (did_drag, canvas_origin, canvas_width) = {
                        let v = entity.read(cx);
                        (v.did_drag, v.canvas_origin, v.canvas_width)
                    };

                    // If it was a click (not a drag), jump pipeline viewport
                    // and set cursor to the clicked position.
                    if !did_drag && canvas_width > 0.0 {
                        let local_x = px_val(ev.position.x - canvas_origin.x);
                        let max_cycle = state.read(cx).trace.max_cycle();
                        if max_cycle > 0 {
                            let clicked_cycle =
                                (local_x as f64 / canvas_width as f64) * max_cycle as f64;
                            let clicked_cycle = clicked_cycle.clamp(0.0, max_cycle as f64);
                            let cc = clicked_cycle as u32;

                            // Ensure segments around the clicked cycle are loaded
                            // so we can find instructions there.
                            state.update(cx, |ts, _cx| {
                                let vw = (ts.viewport.view_width as f64
                                    / ts.viewport.pixels_per_cycle as f64)
                                    as u32;
                                ts.ensure_segments_loaded(
                                    cc.saturating_sub(vw),
                                    cc.saturating_add(vw).min(max_cycle),
                                );
                            });

                            let ts = state.read(cx);
                            let view_cycles =
                                ts.viewport.view_width as f64 / ts.viewport.pixels_per_cycle as f64;
                            let new_scroll = clicked_cycle - view_cycles / 2.0;

                            // Find the first instruction active at the clicked cycle.
                            let target_row = ts
                                .trace
                                .instructions
                                .iter()
                                .position(|instr| instr.first_cycle <= cc && instr.last_cycle >= cc)
                                .unwrap_or(0);
                            let view_rows =
                                ts.viewport.view_height as f64 / ts.viewport.row_height as f64;

                            state.update(cx, |ts, cx| {
                                ts.viewport.scroll_cycle = new_scroll.max(0.0);
                                ts.viewport.scroll_row =
                                    (target_row as f64 - view_rows / 2.0).max(0.0);
                                ts.viewport.clamp();
                                cx.notify();
                            });
                        }
                    }

                    entity.update(cx, |v, _| {
                        v.drag_state = None;
                        v.click_start = None;
                        v.did_drag = false;
                    });
                }
            })
            .on_scroll_wheel(cx.listener(|this, ev: &ScrollWheelEvent, _window, cx| {
                // Scroll wheel on minimap zooms the counter range, not pipeline.
                let local_x = px_val(ev.position.x - this.canvas_origin.x);
                let ts = this.state.read(cx);
                let max_cycle = ts.trace.max_cycle();
                let focal_cycle = this.pixel_to_cycle(local_x, max_cycle);
                let (cr_start, cr_end) = ts.effective_counter_range();

                let delta = ev.delta.pixel_delta(px(20.0));
                let dy = px_val(delta.y);
                let factor = (1.0_f64 + dy as f64 * 0.005).clamp(0.5, 2.0);

                let cr_width = (cr_end as f64 - cr_start as f64) * factor;
                let cr_width = cr_width.clamp(10.0, max_cycle as f64);
                let focal_frac =
                    (focal_cycle - cr_start as f64) / (cr_end as f64 - cr_start as f64).max(1.0);
                let new_start = (focal_cycle - focal_frac * cr_width)
                    .clamp(0.0, (max_cycle as f64 - cr_width).max(0.0));
                let new_end = (new_start + cr_width).min(max_cycle as f64);

                this.state.update(cx, |ts, cx| {
                    ts.counter_range = Some((new_start as u32, new_end as u32));
                    cx.notify();
                });
                cx.stop_propagation();
            }))
    }
}

impl Focusable for MinimapView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
