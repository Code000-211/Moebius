//! The corners of the window: the script every page of the window gets.
//!
//! The window has no frame of the system and is transparent (`window`), so each page draws its own round corners: the
//! web app on its root (`web/src/styles/tailwind.css`), the connect page on its body. A page with round corners must
//! square them when the window is maximized or full screen, where they would leave a gap at the screen's edge, so this
//! script sets `data-maximized` on the page's root element while either holds, at the start and at every resize. It
//! asks through Tauri's `invoke`, so a page may use it only where its capability allows `is_maximized` and
//! `is_fullscreen` (`capabilities/`).

/// The script every page of the window gets first.
pub const SCRIPT: &str = r#"
(() => {
  if (!window.__TAURI_INTERNALS__) return;
  const invoke = (command) => window.__TAURI_INTERNALS__.invoke(command, { label: "main" });
  const sync = () => {
    Promise.all([invoke("plugin:window|is_maximized"), invoke("plugin:window|is_fullscreen")]).then(
      ([maximized, fullscreen]) => {
        if (document.documentElement) document.documentElement.toggleAttribute("data-maximized", maximized === true || fullscreen === true);
      },
      () => {},
    );
  };
  window.addEventListener("resize", sync);
  document.addEventListener("DOMContentLoaded", sync);
  sync();
})();
"#;

#[cfg(test)]
mod tests {
    use super::*;

    /// Every capability of the window lets its pages ask what the script asks, or the corners would stay round on a
    /// maximized window.
    #[test]
    fn the_capabilities_allow_what_the_script_asks() {
        for file in ["web-app", "connect"] {
            let path = format!("{}/capabilities/{file}.json", env!("CARGO_MANIFEST_DIR"));
            let capability = std::fs::read_to_string(&path).unwrap();
            for (command, permission) in [
                (
                    "plugin:window|is_maximized",
                    "core:window:allow-is-maximized",
                ),
                (
                    "plugin:window|is_fullscreen",
                    "core:window:allow-is-fullscreen",
                ),
            ] {
                assert!(SCRIPT.contains(command), "{command}");
                assert!(
                    capability.contains(permission),
                    "{file} allows {permission}"
                );
            }
        }
    }
}
