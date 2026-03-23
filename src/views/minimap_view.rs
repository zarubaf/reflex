use gpui::*;

use crate::app::TraceState;
use crate::theme::colors;

/// Height of the minimap strip in pixels.
const MINIMAP_HEIGHT: f32 = 40.0;
/// Width of the viewport edge grab handles.
const HANDLE_WIDTH: f32 = 6.0;
/// Height of the viewport edge grab handles.
const HANDLE_HEIGHT: f32 = 20.0;
/// Handle corner radius.
const HANDLE_RADIUS: f32 = 3.0;
/// Minimum viewport width in pixels to prevent collapsing.
const MIN_VIEWPORT_PX: f32 = 10.0;

/// Accent color for viewport handles and trendline.
const ACCENT: Hsla = Hsla {
    h: 210.0 / 360.0,
    s: 0.65,
    l: 0.55,
    a: 1.0,
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
    ResizeLeft {
        start_mouse_x: f32,
    },
    ResizeRight {
        start_mouse_x: f32,
    },
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

impl Render for MinimapView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.clone();

        div()
            .id("minimap")
            .w_full()
            .h(px(MINIMAP_HEIGHT))
            .bg(colors::BG_SECONDARY)
            .border_b_1()
            .border_color(colors::GRID_LINE)
            .child(
                canvas(
                    {
                        let view = cx.entity().clone();
                        move |bounds, _window, cx| {
                            view.update(cx, |v, _cx| {
                                v.canvas_origin = bounds.origin;
                                v.canvas_width = px_val(bounds.size.width);
                            });
                            bounds
                        }
                    },
                    {
                        let state = state.clone();
                        let view = cx.entity().clone();
                        move |bounds, _bounds_data, window, cx| {
                            let v = view.read(cx);
                            let ts = state.read(cx);
                            let max_cycle = ts.trace.max_cycle();
                            if max_cycle == 0 {
                                return;
                            }
                            let width = px_val(bounds.size.width);
                            let height = px_val(bounds.size.height);

                            // 1. Paint trendline across full width
                            if let Some(counter_idx) = v.selected_counter {
                                if counter_idx < ts.trace.counters.len() {
                                    let bucket_count = (width as usize).max(1);
                                    let data = ts.trace.counter_downsample_minmax(
                                        counter_idx,
                                        0,
                                        max_cycle,
                                        bucket_count,
                                    );
                                    let global_max =
                                        data.iter().map(|(_, mx)| *mx).max().unwrap_or(1).max(1);

                                    for (i, (_min_d, max_d)) in data.iter().enumerate() {
                                        let bar_top = *max_d as f32 / global_max as f32;
                                        if bar_top < 0.01 {
                                            continue;
                                        }
                                        let bar_h = (bar_top * height).max(1.0);
                                        let y_top = height - bar_h;

                                        window.paint_quad(fill(
                                            Bounds::new(
                                                point(
                                                    bounds.origin.x + px(i as f32),
                                                    bounds.origin.y + px(y_top),
                                                ),
                                                size(px(1.0), px(bar_h)),
                                            ),
                                            Hsla { a: 0.15, ..ACCENT },
                                        ));
                                    }
                                }
                            }

                            // 2. Compute viewport rectangle
                            let vp = &ts.viewport;
                            let (vis_start, vis_end) = vp.visible_cycle_range();
                            let vp_left = (vis_start as f64 / max_cycle as f64) as f32 * width;
                            let vp_right = (vis_end as f64 / max_cycle as f64) as f32 * width;
                            let vp_width = (vp_right - vp_left).max(MIN_VIEWPORT_PX);

                            // 3. Dimmed overlays outside viewport
                            let dim_color = Hsla {
                                h: 0.0,
                                s: 0.0,
                                l: 0.0,
                                a: 0.5,
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
                                corner_radii: Corners::all(px(0.0)),
                                background: gpui::transparent_black().into(),
                                border_widths: Edges::all(px(1.0)),
                                border_color: Hsla { a: 0.6, ..ACCENT },
                                border_style: BorderStyle::default(),
                            });

                            // 5. Edge handles (rounded pills)
                            let handle_y = bounds.origin.y + px((height - HANDLE_HEIGHT) / 2.0);
                            // Left handle
                            window.paint_quad(PaintQuad {
                                bounds: Bounds::new(
                                    point(
                                        bounds.origin.x + px(vp_left - HANDLE_WIDTH / 2.0),
                                        handle_y,
                                    ),
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
                                    point(
                                        bounds.origin.x
                                            + px(vp_left + vp_width - HANDLE_WIDTH / 2.0),
                                        handle_y,
                                    ),
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
            // Mouse interactions
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, ev: &MouseDownEvent, _window, cx| {
                    let ts = this.state.read(cx);
                    let max_cycle = ts.trace.max_cycle();
                    let vp = &ts.viewport;
                    let (vis_start, vis_end) = vp.visible_cycle_range();

                    let local_x = px_val(ev.position.x - this.canvas_origin.x);
                    let vp_left = this.cycle_to_pixel(vis_start as f64, max_cycle);
                    let vp_right = this.cycle_to_pixel(vis_end as f64, max_cycle);

                    let near_left = (local_x - vp_left).abs() < HANDLE_WIDTH * 2.0;
                    let near_right = (local_x - vp_right).abs() < HANDLE_WIDTH * 2.0;
                    let inside = local_x >= vp_left && local_x <= vp_right;

                    if near_left && !near_right {
                        this.drag_state = Some(MinimapDrag::ResizeLeft {
                            start_mouse_x: local_x,
                        });
                    } else if near_right {
                        this.drag_state = Some(MinimapDrag::ResizeRight {
                            start_mouse_x: local_x,
                        });
                    } else if inside {
                        this.drag_state = Some(MinimapDrag::Pan {
                            start_mouse_x: local_x,
                            start_scroll: vp.scroll_cycle,
                        });
                    } else {
                        let clicked_cycle = this.pixel_to_cycle(local_x, max_cycle);
                        let view_cycles = vp.view_width as f64 / vp.pixels_per_cycle as f64;
                        let new_scroll = clicked_cycle - view_cycles / 2.0;
                        this.state.update(cx, |ts, cx| {
                            ts.viewport.scroll_cycle = new_scroll.max(0.0);
                            ts.viewport.clamp();
                            cx.notify();
                        });
                    }
                }),
            )
            .on_mouse_move(cx.listener(|this, ev: &MouseMoveEvent, _window, cx| {
                let Some(drag) = this.drag_state else {
                    return;
                };
                if ev.pressed_button.is_none() {
                    this.drag_state = None;
                    return;
                }

                let local_x = px_val(ev.position.x - this.canvas_origin.x);
                let ts = this.state.read(cx);
                let max_cycle = ts.trace.max_cycle();

                match drag {
                    MinimapDrag::Pan {
                        start_mouse_x,
                        start_scroll,
                    } => {
                        let dx_cycles = this.pixel_to_cycle(local_x, max_cycle)
                            - this.pixel_to_cycle(start_mouse_x, max_cycle);
                        this.state.update(cx, |ts, cx| {
                            ts.viewport.scroll_cycle = (start_scroll + dx_cycles).max(0.0);
                            ts.viewport.clamp();
                            cx.notify();
                        });
                    }
                    MinimapDrag::ResizeLeft { start_mouse_x } => {
                        let dx_cycles = this.pixel_to_cycle(local_x, max_cycle)
                            - this.pixel_to_cycle(start_mouse_x, max_cycle);
                        let vp = &ts.viewport;
                        let current_end =
                            vp.scroll_cycle + vp.view_width as f64 / vp.pixels_per_cycle as f64;
                        let new_start = (vp.scroll_cycle + dx_cycles).max(0.0);
                        let new_view_cycles = (current_end - new_start).max(10.0);
                        let new_ppc = vp.view_width as f64 / new_view_cycles;
                        this.state.update(cx, |ts, cx| {
                            ts.viewport.scroll_cycle = new_start;
                            ts.viewport.pixels_per_cycle = (new_ppc as f32).clamp(0.01, 500.0);
                            ts.viewport.clamp();
                            cx.notify();
                        });
                        this.drag_state = Some(MinimapDrag::ResizeLeft {
                            start_mouse_x: local_x,
                        });
                    }
                    MinimapDrag::ResizeRight { start_mouse_x } => {
                        let dx_cycles = this.pixel_to_cycle(local_x, max_cycle)
                            - this.pixel_to_cycle(start_mouse_x, max_cycle);
                        let vp = &ts.viewport;
                        let view_cycles = vp.view_width as f64 / vp.pixels_per_cycle as f64;
                        let new_view_cycles = (view_cycles + dx_cycles).max(10.0);
                        let new_ppc = vp.view_width as f64 / new_view_cycles;
                        this.state.update(cx, |ts, cx| {
                            ts.viewport.pixels_per_cycle = (new_ppc as f32).clamp(0.01, 500.0);
                            ts.viewport.clamp();
                            cx.notify();
                        });
                        this.drag_state = Some(MinimapDrag::ResizeRight {
                            start_mouse_x: local_x,
                        });
                    }
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _cx| {
                    this.drag_state = None;
                }),
            )
            .on_scroll_wheel(cx.listener(|this, ev: &ScrollWheelEvent, _window, cx| {
                let local_x = px_val(ev.position.x - this.canvas_origin.x);
                let ts = this.state.read(cx);
                let max_cycle = ts.trace.max_cycle();
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
