//! The `moebius-desktop` program: the desktop app, with the Tauri side in `window`. Built for release on Windows, it
//! opens no console window of its own.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod notifications;
mod servers;
mod tray_icon;
mod window;

fn main() {
    window::run();
}
