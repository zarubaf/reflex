use gpui::*;

use crate::theme::colors;

const SHORTCUTS: &[(&str, &str)] = &[
    ("Navigation", ""),
    ("Scroll / Trackpad", "Pan"),
    ("Click", "Select instruction"),
    ("Arrow keys", "Pan"),
    ("j / k", "Select next / prev"),
    ("", ""),
    ("Zoom", ""),
    ("Ctrl + Scroll", "Zoom in / out"),
    ("Cmd + =  /  Cmd + -", "Zoom in / out"),
    ("Cmd + 0", "Zoom to fit"),
    ("", ""),
    ("Search & Files", ""),
    ("Cmd + F", "Search instructions"),
    ("Enter", "Next search result"),
    ("Escape", "Close search"),
    ("Cmd + L", "Go to cycle"),
    ("Cmd + O", "Open trace file"),
    ("Cmd + R", "Reload trace"),
    ("Cmd + G", "Generate new trace"),
    ("", ""),
    ("Cursors", ""),
    ("Click", "Move active cursor"),
    ("Cmd + M", "Add cursor"),
    ("Cmd + Shift + M", "Remove active cursor"),
    ("[ / ]", "Prev / next cursor"),
    ("Drag head", "Reposition cursor"),
    ("", ""),
    ("?", "Toggle this help"),
];

pub struct HelpOverlay {
    visible: bool,
    focus_handle: FocusHandle,
    parent_focus: FocusHandle,
}

impl HelpOverlay {
    pub fn new(parent_focus: FocusHandle, cx: &mut Context<Self>) -> Self {
        Self {
            visible: false,
            focus_handle: cx.focus_handle(),
            parent_focus,
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

    #[allow(dead_code)]
    pub fn is_visible(&self) -> bool {
        self.visible
    }
}

impl Render for HelpOverlay {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.visible {
            return div().id("help-hidden");
        }

        // Full-screen semi-transparent backdrop.
        div()
            .id("help-overlay")
            .track_focus(&self.focus_handle)
            .key_context("HelpOverlay")
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, _cx| {
                if event.keystroke.key == "escape"
                    || event.keystroke.key == "?"
                    || (event.keystroke.key == "/" && event.keystroke.modifiers.shift)
                {
                    this.visible = false;
                    window.focus(&this.parent_focus);
                    _cx.notify();
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
                    .w(px(400.0))
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
                            .child("Keyboard Shortcuts"),
                    )
                    .children(SHORTCUTS.iter().map(|(key, desc)| {
                        if key.is_empty() && desc.is_empty() {
                            // Spacer.
                            return div().h(px(4.0));
                        }
                        if desc.is_empty() {
                            // Section header.
                            return div()
                                .text_size(px(11.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(colors::TEXT_PRIMARY)
                                .pt_1()
                                .child(key.to_string());
                        }
                        // Key-description row.
                        div()
                            .flex()
                            .justify_between()
                            .items_center()
                            .h(px(20.0))
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(colors::TEXT_DIMMED)
                                    .min_w(px(180.0))
                                    .child(key.to_string()),
                            )
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(colors::TEXT_PRIMARY)
                                    .child(desc.to_string()),
                            )
                    }))
                    .child(
                        div()
                            .pt_2()
                            .text_size(px(10.0))
                            .text_color(colors::TEXT_DIMMED)
                            .child("Press ? or Escape to close"),
                    ),
            )
    }
}
