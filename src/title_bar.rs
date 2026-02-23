use gpui::*;
use gpui_component::TitleBar;

use crate::app::TraceState;
use crate::theme::colors;

pub fn render_title_bar(state: &Entity<TraceState>, cx: &App) -> TitleBar {
    let _state = state.read(cx);

    TitleBar::new().bg(colors::BG_PRIMARY).border_b_0().child(
        div()
            .text_size(px(13.0))
            .font_weight(FontWeight::BOLD)
            .text_color(colors::TEXT_PRIMARY)
            .child("Reflex"),
    )
}
