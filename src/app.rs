use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::tab::{Tab, TabBar};
use gpui_component::{Icon, IconName};

use crate::interaction::actions::*;
use crate::interaction::viewport::ViewportState;
use crate::theme::colors;
use crate::title_bar::render_title_bar;
use crate::trace::generator::{self, GeneratorConfig};
use crate::trace::model::PipelineTrace;
use crate::trace::TraceRegistry;
use crate::views::goto_bar::GotoBar;
use crate::views::help_overlay::HelpOverlay;
use crate::views::info_overlay::InfoOverlay;
use crate::views::pipeline_panel::PipelinePanel;
use crate::views::queue_panel::QueuePanel;
use crate::views::search_bar::SearchBar;
use crate::views::status_bar::StatusBar;

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

/// State for multicursor support.
#[derive(Clone)]
pub struct CursorState {
    pub cursors: Vec<Cursor>,
    pub active_idx: usize,
    next_color: usize,
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
        }
    }

    pub fn add_cursor(&mut self, cycle: f64) {
        let color_idx = self.next_color % colors::CURSOR_PALETTE.len();
        self.next_color += 1;
        self.cursors.push(Cursor { cycle, color_idx });
        self.active_idx = self.cursors.len() - 1;
    }

    pub fn remove_active(&mut self) {
        if self.cursors.len() <= 1 {
            return;
        }
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
    pub trace: PipelineTrace,
    pub viewport: ViewportState,
    pub selected_row: Option<usize>,
    /// Active tooltip hover info, if any.
    pub tooltip_hover: Option<TooltipHover>,
    /// Multicursor state.
    pub cursor_state: CursorState,
    /// FPS tracking.
    pub frame_times: VecDeque<Instant>,
    pub fps: f64,
}

impl TraceState {
    pub fn new() -> Self {
        Self {
            trace: PipelineTrace::new(),
            viewport: ViewportState::new(),
            selected_row: None,
            tooltip_hover: None,
            cursor_state: CursorState::new(),
            frame_times: VecDeque::new(),
            fps: 0.0,
        }
    }

    pub fn load_trace(&mut self, trace: PipelineTrace) {
        self.viewport.max_cycle = trace.max_cycle();
        self.viewport.max_row = trace.row_count();
        self.viewport.clamp();
        self.trace = trace;
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

        let min_cycle = (row_start..row_end)
            .filter(|&r| {
                let instr = &self.trace.instructions[r];
                instr.stage_range.start != instr.stage_range.end
            })
            .map(|r| self.trace.instructions[r].first_cycle)
            .min();

        let first_cycle = match min_cycle {
            Some(c) => c,
            None => return,
        };

        let margin = self.viewport.view_width as f64 * 0.05 / self.viewport.pixels_per_cycle as f64;
        let target_cycle = (first_cycle as f64 - margin).max(0.0);

        let current = self.viewport.scroll_cycle;
        let blend = 0.3;
        self.viewport.scroll_cycle = current + (target_cycle - current) * blend;
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
    queue_panel: Entity<QueuePanel>,
    status_bar: Entity<StatusBar>,
    search_bar: Entity<SearchBar>,
    goto_bar: Entity<GotoBar>,
    help_overlay: Entity<HelpOverlay>,
    info_overlay: Entity<InfoOverlay>,
    focus_handle: FocusHandle,
    pending_open_urls: Arc<Mutex<Vec<String>>>,
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

        let mut app = Self {
            tabs: Vec::new(),
            active_tab: 0,
            next_tab_id: 0,
            needs_rebuild: false,
            pipeline_panel: cx.new(|cx| PipelinePanel::new(placeholder.clone(), window, cx)),
            queue_panel: cx.new(|cx| QueuePanel::new(placeholder.clone(), cx)),
            status_bar: cx.new(|_cx| StatusBar::new(placeholder.clone())),
            search_bar: cx.new(|cx| SearchBar::new(placeholder.clone(), focus_handle.clone(), cx)),
            goto_bar: cx.new(|cx| GotoBar::new(placeholder.clone(), focus_handle.clone(), cx)),
            help_overlay,
            info_overlay,
            focus_handle,
            pending_open_urls,
        };

        if let Some(ref path) = file_path {
            // CLI argument: open file in a tab.
            app.open_in_new_tab(std::path::Path::new(path), window, cx);
        } else if cfg!(debug_assertions) {
            // Debug builds: open a generated trace for quick iteration.
            let trace = generator::generate(&GeneratorConfig {
                instruction_count: 10_000,
                ..Default::default()
            });
            let state = cx.new(|_cx| TraceState::new());
            state.update(cx, |ts, _cx| ts.load_trace(trace));
            let id = app.next_tab_id;
            app.next_tab_id += 1;
            app.tabs.push(TabEntry {
                id,
                file_path: None,
                state,
            });
            app.active_tab = 0;
            app.rebuild_panel(window, cx);
        }
        // Release builds with no file argument: start empty.

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

    /// Rebuild pipeline panel, status bar, search bar, and goto bar to point at the active tab's state.
    fn rebuild_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(state) = self.active_state().cloned() {
            self.pipeline_panel = cx.new(|cx| PipelinePanel::new(state.clone(), window, cx));
            // Preserve queue panel visibility across rebuilds.
            let queue_visible = self.queue_panel.read(cx).is_visible();
            self.queue_panel = cx.new(|cx| {
                let mut qp = QueuePanel::new(state.clone(), cx);
                if queue_visible {
                    qp.toggle(cx);
                }
                qp
            });
            self.status_bar = cx.new(|_cx| StatusBar::new(state.clone()));
            let focus = self.focus_handle.clone();
            self.search_bar = cx.new(|cx| SearchBar::new(state.clone(), focus.clone(), cx));
            self.goto_bar = cx.new(|cx| GotoBar::new(state.clone(), focus, cx));
        }
        cx.notify();
    }

    /// Open a trace file in a new tab.
    fn open_in_new_tab(
        &mut self,
        path: &std::path::Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let registry = TraceRegistry::new();
        match registry.load_file(path) {
            Ok(trace) => {
                let state = cx.new(|_cx| TraceState::new());
                state.update(cx, |ts, _cx| ts.load_trace(trace));
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
            ..Default::default()
        });
        let state = cx.new(|_cx| TraceState::new());
        state.update(cx, |ts, _cx| ts.load_trace(trace));
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
                    let registry = TraceRegistry::new();
                    match registry.load_file(path) {
                        Ok(trace) => {
                            let path_str = path.to_string_lossy().into_owned();
                            let _ = cx.update(|cx| {
                                let _ = this.update(cx, |app, cx| {
                                    let state = cx.new(|_cx| TraceState::new());
                                    state.update(cx, |ts, _cx| ts.load_trace(trace));
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
        let trace = if let Some(ref path) = file_path {
            let registry = TraceRegistry::new();
            match registry.load_file(std::path::Path::new(path)) {
                Ok(trace) => trace,
                Err(e) => {
                    eprintln!("Error reloading trace: {}", e);
                    return;
                }
            }
        } else {
            generator::generate(&GeneratorConfig {
                instruction_count: 10_000,
                ..Default::default()
            })
        };
        self.with_active_state(cx, |ts, cx| {
            ts.load_trace(trace);
            cx.notify();
        });
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

    fn handle_toggle_queues(
        &mut self,
        _: &ToggleQueues,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.queue_panel.update(cx, |qp, cx| qp.toggle(cx));
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
            .on_action(cx.listener(Self::handle_toggle_queues))
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
            // Main content or empty state.
            .child(if has_tabs {
                div()
                    .flex_1()
                    .w_full()
                    .overflow_hidden()
                    .relative()
                    .font_family("SF Mono, Menlo, Monaco, monospace")
                    .child(self.pipeline_panel.clone())
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
            // Queue panel (only if tabs exist).
            .when(has_tabs, |el| {
                el.child(
                    div()
                        .font_family("SF Mono, Menlo, Monaco, monospace")
                        .child(self.queue_panel.clone()),
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
