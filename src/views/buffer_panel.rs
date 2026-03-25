use gpui::*;

use crate::app::TraceState;
use crate::theme::colors;

/// A dynamic buffer panel that displays buffer metadata (name, capacity, fields)
/// for a buffer storage detected from the uscope schema.
///
/// Created once per `BufferInfo` entry in `PipelineTrace::buffers`.
/// For now, shows static metadata. Per-cycle state rendering will be added
/// when segment_replay wiring is implemented.
pub struct BufferPanel {
    state: Entity<TraceState>,
    focus_handle: FocusHandle,
    buffer_idx: usize,
}

impl BufferPanel {
    pub fn new(state: Entity<TraceState>, buffer_idx: usize, cx: &mut Context<Self>) -> Self {
        // Invalidate this panel's render cache when TraceState changes,
        // so that TabPanel's cached() wrapper re-renders us.
        cx.observe(&state, |_this, _state, cx| {
            cx.notify();
        })
        .detach();

        Self {
            state,
            focus_handle: cx.focus_handle(),
            buffer_idx,
        }
    }

    /// Get the buffer name for use in Panel trait methods.
    fn buffer_name(&self, cx: &App) -> String {
        let ts = self.state.read(cx);
        ts.trace
            .buffers
            .get(self.buffer_idx)
            .map(|b| b.name.clone())
            .unwrap_or_else(|| format!("buffer_{}", self.buffer_idx))
    }
}

impl Render for BufferPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ts = self.state.read(cx);
        let cursor_cycle = ts.cursor_state.cursors[ts.cursor_state.active_idx]
            .cycle
            .round() as u32;

        let content = if let Some(buf) = ts.trace.buffers.get(self.buffer_idx) {
            let field_descriptions: Vec<AnyElement> = buf
                .fields
                .iter()
                .enumerate()
                .map(|(i, (name, ftype))| {
                    let type_str = match *ftype {
                        0 => "U8",
                        1 => "U16",
                        2 => "U32",
                        3 => "U64",
                        4 => "Bool",
                        _ => "?",
                    };
                    div()
                        .id(("field", i))
                        .flex()
                        .gap_2()
                        .px_2()
                        .py(px(1.0))
                        .child(div().text_color(colors::TEXT_PRIMARY).child(name.clone()))
                        .child(
                            div()
                                .text_color(colors::TEXT_ROW_NUMBER)
                                .child(type_str.to_string()),
                        )
                        .into_any_element()
                })
                .collect();

            div()
                .size_full()
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
                            "{} (capacity: {}) @ cycle {}",
                            buf.name, buf.capacity, cursor_cycle
                        )),
                )
                .child(
                    div()
                        .px_2()
                        .py_1()
                        .text_color(colors::TEXT_DIMMED)
                        .border_b_1()
                        .border_color(colors::GRID_LINE)
                        .child("Fields:"),
                )
                .child(
                    div()
                        .id(("buf-scroll", self.buffer_idx))
                        .flex_1()
                        .overflow_y_scroll()
                        .children(field_descriptions),
                )
        } else {
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_color(colors::TEXT_DIMMED)
                .child("Buffer not found")
        };

        div()
            .id(("buffer-panel", self.buffer_idx))
            .size_full()
            .bg(colors::BG_PRIMARY)
            .text_size(px(11.0))
            .font_family("Menlo")
            .child(content)
    }
}

impl Focusable for BufferPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<gpui_component::dock::PanelEvent> for BufferPanel {}

impl gpui_component::dock::Panel for BufferPanel {
    fn panel_name(&self) -> &'static str {
        // Static string required by trait; use a generic name.
        "BufferPanel"
    }

    fn title(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.buffer_name(cx)
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
