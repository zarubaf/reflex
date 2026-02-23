mod app;
mod interaction;
mod theme;
mod title_bar;
mod trace;
mod views;

use app::AppView;
use gpui::*;
use gpui_component::TitleBar;

/// Force the macOS window appearance to dark so native traffic lights
/// render with proper contrast on our dark background.
#[cfg(target_os = "macos")]
fn force_dark_appearance() {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSAppearance, NSAppearanceNameDarkAqua, NSApplication};
    let mtm = MainThreadMarker::from(unsafe { MainThreadMarker::new_unchecked() });
    let app = NSApplication::sharedApplication(mtm);
    let dark = NSAppearance::appearanceNamed(unsafe { NSAppearanceNameDarkAqua });
    app.setAppearance(dark.as_deref());
}

fn main() {
    let file_path = std::env::args().nth(1);

    let application = Application::new()
        .with_assets(gpui_component_assets::Assets);

    application.run(move |cx| {
        gpui_component::init(cx);
        interaction::keybindings::register(cx);

        #[cfg(target_os = "macos")]
        force_dark_appearance();

        // Override theme colors to match our dark color scheme.
        {
            use gpui_component::theme::Theme;
            let theme = Theme::global_mut(cx);
            theme.tab_bar = theme::colors::BG_PRIMARY;
            theme.tab = gpui::transparent_black();
            theme.tab_active = theme::colors::BG_SECONDARY;
            theme.tab_foreground = theme::colors::TEXT_DIMMED;
            theme.tab_active_foreground = theme::colors::TEXT_PRIMARY;
            theme.title_bar = theme::colors::BG_PRIMARY;
            theme.title_bar_border = gpui::transparent_black();
            theme.border = theme::colors::GRID_LINE;
            theme.foreground = theme::colors::TEXT_PRIMARY;
        }

        let window_options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: Point::default(),
                size: Size {
                    width: px(1400.0),
                    height: px(900.0),
                },
            })),
            titlebar: Some(TitleBar::title_bar_options()),
            focus: true,
            show: true,
            kind: WindowKind::Normal,
            is_movable: true,
            ..Default::default()
        };

        cx.open_window(window_options, |window, cx| {
            cx.new(|cx| AppView::new(file_path.clone(), window, cx))
        })
        .expect("Failed to open window");

        cx.activate(true);
    });
}
