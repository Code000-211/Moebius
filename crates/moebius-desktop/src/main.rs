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

#[cfg(test)]
mod tests {
    #[test]
    fn the_window_icon_decodes() {
        assert!(super::icon().is_some());
    }
}
