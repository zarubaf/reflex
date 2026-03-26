use gpui::prelude::FluentBuilder as _;
use gpui::*;

use crate::app::TraceState;
use crate::theme::colors;

/// Map uscope FieldType raw u8 to display string.
fn field_type_str(ft: u8) -> &'static str {
    match ft {
        0x01 => "U8",
        0x02 => "U16",
        0x03 => "U32",
        0x04 => "U64",
        0x05 => "I8",
        0x06 => "I16",
        0x07 => "I32",
        0x08 => "I64",
        0x09 => "Bool",
        0x0A => "Str",
        0x0B => "Enum",
        _ => "?",
    }
}

/// A dynamic buffer panel that displays per-cycle buffer state from uscope.
///
/// Created once per `BufferInfo` entry in `PipelineTrace::buffers`.
/// Shows occupied slots at the current cursor cycle, with field values.
pub struct BufferPanel {
    state: Entity<TraceState>,
    focus_handle: FocusHandle,
    buffer_idx: usize,
    /// Cached cycle for which `cached_slots` is valid.
    cached_cycle: u32,
    /// Cached occupied slots: (slot_index, field_values).
    cached_slots: Vec<(u16, Vec<u64>)>,
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
            cached_cycle: u32::MAX, // sentinel: no valid cache
            cached_slots: Vec::new(),
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
        let cursor_cycle = {
            let ts = self.state.read(cx);
            ts.cursor_state.cursors[ts.cursor_state.active_idx]
                .cycle
                .round() as u32
        };

        // Refresh cached buffer state when cursor changes.
        if cursor_cycle != self.cached_cycle {
            self.cached_cycle = cursor_cycle;
            let buffer_idx = self.buffer_idx;
            let new_slots = self.state.update(cx, |ts, _cx| {
                ts.query_buffer_state(buffer_idx, cursor_cycle)
            });
            self.cached_slots = new_slots;
        }

        let ts = self.state.read(cx);

        let content = if let Some(buf) = ts.trace.buffers.get(self.buffer_idx) {
            let occupied = self.cached_slots.len();
            let capacity = buf.capacity;
            let field_names: Vec<String> = buf.fields.iter().map(|(n, _)| n.clone()).collect();
            let field_types: Vec<u8> = buf.fields.iter().map(|(_, ft)| *ft).collect();

            // Header row: column names
            let mut header_children: Vec<AnyElement> = Vec::new();
            header_children.push(
                div()
                    .w(px(40.0))
                    .flex_shrink_0()
                    .text_color(colors::TEXT_DIMMED)
                    .child("Slot")
                    .into_any_element(),
            );
            for (i, name) in field_names.iter().enumerate() {
                let type_str = field_type_str(field_types[i]);
                header_children.push(
                    div()
                        .flex_1()
                        .min_w(px(60.0))
                        .text_color(colors::TEXT_DIMMED)
                        .child(format!("{} ({})", name, type_str))
                        .into_any_element(),
                );
            }
            let header = div()
                .id("buf-header")
                .flex()
                .gap_1()
                .px_2()
                .py(px(2.0))
                .border_b_1()
                .border_color(colors::GRID_LINE)
                .children(header_children);

            // Slot rows
            let slot_rows: Vec<AnyElement> = self
                .cached_slots
                .iter()
                .enumerate()
                .map(|(row_idx, (slot, field_values))| {
                    let mut row_children: Vec<AnyElement> = Vec::new();

                    // Slot number
                    row_children.push(
                        div()
                            .w(px(40.0))
                            .flex_shrink_0()
                            .text_color(colors::TEXT_ROW_NUMBER)
                            .child(format!("{}", slot))
                            .into_any_element(),
                    );

                    // Field values
                    for (fi, val) in field_values.iter().enumerate() {
                        let display = format_field_value(*val, field_types.get(fi).copied());
                        row_children.push(
                            div()
                                .flex_1()
                                .min_w(px(60.0))
                                .text_color(colors::TEXT_PRIMARY)
                                .child(display)
                                .into_any_element(),
                        );
                    }

                    div()
                        .id(("slot-row", row_idx))
                        .flex()
                        .gap_1()
                        .px_2()
                        .py(px(1.0))
                        .when(row_idx % 2 == 0, |d| d.bg(colors::BG_SECONDARY))
                        .children(row_children)
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
                            "{} ({}/{}) @ cycle {}",
                            buf.name, occupied, capacity, cursor_cycle
                        )),
                )
                .child(header)
                .child(
                    div()
                        .id(("buf-scroll", self.buffer_idx))
                        .flex_1()
                        .overflow_y_scroll()
                        .children(slot_rows),
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

/// Format a field value for display based on its type.
fn format_field_value(val: u64, field_type: Option<u8>) -> String {
    match field_type {
        // Bool
        Some(0x09) => {
            if val != 0 {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
        // U8 / I8 / Enum - small values, show decimal
        Some(0x01) | Some(0x05) | Some(0x0B) => format!("{}", val as u8),
        // U16 / I16
        Some(0x02) | Some(0x06) => format!("{}", val as u16),
        // U32 / I32 / StringRef - hex for large values, decimal for small
        Some(0x03) | Some(0x07) | Some(0x0A) => {
            if val > 0xFFFF {
                format!("0x{:x}", val as u32)
            } else {
                format!("{}", val as u32)
            }
        }
        // U64 / I64
        Some(0x04) | Some(0x08) => {
            if val > 0xFFFF {
                format!("0x{:x}", val)
            } else {
                format!("{}", val)
            }
        }
        _ => format!("{}", val),
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
