use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::dock::{DockArea, DockItem, DockPlacement, PanelView};
use gpui_component::tab::{Tab, TabBar};
use gpui_component::{Icon, IconName};

use crate::interaction::actions::*;
use crate::interaction::viewport::ViewportState;
use crate::theme::colors;
use crate::title_bar::render_title_bar;
use crate::trace::generator::{self, GeneratorConfig};
use crate::trace::model::{PipelineTrace, SegmentIndex};
use crate::views::buffer_panel::BufferPanel;
use crate::views::counter_panel::CounterPanel;
use crate::views::goto_bar::GotoBar;
use crate::views::help_overlay::HelpOverlay;
use crate::views::info_overlay::InfoOverlay;
use crate::views::log_panel::{LogBuffer, LogPanel};
use crate::views::minimap_view::MinimapView;
use crate::views::pipeline_panel::PipelinePanel;
use crate::views::search_bar::SearchBar;
use crate::views::status_bar::StatusBar;
use crate::wcp::WcpClient;
use uscope::reader::Reader;

/// Decode percent-encoded characters in a URL path (e.g. `%20` → ` `).
fn percent_decode(input: &str) -> String {
    let mut result = Vec::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(byte) =
                u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""), 16)
            {
                result.push(byte);
                i += 3;
                continue;
            }
        }
        result.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&result).into_owned()
}

/// Info for a tooltip currently being shown on hover.
#[derive(Clone)]
pub struct TooltipHover {
    /// Tooltip text (newline-separated annotation lines).
    pub text: String,
    /// Mouse position in window coordinates.
    pub position: gpui::Point<gpui::Pixels>,
}

/// A single cycle cursor.
#[derive(Clone)]
pub struct Cursor {
    /// Sub-cycle precision position.
    pub cycle: f64,
    /// Index into CURSOR_PALETTE.
    pub color_idx: usize,
}

/// Maximum cursor history entries.
const CURSOR_HISTORY_MAX: usize = 100;

/// Snapshot of all cursors for undo/redo.
#[derive(Clone)]
struct CursorSnapshot {
    cursors: Vec<Cursor>,
    active_idx: usize,
}

/// State for multicursor support.
#[derive(Clone)]
pub struct CursorState {
    pub cursors: Vec<Cursor>,
    pub active_idx: usize,
    next_color: usize,
    /// Full cursor state history for undo.
    history_back: Vec<CursorSnapshot>,
    /// Full cursor state history for redo.
    history_forward: Vec<CursorSnapshot>,
}

impl CursorState {
    pub fn new() -> Self {
        Self {
            cursors: vec![Cursor {
                cycle: 0.0,
                color_idx: 0,
            }],
            active_idx: 0,
            next_color: 1,
            history_back: Vec::new(),
            history_forward: Vec::new(),
        }
    }

    /// Save current state to undo history before making a change.
    fn push_history(&mut self) {
        self.history_back.push(CursorSnapshot {
            cursors: self.cursors.clone(),
            active_idx: self.active_idx,
        });
        if self.history_back.len() > CURSOR_HISTORY_MAX {
            self.history_back.remove(0);
        }
        self.history_forward.clear();
    }

    /// Move the active cursor to a new cycle, recording history for undo.
    pub fn move_cursor(&mut self, cycle: f64) {
        if let Some(cursor) = self.cursors.get(self.active_idx) {
            if (cursor.cycle - cycle).abs() < 0.5 {
                return;
            }
        }
        self.push_history();
        if let Some(cursor) = self.cursors.get_mut(self.active_idx) {
            cursor.cycle = cycle;
        }
    }

    /// Undo: restore previous full cursor state.
    pub fn undo(&mut self) {
        if let Some(snapshot) = self.history_back.pop() {
            self.history_forward.push(CursorSnapshot {
                cursors: self.cursors.clone(),
                active_idx: self.active_idx,
            });
            self.cursors = snapshot.cursors;
            self.active_idx = snapshot.active_idx;
        }
    }

    /// Redo: restore undone full cursor state.
    pub fn redo(&mut self) {
        if let Some(snapshot) = self.history_forward.pop() {
            self.history_back.push(CursorSnapshot {
                cursors: self.cursors.clone(),
                active_idx: self.active_idx,
            });
            self.cursors = snapshot.cursors;
            self.active_idx = snapshot.active_idx;
        }
    }

    pub fn add_cursor(&mut self, cycle: f64) {
        self.push_history();
        let color_idx = self.next_color % colors::CURSOR_PALETTE.len();
        self.next_color += 1;
        self.cursors.push(Cursor { cycle, color_idx });
        self.active_idx = self.cursors.len() - 1;
    }

    pub fn remove_active(&mut self) {
        if self.cursors.len() <= 1 {
            return;
        }
        self.push_history();
        self.cursors.remove(self.active_idx);
        if self.active_idx >= self.cursors.len() {
            self.active_idx = self.cursors.len() - 1;
        }
    }

    pub fn next(&mut self) {
        if !self.cursors.is_empty() {
            self.active_idx = (self.active_idx + 1) % self.cursors.len();
        }
    }

    pub fn prev(&mut self) {
        if !self.cursors.is_empty() {
            self.active_idx = if self.active_idx == 0 {
                self.cursors.len() - 1
            } else {
                self.active_idx - 1
            };
        }
    }
}

/// Shared application state — single entity observed by all panels.
pub struct TraceState {
    pub trace: Arc<PipelineTrace>,
    pub viewport: ViewportState,
    pub selected_row: Option<usize>,
    /// Active tooltip hover info, if any.
    pub tooltip_hover: Option<TooltipHover>,
    /// Multicursor state.
    pub cursor_state: CursorState,
    /// Counter index to show as overlay on the pipeline timeline. None = no overlay.
    pub overlay_counter: Option<usize>,
    /// Counter panel cycle range (independent from pipeline viewport).
    /// `None` = full trace. `Some((start, end))` = selected range.
    pub counter_range: Option<(u32, u32)>,
    /// FPS tracking.
    pub frame_times: VecDeque<Instant>,
    pub fps: f64,
    /// uscope reader kept alive for on-demand segment loading.
    /// `None` for generator-produced traces.
    pub reader: Option<Reader>,
    /// Segment-to-cycle index for fast lookup of which segments cover a cycle range.
    pub segment_index: SegmentIndex,
    /// Context needed for on-demand segment loading (protocol IDs, decoder, etc.).
    /// `None` for generator-produced traces.
    pub uscope_ctx: Option<crate::trace::uscope_source::UscopeContext>,
    /// Set of segment indices whose instructions have been loaded into the trace.
    pub loaded_segments: HashSet<usize>,
    /// Pre-computed trace summary (counter mipmaps + instruction density) for fast queries.
    pub trace_summary: Option<uscope::summary::TraceSummary>,
}

impl TraceState {
    pub fn new() -> Self {
        Self {
            trace: Arc::new(PipelineTrace::new()),
            viewport: ViewportState::new(),
            selected_row: None,
            tooltip_hover: None,
            cursor_state: CursorState::new(),
            overlay_counter: None,
            counter_range: None,
            frame_times: VecDeque::new(),
            fps: 0.0,
            reader: None,
            segment_index: SegmentIndex::default(),
            uscope_ctx: None,
            loaded_segments: HashSet::new(),
            trace_summary: None,
        }
    }

    /// Get the effective counter display range (defaults to full trace).
    pub fn effective_counter_range(&self) -> (u32, u32) {
        self.counter_range.unwrap_or((0, self.trace.max_cycle()))
    }

    /// Get counter value at a cycle, using mipmap if available.
    pub fn counter_value_at(&self, counter_idx: usize, cycle: u32) -> u64 {
        if let Some(ref summary) = self.trace_summary {
            summary.counter_value_at(counter_idx, cycle)
        } else {
            self.trace.counter_value_at(counter_idx, cycle)
        }
    }

    /// Get counter rate at a cycle, using mipmap if available.
    pub fn counter_rate_at(&self, counter_idx: usize, cycle: u32, window: u32) -> f64 {
        let end_val = self.counter_value_at(counter_idx, cycle);
        let start_val = self.counter_value_at(counter_idx, cycle.saturating_sub(window));
        let actual_window = cycle.saturating_sub(cycle.saturating_sub(window));
        if actual_window == 0 {
            return 0.0;
        }
        (end_val.wrapping_sub(start_val)) as f64 / actual_window as f64
    }

    /// Get counter delta at a cycle, using mipmap if available.
    pub fn counter_delta_at(&self, counter_idx: usize, cycle: u32) -> u64 {
        let curr = self.counter_value_at(counter_idx, cycle);
        let prev = if cycle > 0 {
            self.counter_value_at(counter_idx, cycle - 1)
        } else {
            0
        };
        curr.wrapping_sub(prev)
    }

    /// Fast counter downsampling using mipmaps when available.
    /// Falls back to PipelineTrace::counter_downsample_minmax() for sparse samples.
    pub fn counter_downsample(
        &self,
        counter_idx: usize,
        start_cycle: u32,
        end_cycle: u32,
        bucket_count: usize,
    ) -> Vec<(u64, u64)> {
        if bucket_count == 0 || start_cycle >= end_cycle {
            return Vec::new();
        }

        // Try mipmap first.
        if let Some(ref summary) = self.trace_summary {
            if counter_idx < summary.counters.len() {
                let mipmap = &summary.counters[counter_idx];
                let range_cycles = end_cycle - start_cycle;
                let cycles_per_bucket = range_cycles as f64 / bucket_count as f64;

                // Pick the mipmap level where each mipmap entry covers
                // roughly one output bucket (or slightly finer).
                let base = summary.base_interval_cycles as f64;
                let fan = summary.fan_out as f64;
                let mut level = 0usize;
                let mut level_interval = base;
                while level + 1 < mipmap.levels.len() && level_interval * fan <= cycles_per_bucket {
                    level += 1;
                    level_interval *= fan;
                }

                let entries = &mipmap.levels[level];
                if entries.is_empty() {
                    return vec![(0, 0); bucket_count];
                }

                // Map output buckets to mipmap entries.
                let level_interval = summary.base_interval_cycles as f64
                    * (summary.fan_out as f64).powi(level as i32);

                let mut result = Vec::with_capacity(bucket_count);
                for b in 0..bucket_count {
                    let b_start = start_cycle as f64 + b as f64 * cycles_per_bucket;
                    let b_end = b_start + cycles_per_bucket;

                    // Find mipmap entries overlapping this bucket.
                    let entry_start = (b_start / level_interval) as usize;
                    let entry_end = ((b_end / level_interval).ceil() as usize).min(entries.len());

                    // Use weighted average rate instead of raw min/max to avoid
                    // moiré patterns from mipmap bucket boundary misalignment.
                    let mut total_sum = 0u64;
                    let mut total_cycles = 0.0f64;
                    for (ei, entry) in entries[entry_start..entry_end].iter().enumerate() {
                        let e_start = (entry_start + ei) as f64 * level_interval;
                        let e_end = e_start + level_interval;
                        let overlap_start = b_start.max(e_start);
                        let overlap_end = b_end.min(e_end);
                        let overlap = (overlap_end - overlap_start).max(0.0);
                        let entry_frac = overlap / level_interval;
                        total_sum += (entry.sum as f64 * entry_frac) as u64;
                        total_cycles += overlap;
                    }
                    let avg_rate = if total_cycles > 0.0 {
                        (total_sum as f64 / total_cycles * level_interval) as u64
                    } else {
                        0
                    };
                    result.push((avg_rate, avg_rate));
                }
                return result;
            }
        }

        // Fallback to sparse sample method.
        self.trace
            .counter_downsample_minmax(counter_idx, start_cycle, end_cycle, bucket_count)
    }

    pub fn load_trace(
        &mut self,
        trace: PipelineTrace,
        reader: Option<Reader>,
        segment_index: SegmentIndex,
    ) {
        self.viewport.max_cycle = trace.max_cycle();
        self.viewport.max_row = trace.row_count();
        self.viewport.clamp();
        self.trace = Arc::new(trace);
        // Reset counter range to full trace when loading new file
        self.counter_range = None;
        self.reader = reader;
        self.segment_index = segment_index;
        self.uscope_ctx = None;
        self.loaded_segments.clear();
    }

    /// Load a trace in lazy mode: metadata + counters are loaded, but
    /// instructions are loaded on demand as the viewport scrolls.
    pub fn load_trace_lazy(
        &mut self,
        mut trace: PipelineTrace,
        reader: Reader,
        segment_index: SegmentIndex,
        uscope_ctx: crate::trace::uscope_source::UscopeContext,
        trace_summary: Option<uscope::summary::TraceSummary>,
    ) {
        self.viewport.max_cycle = trace.max_cycle();

        // Use trace summary for total instruction count and max_row if available.
        if let Some(ref summary) = trace_summary {
            trace.total_instruction_count = summary.total_instructions as usize;
            self.viewport.max_row = summary.total_instructions as usize;
        } else {
            eprintln!("warning: no trace summary; instruction count unknown until segments load");
            self.viewport.max_row = 0;
        }

        self.viewport.clamp();
        self.trace = Arc::new(trace);
        self.reader = Some(reader);
        self.trace_summary = trace_summary;
        self.segment_index = segment_index;
        self.uscope_ctx = Some(uscope_ctx);
        self.loaded_segments.clear();
    }

    /// Ensure that all segments covering the given cycle range have their
    /// instructions loaded. Call this before rendering the viewport.
    ///
    /// Returns `true` if new segments were loaded (trace data changed).
    pub fn ensure_segments_loaded(&mut self, start_cycle: u32, end_cycle: u32) -> bool {
        // Only applies to lazy-loaded uscope traces.
        if self.uscope_ctx.is_none() {
            return false;
        }

        // Add a buffer zone: 50% of the visible range on each side.
        let range_width = end_cycle.saturating_sub(start_cycle);
        let buffer = range_width / 2;
        let buffered_start = start_cycle.saturating_sub(buffer);
        let buffered_end = end_cycle
            .saturating_add(buffer)
            .min(self.viewport.max_cycle);

        let needed = self
            .segment_index
            .segments_in_range(buffered_start, buffered_end);
        let unloaded: Vec<usize> = needed
            .into_iter()
            .filter(|idx| !self.loaded_segments.contains(idx))
            .collect();

        if unloaded.is_empty() {
            return false;
        }

        // Load the segments.
        let ctx = self.uscope_ctx.as_ref().unwrap();
        let reader = self.reader.as_mut().unwrap();
        match crate::trace::uscope_source::load_segments(reader, ctx, &unloaded) {
            Ok(result) => {
                for &idx in &unloaded {
                    self.loaded_segments.insert(idx);
                }

                let old_row_count = self.trace.row_count();

                // Clone the trace, merge new data, re-wrap in Arc.
                let mut trace = (*self.trace).clone();
                trace.merge_loaded(result.instructions, result.stages, result.dependencies);

                // Keep max_row at total_instruction_count (not loaded count)
                // so the scrollbar represents the full trace.
                // Only update if the loaded count exceeds the current max
                // (can happen with generator traces where total isn't set).
                if trace.row_count() > self.viewport.max_row {
                    self.viewport.max_row = trace.row_count();
                }

                // If this is the FIRST load (transitioning from 0 rows to having data),
                // position the viewport at the first loaded instruction.
                if old_row_count == 0 && trace.row_count() > 0 {
                    let anchor_cycle = self.viewport.pixel_to_cycle(0.0);
                    let target_row = trace
                        .instructions
                        .iter()
                        .position(|instr| instr.first_cycle >= anchor_cycle as u32)
                        .unwrap_or(0);
                    let view_rows =
                        self.viewport.view_height as f64 / self.viewport.row_height as f64;
                    self.viewport.scroll_row = (target_row as f64 - view_rows / 4.0).max(0.0);
                }
                self.viewport.clamp();

                self.trace = Arc::new(trace);
                true
            }
            Err(e) => {
                eprintln!("Error loading segments: {}", e);
                false
            }
        }
    }

    /// Record a frame and update FPS.
    pub fn record_frame(&mut self) {
        let now = Instant::now();
        self.frame_times.push_back(now);
        while self.frame_times.len() > 1 {
            if now
                .duration_since(*self.frame_times.front().unwrap())
                .as_secs_f64()
                > 1.0
            {
                self.frame_times.pop_front();
            } else {
                break;
            }
        }
        self.fps = self.frame_times.len() as f64;
    }

    /// Auto-follow: adjust scroll_cycle so the visible rows' stages are in view.
    pub fn auto_follow(&mut self) {
        if self.trace.row_count() == 0 {
            return;
        }
        let (row_start, row_end) = self.viewport.visible_row_range();
        let row_end = row_end.min(self.trace.row_count());
        if row_start >= row_end {
            return;
        }

        // Find the cycle range of instructions with stages in the visible rows.
        let mut min_cycle = u32::MAX;
        let mut max_cycle = 0u32;
        let mut has_stages = false;
        for r in row_start..row_end {
            let instr = &self.trace.instructions[r];
            if instr.stage_range.start != instr.stage_range.end {
                min_cycle = min_cycle.min(instr.first_cycle);
                max_cycle = max_cycle.max(instr.last_cycle);
                has_stages = true;
            }
        }
        if !has_stages {
            return;
        }

        let view_cycles = self.viewport.view_width as f64 / self.viewport.pixels_per_cycle as f64;
        let margin = view_cycles * 0.05;
        let target_cycle = (min_cycle as f64 - margin).max(0.0);

        let current = self.viewport.scroll_cycle;
        let distance = (target_cycle - current).abs();

        // For large jumps (stages far from viewport), snap directly.
        // For small adjustments, blend smoothly.
        if distance > view_cycles * 2.0 {
            // Snap: stages are way outside the viewport.
            self.viewport.scroll_cycle = target_cycle;
        } else {
            // Smooth blend for small adjustments.
            let blend = 0.3;
            self.viewport.scroll_cycle = current + (target_cycle - current) * blend;
        }
        self.viewport.clamp();
    }
}

// ─── Tab model ───────────────────────────────────────────────────────────────

/// A single open tab with its own trace state.
struct TabEntry {
    #[allow(dead_code)]
    id: usize,
    file_path: Option<String>,
    state: Entity<TraceState>,
}

fn tab_label(file_path: &Option<String>) -> String {
    match file_path {
        Some(p) => std::path::Path::new(p)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Trace")
            .to_string(),
        None => "Generated".to_string(),
    }
}

// ─── AppView ─────────────────────────────────────────────────────────────────

/// Root application view.
pub struct AppView {
    tabs: Vec<TabEntry>,
    active_tab: usize,
    next_tab_id: usize,
    /// Set to true when a tab change requires rebuilding the panel
    /// but a Window reference was not available (e.g., from an async spawn).
    needs_rebuild: bool,
    pipeline_panel: Entity<PipelinePanel>,
    counter_panel: Entity<CounterPanel>,
    minimap_view: Entity<MinimapView>,
    buffer_panels: Vec<Entity<BufferPanel>>,
    log_panel: Entity<LogPanel>,
    log_buffer: LogBuffer,
    queue_placement: DockPlacement,
    dock_area: Entity<DockArea>,
    status_bar: Entity<StatusBar>,
    search_bar: Entity<SearchBar>,
    goto_bar: Entity<GotoBar>,
    help_overlay: Entity<HelpOverlay>,
    info_overlay: Entity<InfoOverlay>,
    focus_handle: FocusHandle,
    pending_open_urls: Arc<Mutex<Vec<String>>>,
    wcp_client: Option<Arc<WcpClient>>,
    /// Last cursor cycle sent to WCP, to avoid redundant sends.
    last_wcp_cursor: Option<u64>,
}

impl AppView {
    pub fn new(
        file_path: Option<String>,
        pending_open_urls: Arc<Mutex<Vec<String>>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        let help_overlay = cx.new(|cx| HelpOverlay::new(focus_handle.clone(), cx));
        let info_overlay = cx.new(|cx| InfoOverlay::new(focus_handle.clone(), cx));

        // Placeholder state for initial empty panel construction.
        let placeholder = cx.new(|_cx| TraceState::new());

        let pipeline_panel = cx.new(|cx| PipelinePanel::new(placeholder.clone(), window, cx));
        let counter_panel = cx.new(|cx| CounterPanel::new(placeholder.clone(), cx));
        let minimap_view = cx.new(|cx| MinimapView::new(placeholder.clone(), cx));
        let buffer_panels: Vec<Entity<BufferPanel>> = (0..placeholder.read(cx).trace.buffers.len())
            .map(|i| cx.new(|cx| BufferPanel::new(placeholder.clone(), i, cx)))
            .collect();
        let log_buffer = LogBuffer::new();
        let log_panel = cx.new(|cx| LogPanel::new(log_buffer.clone(), cx));
        let queue_placement = DockPlacement::Bottom;
        let dock_area = Self::build_dock_area(
            &pipeline_panel,
            &counter_panel,
            &buffer_panels,
            &log_panel,
            queue_placement,
            false,
            window,
            cx,
        );

        let mut app = Self {
            tabs: Vec::new(),
            active_tab: 0,
            next_tab_id: 0,
            needs_rebuild: false,
            pipeline_panel,
            counter_panel,
            minimap_view,
            buffer_panels,
            log_panel,
            log_buffer,
            queue_placement,
            dock_area,
            status_bar: cx.new(|_cx| StatusBar::new(placeholder.clone())),
            search_bar: cx.new(|cx| SearchBar::new(placeholder.clone(), focus_handle.clone(), cx)),
            goto_bar: cx.new(|cx| GotoBar::new(placeholder.clone(), focus_handle.clone(), cx)),
            help_overlay,
            info_overlay,
            focus_handle,
            pending_open_urls,
            wcp_client: None,
            last_wcp_cursor: None,
        };

        if let Some(ref path) = file_path {
            // CLI argument: open file in a tab.
            app.open_in_new_tab(std::path::Path::new(path), window, cx);
        }
        // No file argument: start empty (drop a trace file or press Cmd+O).

        app
    }

    /// Process any file URLs queued by the platform `on_open_urls` callback.
    fn drain_pending_urls(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let urls: Vec<String> = {
            let mut pending = self.pending_open_urls.lock().unwrap();
            std::mem::take(&mut *pending)
        };
        for url in urls {
            // macOS sends file:// URLs; strip the scheme to get the path.
            let path_str = if let Some(stripped) = url.strip_prefix("file://") {
                // URL-decode percent-encoded characters
                percent_decode(stripped)
            } else {
                url
            };
            let path = std::path::Path::new(&path_str);
            if path.exists() {
                self.open_in_new_tab(path, window, cx);
            }
        }
    }

    /// Get the active tab's state entity, if any tab is open.
    fn active_state(&self) -> Option<&Entity<TraceState>> {
        self.tabs.get(self.active_tab).map(|t| &t.state)
    }

    /// Switch to a different tab, rebuilding the panel to point at the new state.
    fn activate_tab(&mut self, idx: usize, window: &mut Window, cx: &mut Context<Self>) {
        if idx >= self.tabs.len() || idx == self.active_tab {
            return;
        }
        self.active_tab = idx;
        self.rebuild_panel(window, cx);
    }

    /// Build a DockArea with the pipeline panel in the center and buffer panels at the given placement.
    #[allow(clippy::too_many_arguments)]
    fn build_dock_area(
        pipeline_panel: &Entity<PipelinePanel>,
        counter_panel: &Entity<CounterPanel>,
        buffer_panels: &[Entity<BufferPanel>],
        log_panel: &Entity<LogPanel>,
        placement: DockPlacement,
        dock_visible: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<DockArea> {
        let pp = pipeline_panel.clone();
        let cp = counter_panel.clone();
        let lp = log_panel.clone();
        let buffers: Vec<Entity<BufferPanel>> = buffer_panels.to_vec();
        cx.new(|cx| {
            let mut dock_area = DockArea::new("main", None, window, cx);
            let weak = cx.entity().downgrade();
            let center_tabs: Vec<Arc<dyn PanelView>> = vec![Arc::new(pp), Arc::new(cp)];
            let center = DockItem::tabs(center_tabs, &weak, window, cx);
            dock_area.set_center(center, window, cx);

            // Only set a dock if there are buffer panels or a log panel to show.
            if !buffers.is_empty() {
                // Build one dock item per buffer panel. The last one also gets the log tab.
                let mut dock_items: Vec<DockItem> = Vec::new();
                for (i, bp) in buffers.iter().enumerate() {
                    if i == buffers.len() - 1 {
                        // Group last buffer with the log panel as tabs.
                        let tabs: Vec<Arc<dyn PanelView>> =
                            vec![Arc::new(bp.clone()), Arc::new(lp.clone())];
                        dock_items.push(DockItem::tabs(tabs, &weak, window, cx));
                    } else {
                        dock_items.push(DockItem::tab(bp.clone(), &weak, window, cx));
                    }
                }

                let dock_split = match placement {
                    DockPlacement::Bottom => DockItem::h_split(dock_items, &weak, window, cx),
                    _ => DockItem::v_split(dock_items, &weak, window, cx),
                };

                match placement {
                    DockPlacement::Left => {
                        dock_area.set_left_dock(
                            dock_split,
                            Some(px(300.0)),
                            dock_visible,
                            window,
                            cx,
                        );
                        dock_area.set_dock_collapsible(
                            Edges {
                                left: false,
                                right: true,
                                top: true,
                                bottom: true,
                            },
                            window,
                            cx,
                        );
                    }
                    DockPlacement::Right => {
                        dock_area.set_right_dock(
                            dock_split,
                            Some(px(300.0)),
                            dock_visible,
                            window,
                            cx,
                        );
                        dock_area.set_dock_collapsible(
                            Edges {
                                left: true,
                                right: false,
                                top: true,
                                bottom: true,
                            },
                            window,
                            cx,
                        );
                    }
                    DockPlacement::Bottom | DockPlacement::Center => {
                        dock_area.set_bottom_dock(
                            dock_split,
                            Some(px(250.0)),
                            dock_visible,
                            window,
                            cx,
                        );
                    }
                }
            } else {
                // No buffers: just put the log panel in the dock.
                let log_tab = DockItem::tab(lp, &weak, window, cx);
                dock_area.set_bottom_dock(log_tab, Some(px(200.0)), dock_visible, window, cx);
            }

            dock_area
        })
    }

    /// Rebuild pipeline panel, status bar, search bar, and goto bar to point at the active tab's state.
    fn rebuild_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(state) = self.active_state().cloned() {
            self.pipeline_panel = cx.new(|cx| PipelinePanel::new(state.clone(), window, cx));
            self.counter_panel = cx.new(|cx| CounterPanel::new(state.clone(), cx));
            self.minimap_view = cx.new(|cx| MinimapView::new(state.clone(), cx));
            // Preserve dock panel visibility across rebuilds.
            let dock_visible = self
                .dock_area
                .read(cx)
                .is_dock_open(self.queue_placement, cx);
            // Create one BufferPanel per buffer in the trace.
            let num_buffers = state.read(cx).trace.buffers.len();
            self.buffer_panels = (0..num_buffers)
                .map(|i| cx.new(|cx| BufferPanel::new(state.clone(), i, cx)))
                .collect();
            self.log_panel = cx.new(|cx| LogPanel::new(self.log_buffer.clone(), cx));
            self.dock_area = Self::build_dock_area(
                &self.pipeline_panel,
                &self.counter_panel,
                &self.buffer_panels,
                &self.log_panel,
                self.queue_placement,
                dock_visible,
                window,
                cx,
            );
            self.status_bar = cx.new(|_cx| StatusBar::new(state.clone()));
            let focus = self.focus_handle.clone();
            self.search_bar = cx.new(|cx| SearchBar::new(state.clone(), focus.clone(), cx));
            self.goto_bar = cx.new(|cx| GotoBar::new(state.clone(), focus, cx));
        }
        cx.notify();
    }

    /// Open a trace file in a new tab using lazy segment loading.
    fn open_in_new_tab(
        &mut self,
        path: &std::path::Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match crate::trace::uscope_source::open_uscope(path) {
            Ok((reader, trace, segment_index, uscope_ctx, trace_summary)) => {
                let state = cx.new(|_cx| TraceState::new());
                state.update(cx, |ts, _cx| {
                    ts.load_trace_lazy(trace, reader, segment_index, uscope_ctx, trace_summary)
                });
                let id = self.next_tab_id;
                self.next_tab_id += 1;
                self.tabs.push(TabEntry {
                    id,
                    file_path: Some(path.to_string_lossy().into_owned()),
                    state,
                });
                self.active_tab = self.tabs.len() - 1;
                self.rebuild_panel(window, cx);
            }
            Err(e) => {
                eprintln!("Error loading trace file: {}", e);
            }
        }
    }

    /// Close a tab by index.
    fn close_tab(&mut self, idx: usize, window: &mut Window, cx: &mut Context<Self>) {
        if idx >= self.tabs.len() {
            return;
        }
        self.tabs.remove(idx);
        if self.tabs.is_empty() {
            self.active_tab = 0;
            cx.notify();
        } else {
            // Adjust active_tab index.
            if self.active_tab >= self.tabs.len() {
                self.active_tab = self.tabs.len() - 1;
            } else if self.active_tab > idx {
                self.active_tab -= 1;
            } else if self.active_tab == idx {
                self.active_tab = self.active_tab.min(self.tabs.len() - 1);
            }
            self.rebuild_panel(window, cx);
        }
    }

    // ─── Action handlers ─────────────────────────────────────────────────────

    /// Helper: run a closure on the active state if a tab is open.
    fn with_active_state(
        &mut self,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut TraceState, &mut Context<TraceState>),
    ) {
        if let Some(state) = self.active_state().cloned() {
            state.update(cx, f);
        }
    }

    fn handle_zoom_in(&mut self, _: &ZoomIn, _window: &mut Window, cx: &mut Context<Self>) {
        self.with_active_state(cx, |ts, cx| {
            let mid_x = ts.viewport.view_width / 2.0;
            ts.viewport.zoom(1.2, mid_x);
            cx.notify();
        });
    }

    fn handle_zoom_out(&mut self, _: &ZoomOut, _window: &mut Window, cx: &mut Context<Self>) {
        self.with_active_state(cx, |ts, cx| {
            let mid_x = ts.viewport.view_width / 2.0;
            ts.viewport.zoom(1.0 / 1.2, mid_x);
            cx.notify();
        });
    }

    fn handle_zoom_to_fit(&mut self, _: &ZoomToFit, _window: &mut Window, cx: &mut Context<Self>) {
        self.with_active_state(cx, |ts, cx| {
            let vw = if ts.viewport.view_width > 0.0 {
                ts.viewport.view_width
            } else {
                800.0
            };
            let vh = if ts.viewport.view_height > 0.0 {
                ts.viewport.view_height
            } else {
                600.0
            };
            if ts.trace.max_cycle() > 0 {
                ts.viewport.pixels_per_cycle =
                    (vw / ts.trace.max_cycle() as f32).clamp(0.01, 500.0);
                ts.viewport.scroll_cycle = 0.0;
            }
            if ts.trace.row_count() > 0 {
                ts.viewport.row_height = (vh / ts.trace.row_count() as f32).clamp(0.05, 500.0);
                ts.viewport.scroll_row = 0.0;
            }
            ts.viewport.clamp();
            cx.notify();
        });
    }

    fn handle_pan_left(&mut self, _: &PanLeft, _window: &mut Window, cx: &mut Context<Self>) {
        self.with_active_state(cx, |ts, cx| {
            ts.viewport.pan(50.0, 0.0);
            cx.notify();
        });
    }

    fn handle_pan_right(&mut self, _: &PanRight, _window: &mut Window, cx: &mut Context<Self>) {
        self.with_active_state(cx, |ts, cx| {
            ts.viewport.pan(-50.0, 0.0);
            cx.notify();
        });
    }

    fn handle_pan_up(&mut self, _: &PanUp, _window: &mut Window, cx: &mut Context<Self>) {
        self.with_active_state(cx, |ts, cx| {
            ts.viewport.pan(0.0, 50.0);
            cx.notify();
        });
    }

    fn handle_pan_down(&mut self, _: &PanDown, _window: &mut Window, cx: &mut Context<Self>) {
        self.with_active_state(cx, |ts, cx| {
            ts.viewport.pan(0.0, -50.0);
            cx.notify();
        });
    }

    fn handle_select_next(&mut self, _: &SelectNext, _window: &mut Window, cx: &mut Context<Self>) {
        self.with_active_state(cx, |ts, cx| {
            let max = ts.trace.row_count().saturating_sub(1);
            let next = ts.selected_row.map(|r| (r + 1).min(max)).unwrap_or(0);
            ts.selected_row = Some(next);
            cx.notify();
        });
    }

    fn handle_select_previous(
        &mut self,
        _: &SelectPrevious,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.with_active_state(cx, |ts, cx| {
            let prev = ts.selected_row.map(|r| r.saturating_sub(1)).unwrap_or(0);
            ts.selected_row = Some(prev);
            cx.notify();
        });
    }

    fn handle_toggle_search(
        &mut self,
        _: &ToggleSearch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.search_bar.update(cx, |sb, cx| sb.toggle(window, cx));
    }

    fn handle_goto_cycle(&mut self, _: &GotoCycle, window: &mut Window, cx: &mut Context<Self>) {
        self.goto_bar.update(cx, |gb, cx| gb.toggle(window, cx));
    }

    fn handle_toggle_help(&mut self, _: &ToggleHelp, window: &mut Window, cx: &mut Context<Self>) {
        self.help_overlay.update(cx, |h, cx| h.toggle(window, cx));
    }

    fn handle_toggle_info(&mut self, _: &ToggleInfo, window: &mut Window, cx: &mut Context<Self>) {
        // Update metadata from the active tab before showing.
        if let Some(state) = self.active_state().cloned() {
            let metadata = state.read(cx).trace.metadata.clone();
            self.info_overlay.update(cx, |info, cx| {
                info.set_metadata(metadata, cx);
            });
        }
        self.info_overlay.update(cx, |h, cx| h.toggle(window, cx));
    }

    fn handle_generate_trace(
        &mut self,
        _: &GenerateTrace,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let trace = generator::generate(&GeneratorConfig {
            instruction_count: 10_000,
            counter_count: 4,
            ..Default::default()
        });
        let state = cx.new(|_cx| TraceState::new());
        state.update(cx, |ts, _cx| {
            ts.load_trace(trace, None, SegmentIndex::default())
        });
        let id = self.next_tab_id;
        self.next_tab_id += 1;
        self.tabs.push(TabEntry {
            id,
            file_path: None,
            state,
        });
        self.active_tab = self.tabs.len() - 1;
        self.rebuild_panel(window, cx);
    }

    fn handle_open_file(&mut self, _: &OpenFile, _window: &mut Window, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        });

        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = receiver.await {
                if let Some(path) = paths.first() {
                    match crate::trace::uscope_source::open_uscope(path) {
                        Ok((reader, trace, segment_index, uscope_ctx, trace_summary)) => {
                            let path_str = path.to_string_lossy().into_owned();
                            let _ = cx.update(|cx| {
                                let _ = this.update(cx, |app, cx| {
                                    let state = cx.new(|_cx| TraceState::new());
                                    state.update(cx, |ts, _cx| {
                                        ts.load_trace_lazy(
                                            trace,
                                            reader,
                                            segment_index,
                                            uscope_ctx,
                                            trace_summary,
                                        )
                                    });
                                    let id = app.next_tab_id;
                                    app.next_tab_id += 1;
                                    app.tabs.push(TabEntry {
                                        id,
                                        file_path: Some(path_str),
                                        state,
                                    });
                                    app.active_tab = app.tabs.len() - 1;
                                    app.needs_rebuild = true;
                                    cx.notify();
                                });
                            });
                        }
                        Err(e) => {
                            eprintln!("Error loading trace: {}", e);
                        }
                    }
                }
            }
        })
        .detach();
    }

    fn handle_reload_trace(
        &mut self,
        _: &ReloadTrace,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.tabs.is_empty() {
            return;
        }
        let file_path = self.tabs[self.active_tab].file_path.clone();
        if let Some(ref path) = file_path {
            match crate::trace::uscope_source::open_uscope(std::path::Path::new(path)) {
                Ok((reader, trace, segment_index, uscope_ctx, trace_summary)) => {
                    self.with_active_state(cx, |ts, cx| {
                        ts.load_trace_lazy(trace, reader, segment_index, uscope_ctx, trace_summary);
                        cx.notify();
                    });
                }
                Err(e) => {
                    eprintln!("Error reloading trace: {}", e);
                }
            }
        } else {
            let trace = generator::generate(&GeneratorConfig {
                instruction_count: 10_000,
                ..Default::default()
            });
            self.with_active_state(cx, |ts, cx| {
                ts.load_trace(trace, None, SegmentIndex::default());
                cx.notify();
            });
        }
    }

    fn handle_close_tab(&mut self, _: &CloseTab, window: &mut Window, cx: &mut Context<Self>) {
        if !self.tabs.is_empty() {
            let idx = self.active_tab;
            self.close_tab(idx, window, cx);
        }
    }

    fn handle_next_tab(&mut self, _: &NextTab, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.len() > 1 {
            let next = (self.active_tab + 1) % self.tabs.len();
            self.activate_tab(next, window, cx);
        }
    }

    fn handle_prev_tab(&mut self, _: &PrevTab, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.len() > 1 {
            let prev = if self.active_tab == 0 {
                self.tabs.len() - 1
            } else {
                self.active_tab - 1
            };
            self.activate_tab(prev, window, cx);
        }
    }

    fn handle_add_cursor(&mut self, _: &AddCursor, _window: &mut Window, cx: &mut Context<Self>) {
        self.with_active_state(cx, |ts, cx| {
            let center_cycle = ts.viewport.scroll_cycle
                + ts.viewport.view_width as f64 / (2.0 * ts.viewport.pixels_per_cycle as f64);
            ts.cursor_state.add_cursor(center_cycle);
            cx.notify();
        });
    }

    fn handle_remove_cursor(
        &mut self,
        _: &RemoveCursor,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.with_active_state(cx, |ts, cx| {
            ts.cursor_state.remove_active();
            cx.notify();
        });
    }

    fn handle_next_cursor(&mut self, _: &NextCursor, _window: &mut Window, cx: &mut Context<Self>) {
        self.with_active_state(cx, |ts, cx| {
            ts.cursor_state.next();
            cx.notify();
        });
    }

    fn handle_prev_cursor(&mut self, _: &PrevCursor, _window: &mut Window, cx: &mut Context<Self>) {
        self.with_active_state(cx, |ts, cx| {
            ts.cursor_state.prev();
            cx.notify();
        });
    }

    fn handle_cursor_undo(&mut self, _: &CursorUndo, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(state) = self.active_state().cloned() {
            state.update(cx, |ts, cx| {
                ts.cursor_state.undo();
                cx.notify();
            });
        }
    }

    fn handle_cursor_redo(&mut self, _: &CursorRedo, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(state) = self.active_state().cloned() {
            state.update(cx, |ts, cx| {
                ts.cursor_state.redo();
                cx.notify();
            });
        }
    }

    fn handle_toggle_overlay(
        &mut self,
        _: &ToggleOverlay,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(state) = self.active_state().cloned() {
            state.update(cx, |ts, cx| {
                if ts.overlay_counter.is_some() {
                    ts.overlay_counter = None;
                } else if !ts.trace.counters.is_empty() {
                    ts.overlay_counter = Some(0);
                }
                cx.notify();
            });
        }
    }

    fn handle_toggle_queues(
        &mut self,
        _: &ToggleQueues,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Only toggle for bottom dock (left/right have no collapsed header).
        if self.queue_placement == DockPlacement::Bottom {
            self.dock_area.update(cx, |da, cx| {
                da.toggle_dock(DockPlacement::Bottom, window, cx)
            });
        }
    }

    fn switch_queue_layout(
        &mut self,
        placement: DockPlacement,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.queue_placement == placement {
            return;
        }
        self.queue_placement = placement;
        if let Some(state) = self.active_state().cloned() {
            self.pipeline_panel = cx.new(|cx| PipelinePanel::new(state.clone(), window, cx));
            self.counter_panel = cx.new(|cx| CounterPanel::new(state.clone(), cx));
            self.minimap_view = cx.new(|cx| MinimapView::new(state.clone(), cx));
            let num_buffers = state.read(cx).trace.buffers.len();
            self.buffer_panels = (0..num_buffers)
                .map(|i| cx.new(|cx| BufferPanel::new(state.clone(), i, cx)))
                .collect();
            self.log_panel = cx.new(|cx| LogPanel::new(self.log_buffer.clone(), cx));
            self.dock_area = Self::build_dock_area(
                &self.pipeline_panel,
                &self.counter_panel,
                &self.buffer_panels,
                &self.log_panel,
                self.queue_placement,
                true, // always open when actively switching layout
                window,
                cx,
            );
            self.status_bar = cx.new(|_cx| StatusBar::new(state.clone()));
            let focus = self.focus_handle.clone();
            self.search_bar = cx.new(|cx| SearchBar::new(state.clone(), focus.clone(), cx));
            self.goto_bar = cx.new(|cx| GotoBar::new(state.clone(), focus, cx));
        }
        self.focus_handle.focus(window);
        cx.notify();
    }

    fn handle_layout_bottom(
        &mut self,
        _: &LayoutBottom,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_queue_layout(DockPlacement::Bottom, window, cx);
    }

    fn handle_layout_left(&mut self, _: &LayoutLeft, window: &mut Window, cx: &mut Context<Self>) {
        self.switch_queue_layout(DockPlacement::Left, window, cx);
    }

    fn handle_layout_right(
        &mut self,
        _: &LayoutRight,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_queue_layout(DockPlacement::Right, window, cx);
    }

    fn handle_wcp_connect(&mut self, _: &WcpConnect, _window: &mut Window, cx: &mut Context<Self>) {
        if self.wcp_client.is_some() {
            return; // Already connected.
        }
        let log = self.log_buffer.clone();
        cx.spawn(async move |this, cx| {
            match WcpClient::connect("127.0.0.1:54321", log.clone()).await {
                Ok(client) => {
                    let client = Arc::new(client);
                    let _ = cx.update(|cx| {
                        let _ = this.update(cx, |app, cx| {
                            app.wcp_client = Some(client);
                            app.status_bar.update(cx, |sb, cx| {
                                sb.wcp_connected = true;
                                cx.notify();
                            });
                            app.log_buffer.push("WCP: connected to Surfer");
                            cx.notify();
                        });
                    });
                }
                Err(e) => {
                    log.push(format!("WCP: connection failed: {}", e));
                }
            }
        })
        .detach();
    }

    fn handle_wcp_disconnect(
        &mut self,
        _: &WcpDisconnect,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.wcp_client.take().is_some() {
            self.last_wcp_cursor = None;
            self.status_bar.update(cx, |sb, cx| {
                sb.wcp_connected = false;
                cx.notify();
            });
            self.log_buffer.push("WCP: disconnected");
            cx.notify();
        }
    }

    /// Push the active cursor position to Surfer via WCP.
    fn sync_cursor_to_wcp(&mut self, cx: &mut Context<Self>) {
        let Some(client) = &self.wcp_client else {
            return;
        };
        let Some(state) = self.active_state() else {
            return;
        };
        let ts = state.read(cx);
        let Some(period_ps) = ts.trace.period_ps else {
            return;
        };
        let cycle = ts.cursor_state.cursors[ts.cursor_state.active_idx].cycle;
        // TODO: make configurable — currently assumes VCD $timescale 100ps.
        let timestamp = ((cycle * period_ps as f64) / 100.0) as u64;

        // Avoid redundant sends.
        if self.last_wcp_cursor == Some(timestamp) {
            return;
        }
        self.last_wcp_cursor = Some(timestamp);

        let client = client.clone();
        let log = self.log_buffer.clone();
        cx.spawn(async move |_, _| {
            if let Err(e) = client.send_cursor(timestamp).await {
                log.push(format!("WCP: send_cursor failed: {}", e));
            }
        })
        .detach();
    }
}

impl Focusable for AppView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for AppView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Deferred panel rebuild (from async open_file where Window wasn't available).
        if self.needs_rebuild {
            self.needs_rebuild = false;
            self.rebuild_panel(window, cx);
        }

        // Process files dropped on dock icon or opened via Finder.
        if !self.pending_open_urls.lock().unwrap().is_empty() {
            self.drain_pending_urls(window, cx);
        }

        // Sync cursor to Surfer via WCP (deduped, only sends on change).
        if self.wcp_client.is_some() {
            self.sync_cursor_to_wcp(cx);
        }

        let has_tabs = !self.tabs.is_empty();
        let active_state = self.active_state().cloned();
        let active_tab_idx = self.active_tab;

        // Build close buttons for each tab. We need entity ref for the click handlers.
        let entity = cx.entity().clone();
        let tab_bar = TabBar::new("trace-tabs")
            .children(self.tabs.iter().enumerate().map(|(idx, entry)| {
                let label = tab_label(&entry.file_path);
                let close_entity = entity.clone();
                let group_id: SharedString = format!("tab-{idx}").into();
                let group_id2 = group_id.clone();
                Tab::new()
                    .group(group_id)
                    .map(|mut tab| {
                        tab.style().overflow.x = Some(gpui::Overflow::Visible);
                        tab.style().overflow.y = Some(gpui::Overflow::Visible);
                        tab
                    })
                    .child(
                        div()
                            .relative()
                            .w_full()
                            .h_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            // Extra right padding to make room for the × button
                            .pr(px(10.0))
                            .pl(px(10.0))
                            .text_sm()
                            .child(div().mt(px(-2.0)).child(label))
                            .child(
                                div()
                                    .id(("close-tab", idx))
                                    .absolute()
                                    .right(px(-9.0))
                                    .top_0()
                                    .bottom_0()
                                    .flex()
                                    .items_center()
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .w(px(16.0))
                                            .h(px(16.0))
                                            .rounded(px(4.0))
                                            .cursor_pointer()
                                            .opacity(0.0)
                                            .group_hover(group_id2, |s| s.opacity(0.5))
                                            .hover(|s| s.opacity(1.0).bg(colors::GRID_LINE))
                                            .text_color(colors::TEXT_PRIMARY)
                                            .child(Icon::new(IconName::Close).size(px(10.0))),
                                    )
                                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                        cx.stop_propagation();
                                    })
                                    .on_click(move |_event: &ClickEvent, window, cx| {
                                        cx.stop_propagation();
                                        close_entity.update(cx, |app: &mut AppView, cx| {
                                            app.close_tab(idx, window, cx);
                                        });
                                    }),
                            ),
                    )
            }))
            .selected_index(active_tab_idx)
            .on_click({
                let switch_entity = entity.clone();
                move |&idx, window, cx| {
                    switch_entity.update(cx, |app: &mut AppView, cx| {
                        app.activate_tab(idx, window, cx);
                    });
                }
            })
            .bg(colors::BG_PRIMARY);

        div()
            .id("app-root")
            .size_full()
            .flex()
            .flex_col()
            .bg(colors::BG_PRIMARY)
            .text_color(colors::TEXT_PRIMARY)
            .font_family(".SystemUIFont")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::handle_zoom_in))
            .on_action(cx.listener(Self::handle_zoom_out))
            .on_action(cx.listener(Self::handle_zoom_to_fit))
            .on_action(cx.listener(Self::handle_pan_left))
            .on_action(cx.listener(Self::handle_pan_right))
            .on_action(cx.listener(Self::handle_pan_up))
            .on_action(cx.listener(Self::handle_pan_down))
            .on_action(cx.listener(Self::handle_select_next))
            .on_action(cx.listener(Self::handle_select_previous))
            .on_action(cx.listener(Self::handle_toggle_search))
            .on_action(cx.listener(Self::handle_goto_cycle))
            .on_action(cx.listener(Self::handle_toggle_help))
            .on_action(cx.listener(Self::handle_toggle_info))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key_char.as_deref() == Some("?") {
                    this.help_overlay.update(cx, |h, cx| h.toggle(window, cx));
                }
            }))
            .on_action(cx.listener(Self::handle_generate_trace))
            .on_action(cx.listener(Self::handle_open_file))
            .on_action(cx.listener(Self::handle_reload_trace))
            .on_action(cx.listener(Self::handle_close_tab))
            .on_action(cx.listener(Self::handle_next_tab))
            .on_action(cx.listener(Self::handle_prev_tab))
            .on_action(cx.listener(Self::handle_add_cursor))
            .on_action(cx.listener(Self::handle_remove_cursor))
            .on_action(cx.listener(Self::handle_next_cursor))
            .on_action(cx.listener(Self::handle_prev_cursor))
            .on_action(cx.listener(Self::handle_cursor_undo))
            .on_action(cx.listener(Self::handle_cursor_redo))
            .on_action(cx.listener(Self::handle_toggle_overlay))
            .on_action(cx.listener(Self::handle_toggle_queues))
            .on_action(cx.listener(Self::handle_layout_bottom))
            .on_action(cx.listener(Self::handle_layout_left))
            .on_action(cx.listener(Self::handle_layout_right))
            .on_action(cx.listener(Self::handle_wcp_connect))
            .on_action(cx.listener(Self::handle_wcp_disconnect))
            // File drag & drop onto window — opens in new tab.
            .on_drop(cx.listener(|this, paths: &ExternalPaths, window, cx| {
                if let Some(path) = paths.paths().first() {
                    this.open_in_new_tab(path, window, cx);
                }
            }))
            // Title bar.
            .child({
                // Pass a dummy state if no tabs open.
                let title_state = active_state
                    .clone()
                    .unwrap_or_else(|| cx.new(|_| TraceState::new()));
                render_title_bar(&title_state, cx)
            })
            // Tab bar (only if tabs exist).
            .when(has_tabs, |el| el.child(tab_bar))
            // Minimap (only if tabs exist).
            .when(has_tabs, |el| el.child(self.minimap_view.clone()))
            // Main content or empty state.
            .child(if has_tabs {
                div()
                    .flex_1()
                    .w_full()
                    .relative()
                    .font_family("SF Mono, Menlo, Monaco, monospace")
                    .child(self.dock_area.clone())
                    .child(self.search_bar.clone())
                    .child(self.goto_bar.clone())
            } else {
                div()
                    .flex_1()
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .text_size(px(16.0))
                            .text_color(colors::TEXT_DIMMED)
                            .child("Drop a Konata trace file here"),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(colors::TEXT_ROW_NUMBER)
                            .child("or press Cmd+O to open"),
                    )
            })
            // Status bar (only if tabs exist).
            .when(has_tabs, |el| {
                el.child(
                    div()
                        .font_family("SF Mono, Menlo, Monaco, monospace")
                        .child(self.status_bar.clone()),
                )
            })
            // Help overlay.
            .child(self.help_overlay.clone())
            // Info overlay.
            .child(self.info_overlay.clone())
            // Annotation tooltip (topmost layer).
            .when_some(
                active_state.and_then(|s| s.read(cx).tooltip_hover.clone()),
                |el, hover| {
                    let x = f32::from(hover.position.x) + 12.0;
                    let y = f32::from(hover.position.y) + 16.0;
                    el.child(
                        div()
                            .absolute()
                            .top(px(y))
                            .left(px(x))
                            .max_w(px(360.0))
                            .bg(Hsla {
                                h: 220.0 / 360.0,
                                s: 0.15,
                                l: 0.13,
                                a: 0.92,
                            })
                            .border_1()
                            .border_color(Hsla {
                                h: 220.0 / 360.0,
                                s: 0.20,
                                l: 0.28,
                                a: 0.5,
                            })
                            .rounded(px(8.0))
                            .px_3()
                            .py(px(6.0))
                            .shadow_lg()
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(colors::TEXT_PRIMARY)
                                    .font_family("Menlo")
                                    .children(
                                        hover
                                            .text
                                            .split('\n')
                                            .map(|line| div().child(line.to_string()))
                                            .collect::<Vec<_>>(),
                                    ),
                            ),
                    )
                },
            )
    }
}
