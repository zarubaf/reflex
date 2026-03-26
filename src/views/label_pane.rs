use std::sync::Arc;

use gpui::*;

use crate::app::{TooltipHover, TraceState};
use crate::theme::colors;
use crate::trace::model::RetireStatus;
use crate::views::status_bar::fmt_num;
use crate::views::timeline_pane::header_height;

pub struct LabelPane {
    state: Entity<TraceState>,
    focus_handle: FocusHandle,
    canvas_origin: Point<Pixels>,
    /// Row currently under the mouse cursor.
    hovered_row: Option<usize>,
}

impl LabelPane {
    pub fn new(state: Entity<TraceState>, cx: &mut Context<Self>) -> Self {
        Self {
            state,
            focus_handle: cx.focus_handle(),
            canvas_origin: Point::default(),
            hovered_row: None,
        }
    }
}

fn px_val(p: Pixels) -> f32 {
    f32::from(p)
}

impl Render for LabelPane {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.clone();
        let state_for_click = self.state.clone();
        let state_for_scroll = self.state.clone();
        let state_for_hover = self.state.clone();
        let state_for_leave = self.state.clone();
        let entity_for_prepaint = cx.entity().clone();
        let entity_for_hover = cx.entity().clone();
        let entity_for_leave = cx.entity().clone();
        let canvas_origin = self.canvas_origin;
        let hdr_h = header_height(self.state.read(cx).cursor_state.cursors.len());

        div()
            .id("label-pane")
            .bg(colors::BG_SECONDARY)
            .size_full()
            .overflow_hidden()
            .track_focus(&self.focus_handle)
            .on_scroll_wheel(move |event: &ScrollWheelEvent, _window, cx| {
                cx.stop_propagation();
                state_for_scroll.update(cx, |ts, cx| {
                    let delta = event.delta.pixel_delta(px(20.0));
                    let dy = px_val(delta.y);
                    if event.modifiers.control {
                        if dy.abs() > 0.5 {
                            let factor = (1.0 + dy * 0.005).clamp(0.8, 1.25);
                            ts.viewport.zoom_both(factor, 0.0, 0.0);
                        }
                    } else {
                        ts.viewport.pan(0.0, dy);
                        if dy.abs() > 0.5 {
                            ts.auto_follow();
                        }
                    }
                    cx.notify();
                });
            })
            .on_mouse_down(
                MouseButton::Left,
                move |event: &MouseDownEvent, _window, cx| {
                    let local_y = px_val(event.position.y) - px_val(canvas_origin.y) - hdr_h;
                    state_for_click.update(cx, |ts, cx| {
                        let row = ts.viewport.pixel_to_row(local_y) as usize;
                        if row < ts.trace.row_count() {
                            ts.selected_row = Some(row);
                        }
                        cx.notify();
                    });
                },
            )
            .on_mouse_move(move |event: &MouseMoveEvent, _window, cx| {
                let local_y = px_val(event.position.y) - px_val(canvas_origin.y) - hdr_h;
                let (row, max_row) = {
                    let ts = state_for_hover.read(cx);
                    (
                        ts.viewport.pixel_to_row(local_y) as usize,
                        ts.trace.row_count(),
                    )
                };
                let new_row = if row < max_row { Some(row) } else { None };

                // Build tooltip hover info.
                let tooltip = new_row.and_then(|r| {
                    let ts = state_for_hover.read(cx);
                    let instr = &ts.trace.instructions[r];
                    if !instr.tooltip.is_empty() {
                        Some(TooltipHover {
                            text: instr.tooltip.clone(),
                            position: event.position,
                        })
                    } else {
                        None
                    }
                });

                entity_for_hover.update(cx, |pane: &mut LabelPane, _cx| {
                    pane.hovered_row = new_row;
                });
                state_for_hover.update(cx, |ts, cx| {
                    ts.tooltip_hover = tooltip;
                    cx.notify();
                });
            })
            .on_hover(move |hovered: &bool, _window, cx| {
                if !hovered {
                    entity_for_leave.update(cx, |pane: &mut LabelPane, _cx| {
                        pane.hovered_row = None;
                    });
                    state_for_leave.update(cx, |ts, cx| {
                        if ts.tooltip_hover.is_some() {
                            ts.tooltip_hover = None;
                            cx.notify();
                        }
                    });
                }
            })
            .child(
                canvas(
                    {
                        let state_pre = state.clone();
                        move |bounds, _window, cx| {
                            entity_for_prepaint.update(cx, |pane: &mut LabelPane, _cx| {
                                pane.canvas_origin = bounds.origin;
                            });
                            state_pre.update(cx, |ts, _cx| {
                                ts.viewport.view_height =
                                    (px_val(bounds.size.height) - hdr_h).max(0.0);
                            });
                            bounds
                        }
                    },
                    move |bounds, _bounds_data, window, cx| {
                        let (viewport, selected_row, trace, trace_summary) = {
                            let ts = state.read(cx);
                            (
                                ts.viewport.clone(),
                                ts.selected_row,
                                Arc::clone(&ts.trace),
                                ts.trace_summary.clone(),
                            )
                        };
                        paint_labels(
                            bounds,
                            &trace,
                            &viewport,
                            selected_row,
                            trace_summary.as_ref(),
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

#[allow(clippy::too_many_arguments)]
fn paint_labels(
    bounds: Bounds<Pixels>,
    trace: &crate::trace::model::PipelineTrace,
    vp: &crate::interaction::viewport::ViewportState,
    selected_row: Option<usize>,
    trace_summary: Option<&uscope::summary::TraceSummary>,
    hdr_h: f32,
    window: &mut Window,
    cx: &mut App,
) {
    let canvas_h = px_val(bounds.size.height);
    let content_h = (canvas_h - hdr_h).max(0.0);

    window.paint_quad(fill(bounds, colors::BG_SECONDARY));

    // Header-aligned top strip (empty space matching the timeline header).
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

    if trace.row_count() == 0 {
        return;
    }

    let content_origin_y = bounds.origin.y + px(hdr_h);

    // Clip label content to the area below the header.
    let content_clip = ContentMask {
        bounds: Bounds {
            origin: Point {
                x: bounds.origin.x,
                y: content_origin_y,
            },
            size: Size {
                width: bounds.size.width,
                height: px(content_h),
            },
        },
    };

    // Compute row label column width from the widest possible row number.
    let max_row_num = trace.total_instruction_count.max(trace.row_count());
    let max_row_str = fmt_num(max_row_num);
    // Approximate width: ~7px per char at 9pt Menlo + 8px padding.
    let row_label_w = (max_row_str.len() as f32 * 7.0 + 8.0).max(44.0);

    window.with_content_mask(Some(content_clip), |window| {
        let (row_start, row_end) = vp.visible_row_range();

        let mut last_pixel_y: i32 = -2;
        let mut row = row_start;
        while row < row_end && row < trace.row_count() {
            let y = vp.row_to_pixel(row as f64);
            if y + vp.row_height < 0.0 || y > content_h {
                row += 1;
                continue;
            }

            let pixel_y = y as i32;
            if vp.row_height < 1.0 && pixel_y == last_pixel_y {
                row += 1;
                continue;
            }
            last_pixel_y = pixel_y;

            let instr = &trace.instructions[row];
            let is_flushed = instr.retire_status == RetireStatus::Flushed;
            let is_selected = selected_row == Some(row);

            if is_selected {
                window.paint_quad(fill(
                    Bounds {
                        origin: Point {
                            x: bounds.origin.x,
                            y: content_origin_y + px(y),
                        },
                        size: Size {
                            width: bounds.size.width,
                            height: px(vp.row_height.max(1.0)),
                        },
                    },
                    colors::SELECTION_BG,
                ));
            }

            if vp.row_height < 8.0 {
                row += 1;
                continue;
            }

            let text_color = if is_flushed {
                colors::TEXT_DIMMED
            } else {
                colors::TEXT_PRIMARY
            };

            let font_size = px((vp.row_height - 4.0).clamp(6.0, 12.0));
            let text_y = y + (vp.row_height - px_val(font_size)) / 2.0;

            // Show global instruction index if density mipmap is available,
            // otherwise show loaded-array index.
            let global_row = if let Some(summary) = trace_summary {
                summary.cycle_to_row(instr.first_cycle)
            } else {
                row
            };
            let row_str: SharedString = fmt_num(global_row).into();
            let row_run = TextRun {
                len: row_str.len(),
                font: Font {
                    family: "Menlo".into(),
                    features: Default::default(),
                    fallbacks: None,
                    weight: FontWeight::NORMAL,
                    style: FontStyle::Normal,
                },
                color: colors::TEXT_ROW_NUMBER,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let row_line = window
                .text_system()
                .shape_line(row_str, font_size, &[row_run], None);
            let _ = row_line.paint(
                Point {
                    x: bounds.origin.x + px(4.0),
                    y: content_origin_y + px(text_y),
                },
                font_size,
                window,
                cx,
            );

            let disasm: SharedString = instr.disasm.clone().into();
            let disasm_run = TextRun {
                len: disasm.len(),
                font: Font {
                    family: "Menlo".into(),
                    features: Default::default(),
                    fallbacks: None,
                    weight: FontWeight::NORMAL,
                    style: FontStyle::Normal,
                },
                color: text_color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let disasm_line =
                window
                    .text_system()
                    .shape_line(disasm, font_size, &[disasm_run], None);
            let _ = disasm_line.paint(
                Point {
                    x: bounds.origin.x + px(row_label_w),
                    y: content_origin_y + px(text_y),
                },
                font_size,
                window,
                cx,
            );

            row += 1;
        }
    }); // end content_clip
}

impl Focusable for LabelPane {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<gpui_component::dock::PanelEvent> for LabelPane {}

impl gpui_component::dock::Panel for LabelPane {
    fn panel_name(&self) -> &'static str {
        "LabelPane"
    }

    fn title(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        "Instructions"
    }

    fn dump(&self, _cx: &App) -> gpui_component::dock::PanelState {
        gpui_component::dock::PanelState::new(self)
    }
}
