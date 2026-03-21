use gpui::prelude::FluentBuilder as _;
use gpui::*;

use crate::app::TraceState;
use crate::theme::colors;
use crate::trace::model::StageNameIdx;

/// Highlight color for selected row in queue tables.
const SELECTED_BG: Hsla = Hsla {
    h: 210.0 / 360.0,
    s: 0.6,
    l: 0.25,
    a: 1.0,
};

/// Collapsible bottom panel showing retire queue and issue queue slot occupancy.
pub struct QueuePanel {
    state: Entity<TraceState>,
    visible: bool,
    retire_queue_size: usize,
    dispatch_stages: Vec<StageNameIdx>,
    issue_stages: Vec<StageNameIdx>,
    retire_stages: Vec<StageNameIdx>,
    dq_names: Vec<String>,
    iq_names: Vec<String>,
}

impl QueuePanel {
    pub fn new(state: Entity<TraceState>, cx: &App) -> Self {
        let ts = state.read(cx);

        let retire_queue_size = ts
            .trace
            .metadata
            .iter()
            .find(|(k, _)| k == "cpu.retire_queue_size")
            .and_then(|(_, v)| v.parse::<usize>().ok())
            .unwrap_or(128);

        let dispatch_stages =
            Self::resolve_stages_from_metadata(&ts.trace, "cpu.dispatch_queue_stages");
        let issue_stages = Self::resolve_stages_from_metadata(&ts.trace, "cpu.issue_queue_stages");
        let retire_stages =
            Self::resolve_stages_from_metadata(&ts.trace, "cpu.retire_queue_stages");

        let dq_names = Self::read_names_from_metadata(&ts.trace, "cpu.dispatch_queue_names");
        let iq_names = Self::read_names_from_metadata(&ts.trace, "cpu.issue_queue_names");

        Self {
            state,
            visible: false,
            retire_queue_size,
            dispatch_stages,
            issue_stages,
            retire_stages,
            dq_names,
            iq_names,
        }
    }

    fn resolve_stages_from_metadata(
        trace: &crate::trace::model::PipelineTrace,
        key: &str,
    ) -> Vec<StageNameIdx> {
        trace
            .metadata
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| {
                v.split(',')
                    .filter_map(|name| trace.stage_name_idx(name.trim()))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn read_names_from_metadata(
        trace: &crate::trace::model::PipelineTrace,
        key: &str,
    ) -> Vec<String> {
        trace
            .metadata
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.split(',').map(|s| s.trim().to_string()).collect())
            .unwrap_or_default()
    }

    pub fn toggle(&mut self, cx: &mut Context<Self>) {
        self.visible = !self.visible;
        cx.notify();
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Build a clickable, highlightable row for a queue entry.
    fn queue_row(
        &self,
        id: impl Into<ElementId>,
        row: usize,
        selected_row: Option<usize>,
        children: Vec<Div>,
    ) -> Stateful<Div> {
        let is_selected = selected_row == Some(row);
        let state = self.state.clone();

        let mut d = div()
            .id(id.into())
            .flex()
            .gap_2()
            .px_2()
            .py(px(1.0))
            .cursor_pointer()
            .when(is_selected, |s| s.bg(SELECTED_BG))
            .when(!is_selected, |s| s.hover(|s| s.bg(colors::GRID_LINE)))
            .on_click(move |_, _, cx| {
                state.update(cx, |ts, cx| {
                    ts.selected_row = Some(row);
                    cx.notify();
                });
            });

        for child in children {
            d = d.child(child);
        }
        d
    }
}

impl Render for QueuePanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.visible {
            return div().id("queue-panel-hidden");
        }

        let ts = self.state.read(cx);
        let cursor_cycle = ts.cursor_state.cursors[ts.cursor_state.active_idx]
            .cycle
            .round() as u32;
        let selected_row = ts.selected_row;

        let occupancy = ts.trace.queue_occupancy_at(
            cursor_cycle,
            self.retire_queue_size,
            &self.dispatch_stages,
            &self.issue_stages,
            &self.retire_stages,
        );

        let rq_occupied = occupancy
            .retire_queue
            .iter()
            .filter(|s| s.is_some())
            .count();

        // Retire queue rows.
        let mut rq_rows: Vec<Stateful<Div>> = Vec::new();
        for (slot, entry) in occupancy.retire_queue.iter().enumerate() {
            if let Some(e) = entry {
                let instr = &ts.trace.instructions[e.row];
                let stage_name = ts.trace.stage_name(e.stage).to_string();
                let stage_color = colors::stage_color(e.stage);
                let disasm = truncate_disasm(&instr.disasm, 40);

                rq_rows.push(self.queue_row(
                    ("rq", slot),
                    e.row,
                    selected_row,
                    vec![
                        div()
                            .w(px(36.0))
                            .text_color(colors::TEXT_ROW_NUMBER)
                            .child(format!("0x{:02x}", slot)),
                        div().w(px(24.0)).text_color(stage_color).child(stage_name),
                        div()
                            .flex_1()
                            .text_color(colors::TEXT_PRIMARY)
                            .overflow_x_hidden()
                            .child(disasm),
                    ],
                ));
            }
        }

        // Dispatch queue sections.
        let total_dq_entries: usize = occupancy
            .dispatch_queues
            .iter()
            .map(|(_, entries)| entries.len())
            .sum();

        let mut dq_sections: Vec<AnyElement> = Vec::new();
        for (dq_id, entries) in &occupancy.dispatch_queues {
            let fallback;
            let queue_name = if (*dq_id as usize) < self.dq_names.len() {
                self.dq_names[*dq_id as usize].as_str()
            } else if *dq_id == u32::MAX {
                "unknown"
            } else {
                fallback = format!("dq{}", dq_id);
                fallback.as_str()
            };

            dq_sections.push(
                div()
                    .px_2()
                    .py(px(2.0))
                    .text_color(colors::TEXT_DIMMED)
                    .border_b_1()
                    .border_color(colors::GRID_LINE)
                    .child(format!("{} ({})", queue_name, entries.len()))
                    .into_any_element(),
            );

            for (i, e) in entries.iter().enumerate() {
                let instr = &ts.trace.instructions[e.row];
                let disasm = truncate_disasm(&instr.disasm, 32);
                let wait_cycles = cursor_cycle.saturating_sub(instr.first_cycle);

                dq_sections.push(
                    self.queue_row(
                        ("dq", *dq_id as usize * 1000 + i),
                        e.row,
                        selected_row,
                        vec![
                            div()
                                .flex_1()
                                .text_color(colors::TEXT_PRIMARY)
                                .overflow_x_hidden()
                                .child(disasm),
                            div()
                                .w(px(36.0))
                                .text_color(colors::TEXT_DIMMED)
                                .child(format!("{} cy", wait_cycles)),
                        ],
                    )
                    .into_any_element(),
                );
            }
        }

        // Issue queue sections.
        let total_iq_entries: usize = occupancy
            .issue_queues
            .iter()
            .map(|(_, entries)| entries.len())
            .sum();

        let mut iq_sections: Vec<AnyElement> = Vec::new();
        for (iq_id, entries) in &occupancy.issue_queues {
            let fallback;
            let queue_name = if (*iq_id as usize) < self.iq_names.len() {
                self.iq_names[*iq_id as usize].as_str()
            } else if *iq_id == u32::MAX {
                "unknown"
            } else {
                fallback = format!("iq{}", iq_id);
                fallback.as_str()
            };

            iq_sections.push(
                div()
                    .px_2()
                    .py(px(2.0))
                    .text_color(colors::TEXT_DIMMED)
                    .border_b_1()
                    .border_color(colors::GRID_LINE)
                    .child(format!("{} ({})", queue_name, entries.len()))
                    .into_any_element(),
            );

            for (i, e) in entries.iter().enumerate() {
                let instr = &ts.trace.instructions[e.row];
                let stage_name = ts.trace.stage_name(e.stage).to_string();
                let stage_color = colors::stage_color(e.stage);
                let disasm = truncate_disasm(&instr.disasm, 32);
                let wait_cycles = cursor_cycle.saturating_sub(instr.first_cycle);

                iq_sections.push(
                    self.queue_row(
                        ("iq", *iq_id as usize * 1000 + i),
                        e.row,
                        selected_row,
                        vec![
                            div().w(px(24.0)).text_color(stage_color).child(stage_name),
                            div()
                                .flex_1()
                                .text_color(colors::TEXT_PRIMARY)
                                .overflow_x_hidden()
                                .child(disasm),
                            div()
                                .w(px(36.0))
                                .text_color(colors::TEXT_DIMMED)
                                .child(format!("{} cy", wait_cycles)),
                        ],
                    )
                    .into_any_element(),
                );
            }
        }

        div()
            .id("queue-panel")
            .w_full()
            .h(px(250.0))
            .bg(colors::BG_PRIMARY)
            .border_t_1()
            .border_color(colors::GRID_LINE)
            .flex()
            .flex_row()
            .text_size(px(11.0))
            .font_family("Menlo")
            // Retire queue (left).
            .child(
                div()
                    .flex_1()
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
                                "Retire Queue ({}/{}) @ cycle {}",
                                rq_occupied, self.retire_queue_size, cursor_cycle
                            )),
                    )
                    .child(
                        div()
                            .flex_1()
                            .map(|mut el| {
                                el.style().overflow.y = Some(gpui::Overflow::Scroll);
                                el
                            })
                            .children(rq_rows),
                    ),
            )
            // Vertical divider.
            .child(div().w(px(1.0)).h_full().bg(colors::GRID_LINE))
            // Dispatch queues (middle).
            .child(
                div()
                    .flex_1()
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
                                "Dispatch Queues ({}) @ cycle {}",
                                total_dq_entries, cursor_cycle
                            )),
                    )
                    .child(
                        div()
                            .flex_1()
                            .map(|mut el| {
                                el.style().overflow.y = Some(gpui::Overflow::Scroll);
                                el
                            })
                            .children(dq_sections),
                    ),
            )
            // Vertical divider.
            .child(div().w(px(1.0)).h_full().bg(colors::GRID_LINE))
            // Issue queues (right).
            .child(
                div()
                    .flex_1()
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
                                "Issue Queues ({}) @ cycle {}",
                                total_iq_entries, cursor_cycle
                            )),
                    )
                    .child(
                        div()
                            .flex_1()
                            .map(|mut el| {
                                el.style().overflow.y = Some(gpui::Overflow::Scroll);
                                el
                            })
                            .children(iq_sections),
                    ),
            )
    }
}

fn truncate_disasm(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}…", &s[..max_len])
    }
}
