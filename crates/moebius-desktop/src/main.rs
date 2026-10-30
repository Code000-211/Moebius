//! Starts the desktop app: loads the fonts, opens the window with its icon.

// A release build on Windows opens its window without a console beside it.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use moebius_desktop::app::App;
use moebius_desktop::theme;

/// The window icon, decoded from the PNG that is also the icon of the application bundle and the desktop entry. The
/// window has none rather than the app failing when the picture cannot be decoded.
fn icon() -> Option<iced::window::Icon> {
    let mut reader = png::Decoder::new(std::io::Cursor::new(include_bytes!("../assets/icon.png")))
        .read_info()
        .ok()?;
    let mut pixels = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut pixels).ok()?;
    if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
        return None;
    }
    pixels.truncate(info.buffer_size());
    iced::window::icon::from_rgba(pixels, info.width, info.height).ok()
}

fn main() -> iced::Result {
    // `ssh` starts this program to ask for a password: it asks the running app and prints the answer.
    if std::env::var_os(moebius_desktop::ssh::askpass::ADDRESS_VARIABLE).is_some() {
        let prompt = std::env::args().nth(1).unwrap_or_default();
        std::process::exit(moebius_desktop::ssh::askpass::helper(&prompt));
    }
    #[cfg(target_os = "linux")]
    start_on_x11();
    let mut app = iced::application(App::new, App::update, App::view)
        .title("Moebius")
        .subscription(App::subscription)
        .theme(App::theme)
        .default_font(theme::MANROPE)
        .window(iced::window::Settings {
            size: iced::Size::new(1280.0, 800.0),
            icon: icon(),
            ..iced::window::Settings::default()
        });
    for font in theme::fonts() {
        app = app.font(font);
    }
    app.run()
}

/// Whether the app opens its window through XWayland: in a Wayland session that has one. Through Wayland itself the
/// window cannot reach the clipboard, so that nothing could be copied or pasted.
#[cfg(target_os = "linux")]
fn x11_instead(wayland: Option<&std::ffi::OsStr>, display: Option<&std::ffi::OsStr>) -> bool {
    wayland.is_some_and(|w| !w.is_empty()) && display.is_some_and(|d| !d.is_empty())
}

/// Starts this program again without `WAYLAND_DISPLAY`, so that its window opens through XWayland (see
/// [`x11_instead`]). When it cannot be started again, the app goes on through Wayland.
#[cfg(target_os = "linux")]
fn start_on_x11() {
    use std::os::unix::process::CommandExt;
    if !x11_instead(
        std::env::var_os("WAYLAND_DISPLAY").as_deref(),
        std::env::var_os("DISPLAY").as_deref(),
    ) {
        return;
    }
    let Ok(program) = std::env::current_exe() else {
        return;
    };
    // `exec` returns only when the program could not be started.
    let _ = std::process::Command::new(program)
        .args(std::env::args_os().skip(1))
        .env_remove("WAYLAND_DISPLAY")
        .exec();
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_window_icon_decodes() {
        assert!(super::icon().is_some());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_wayland_session_with_xwayland_opens_the_window_through_x11() {
        use std::ffi::OsStr;
        let some = |s: &'static str| Some(OsStr::new(s));
        assert!(super::x11_instead(some("wayland-0"), some(":0")));
        // Without XWayland the window stays on Wayland; on X11 alone nothing changes.
        assert!(!super::x11_instead(some("wayland-0"), None));
        assert!(!super::x11_instead(some("wayland-0"), some("")));
        assert!(!super::x11_instead(None, some(":0")));
    }
}
