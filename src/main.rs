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
    let mtm = unsafe { MainThreadMarker::new_unchecked() };
    let app = NSApplication::sharedApplication(mtm);
    let dark = NSAppearance::appearanceNamed(unsafe { NSAppearanceNameDarkAqua });
    app.setAppearance(dark.as_deref());
}

/// Set the application dock icon from the bundled .icns file.
#[cfg(target_os = "macos")]
fn set_app_icon() {
    use objc2::{AnyThread, MainThreadMarker};
    use objc2_app_kit::{NSApplication, NSImage};

    let icon_bytes = include_bytes!("../resources/reflex.icns");
    let mtm = unsafe { MainThreadMarker::new_unchecked() };
    let app = NSApplication::sharedApplication(mtm);

    unsafe {
        let data = objc2_foundation::NSData::with_bytes(icon_bytes);
        if let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) {
            app.setApplicationIconImage(Some(&image));
        }
    }
}

fn main() {
    let file_path = std::env::args().nth(1);

    let application = Application::new().with_assets(gpui_component_assets::Assets);

    application.run(move |cx| {
        gpui_component::init(cx);
        interaction::keybindings::register(cx);

        #[cfg(target_os = "macos")]
        force_dark_appearance();
        #[cfg(target_os = "macos")]
        set_app_icon();

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
            window_background: WindowBackgroundAppearance::Opaque,
            ..Default::default()
        };

        cx.open_window(window_options, |window, cx| {
            cx.new(|cx| AppView::new(file_path.clone(), window, cx))
        })
        .expect("Failed to open window");

        // Allow screen recording / screenshots by setting window sharing type.
        #[cfg(target_os = "macos")]
        {
            use objc2::MainThreadMarker;
            use objc2_app_kit::{NSApplication, NSWindowSharingType};
            let mtm = unsafe { MainThreadMarker::new_unchecked() };
            let app = NSApplication::sharedApplication(mtm);
            for window in app.windows().iter() {
                window.setSharingType(NSWindowSharingType::ReadOnly);
            }
        }

        cx.activate(true);
    });
}
