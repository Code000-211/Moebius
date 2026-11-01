//! Builds the server again when the web app in `web/dist` changes, which `server::api::web` embeds: without it Cargo
//! keeps a binary built before `pnpm -C web build`, which serves the page that says the web app was not built.

fn main() {
    println!("cargo:rerun-if-changed=../../web/dist");
    println!("cargo:rerun-if-changed=build.rs");
}
