mod app;
mod config;
mod interaction;
mod theme;
mod title_bar;
mod trace;
mod views;
mod wcp;

use app::AppView;
use gpui::*;
use gpui_component::TitleBar;
use std::sync::{Arc, Mutex};

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
/// Set the application dock icon from the bundled .icns file.
/// Only used when running outside a .app bundle (e.g. `cargo run`),
/// since bundled apps get their icon from Info.plist's CFBundleIconFile
/// with proper macOS superellipse masking applied by the system.
#[cfg(target_os = "macos")]
fn set_app_icon() {
    use objc2::{AnyThread, MainThreadMarker};
    use objc2_app_kit::{NSApplication, NSImage};

    // Skip if running from a .app bundle — the system handles the icon.
    if let Some(bundle_path) = std::env::current_exe()
        .ok()
        .and_then(|p| p.canonicalize().ok())
    {
        if bundle_path
            .to_string_lossy()
            .contains(".app/Contents/MacOS/")
        {
            return;
        }
    }

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

    // Shared queue for file URLs received via dock icon drop or Finder "Open With".
    let pending_open_urls: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    let application = Application::new().with_assets(gpui_component_assets::Assets);

    let pending_for_callback = pending_open_urls.clone();
    application.on_open_urls(move |urls| {
        let mut pending = pending_for_callback.lock().unwrap();
        pending.extend(urls);
    });

    let pending_for_app = pending_open_urls;
    application.run(move |cx| {
        gpui_component::init(cx);
        interaction::keybindings::register(cx);
        interaction::menus::setup(cx);

        cx.on_action(|_: &interaction::actions::Quit, cx: &mut App| {
            cx.quit();
        });

        #[cfg(target_os = "macos")]
        force_dark_appearance();
        #[cfg(target_os = "macos")]
        set_app_icon();

        // Force dark theme, then fine-tune colors to match our palette.
        {
            use gpui_component::theme::{Theme, ThemeMode};
            Theme::change(ThemeMode::Dark, None, cx);
            let theme = Theme::global_mut(cx);
            theme.background = theme::colors::BG_PRIMARY;
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
            cx.new(|cx| AppView::new(file_path.clone(), pending_for_app.clone(), window, cx))
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
