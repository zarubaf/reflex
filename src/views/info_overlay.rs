use gpui::prelude::FluentBuilder as _;
use gpui::*;

use crate::theme::colors;

pub struct InfoOverlay {
    visible: bool,
    focus_handle: FocusHandle,
    parent_focus: FocusHandle,
    entries: Vec<(String, String)>,
}

impl InfoOverlay {
    pub fn new(parent_focus: FocusHandle, cx: &mut Context<Self>) -> Self {
        Self {
            visible: false,
            focus_handle: cx.focus_handle(),
            parent_focus,
            entries: Vec::new(),
        }
    }

    pub fn toggle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.visible = !self.visible;
        if self.visible {
            window.focus(&self.focus_handle);
        } else {
            window.focus(&self.parent_focus);
        }
        cx.notify();
    }

    pub fn set_metadata(&mut self, entries: Vec<(String, String)>, cx: &mut Context<Self>) {
        self.entries = entries;
        cx.notify();
    }
}

impl Render for InfoOverlay {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.visible {
            return div().id("info-hidden");
        }

        div()
            .id("info-overlay")
            .track_focus(&self.focus_handle)
            .key_context("InfoOverlay")
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" {
                    this.visible = false;
                    window.focus(&this.parent_focus);
                    cx.notify();
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    this.visible = false;
                    window.focus(&this.parent_focus);
                    cx.notify();
                }),
            )
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .size_full()
            .flex()
            .justify_center()
            .items_center()
            .bg(Hsla {
                h: 0.0,
                s: 0.0,
                l: 0.0,
                a: 0.6,
            })
            .child(
                div()
                    .w(px(480.0))
                    .bg(colors::BG_SECONDARY)
                    .border_1()
                    .border_color(colors::GRID_LINE_MAJOR)
                    .rounded(px(8.0))
                    .p_4()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_size(px(15.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(colors::TEXT_PRIMARY)
                            .pb_2()
                            .child("Trace Info"),
                    )
                    .overflow_hidden()
                    .children(self.entries.iter().map(|(key, value)| {
                        div()
                            .flex()
                            .items_start()
                            .gap_2()
                            .py(px(2.0))
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(colors::TEXT_DIMMED)
                                    .min_w(px(120.0))
                                    .flex_shrink_0()
                                    .child(key.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(colors::TEXT_PRIMARY)
                                    .min_w(px(0.0))
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .whitespace_nowrap()
                                    .child(value.clone()),
                            )
                    }))
                    .when(self.entries.is_empty(), |el| {
                        el.child(
                            div()
                                .text_size(px(11.0))
                                .text_color(colors::TEXT_DIMMED)
                                .child("No trace loaded"),
                        )
                    })
                    .child(
                        div()
                            .pt_2()
                            .text_size(px(10.0))
                            .text_color(colors::TEXT_DIMMED)
                            .child("Press Escape to close"),
                    ),
            )
    }
}
