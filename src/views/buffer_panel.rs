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
/// Width for the pointer margin column.
const POINTER_COL_W: f32 = 14.0;

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

/// Occupied buffer slot: (slot_index, buffer_field_values, entity_field_name_value_pairs).
pub type BufferSlot = (u16, Vec<u64>, Vec<(String, u64)>);

/// Property value with pointer-pair metadata.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct PropertyValue {
    pub name: String,
    pub value: u64,
    pub role: u8, // 0=plain, 1=HEAD_PTR, 2=TAIL_PTR
    pub pair_id: u8,
}

/// Result of querying buffer state at a cycle.
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct BufferQueryResult {
    pub slots: Vec<BufferSlot>,
    pub properties: Vec<PropertyValue>,
    pub capacity: u16,
}

/// A dynamic buffer panel that displays per-cycle buffer state from uscope.
///
/// Created once per `BufferInfo` entry in `PipelineTrace::buffers`.
/// Shows occupied slots at the current cursor cycle, with field values.
pub struct BufferPanel {
    state: Entity<TraceState>,
    focus_handle: FocusHandle,
    pub buffer_idx: usize,
    /// Cached cycle for which `cached_result` is valid.
    cached_cycle: u32,
    /// Cached buffer query result (slots + properties).
    cached_result: BufferQueryResult,
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
            cached_result: BufferQueryResult::default(),
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
            let new_result = self.state.update(cx, |ts, _cx| {
                ts.query_buffer_state(buffer_idx, cursor_cycle)
            });
            // Discover entity field names from first non-empty result.
            if self.entity_field_names.is_empty() {
                if let Some((_, _, ref ef)) = new_result.slots.first() {
                    self.entity_field_names = ef.iter().map(|(n, _)| n.clone()).collect();
                }
            }
            self.cached_result = new_result;
        }

        let ts = self.state.read(cx);
        let period_ps = ts.trace.period_ps.unwrap_or(1);

        let content = if let Some(buf) = ts.trace.buffers.get(self.buffer_idx) {
            let occupied = self.cached_result.slots.len();
            let capacity = buf.capacity;
            let _field_types: Vec<u8> = buf.fields.iter().map(|(_, ft)| *ft).collect();

            // Detect pointer pairs from properties.
            let has_pointers = buf.properties.iter().any(|p| p.role != 0);

            // Build pointer pair info from cached property values.
            struct PointerPair {
                head: Option<u16>,
                tail: Option<u16>,
                pair_id: u8,
            }
            let mut pairs: Vec<PointerPair> = Vec::new();
            if has_pointers {
                for pv in &self.cached_result.properties {
                    if pv.role == 0 {
                        continue;
                    }
                    let pair = pairs.iter_mut().find(|p| p.pair_id == pv.pair_id);
                    match pair {
                        Some(p) => {
                            if pv.role == 1 {
                                p.head = Some(pv.value as u16);
                            } else {
                                p.tail = Some(pv.value as u16);
                            }
                        }
                        None => {
                            let mut pp = PointerPair {
                                head: None,
                                tail: None,
                                pair_id: pv.pair_id,
                            };
                            if pv.role == 1 {
                                pp.head = Some(pv.value as u16);
                            } else {
                                pp.tail = Some(pv.value as u16);
                            }
                            pairs.push(pp);
                        }
                    }
                }
            }

            // Compute fill level from pair 0.
            let fill_level = pairs.first().and_then(|p| match (p.head, p.tail) {
                (Some(h), Some(t)) => Some(if t >= h { t - h } else { capacity - h + t }),
                _ => None,
            });

            // Slot-to-data lookup.
            let slot_data: std::collections::HashMap<u16, &BufferSlot> =
                self.cached_result.slots.iter().map(|s| (s.0, s)).collect();

            // Visible entity field names.
            let visible_fields: Vec<&String> = self
                .entity_field_names
                .iter()
                .filter(|n| !self.hidden_columns.contains(n.as_str()))
                .collect();

            // ── Header row ───────────────────────────────────────────
            let mut header_children: Vec<AnyElement> = Vec::new();
            // Pointer margin column (only for buffers with pointers).
            if has_pointers {
                header_children.push(
                    div()
                        .w(px(POINTER_COL_W))
                        .flex_shrink_0()
                        .border_r_1()
                        .border_color(colors::GRID_LINE)
                        .into_any_element(),
                );
            }
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

            // Helper: check if a slot is in a pointer pair's range (with wrap).
            let in_range = |slot: u16, head: u16, tail: u16, _cap: u16| -> bool {
                if tail >= head {
                    slot >= head && slot < tail
                } else {
                    slot >= head || slot < tail
                }
            };

            // Determine which slots to iterate.
            let slot_indices: Vec<u16> = if has_pointers {
                (0..capacity).collect() // Fixed: all slots
            } else {
                self.cached_result
                    .slots
                    .iter()
                    .map(|(s, _, _)| *s)
                    .collect() // Sparse: occupied only
            };

            let slot_rows: Vec<AnyElement> = slot_indices
                .iter()
                .enumerate()
                .map(|(row_idx, &slot)| {
                    let slot_entry = slot_data.get(&slot);
                    let (field_values, entity_fields) = match slot_entry {
                        Some((_, fv, ef)) => (Some(fv), Some(ef)),
                        None => (None, None),
                    };
                    let entity_id =
                        field_values.and_then(|fv| fv.first().copied()).unwrap_or(0) as u32;
                    let is_occupied = slot_entry.is_some();

                    let instr_row = if is_occupied {
                        ts.trace.row_for_id(entity_id)
                    } else {
                        None
                    };
                    let disasm = instr_row
                        .map(|r| ts.trace.instructions[r].disasm.clone())
                        .unwrap_or_default();
                    let stage_name = instr_row.and_then(|r| {
                        let stages = ts.trace.stages_for(r);
                        stages
                            .iter()
                            .rev()
                            .find(|s| s.start_cycle <= cursor_cycle && cursor_cycle < s.end_cycle)
                            .map(|s| ts.trace.stage_name(s.stage_name_idx).to_string())
                    });

                    let is_selected = instr_row == selected_row && selected_row.is_some();
                    let is_flushed = instr_row
                        .map(|r| {
                            ts.trace.instructions[r].retire_status
                                == crate::trace::model::RetireStatus::Flushed
                        })
                        .unwrap_or(false);

                    // Fade flushed or empty slots.
                    let text_primary = if is_flushed || !is_occupied {
                        colors::TEXT_DIMMED
                    } else {
                        colors::TEXT_PRIMARY
                    };
                    let text_secondary = if is_flushed || !is_occupied {
                        Hsla {
                            a: 0.25,
                            ..colors::TEXT_DIMMED
                        }
                    } else {
                        colors::TEXT_DIMMED
                    };

                    let mut row_children: Vec<AnyElement> = Vec::new();

                    // Pointer margin (left arrow indicator + region line).
                    if has_pointers {
                        // Color palette for pointer pairs.
                        let pair_colors: &[Hsla] = &[
                            Hsla {
                                h: 120.0 / 360.0,
                                s: 0.7,
                                l: 0.5,
                                a: 1.0,
                            }, // pair 0: green
                            Hsla {
                                h: 0.0 / 360.0,
                                s: 0.7,
                                l: 0.5,
                                a: 1.0,
                            }, // pair 0 tail: red
                            Hsla {
                                h: 30.0 / 360.0,
                                s: 0.8,
                                l: 0.5,
                                a: 1.0,
                            }, // pair 1+: orange
                            Hsla {
                                h: 280.0 / 360.0,
                                s: 0.6,
                                l: 0.5,
                                a: 1.0,
                            }, // pair 2+: purple
                        ];

                        // Check ALL properties whose value matches this slot index.
                        let mut marker = String::new();
                        let mut marker_color = colors::TEXT_DIMMED;
                        let mut _tooltip_name = String::new();

                        for pv in &self.cached_result.properties {
                            if pv.value as u16 == slot {
                                marker = "▸".to_string();
                                let ci = match (pv.role, pv.pair_id) {
                                    (1, 0) => 0,                              // head pair 0: green
                                    (2, 0) => 1,                              // tail pair 0: red
                                    (_, p) if p >= 1 => 2 + (p as usize % 2), // other pairs
                                    _ => 2,                                   // plain/unknown
                                };
                                marker_color = pair_colors[ci.min(pair_colors.len() - 1)];
                                _tooltip_name = pv.name.clone();
                            }
                        }

                        // Solid region line if inside pair 0's range.
                        let in_pair0 = pairs
                            .first()
                            .and_then(|p| match (p.head, p.tail) {
                                (Some(h), Some(t)) => Some(in_range(slot, h, t, capacity)),
                                _ => None,
                            })
                            .unwrap_or(false);

                        if marker.is_empty() && in_pair0 {
                            marker = "┃".to_string(); // solid line
                            marker_color = Hsla {
                                h: 210.0 / 360.0,
                                s: 0.5,
                                l: 0.5,
                                a: 0.6,
                            };
                        }

                        // Pointer margin: narrow column with just the arrow symbol.
                        row_children.push(
                            div()
                                .id(("ptr-margin", row_idx))
                                .w(px(POINTER_COL_W))
                                .flex_shrink_0()
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_color(marker_color)
                                .border_r_1()
                                .border_color(colors::GRID_LINE)
                                .child(marker)
                                .into_any_element(),
                        );
                    }

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
                        .into_iter()
                        .flatten()
                        .map(|(n, v)| (n.as_str(), *v))
                        .collect();
                    for name in &visible_fields {
                        let text = if !is_occupied {
                            String::new()
                        } else if let Some(&val) = ef_map.get(name.as_str()) {
                            format_field(name, val, period_ps)
                        } else {
                            String::new()
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
                        .when(has_pointers && !is_selected, |d| {
                            // Tint occupied region.
                            let in_p0 = pairs
                                .first()
                                .and_then(|p| match (p.head, p.tail) {
                                    (Some(h), Some(t)) => Some(in_range(slot, h, t, capacity)),
                                    _ => None,
                                })
                                .unwrap_or(false);
                            if in_p0 {
                                d.bg(Hsla {
                                    h: 210.0 / 360.0,
                                    s: 0.4,
                                    l: 0.18,
                                    a: 1.0,
                                })
                            } else {
                                d
                            }
                        })
                        .hover(|d| d.bg(colors::GRID_LINE))
                        .on_click(move |_, _, cx| {
                            state.update(cx, |ts, cx| {
                                if let Some(row) = ts.trace.row_for_id(entity_id) {
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
                        .child(if let Some(fill) = fill_level {
                            let pct = if capacity > 0 {
                                fill as f32 / capacity as f32 * 100.0
                            } else {
                                0.0
                            };
                            format!(
                                "{} ({}/{}) @ cycle {} — fill: {} ({:.0}%)",
                                buf.name, occupied, capacity, cursor_cycle, fill, pct
                            )
                        } else {
                            format!(
                                "{} ({}/{}) @ cycle {}",
                                buf.name, occupied, capacity, cursor_cycle
                            )
                        }),
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
