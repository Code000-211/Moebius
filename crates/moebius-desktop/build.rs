//! Generates what Tauri needs from `tauri.conf.json` and `capabilities/`, with the app's own commands (`connect` and the
//! local server's) as permissions only the connect page gets, and builds the app again when the web app in `web/dist` changes, which
//! `proxy::assets` embeds.

fn main() {
    println!("cargo:rerun-if-changed=../../web/dist");
    let manifest =
        tauri_build::AppManifest::new().commands(&["connect", "local_state", "local_connect"]);
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(manifest))
        .expect("the Tauri configuration builds");
}
