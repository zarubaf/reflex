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
pub struct MinimapView {
    state: Entity<TraceState>,
    focus_handle: FocusHandle,
    selected_counter: Option<usize>,
    drag_state: Option<MinimapDrag>,
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

        div()
            .id("minimap")
            .w_full()
            .h(px(MINIMAP_HEIGHT))
            .mx_2()
            .my_1()
            .rounded(px(MINIMAP_RADIUS))
            .bg(colors::BG_SECONDARY)
            .overflow_hidden()
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
                                        let data = ts.trace.counter_downsample_minmax(
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
                                let vp_width_px = (vp_right_px - vp_left_px).max(MIN_VIEWPORT_PX);

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

                            // 2. Compute viewport rectangle (strictly clamped to canvas)
                            let vp = &ts.viewport;
                            let (vis_start, vis_end) = vp.visible_cycle_range();
                            let vp_left = ((vis_start as f64 / max_cycle as f64) as f32 * width)
                                .clamp(0.0, width);
                            let vp_right = ((vis_end as f64 / max_cycle as f64) as f32 * width)
                                .clamp(0.0, width);
                            let vp_width = (vp_right - vp_left)
                                .max(MIN_VIEWPORT_PX)
                                .min(width - vp_left); // ensure right edge stays within canvas

                            // 3. Dimmed overlays outside viewport
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
                                        point(bounds.origin.x + px(right_start), bounds.origin.y),
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

                            // 5. Edge handles (rounded pills, centered on viewport edges)
                            // Clamped inward by corner radius so they're not clipped.
                            let handle_y = bounds.origin.y + px((height - HANDLE_HEIGHT) / 2.0);
                            let inset = MINIMAP_RADIUS;
                            let left_hx =
                                (vp_left - HANDLE_WIDTH / 2.0).clamp(inset, width - HANDLE_WIDTH);
                            let right_hx = (vp_left + vp_width - HANDLE_WIDTH / 2.0)
                                .clamp(left_hx + HANDLE_WIDTH, width - HANDLE_WIDTH - inset);
                            // Left handle
                            window.paint_quad(PaintQuad {
                                bounds: Bounds::new(
                                    point(bounds.origin.x + px(left_hx), handle_y),
                                    size(px(HANDLE_WIDTH), px(HANDLE_HEIGHT)),
                                ),
                                corner_radii: Corners::all(px(HANDLE_RADIUS)),
                                background: ACCENT.into(),
                                border_widths: Edges::default(),
                                border_color: gpui::transparent_black(),
                                border_style: BorderStyle::default(),
                            });
                            // Right handle
                            window.paint_quad(PaintQuad {
                                bounds: Bounds::new(
                                    point(bounds.origin.x + px(right_hx), handle_y),
                                    size(px(HANDLE_WIDTH), px(HANDLE_HEIGHT)),
                                ),
                                corner_radii: Corners::all(px(HANDLE_RADIUS)),
                                background: ACCENT.into(),
                                border_widths: Edges::default(),
                                border_color: gpui::transparent_black(),
                                border_style: BorderStyle::default(),
                            });

                            // 6. Cursor marker
                            let active_cursor =
                                &ts.cursor_state.cursors[ts.cursor_state.active_idx];
                            let cursor_x = (active_cursor.cycle / max_cycle as f64) as f32 * width;
                            let cursor_color = colors::cursor_color(active_cursor.color_idx);
                            window.paint_quad(fill(
                                Bounds::new(
                                    point(bounds.origin.x + px(cursor_x), bounds.origin.y),
                                    size(px(1.0), px(height)),
                                ),
                                cursor_color,
                            ));
                        }
                    },
                )
                .size_full(),
            )
            // Mouse interactions — use bare closures (not cx.listener) so drag
            // events continue firing even when the mouse leaves the minimap bounds.
            .on_mouse_down(MouseButton::Left, {
                let entity = cx.entity().clone();
                let state = self.state.clone();
                move |ev: &MouseDownEvent, _window, cx| {
                    let ts = state.read(cx);
                    let max_cycle = ts.trace.max_cycle();
                    let vp = &ts.viewport;
                    let (vis_start, vis_end) = vp.visible_cycle_range();
                    let scroll = vp.scroll_cycle;
                    let view_w = vp.view_width;
                    let ppc = vp.pixels_per_cycle;

                    entity.update(cx, |this, cx| {
                        let local_x = px_val(ev.position.x - this.canvas_origin.x);
                        let vp_left = this.cycle_to_pixel(vis_start as f64, max_cycle);
                        let vp_right = this.cycle_to_pixel(vis_end as f64, max_cycle);

                        let near_left = (local_x - vp_left).abs() < HANDLE_HIT_ZONE;
                        let near_right = (local_x - vp_right).abs() < HANDLE_HIT_ZONE;
                        let inside = local_x >= vp_left && local_x <= vp_right;

                        if near_left && !near_right {
                            this.drag_state = Some(MinimapDrag::ResizeLeft);
                        } else if near_right {
                            this.drag_state = Some(MinimapDrag::ResizeRight);
                        } else if inside {
                            this.drag_state = Some(MinimapDrag::Pan {
                                start_mouse_x: local_x,
                                start_scroll: scroll,
                            });
                        } else {
                            // Click outside: jump viewport center to click position.
                            let clicked_cycle = this.pixel_to_cycle(local_x, max_cycle);
                            let view_cycles = view_w as f64 / ppc as f64;
                            let new_scroll = clicked_cycle - view_cycles / 2.0;
                            state.update(cx, |ts, cx| {
                                ts.viewport.scroll_cycle = new_scroll.max(0.0);
                                ts.viewport.clamp();
                                cx.notify();
                            });
                        }
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

                    let (local_x, canvas_width) = {
                        let v = entity.read(cx);
                        (px_val(ev.position.x - v.canvas_origin.x), v.canvas_width)
                    };
                    let max_cycle = state.read(cx).trace.max_cycle();
                    if max_cycle == 0 || canvas_width <= 0.0 {
                        return;
                    }

                    // Convert mouse pixel to absolute cycle position.
                    let mouse_cycle = (local_x as f64 / canvas_width as f64) * max_cycle as f64;

                    match drag {
                        MinimapDrag::Pan {
                            start_mouse_x,
                            start_scroll,
                        } => {
                            let start_cycle =
                                (start_mouse_x as f64 / canvas_width as f64) * max_cycle as f64;
                            let dx = mouse_cycle - start_cycle;
                            state.update(cx, |ts, cx| {
                                ts.viewport.scroll_cycle = (start_scroll + dx).max(0.0);
                                ts.viewport.clamp();
                                cx.notify();
                            });
                        }
                        MinimapDrag::ResizeLeft => {
                            // Left edge follows mouse directly.
                            let vp = &state.read(cx).viewport;
                            let current_end =
                                vp.scroll_cycle + vp.view_width as f64 / vp.pixels_per_cycle as f64;
                            let new_start = mouse_cycle.clamp(0.0, current_end - 10.0);
                            let new_view = current_end - new_start;
                            let new_ppc = vp.view_width as f64 / new_view;
                            state.update(cx, |ts, cx| {
                                ts.viewport.scroll_cycle = new_start;
                                ts.viewport.pixels_per_cycle = (new_ppc as f32).clamp(0.01, 500.0);
                                ts.viewport.clamp();
                                cx.notify();
                            });
                        }
                        MinimapDrag::ResizeRight => {
                            // Right edge follows mouse directly.
                            let vp = &state.read(cx).viewport;
                            let new_end =
                                mouse_cycle.clamp(vp.scroll_cycle + 10.0, max_cycle as f64);
                            let new_view = new_end - vp.scroll_cycle;
                            let new_ppc = vp.view_width as f64 / new_view;
                            state.update(cx, |ts, cx| {
                                ts.viewport.pixels_per_cycle = (new_ppc as f32).clamp(0.01, 500.0);
                                ts.viewport.clamp();
                                cx.notify();
                            });
                        }
                    }
                }
            })
            .on_mouse_up(MouseButton::Left, {
                let entity = cx.entity().clone();
                move |_: &MouseUpEvent, _window, cx| {
                    entity.update(cx, |v, _| {
                        v.drag_state = None;
                    });
                }
            })
            .on_scroll_wheel(cx.listener(|this, ev: &ScrollWheelEvent, _window, cx| {
                let local_x = px_val(ev.position.x - this.canvas_origin.x);
                let max_cycle = this.state.read(cx).trace.max_cycle();
                let focal_cycle = this.pixel_to_cycle(local_x, max_cycle);

                let delta = ev.delta.pixel_delta(px(20.0));
                let dy = px_val(delta.y);
                let factor = (1.0_f32 + dy * 0.005).clamp(0.5, 2.0);

                this.state.update(cx, |ts, cx| {
                    let vp = &mut ts.viewport;
                    let new_ppc = (vp.pixels_per_cycle * factor).clamp(0.01, 500.0);
                    let focal_px = (focal_cycle - vp.scroll_cycle) * vp.pixels_per_cycle as f64;
                    vp.pixels_per_cycle = new_ppc;
                    vp.scroll_cycle = focal_cycle - focal_px / new_ppc as f64;
                    vp.clamp();
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
