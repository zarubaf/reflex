use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use gpui::*;

use crate::theme::colors;

const MAX_LOG_LINES: usize = 500;

/// Shared log buffer that can be written to from anywhere.
#[derive(Clone)]
pub struct LogBuffer(pub Arc<Mutex<VecDeque<String>>>);

impl LogBuffer {
    pub fn new() -> Self {
        Self(Arc::new(Mutex::new(VecDeque::new())))
    }

    pub fn push(&self, msg: impl Into<String>) {
        let mut buf = self.0.lock().unwrap();
        buf.push_back(msg.into());
        while buf.len() > MAX_LOG_LINES {
            buf.pop_front();
        }
    }
}

/// Panel that displays log messages.
pub struct LogPanel {
    log: LogBuffer,
    focus_handle: FocusHandle,
}

impl LogPanel {
    pub fn new(log: LogBuffer, cx: &mut Context<Self>) -> Self {
        Self {
            log,
            focus_handle: cx.focus_handle(),
        }
    }
}

impl Render for LogPanel {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let lines: Vec<String> = self.log.0.lock().unwrap().iter().cloned().collect();

        div()
            .id("log-panel")
            .size_full()
            .bg(colors::BG_PRIMARY)
            .text_size(px(11.0))
            .font_family("Menlo")
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
                    .child(format!("Log ({})", lines.len())),
            )
            .child(
                div()
                    .id("log-scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .children(lines.into_iter().map(|line| {
                        div()
                            .px_2()
                            .py(px(1.0))
                            .text_color(colors::TEXT_PRIMARY)
                            .child(line)
                    })),
            )
    }
}

impl Focusable for LogPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<gpui_component::dock::PanelEvent> for LogPanel {}

impl gpui_component::dock::Panel for LogPanel {
    fn panel_name(&self) -> &'static str {
        "LogPanel"
    }

    fn title(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        "Log"
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
