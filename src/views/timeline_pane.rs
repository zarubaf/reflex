use gpui::*;

use crate::app::TraceState;
use crate::theme::colors;
use crate::trace::model::RetireStatus;

pub struct TimelinePane {
    state: Entity<TraceState>,
    focus_handle: FocusHandle,
    dragging: bool,
    last_mouse: Option<Point<Pixels>>,
    canvas_origin: Point<Pixels>,
}

impl TimelinePane {
    pub fn new(state: Entity<TraceState>, cx: &mut Context<Self>) -> Self {
        Self {
            state,
            focus_handle: cx.focus_handle(),
            dragging: false,
            last_mouse: None,
            canvas_origin: Point::default(),
        }
    }
}

fn px_val(p: Pixels) -> f32 {
    f32::from(p)
}

impl Render for TimelinePane {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.clone();
        let state_for_scroll = self.state.clone();
        let state_for_down = self.state.clone();
        let state_for_move = self.state.clone();
        let entity_for_prepaint = cx.entity().clone();
        let entity_for_down = cx.entity().clone();
        let entity_for_move = cx.entity().clone();
        let entity_for_up = cx.entity().clone();

        let canvas_origin = self.canvas_origin;

        div()
            .id("timeline-pane")
            .size_full()
            .bg(colors::BG_PRIMARY)
            .track_focus(&self.focus_handle)
            .on_scroll_wheel(move |event: &ScrollWheelEvent, _window, cx| {
                let local_x = px_val(event.position.x) - px_val(canvas_origin.x);
                let local_y = px_val(event.position.y) - px_val(canvas_origin.y);
                state_for_scroll.update(cx, |ts, cx| {
                    let delta = event.delta.pixel_delta(px(20.0));
                    let dy = px_val(delta.y);
                    let dx = px_val(delta.x);
                    if event.modifiers.control {
                        // Ctrl + scroll: immediate zoom both axes.
                        if dy.abs() > 0.5 {
                            let factor = (1.0 + dy * 0.005).clamp(0.8, 1.25);
                            ts.viewport.zoom_both(factor, local_x, local_y);
                        }
                    } else {
                        ts.viewport.pan(dx, dy);
                        // Auto-follow only on vertical scroll, not horizontal pan.
                        if dy.abs() > dx.abs() && dy.abs() > 0.5 {
                            ts.auto_follow();
                        }
                    }
                    cx.notify();
                });
            })
            .on_mouse_down(MouseButton::Left, move |event: &MouseDownEvent, _window, cx| {
                entity_for_down.update(cx, |pane: &mut TimelinePane, _cx| {
                    pane.dragging = true;
                    pane.last_mouse = Some(event.position);
                });
                let local_y = px_val(event.position.y) - px_val(canvas_origin.y);
                state_for_down.update(cx, |ts, cx| {
                    let row = ts.viewport.pixel_to_row(local_y) as usize;
                    if row < ts.trace.row_count() {
                        ts.selected_row = Some(row);
                    }
                    cx.notify();
                });
            })
            .on_mouse_move(move |event: &MouseMoveEvent, _window, cx| {
                let mut drag_delta = None;
                entity_for_move.update(cx, |pane: &mut TimelinePane, _cx| {
                    if pane.dragging {
                        if let Some(last) = pane.last_mouse {
                            let dx = px_val(event.position.x) - px_val(last.x);
                            let dy = px_val(event.position.y) - px_val(last.y);
                            drag_delta = Some((dx, dy));
                        }
                        pane.last_mouse = Some(event.position);
                    }
                });
                if let Some((dx, dy)) = drag_delta {
                    state_for_move.update(cx, |ts, cx| {
                        ts.viewport.pan(dx, dy);
                        cx.notify();
                    });
                }
            })
            .on_mouse_up(MouseButton::Left, move |_event: &MouseUpEvent, _window, cx| {
                entity_for_up.update(cx, |pane: &mut TimelinePane, _cx| {
                    pane.dragging = false;
                    pane.last_mouse = None;
                });
            })
            .child(
                canvas(
                    {
                        let state = state.clone();
                        move |bounds, _window, cx| {
                            let canvas_w = px_val(bounds.size.width);
                            let canvas_h = px_val(bounds.size.height);
                            state.update(cx, |ts, _cx| {
                                ts.viewport.view_width = canvas_w;
                                ts.viewport.view_height = canvas_h;
                            });
                            entity_for_prepaint.update(cx, |pane: &mut TimelinePane, _cx| {
                                pane.canvas_origin = bounds.origin;
                            });
                            bounds
                        }
                    },
                    move |bounds, _bounds_data, window, cx| {
                        state.update(cx, |ts, _cx| {
                            ts.record_frame();
                        });
                        let (viewport, selected_row, trace) = {
                            let ts = state.read(cx);
                            (ts.viewport.clone(), ts.selected_row, ts.trace.clone())
                        };
                        paint_timeline(bounds, &trace, &viewport, selected_row, window, cx);
                    },
                )
                .size_full(),
            )
    }
}

impl Focusable for TimelinePane {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<gpui_component::dock::PanelEvent> for TimelinePane {}

impl gpui_component::dock::Panel for TimelinePane {
    fn panel_name(&self) -> &'static str {
        "TimelinePane"
    }

    fn title(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        "Pipeline"
    }

    fn dump(&self, _cx: &App) -> gpui_component::dock::PanelState {
        gpui_component::dock::PanelState::new(self)
    }
}

fn paint_timeline(
    bounds: Bounds<Pixels>,
    trace: &crate::trace::model::PipelineTrace,
    vp: &crate::interaction::viewport::ViewportState,
    selected_row: Option<usize>,
    window: &mut Window,
    cx: &mut App,
) {
    let canvas_w = px_val(bounds.size.width);
    let canvas_h = px_val(bounds.size.height);

    window.paint_quad(fill(bounds, colors::BG_PRIMARY));

    if trace.row_count() == 0 {
        return;
    }

    let (row_start, row_end) = vp.visible_row_range();
    let (cycle_start, cycle_end) = vp.visible_cycle_range();

    // Limit how many primitives we draw. At extreme zoom-out, skip rows/cols.
    // row_step: draw at most ~canvas_h rows (1 per pixel).
    let visible_rows = row_end.saturating_sub(row_start).max(1);
    let row_step = (visible_rows as f32 / canvas_h.max(1.0)).ceil().max(1.0) as usize;

    // Vertical grid lines — cap at ~200 lines max.
    let grid_interval = adaptive_grid_interval(vp.pixels_per_cycle);
    if grid_interval > 0 {
        let first_grid = (cycle_start / grid_interval) * grid_interval;
        let mut count = 0;
        for c in (first_grid..=cycle_end).step_by(grid_interval as usize) {
            let x = vp.cycle_to_pixel(c as f64);
            if x < 0.0 || x > canvas_w {
                continue;
            }
            let is_major = c % (grid_interval * 5) == 0;
            let color = if is_major {
                colors::GRID_LINE_MAJOR
            } else {
                colors::GRID_LINE
            };
            window.paint_quad(fill(
                Bounds {
                    origin: Point {
                        x: bounds.origin.x + px(x),
                        y: bounds.origin.y,
                    },
                    size: Size {
                        width: px(1.0),
                        height: bounds.size.height,
                    },
                },
                color,
            ));
            count += 1;
            if count > 200 {
                break;
            }
        }
    }

    // Horizontal grid lines — only when row_height >= 4px.
    if vp.row_height >= 4.0 {
        let mut row = row_start;
        while row <= row_end {
            let y = vp.row_to_pixel(row as f64);
            if y >= 0.0 && y <= canvas_h {
                window.paint_quad(fill(
                    Bounds {
                        origin: Point {
                            x: bounds.origin.x,
                            y: bounds.origin.y + px(y),
                        },
                        size: Size {
                            width: bounds.size.width,
                            height: px(1.0),
                        },
                    },
                    colors::GRID_LINE,
                ));
            }
            row += row_step;
        }
    }

    // Selected row.
    if let Some(sel) = selected_row {
        if sel >= row_start && sel < row_end {
            let y = vp.row_to_pixel(sel as f64);
            window.paint_quad(fill(
                Bounds {
                    origin: Point {
                        x: bounds.origin.x,
                        y: bounds.origin.y + px(y),
                    },
                    size: Size {
                        width: bounds.size.width,
                        height: px(vp.row_height.max(1.0)),
                    },
                },
                colors::SELECTION_BG,
            ));
        }
    }

    // Determine LOD mode.
    let detail_mode = vp.row_height >= 3.0; // per-stage rendering
    let padding = if detail_mode { 2.0f32.min(vp.row_height * 0.1) } else { 0.0 };

    // Stage rectangles — with LOD.
    let mut last_pixel_y: i32 = -2; // track last painted pixel row to skip duplicates
    let mut row = row_start;
    while row < row_end && row < trace.row_count() {
        let y = vp.row_to_pixel(row as f64);
        let pixel_y = y as i32;

        // Skip rows that map to the same pixel as the last painted row.
        if !detail_mode && pixel_y == last_pixel_y {
            row += 1;
            continue;
        }
        last_pixel_y = pixel_y;

        let instr = &trace.instructions[row];
        let h = (vp.row_height - padding * 2.0).max(0.5);
        let is_flushed = instr.retire_status == RetireStatus::Flushed;

        if detail_mode {
            // Full detail: single box per stage with centered text.
            for span in trace.stages_for(row) {
                if span.end_cycle < cycle_start || span.start_cycle > cycle_end {
                    continue;
                }

                let x = vp.cycle_to_pixel(span.start_cycle as f64);
                let w = ((span.end_cycle - span.start_cycle) as f32) * vp.pixels_per_cycle;

                if x + w < 0.0 || x > canvas_w {
                    continue;
                }

                let color = if is_flushed {
                    colors::stage_color_flushed(span.stage_name_idx)
                } else {
                    colors::stage_color(span.stage_name_idx)
                };

                window.paint_quad(PaintQuad {
                    bounds: Bounds {
                        origin: Point {
                            x: bounds.origin.x + px(x),
                            y: bounds.origin.y + px(y + padding),
                        },
                        size: Size {
                            width: px(w.max(1.0)),
                            height: px(h),
                        },
                    },
                    corner_radii: Corners::all(px(h.min(2.0))),
                    background: color.into(),
                    border_widths: Edges::default(),
                    border_color: gpui::transparent_black(),
                    border_style: BorderStyle::default(),
                });

                // Stage name text — monospace, sized so 2 chars fit per cycle.
                // Monospace char width ≈ 0.6 * font_size, so for 2 chars:
                // 2 * 0.6 * font_size ≤ pixels_per_cycle → font_size ≤ ppc / 1.2
                let font_size_val = (vp.pixels_per_cycle / 1.4).min(h - 2.0).max(1.0);
                if font_size_val >= 5.0 {
                    let stage_name: SharedString =
                        trace.stage_name(span.stage_name_idx).to_string().into();
                    let font_size = px(font_size_val);
                    let run = TextRun {
                        len: stage_name.len(),
                        font: Font {
                            family: "Menlo".into(),
                            features: Default::default(),
                            fallbacks: None,
                            weight: FontWeight::NORMAL,
                            style: FontStyle::Normal,
                        },
                        color: colors::TEXT_PRIMARY,
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    };
                    let line = window.text_system().shape_line(
                        stage_name,
                        font_size,
                        &[run],
                        None,
                    );
                    let text_width = px_val(line.width);
                    let text_x = x + ((w - text_width) / 2.0).max(1.0);
                    let text_y = y + padding + (h - font_size_val) / 2.0;

                    // Clip text to the stage box bounds.
                    let clip = ContentMask {
                        bounds: Bounds {
                            origin: Point {
                                x: bounds.origin.x + px(x),
                                y: bounds.origin.y + px(y + padding),
                            },
                            size: Size {
                                width: px(w),
                                height: px(h),
                            },
                        },
                    };
                    window.with_content_mask(Some(clip), |window| {
                        let _ = line.paint(
                            Point {
                                x: bounds.origin.x + px(text_x),
                                y: bounds.origin.y + px(text_y),
                            },
                            font_size,
                            window,
                            cx,
                        );
                    });
                }
            }
        } else {
            // Overview mode: draw one consolidated bar per instruction.
            let x = vp.cycle_to_pixel(instr.first_cycle as f64);
            let x_end = vp.cycle_to_pixel(instr.last_cycle as f64);
            let w = (x_end - x).max(1.0);

            if x + w >= 0.0 && x <= canvas_w {
                // Use the first stage's color for the consolidated bar.
                let spans = trace.stages_for(row);
                let color = if !spans.is_empty() {
                    if is_flushed {
                        colors::stage_color_flushed(spans[0].stage_name_idx)
                    } else {
                        colors::stage_color(spans[0].stage_name_idx)
                    }
                } else {
                    colors::GRID_LINE
                };

                window.paint_quad(fill(
                    Bounds {
                        origin: Point {
                            x: bounds.origin.x + px(x),
                            y: bounds.origin.y + px(y),
                        },
                        size: Size {
                            width: px(w),
                            height: px(vp.row_height.max(0.5)),
                        },
                    },
                    color,
                ));
            }
        }

        row += 1;
    }
}

fn adaptive_grid_interval(pixels_per_cycle: f32) -> u32 {
    let target_spacing = 80.0;
    let raw = target_spacing / pixels_per_cycle;
    let candidates = [1, 2, 5, 10, 20, 50, 100, 200, 500, 1000, 2000, 5000, 10000];
    for &c in &candidates {
        if c as f32 >= raw {
            return c;
        }
    }
    10000
}
