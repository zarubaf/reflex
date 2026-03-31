use gpui::prelude::FluentBuilder as _;
use gpui::*;
use std::collections::HashSet;

use crate::app::TraceState;
use crate::theme::colors;

/// Column width for entity field values.
const FIELD_COL_W: f32 = 72.0;
/// Column width for the slot number.
const SLOT_COL_W: f32 = 40.0;
/// Column width for the stage name.
const STAGE_COL_W: f32 = 28.0;
/// Minimum width for the disasm column.
const DISASM_MIN_W: f32 = 180.0;

/// Format an entity field value for display.
fn format_field(name: &str, value: u64, period_ps: u64) -> String {
    match name {
        "inst_bits" => format!("0x{:08x}", value as u32),
        "ready_time_ps" => {
            if period_ps > 0 {
                format!("{}", value / period_ps)
            } else {
                format!("{}", value)
            }
        }
        _ => format!("{}", value),
    }
}

/// Display name for a field (shorten long names).
fn field_display_name(name: &str) -> &str {
    match name {
        "ready_time_ps" => "ready",
        _ => name,
    }
}

/// Default-hidden fields.
fn is_default_hidden(name: &str) -> bool {
    name == "inst_bits"
}

/// A dynamic buffer panel that displays per-cycle buffer state from uscope.
///
/// Created once per `BufferInfo` entry in `PipelineTrace::buffers`.
/// Shows occupied slots at the current cursor cycle, with field values.
pub struct BufferPanel {
    state: Entity<TraceState>,
    focus_handle: FocusHandle,
    pub buffer_idx: usize,
    /// Cached cycle for which `cached_slots` is valid.
    cached_cycle: u32,
    /// Cached occupied slots: (slot_index, field_values, entity_fields).
    cached_slots: Vec<(u16, Vec<u64>, Vec<(String, u64)>)>,
    /// Entity field names discovered from the first query (stable per trace).
    entity_field_names: Vec<String>,
    /// Hidden column names.
    pub hidden_columns: HashSet<String>,
}

impl BufferPanel {
    pub fn new(state: Entity<TraceState>, buffer_idx: usize, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_this, _state, cx| {
            cx.notify();
        })
        .detach();

        let mut hidden = HashSet::new();
        // Apply default-hidden fields.
        hidden.insert("inst_bits".to_string());

        Self {
            state,
            focus_handle: cx.focus_handle(),
            buffer_idx,
            cached_cycle: u32::MAX,
            cached_slots: Vec::new(),
            entity_field_names: Vec::new(),
            hidden_columns: hidden,
        }
    }

    fn buffer_name(&self, cx: &App) -> String {
        let ts = self.state.read(cx);
        ts.trace
            .buffers
            .get(self.buffer_idx)
            .map(|b| b.name.clone())
            .unwrap_or_else(|| format!("buffer_{}", self.buffer_idx))
    }

    /// Get the list of hidden column names (for session persistence).
    pub fn hidden_column_names(&self) -> Vec<String> {
        self.hidden_columns.iter().cloned().collect()
    }

    /// Set hidden columns (for session restore).
    pub fn set_hidden_columns(&mut self, names: Vec<String>) {
        self.hidden_columns = names.into_iter().collect();
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
            // Discover entity field names from first non-empty result.
            if self.entity_field_names.is_empty() {
                if let Some((_, _, ref ef)) = new_slots.first() {
                    self.entity_field_names = ef.iter().map(|(n, _)| n.clone()).collect();
                }
            }
            self.cached_slots = new_slots;
        }

        let ts = self.state.read(cx);
        let period_ps = ts.trace.period_ps.unwrap_or(1);

        let content = if let Some(buf) = ts.trace.buffers.get(self.buffer_idx) {
            let occupied = self.cached_slots.len();
            let capacity = buf.capacity;
            let field_types: Vec<u8> = buf.fields.iter().map(|(_, ft)| *ft).collect();

            // Visible entity field names.
            let visible_fields: Vec<&String> = self
                .entity_field_names
                .iter()
                .filter(|n| !self.hidden_columns.contains(n.as_str()))
                .collect();

            // ── Header row ───────────────────────────────────────────
            let mut header_children: Vec<AnyElement> = Vec::new();
            header_children.push(
                div()
                    .w(px(SLOT_COL_W))
                    .flex_shrink_0()
                    .text_color(colors::TEXT_DIMMED)
                    .child("Slot")
                    .into_any_element(),
            );
            header_children.push(
                div()
                    .w(px(STAGE_COL_W))
                    .flex_shrink_0()
                    .text_color(colors::TEXT_DIMMED)
                    .child("Stg")
                    .into_any_element(),
            );
            header_children.push(
                div()
                    .min_w(px(DISASM_MIN_W))
                    .flex_1()
                    .text_color(colors::TEXT_DIMMED)
                    .child("Instruction")
                    .into_any_element(),
            );
            for name in &visible_fields {
                header_children.push(
                    div()
                        .w(px(FIELD_COL_W))
                        .flex_shrink_0()
                        .text_color(colors::TEXT_DIMMED)
                        .child(field_display_name(name).to_string())
                        .into_any_element(),
                );
            }

            // ── Data rows ────────────────────────────────────────────
            let ts = self.state.read(cx);
            let selected_row = ts.selected_row;
            let slot_rows: Vec<AnyElement> = self
                .cached_slots
                .iter()
                .enumerate()
                .map(|(row_idx, (slot, field_values, entity_fields))| {
                    let entity_id = field_values.first().copied().unwrap_or(0) as u32;

                    let instr_row = ts.trace.instructions.iter().position(|i| i.id == entity_id);
                    let disasm = instr_row
                        .map(|r| ts.trace.instructions[r].disasm.clone())
                        .unwrap_or_else(|| format!("entity {}", entity_id));
                    let stage_name = instr_row.and_then(|r| {
                        let stages = ts.trace.stages_for(r);
                        stages
                            .iter()
                            .filter(|s| s.start_cycle <= cursor_cycle && cursor_cycle < s.end_cycle)
                            .last()
                            .map(|s| ts.trace.stage_name(s.stage_name_idx).to_string())
                    });

                    let is_selected = instr_row == selected_row && selected_row.is_some();
                    let is_flushed = instr_row
                        .map(|r| {
                            ts.trace.instructions[r].retire_status
                                == crate::trace::model::RetireStatus::Flushed
                        })
                        .unwrap_or(false);

                    // Fade flushed instructions.
                    let text_primary = if is_flushed {
                        colors::TEXT_DIMMED
                    } else {
                        colors::TEXT_PRIMARY
                    };
                    let text_secondary = if is_flushed {
                        Hsla {
                            a: 0.25,
                            ..colors::TEXT_DIMMED
                        }
                    } else {
                        colors::TEXT_DIMMED
                    };

                    let mut row_children: Vec<AnyElement> = Vec::new();

                    // Slot number.
                    row_children.push(
                        div()
                            .w(px(SLOT_COL_W))
                            .flex_shrink_0()
                            .text_color(if is_flushed {
                                text_secondary
                            } else {
                                colors::TEXT_ROW_NUMBER
                            })
                            .child(format!("0x{:02x}", slot))
                            .into_any_element(),
                    );

                    // Stage name.
                    row_children.push(if let Some(ref stage) = stage_name {
                        let stage_idx = ts.trace.stage_name_idx(stage).unwrap_or(0);
                        let stage_col = if is_flushed {
                            Hsla {
                                a: 0.3,
                                ..colors::stage_color(stage_idx)
                            }
                        } else {
                            colors::stage_color(stage_idx)
                        };
                        div()
                            .w(px(STAGE_COL_W))
                            .flex_shrink_0()
                            .text_color(stage_col)
                            .child(stage.clone())
                            .into_any_element()
                    } else {
                        div().w(px(STAGE_COL_W)).flex_shrink_0().into_any_element()
                    });

                    // Disasm.
                    row_children.push(
                        div()
                            .min_w(px(DISASM_MIN_W))
                            .flex_1()
                            .flex_shrink_0()
                            .text_color(text_primary)
                            .child(disasm)
                            .into_any_element(),
                    );

                    // Entity fields — only visible ones, in consistent order.
                    let ef_map: std::collections::HashMap<&str, u64> = entity_fields
                        .iter()
                        .map(|(n, v)| (n.as_str(), *v))
                        .collect();
                    for name in &visible_fields {
                        let val = ef_map.get(name.as_str()).copied().unwrap_or(0);
                        let text = if val == 0 {
                            String::new()
                        } else {
                            format_field(name, val, period_ps)
                        };
                        row_children.push(
                            div()
                                .w(px(FIELD_COL_W))
                                .flex_shrink_0()
                                .text_color(text_secondary)
                                .child(text)
                                .into_any_element(),
                        );
                    }

                    let state = self.state.clone();

                    div()
                        .id(("slot-row", row_idx))
                        .flex()
                        .gap(px(4.0))
                        .px_2()
                        .py(px(1.0))
                        .cursor_pointer()
                        .when(is_selected, |d| {
                            d.bg(Hsla {
                                h: 210.0 / 360.0,
                                s: 0.6,
                                l: 0.25,
                                a: 1.0,
                            })
                        })
                        .when(!is_selected && row_idx % 2 == 0, |d| {
                            d.bg(colors::BG_SECONDARY)
                        })
                        .hover(|d| d.bg(colors::GRID_LINE))
                        .on_click(move |_, _, cx| {
                            state.update(cx, |ts, cx| {
                                if let Some(row) =
                                    ts.trace.instructions.iter().position(|i| i.id == entity_id)
                                {
                                    ts.selected_row = Some(row);
                                }
                                cx.notify();
                            });
                        })
                        .children(row_children)
                        .into_any_element()
                })
                .collect();

            div()
                .size_full()
                .flex()
                .flex_col()
                .overflow_hidden()
                // Title bar.
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
                // Scrollable table.
                .child(
                    div()
                        .id(("buf-scroll", self.buffer_idx))
                        .flex_1()
                        .overflow_y_scroll()
                        .overflow_x_scroll()
                        .child(
                            div().flex().flex_col().child(
                                // Header + rows in a single column so they scroll together.
                                div()
                                    .flex()
                                    .flex_col()
                                    // Header row.
                                    .child(
                                        div()
                                            .flex()
                                            .gap(px(4.0))
                                            .px_2()
                                            .py(px(1.0))
                                            .border_b_1()
                                            .border_color(colors::GRID_LINE)
                                            .children(header_children),
                                    )
                                    // Data rows.
                                    .children(slot_rows),
                            ),
                        ),
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
        let mut state = gpui_component::dock::PanelState::new(self);
        state.info = gpui_component::dock::PanelInfo::Panel(
            serde_json::json!({ "buffer_idx": self.buffer_idx }),
        );
        state
    }
}
