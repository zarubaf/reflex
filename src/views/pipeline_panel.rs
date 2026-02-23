use gpui::*;

use crate::app::TraceState;
use crate::theme::colors;
use crate::views::label_pane::LabelPane;
use crate::views::timeline_pane::TimelinePane;

/// Orchestrates the label pane (left) + timeline pane (right) with a resizable split.
pub struct PipelinePanel {
    label_pane: Entity<LabelPane>,
    timeline_pane: Entity<TimelinePane>,
    focus_handle: FocusHandle,
    label_width: f32,
    dragging_splitter: bool,
}

impl PipelinePanel {
    pub fn new(
        state: Entity<TraceState>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let label_pane = cx.new(|cx| LabelPane::new(state.clone(), cx));
        let timeline_pane = cx.new(|cx| TimelinePane::new(state.clone(), cx));

        // Estimate label width from the longest disasm string.
        // Row number column ~44px + text at ~7.2px per char (Menlo 12px) + padding.
        let label_width = {
            let ts = state.read(cx);
            let max_disasm_len = ts
                .trace
                .instructions
                .iter()
                .take(100)
                .map(|i| i.disasm.len())
                .max()
                .unwrap_or(0);
            let row_digits = if ts.trace.row_count() > 0 {
                (ts.trace.row_count() as f32).log10().floor() as usize + 1
            } else {
                1
            };
            let row_col_width = (row_digits as f32) * 7.2 + 12.0;
            let text_width = (max_disasm_len as f32) * 7.2;
            (row_col_width + text_width + 16.0).clamp(120.0, 600.0)
        };

        Self {
            label_pane,
            timeline_pane,
            focus_handle: cx.focus_handle(),
            label_width,
            dragging_splitter: false,
        }
    }
}

impl Render for PipelinePanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let label_width = self.label_width;
        let entity = cx.entity().clone();
        let entity_for_move = cx.entity().clone();
        let entity_for_up = cx.entity().clone();
        div()
            .id("pipeline-panel")
            .size_full()
            .flex()
            .flex_row()
            .track_focus(&self.focus_handle)
            .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                entity_for_up.update(cx, |panel: &mut PipelinePanel, _| {
                    panel.dragging_splitter = false;
                });
            })
            .on_mouse_move(move |event: &MouseMoveEvent, _, cx| {
                entity_for_move.update(cx, |panel: &mut PipelinePanel, cx| {
                    if panel.dragging_splitter {
                        panel.label_width = f32::from(event.position.x).clamp(100.0, 600.0);
                        cx.notify();
                    }
                });
            })
            // Left: label pane.
            .child(
                div()
                    .w(px(label_width))
                    .h_full()
                    .flex_shrink_0()
                    .overflow_hidden()
                    .child(self.label_pane.clone()),
            )
            // Splitter handle.
            .child({
                let entity_for_splitter = entity.clone();
                div()
                    .id("splitter")
                    .w(px(4.0))
                    .h_full()
                    .bg(colors::GRID_LINE)
                    .cursor_col_resize()
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        entity_for_splitter.update(cx, |panel: &mut PipelinePanel, _| {
                            panel.dragging_splitter = true;
                        });
                    })
            })
            // Right: timeline pane (takes remaining space).
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .overflow_hidden()
                    .child(self.timeline_pane.clone()),
            )
    }
}

impl Focusable for PipelinePanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<gpui_component::dock::PanelEvent> for PipelinePanel {}

impl gpui_component::dock::Panel for PipelinePanel {
    fn panel_name(&self) -> &'static str {
        "PipelinePanel"
    }

    fn title(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        "Pipeline Viewer"
    }

    fn dump(&self, _cx: &App) -> gpui_component::dock::PanelState {
        gpui_component::dock::PanelState::new(self)
    }
}
