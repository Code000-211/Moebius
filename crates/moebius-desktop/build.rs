//! Generates what Tauri needs from `tauri.conf.json` and `capabilities/`, with the app's own commands (the rail of servers and
//! the local server's) as permissions, and builds the app again when the web app in `web/dist` changes, which
//! `proxy::assets` embeds.

fn main() {
    println!("cargo:rerun-if-changed=../../web/dist");
    let manifest = tauri_build::AppManifest::new().commands(&[
        "connect",
        "connect_ssh",
        "servers",
        "switch_server",
        "remove_server",
        "local_state",
        "local_connect",
        "quick_shortcut",
        "set_quick_shortcut",
        "quick_finish",
    ]);
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(manifest))
        .expect("the Tauri configuration builds");
}
