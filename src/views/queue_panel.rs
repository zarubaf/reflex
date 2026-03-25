// Legacy queue panel — kept for reference, no longer used by app.rs.
// Buffer panels (`buffer_panel.rs`) dynamically replace this.

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

/// Ready indicator color (green).
const READY_COLOR: Hsla = Hsla {
    h: 120.0 / 360.0,
    s: 0.7,
    l: 0.45,
    a: 1.0,
};

/// Waiting indicator color (red/orange).
const WAITING_COLOR: Hsla = Hsla {
    h: 0.0,
    s: 0.7,
    l: 0.55,
    a: 1.0,
};

/// Which queue this panel displays.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum QueueKind {
    Retire,
    Dispatch,
    Issue,
}

/// A single-queue tab panel. Create one per queue kind and group them as tabs.
pub struct QueuePanel {
    state: Entity<TraceState>,
    focus_handle: FocusHandle,
    kind: QueueKind,
    retire_queue_size: usize,
    dispatch_stages: Vec<StageNameIdx>,
    issue_stages: Vec<StageNameIdx>,
    retire_stages: Vec<StageNameIdx>,
    dq_names: Vec<String>,
    iq_names: Vec<String>,
}

impl QueuePanel {
    pub fn new(state: Entity<TraceState>, kind: QueueKind, cx: &mut Context<Self>) -> Self {
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

        // Invalidate this panel's render cache when TraceState changes,
        // so that TabPanel's cached() wrapper re-renders us.
        cx.observe(&state, |_this, _state, cx| {
            cx.notify();
        })
        .detach();

        Self {
            state,
            focus_handle: cx.focus_handle(),
            kind,
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

    fn render_retire_queue(&self, cx: &mut Context<Self>) -> Div {
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

        let mut rq_rows: Vec<Stateful<Div>> = Vec::new();
        for (slot, entry) in occupancy.retire_queue.iter().enumerate() {
            if let Some(e) = entry {
                let instr = &ts.trace.instructions[e.row];
                let stage_name = ts.trace.stage_name(e.stage).to_string();
                let stage_color = colors::stage_color(e.stage);
                let disasm = truncate_disasm(&instr.disasm, 60);

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
                        "Retire Queue ({}/{}) @ cycle {}",
                        rq_occupied, self.retire_queue_size, cursor_cycle
                    )),
            )
            .child(
                div()
                    .id("rq-scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .children(rq_rows),
            )
    }

    fn render_dispatch_queues(&self, cx: &mut Context<Self>) -> Div {
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
                let disasm = truncate_disasm(&instr.disasm, 50);
                let wait_cycles = cursor_cycle.saturating_sub(e.stage_start_cycle);

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
                        "Dispatch Queues ({}) @ cycle {}",
                        total_dq_entries, cursor_cycle
                    )),
            )
            .child(
                div()
                    .id("dq-scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .children(dq_sections),
            )
    }

    fn render_issue_queues(&self, cx: &mut Context<Self>) -> Div {
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

            let ready_count = entries.iter().filter(|e| e.is_ready).count();

            iq_sections.push(
                div()
                    .px_2()
                    .py(px(2.0))
                    .text_color(colors::TEXT_DIMMED)
                    .border_b_1()
                    .border_color(colors::GRID_LINE)
                    .child(format!(
                        "{} ({}/{} ready)",
                        queue_name,
                        ready_count,
                        entries.len()
                    ))
                    .into_any_element(),
            );

            for (i, e) in entries.iter().enumerate() {
                let instr = &ts.trace.instructions[e.row];
                let disasm = truncate_disasm(&instr.disasm, 45);
                let wait_cycles = cursor_cycle.saturating_sub(e.stage_start_cycle);

                let status_color = if e.is_ready {
                    READY_COLOR
                } else {
                    WAITING_COLOR
                };

                iq_sections.push(
                    self.queue_row(
                        ("iq", *iq_id as usize * 1000 + i),
                        e.row,
                        selected_row,
                        vec![
                            div()
                                .w(px(14.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    div()
                                        .w(px(6.0))
                                        .h(px(6.0))
                                        .rounded(px(3.0))
                                        .bg(status_color),
                                ),
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
                        "Issue Queues ({}) @ cycle {}",
                        total_iq_entries, cursor_cycle
                    )),
            )
            .child(
                div()
                    .id("iq-scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .children(iq_sections),
            )
    }
}

impl Render for QueuePanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("queue-panel")
            .size_full()
            .bg(colors::BG_PRIMARY)
            .text_size(px(11.0))
            .font_family("Menlo")
            .child(match self.kind {
                QueueKind::Retire => self.render_retire_queue(cx),
                QueueKind::Dispatch => self.render_dispatch_queues(cx),
                QueueKind::Issue => self.render_issue_queues(cx),
            })
    }
}

impl Focusable for QueuePanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<gpui_component::dock::PanelEvent> for QueuePanel {}

impl gpui_component::dock::Panel for QueuePanel {
    fn panel_name(&self) -> &'static str {
        match self.kind {
            QueueKind::Retire => "RetireQueue",
            QueueKind::Dispatch => "DispatchQueues",
            QueueKind::Issue => "IssueQueues",
        }
    }

    fn title(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        match self.kind {
            QueueKind::Retire => "Retire Queue",
            QueueKind::Dispatch => "Dispatch Queues",
            QueueKind::Issue => "Issue Queues",
        }
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

fn truncate_disasm(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}…", &s[..max_len])
    }
}
