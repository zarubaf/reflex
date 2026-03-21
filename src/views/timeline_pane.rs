use gpui::*;

use crate::app::{CursorState, TraceState};
use crate::theme::colors;
use crate::trace::model::RetireStatus;

/// Base header height (ruler + cursor heads, no delta lines).
const HEADER_BASE: f32 = 20.0;
/// Vertical position where cursor heads sit.
const HEAD_TOP: f32 = 1.0;
/// Height of cursor head badges.
const HEAD_H: f32 = 16.0;
/// Height of each delta lane.
const DELTA_LANE_H: f32 = 12.0;
/// Maximum number of stacking delta lanes before wrapping.
const MAX_DELTA_LANES: usize = 5;
/// Minimum width of a cursor head (when label is short).
const CURSOR_HEAD_MIN_W: f32 = 14.0;
/// Horizontal padding inside cursor head label.
const CURSOR_HEAD_PAD: f32 = 4.0;

/// Compute header height based on the number of delta lines needed.
pub fn header_height(n_cursors: usize) -> f32 {
    if n_cursors <= 1 {
        HEADER_BASE
    } else {
        let lanes = (n_cursors - 1).min(MAX_DELTA_LANES);
        HEADER_BASE + lanes as f32 * DELTA_LANE_H
    }
}

pub struct TimelinePane {
    state: Entity<TraceState>,
    focus_handle: FocusHandle,
    dragging: bool,
    dragging_cursor: Option<usize>,
    last_mouse: Option<Point<Pixels>>,
    canvas_origin: Point<Pixels>,
}

impl TimelinePane {
    pub fn new(state: Entity<TraceState>, cx: &mut Context<Self>) -> Self {
        Self {
            state,
            focus_handle: cx.focus_handle(),
            dragging: false,
            dragging_cursor: None,
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
        let n_cursors = self.state.read(cx).cursor_state.cursors.len();
        let hdr_h = header_height(n_cursors);

        div()
            .id("timeline-pane")
            .size_full()
            .bg(colors::BG_PRIMARY)
            .track_focus(&self.focus_handle)
            .on_scroll_wheel(move |event: &ScrollWheelEvent, _window, cx| {
                cx.stop_propagation();
                let local_x = px_val(event.position.x) - px_val(canvas_origin.x);
                let local_y = px_val(event.position.y) - px_val(canvas_origin.y) - hdr_h;
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
            .on_mouse_down(
                MouseButton::Left,
                move |event: &MouseDownEvent, window, cx| {
                    let local_x = px_val(event.position.x) - px_val(canvas_origin.x);
                    let local_y = px_val(event.position.y) - px_val(canvas_origin.y);

                    // Check if click is in the header area (cursor head zone).
                    let hit_cursor = if local_y <= hdr_h {
                        let ts = state_for_down.read(cx);
                        hit_test_cursor_head(local_x, &ts.cursor_state, &ts.viewport, window)
                    } else {
                        None
                    };

                    if let Some(cursor_idx) = hit_cursor {
                        // Activate and start dragging that cursor.
                        entity_for_down.update(cx, |pane: &mut TimelinePane, _cx| {
                            pane.dragging_cursor = Some(cursor_idx);
                            pane.last_mouse = Some(event.position);
                        });
                        state_for_down.update(cx, |ts, cx| {
                            ts.cursor_state.active_idx = cursor_idx;
                            cx.notify();
                        });
                    } else {
                        // Normal click: select row + move active cursor.
                        entity_for_down.update(cx, |pane: &mut TimelinePane, _cx| {
                            pane.dragging = true;
                            pane.last_mouse = Some(event.position);
                        });
                        // Content area is offset by hdr_h.
                        let content_y = local_y - hdr_h;
                        state_for_down.update(cx, |ts, cx| {
                            let row = ts.viewport.pixel_to_row(content_y) as usize;
                            if row < ts.trace.row_count() {
                                ts.selected_row = Some(row);
                            }
                            let cycle = ts.viewport.pixel_to_cycle(local_x).round();
                            if !ts.cursor_state.cursors.is_empty() {
                                ts.cursor_state.cursors[ts.cursor_state.active_idx].cycle = cycle;
                            }
                            cx.notify();
                        });
                    }
                },
            )
            .on_mouse_move(move |event: &MouseMoveEvent, _window, cx| {
                let mut drag_delta = None;
                let mut cursor_drag_idx = None;
                entity_for_move.update(cx, |pane: &mut TimelinePane, _cx| {
                    if let Some(idx) = pane.dragging_cursor {
                        cursor_drag_idx = Some(idx);
                        pane.last_mouse = Some(event.position);
                    } else if pane.dragging {
                        if let Some(last) = pane.last_mouse {
                            let dx = px_val(event.position.x) - px_val(last.x);
                            let dy = px_val(event.position.y) - px_val(last.y);
                            drag_delta = Some((dx, dy));
                        }
                        pane.last_mouse = Some(event.position);
                    }
                });
                if let Some(idx) = cursor_drag_idx {
                    let local_x = px_val(event.position.x) - px_val(canvas_origin.x);
                    state_for_move.update(cx, |ts, cx| {
                        let cycle = ts.viewport.pixel_to_cycle(local_x).round();
                        if idx < ts.cursor_state.cursors.len() {
                            ts.cursor_state.cursors[idx].cycle = cycle;
                        }
                        cx.notify();
                    });
                } else if let Some((dx, dy)) = drag_delta {
                    state_for_move.update(cx, |ts, cx| {
                        ts.viewport.pan(dx, dy);
                        cx.notify();
                    });
                }
            })
            .on_mouse_up(
                MouseButton::Left,
                move |_event: &MouseUpEvent, _window, cx| {
                    entity_for_up.update(cx, |pane: &mut TimelinePane, _cx| {
                        pane.dragging = false;
                        pane.dragging_cursor = None;
                        pane.last_mouse = None;
                    });
                },
            )
            .child(
                canvas(
                    {
                        let state = state.clone();
                        move |bounds, _window, cx| {
                            let canvas_w = px_val(bounds.size.width);
                            let canvas_h = px_val(bounds.size.height);
                            state.update(cx, |ts, _cx| {
                                ts.viewport.view_width = canvas_w;
                                // Content area excludes the header strip.
                                ts.viewport.view_height = (canvas_h - hdr_h).max(0.0);
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
                        let (viewport, selected_row, trace, cursor_state) = {
                            let ts = state.read(cx);
                            (
                                ts.viewport.clone(),
                                ts.selected_row,
                                ts.trace.clone(),
                                ts.cursor_state.clone(),
                            )
                        };
                        paint_timeline(
                            bounds,
                            &trace,
                            &viewport,
                            selected_row,
                            &cursor_state,
                            hdr_h,
                            window,
                            cx,
                        );
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

#[allow(clippy::too_many_arguments)]
fn paint_timeline(
    bounds: Bounds<Pixels>,
    trace: &crate::trace::model::PipelineTrace,
    vp: &crate::interaction::viewport::ViewportState,
    selected_row: Option<usize>,
    cursor_state: &CursorState,
    hdr_h: f32,
    window: &mut Window,
    cx: &mut App,
) {
    let canvas_w = px_val(bounds.size.width);
    let canvas_h = px_val(bounds.size.height);

    // Content area starts below the header.
    let content_origin = Point {
        x: bounds.origin.x,
        y: bounds.origin.y + px(hdr_h),
    };
    let content_h = (canvas_h - hdr_h).max(0.0);

    window.paint_quad(fill(bounds, colors::BG_PRIMARY));

    // ─── Timeline header (cycle ruler) ──────────────────────────────────
    // Header background — slightly different shade.
    window.paint_quad(fill(
        Bounds {
            origin: bounds.origin,
            size: Size {
                width: bounds.size.width,
                height: px(hdr_h),
            },
        },
        colors::BG_SECONDARY,
    ));
    // Header bottom border.
    window.paint_quad(fill(
        Bounds {
            origin: Point {
                x: bounds.origin.x,
                y: bounds.origin.y + px(hdr_h - 1.0),
            },
            size: Size {
                width: bounds.size.width,
                height: px(1.0),
            },
        },
        colors::GRID_LINE_MAJOR,
    ));

    let (cycle_start, cycle_end) = vp.visible_cycle_range();
    let grid_interval = adaptive_grid_interval(vp.pixels_per_cycle);

    // Ruler tick marks + cycle numbers in header.
    if grid_interval > 0 {
        let first_grid = (cycle_start / grid_interval) * grid_interval;
        let mut count = 0;
        for c in (first_grid..=cycle_end).step_by(grid_interval as usize) {
            let x = vp.cycle_to_pixel(c as f64);
            if x < 0.0 || x > canvas_w {
                continue;
            }
            let is_major = c % (grid_interval * 5) == 0;

            // Tick mark in header.
            let tick_h = if is_major { 8.0 } else { 4.0 };
            let tick_color = if is_major {
                colors::GRID_LINE_MAJOR
            } else {
                colors::GRID_LINE
            };
            window.paint_quad(fill(
                Bounds {
                    origin: Point {
                        x: bounds.origin.x + px(x),
                        y: bounds.origin.y + px(hdr_h - tick_h),
                    },
                    size: Size {
                        width: px(1.0),
                        height: px(tick_h),
                    },
                },
                tick_color,
            ));

            // Cycle number label on major ticks.
            if is_major {
                let label: SharedString = format!("{}", c).into();
                let font_size = px(9.0);
                let run = TextRun {
                    len: label.len(),
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
                let line = window
                    .text_system()
                    .shape_line(label, font_size, &[run], None);
                let tw = px_val(line.width);
                let text_x = x - tw / 2.0;
                let clip = ContentMask {
                    bounds: Bounds {
                        origin: bounds.origin,
                        size: Size {
                            width: bounds.size.width,
                            height: px(hdr_h),
                        },
                    },
                };
                window.with_content_mask(Some(clip), |window| {
                    let _ = line.paint(
                        Point {
                            x: bounds.origin.x + px(text_x),
                            y: bounds.origin.y + px(1.0),
                        },
                        font_size,
                        window,
                        cx,
                    );
                });
            }

            count += 1;
            if count > 200 {
                break;
            }
        }
    }

    // Content mask: clip everything below to the content area (below header).
    let content_clip = ContentMask {
        bounds: Bounds {
            origin: content_origin,
            size: Size {
                width: bounds.size.width,
                height: px(content_h),
            },
        },
    };

    window.with_content_mask(Some(content_clip), |window| {
        if trace.row_count() == 0 {
            return;
        }

        let (row_start, row_end) = vp.visible_row_range();

        // Limit how many primitives we draw. At extreme zoom-out, skip rows/cols.
        let visible_rows = row_end.saturating_sub(row_start).max(1);
        let row_step = (visible_rows as f32 / content_h.max(1.0)).ceil().max(1.0) as usize;

        // Vertical grid lines in content area.
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
                            x: content_origin.x + px(x),
                            y: content_origin.y,
                        },
                        size: Size {
                            width: px(1.0),
                            height: px(content_h),
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
                if y >= 0.0 && y <= content_h {
                    window.paint_quad(fill(
                        Bounds {
                            origin: Point {
                                x: content_origin.x,
                                y: content_origin.y + px(y),
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
                            x: content_origin.x,
                            y: content_origin.y + px(y),
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
        let detail_mode = vp.row_height >= 3.0;
        let padding = if detail_mode {
            2.0f32.min(vp.row_height * 0.1)
        } else {
            0.0
        };

        // Stage rectangles — with LOD.
        let mut last_pixel_y: i32 = -2;
        let mut row = row_start;
        while row < row_end && row < trace.row_count() {
            let y = vp.row_to_pixel(row as f64);
            let pixel_y = y as i32;

            if !detail_mode && pixel_y == last_pixel_y {
                row += 1;
                continue;
            }
            last_pixel_y = pixel_y;

            let instr = &trace.instructions[row];
            let h = (vp.row_height - padding * 2.0).max(0.5);
            let is_flushed = instr.retire_status == RetireStatus::Flushed;

            if detail_mode {
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
                                x: content_origin.x + px(x),
                                y: content_origin.y + px(y + padding),
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
                        let line =
                            window
                                .text_system()
                                .shape_line(stage_name, font_size, &[run], None);
                        let text_width = px_val(line.width);
                        let text_x = x + ((w - text_width) / 2.0).max(1.0);
                        let text_y = y + padding + (h - font_size_val) / 2.0;

                        let clip = ContentMask {
                            bounds: Bounds {
                                origin: Point {
                                    x: content_origin.x + px(x),
                                    y: content_origin.y + px(y + padding),
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
                                    x: content_origin.x + px(text_x),
                                    y: content_origin.y + px(text_y),
                                },
                                font_size,
                                window,
                                cx,
                            );
                        });
                    }
                }
            } else {
                let x = vp.cycle_to_pixel(instr.first_cycle as f64);
                let x_end = vp.cycle_to_pixel(instr.last_cycle as f64);
                let w = (x_end - x).max(1.0);

                if x + w >= 0.0 && x <= canvas_w {
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
                                x: content_origin.x + px(x),
                                y: content_origin.y + px(y),
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

        // Cursor lines (clipped to content area).
        for (i, cursor) in cursor_state.cursors.iter().enumerate() {
            let is_active = i == cursor_state.active_idx;
            let x = vp.cycle_to_pixel(cursor.cycle);
            if x < -10.0 || x > canvas_w + 10.0 {
                continue;
            }
            let color = if is_active {
                colors::cursor_color(cursor.color_idx)
            } else {
                colors::cursor_color_inactive(cursor.color_idx)
            };
            let line_width = if is_active { 2.0 } else { 1.0 };
            window.paint_quad(fill(
                Bounds {
                    origin: Point {
                        x: content_origin.x + px(x - line_width / 2.0),
                        y: content_origin.y,
                    },
                    size: Size {
                        width: px(line_width),
                        height: px(content_h),
                    },
                },
                color,
            ));
        }
    });

    // ─── Cursor heads (painted in header, on top of everything) ──────────
    paint_cursor_heads(bounds, canvas_w, vp, cursor_state, hdr_h, window, cx);
}

/// Paint cursor heads and delta measurement lines in the header strip.
#[allow(clippy::too_many_arguments)]
fn paint_cursor_heads(
    bounds: Bounds<Pixels>,
    canvas_w: f32,
    vp: &crate::interaction::viewport::ViewportState,
    cursor_state: &CursorState,
    hdr_h: f32,
    window: &mut Window,
    cx: &mut App,
) {
    let head_font_size = 9.0_f32;

    // ─── Delta measurement lines (between active cursor and each other) ──
    if cursor_state.cursors.len() >= 2 {
        let active = &cursor_state.cursors[cursor_state.active_idx];
        let active_x = vp.cycle_to_pixel(active.cycle);
        let active_color = colors::cursor_color(active.color_idx);

        let mut lane_idx = 0usize;
        for (i, cursor) in cursor_state.cursors.iter().enumerate() {
            if i == cursor_state.active_idx {
                continue;
            }
            let other_x = vp.cycle_to_pixel(cursor.cycle);
            let left_x = active_x.min(other_x);
            let right_x = active_x.max(other_x);

            // Skip if entirely off-screen.
            if right_x < 0.0 || left_x > canvas_w {
                lane_idx += 1;
                continue;
            }

            let line_color = Hsla {
                a: 0.5,
                ..active_color
            };
            // Stack delta lines in lanes below the cursor heads.
            let lane = lane_idx % MAX_DELTA_LANES;
            let line_y = HEADER_BASE + lane as f32 * DELTA_LANE_H + DELTA_LANE_H / 2.0;
            lane_idx += 1;

            // Horizontal connecting line (clamped to visible area).
            let draw_left = left_x.max(0.0);
            let draw_right = right_x.min(canvas_w);
            if draw_right > draw_left {
                window.paint_quad(fill(
                    Bounds {
                        origin: Point {
                            x: bounds.origin.x + px(draw_left),
                            y: bounds.origin.y + px(line_y),
                        },
                        size: Size {
                            width: px((draw_right - draw_left).max(1.0)),
                            height: px(1.0),
                        },
                    },
                    line_color,
                ));
            }

            // Small vertical end caps (2px tall).
            for &cap_x in &[left_x, right_x] {
                if cap_x >= 0.0 && cap_x <= canvas_w {
                    window.paint_quad(fill(
                        Bounds {
                            origin: Point {
                                x: bounds.origin.x + px(cap_x - 0.5),
                                y: bounds.origin.y + px(line_y - 2.0),
                            },
                            size: Size {
                                width: px(1.0),
                                height: px(5.0),
                            },
                        },
                        line_color,
                    ));
                }
            }

            // Delta label pill centered on the line.
            let delta = (cursor.cycle - active.cycle).abs();
            let delta_label: SharedString = format!("{:.0}", delta).into();
            let delta_font_size = px(8.0);
            let delta_run = TextRun {
                len: delta_label.len(),
                font: Font {
                    family: "Menlo".into(),
                    features: Default::default(),
                    fallbacks: None,
                    weight: FontWeight::BOLD,
                    style: FontStyle::Normal,
                },
                color: colors::TEXT_PRIMARY,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let delta_shaped =
                window
                    .text_system()
                    .shape_line(delta_label, delta_font_size, &[delta_run], None);
            let delta_tw = px_val(delta_shaped.width);
            let pill_w = delta_tw + 6.0;
            let pill_h = 10.0;
            // Clamp mid_x to the visible viewport so the pill stays on-screen
            // even when one cursor is off-viewport.
            let vis_left = left_x.max(0.0);
            let vis_right = right_x.min(canvas_w);
            let mid_x = ((vis_left + vis_right) / 2.0)
                .max(pill_w / 2.0 + 2.0)
                .min(canvas_w - pill_w / 2.0 - 2.0);
            let pill_x = mid_x - pill_w / 2.0;
            let pill_y = line_y - pill_h / 2.0;

            // Only show label if there's enough room (pill fits between cursors).
            if pill_w + 4.0 < (right_x - left_x) {
                // Pill background.
                window.paint_quad(PaintQuad {
                    bounds: Bounds {
                        origin: Point {
                            x: bounds.origin.x + px(pill_x),
                            y: bounds.origin.y + px(pill_y),
                        },
                        size: Size {
                            width: px(pill_w),
                            height: px(pill_h),
                        },
                    },
                    corner_radii: Corners::all(px(3.0)),
                    background: colors::BG_SECONDARY.into(),
                    border_widths: Edges::all(px(1.0)),
                    border_color: line_color,
                    border_style: BorderStyle::default(),
                });

                // Delta text.
                let clip = ContentMask {
                    bounds: Bounds {
                        origin: Point {
                            x: bounds.origin.x + px(pill_x),
                            y: bounds.origin.y + px(pill_y),
                        },
                        size: Size {
                            width: px(pill_w),
                            height: px(pill_h),
                        },
                    },
                };
                window.with_content_mask(Some(clip), |window| {
                    let _ = delta_shaped.paint(
                        Point {
                            x: bounds.origin.x + px(mid_x - delta_tw / 2.0),
                            y: bounds.origin.y + px(pill_y + 1.0),
                        },
                        delta_font_size,
                        window,
                        cx,
                    );
                });
            }
        }
    }

    // ─── Cursor heads ────────────────────────────────────────────────────
    for (i, cursor) in cursor_state.cursors.iter().enumerate() {
        let is_active = i == cursor_state.active_idx;
        let x = vp.cycle_to_pixel(cursor.cycle);

        let color = if is_active {
            colors::cursor_color(cursor.color_idx)
        } else {
            colors::cursor_color_inactive(cursor.color_idx)
        };

        // Measure the head label to determine head width.
        let label: SharedString = format!("{:.0}", cursor.cycle).into();
        let font_size = px(head_font_size);
        let run = TextRun {
            len: label.len(),
            font: Font {
                family: "Menlo".into(),
                features: Default::default(),
                fallbacks: None,
                weight: FontWeight::BOLD,
                style: FontStyle::Normal,
            },
            color: Hsla {
                h: 0.0,
                s: 0.0,
                l: 0.0,
                a: 0.85,
            },
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let shaped = window
            .text_system()
            .shape_line(label, font_size, &[run], None);
        let label_w = px_val(shaped.width);
        let head_w = (label_w + CURSOR_HEAD_PAD * 2.0).max(CURSOR_HEAD_MIN_W);
        let head_x = x - head_w / 2.0;

        if x + head_w / 2.0 < 0.0 || x - head_w / 2.0 > canvas_w {
            continue;
        }

        // Thin line through the header.
        window.paint_quad(fill(
            Bounds {
                origin: Point {
                    x: bounds.origin.x + px(x - 0.5),
                    y: bounds.origin.y,
                },
                size: Size {
                    width: px(1.0),
                    height: px(hdr_h),
                },
            },
            Hsla {
                a: color.a * 0.3,
                ..color
            },
        ));

        // Cursor head — rounded rect.
        window.paint_quad(PaintQuad {
            bounds: Bounds {
                origin: Point {
                    x: bounds.origin.x + px(head_x),
                    y: bounds.origin.y + px(HEAD_TOP),
                },
                size: Size {
                    width: px(head_w),
                    height: px(HEAD_H),
                },
            },
            corner_radii: Corners::all(px(3.0)),
            background: color.into(),
            border_widths: Edges::default(),
            border_color: gpui::transparent_black(),
            border_style: BorderStyle::default(),
        });

        // Cycle label centered in the head.
        let text_x = x - label_w / 2.0;
        let text_y = HEAD_TOP + (HEAD_H - head_font_size) / 2.0;
        let clip = ContentMask {
            bounds: Bounds {
                origin: Point {
                    x: bounds.origin.x + px(head_x),
                    y: bounds.origin.y + px(HEAD_TOP),
                },
                size: Size {
                    width: px(head_w),
                    height: px(HEAD_H),
                },
            },
        };
        window.with_content_mask(Some(clip), |window| {
            let _ = shaped.paint(
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

/// Hit-test cursor heads in the header. Returns the cursor index if clicked.
fn hit_test_cursor_head(
    local_x: f32,
    cursor_state: &CursorState,
    vp: &crate::interaction::viewport::ViewportState,
    window: &mut Window,
) -> Option<usize> {
    let head_font_size = 9.0_f32;
    for (i, cursor) in cursor_state.cursors.iter().enumerate() {
        let cx = vp.cycle_to_pixel(cursor.cycle);
        let label: SharedString = format!("{:.0}", cursor.cycle).into();
        let font_size = px(head_font_size);
        let run = TextRun {
            len: label.len(),
            font: Font {
                family: "Menlo".into(),
                features: Default::default(),
                fallbacks: None,
                weight: FontWeight::BOLD,
                style: FontStyle::Normal,
            },
            color: Hsla {
                h: 0.0,
                s: 0.0,
                l: 0.0,
                a: 0.85,
            },
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let shaped = window
            .text_system()
            .shape_line(label, font_size, &[run], None);
        let label_w = px_val(shaped.width);
        let head_w = (label_w + CURSOR_HEAD_PAD * 2.0).max(CURSOR_HEAD_MIN_W);
        let head_x = cx - head_w / 2.0;
        if local_x >= head_x && local_x <= head_x + head_w {
            return Some(i);
        }
    }
    None
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
