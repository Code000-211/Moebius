//! Starts the desktop app: loads the fonts, opens the window.

use moebius_desktop::app::App;
use moebius_desktop::theme;

fn main() -> iced::Result {
    let mut app = iced::application(App::new, App::update, App::view)
        .title("Moebius")
        .subscription(App::subscription)
        .theme(App::theme)
        .default_font(theme::MANROPE)
        .window_size((1280.0, 800.0));
    for font in theme::fonts() {
        app = app.font(font);
    }
    app.run()
}
