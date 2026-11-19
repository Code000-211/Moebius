//! Generates what Tauri needs from `tauri.conf.json` and `capabilities/`, with the app's own `connect` command as a
//! permission only the connect page gets, and builds the app again when the web app in `web/dist` changes, which
//! `proxy::assets` embeds.

fn main() {
    println!("cargo:rerun-if-changed=../../web/dist");
    let manifest = tauri_build::AppManifest::new().commands(&["connect"]);
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(manifest))
        .expect("the Tauri configuration builds");
}
