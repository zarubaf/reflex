use gpui::*;

use crate::app::TraceState;
use crate::theme::colors;

pub struct SearchBar {
    state: Entity<TraceState>,
    query: String,
    results: Vec<usize>,
    current_result: usize,
    visible: bool,
    focus_handle: FocusHandle,
    /// Focus handle of the parent to return focus to on close.
    parent_focus: FocusHandle,
    selected_all: bool,
}

impl SearchBar {
    pub fn new(
        state: Entity<TraceState>,
        parent_focus: FocusHandle,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            state,
            query: String::new(),
            results: Vec::new(),
            current_result: 0,
            visible: false,
            focus_handle: cx.focus_handle(),
            parent_focus,
            selected_all: false,
        }
    }

    pub fn toggle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.visible = !self.visible;
        if self.visible {
            self.query.clear();
            self.results.clear();
            self.current_result = 0;
            self.selected_all = false;
            window.focus(&self.focus_handle);
        } else {
            // Return focus to parent so Cmd+F works again.
            window.focus(&self.parent_focus);
        }
        cx.notify();
    }

    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.visible = false;
        self.selected_all = false;
        window.focus(&self.parent_focus);
        cx.notify();
    }

    fn do_search(&mut self, cx: &App) {
        self.results.clear();
        if self.query.is_empty() {
            return;
        }
        let query_lower = self.query.to_lowercase();
        let ts = self.state.read(cx);
        for (i, instr) in ts.trace.instructions.iter().enumerate() {
            if instr.disasm.to_lowercase().contains(&query_lower) {
                self.results.push(i);
            }
        }
        self.current_result = 0;
    }

    fn navigate_to_current(&mut self, cx: &mut App) {
        if let Some(&row) = self.results.get(self.current_result) {
            self.state.update(cx, |ts, cx| {
                ts.selected_row = Some(row);
                // Scroll vertically to center the result.
                let visible_rows = ts.viewport.view_height as f64 / ts.viewport.row_height as f64;
                ts.viewport.scroll_row = (row as f64 - visible_rows / 2.0).max(0.0);
                // Auto-follow horizontally.
                ts.auto_follow();
                cx.notify();
            });
        }
    }
}

impl Render for SearchBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.visible {
            return div().id("search-bar-hidden");
        }

        let result_text = if self.results.is_empty() {
            if self.query.is_empty() {
                "Type to search".to_string()
            } else {
                "No results".to_string()
            }
        } else {
            format!("{}/{}", self.current_result + 1, self.results.len())
        };

        div()
            .id("search-bar")
            .track_focus(&self.focus_handle)
            .key_context("SearchBar")
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                let keystroke = &event.keystroke;
                if keystroke.modifiers.platform && keystroke.key == "a" {
                    this.selected_all = true;
                    cx.notify();
                } else if keystroke.modifiers.platform && keystroke.key == "f" {
                    // Cmd+F while search is open: close it.
                    this.close(window, cx);
                } else if keystroke.key == "backspace" {
                    if this.selected_all {
                        this.query.clear();
                        this.selected_all = false;
                    } else {
                        this.query.pop();
                    }
                    this.do_search(cx);
                    this.navigate_to_current(cx);
                    cx.notify();
                } else if keystroke.key == "enter" {
                    if !this.results.is_empty() {
                        this.current_result = (this.current_result + 1) % this.results.len();
                        this.navigate_to_current(cx);
                        cx.notify();
                    }
                } else if keystroke.key == "escape" {
                    this.close(window, cx);
                } else if !keystroke.modifiers.platform
                    && !keystroke.modifiers.control
                    && keystroke.key_char.is_some()
                {
                    if let Some(ref ch) = keystroke.key_char {
                        if this.selected_all {
                            this.query.clear();
                            this.selected_all = false;
                        }
                        this.query.push_str(ch);
                        this.do_search(cx);
                        this.navigate_to_current(cx);
                        cx.notify();
                    }
                }
            }))
            .absolute()
            .top(px(4.0))
            .right(px(4.0))
            .w(px(300.0))
            .h(px(28.0))
            .bg(colors::BG_PRIMARY)
            .border_1()
            .border_color(colors::GRID_LINE_MAJOR)
            .rounded(px(4.0))
            .flex()
            .items_center()
            .px_2()
            .gap_2()
            .text_size(px(12.0))
            .text_color(colors::TEXT_PRIMARY)
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .child(if self.query.is_empty() {
                        div()
                            .text_color(colors::TEXT_DIMMED)
                            .child("Search...")
                    } else if self.selected_all {
                        div()
                            .bg(colors::SELECTION_BG)
                            .child(self.query.clone())
                    } else {
                        div().child(self.query.clone())
                    }),
            )
            .child(
                div()
                    .text_color(colors::TEXT_DIMMED)
                    .text_size(px(10.0))
                    .flex_shrink_0()
                    .child(result_text),
            )
    }
}
