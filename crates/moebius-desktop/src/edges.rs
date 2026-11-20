//! Resizing the window from its edges: the script every page of the window gets.
//!
//! The window has no frame of the system, so it is resized from its own edges. Tauri attaches its handler for that
//! only to the one webview of a window it made with one; the window here holds child webviews (`pages`) that fill it
//! edge to edge, which take the pointer from the toolkit's handler for the window itself (and on Linux the webviews
//! are packed into the window, so no border can be left uncovered). So each page watches its own outer
//! [`EDGE`] pixels: it shows the resize cursor there, and a press of the main button starts Tauri's
//! `start_resize_dragging` for that edge, as long as the window is not maximized. The script is plain JavaScript in
//! [`SCRIPT`], so that it is tested here (with `node`, in the tests below) rather than only by hand.

/// How wide, in pixels of the page, the edge is that resizes the window.
pub const EDGE: u32 = 6;

/// The script every page of the window gets first. It uses Tauri's `invoke` of the page, so a page may call
/// `start_resize_dragging` and `is_maximized` only where its capability allows them (`capabilities/`).
pub const SCRIPT: &str = r#"
(() => {
  const EDGE = 6;
  const CURSORS = {
    North: "ns-resize", South: "ns-resize", East: "ew-resize", West: "ew-resize",
    NorthEast: "nesw-resize", SouthWest: "nesw-resize", NorthWest: "nwse-resize", SouthEast: "nwse-resize",
  };
  const edgeAt = (x, y) => {
    const vertical = y < EDGE ? "North" : y >= window.innerHeight - EDGE ? "South" : "";
    const horizontal = x < EDGE ? "West" : x >= window.innerWidth - EDGE ? "East" : "";
    return vertical + horizontal || null;
  };
  const invoke = (command, args) => window.__TAURI_INTERNALS__.invoke(command, { label: "main", ...args });
  let shown = null;
  window.addEventListener("mousemove", (event) => {
    const edge = edgeAt(event.clientX, event.clientY);
    if (edge === shown || !document.documentElement) return;
    shown = edge;
    document.documentElement.style.cursor = edge ? CURSORS[edge] : "";
  }, true);
  window.addEventListener("mousedown", (event) => {
    const edge = event.button === 0 ? edgeAt(event.clientX, event.clientY) : null;
    if (!edge) return;
    event.preventDefault();
    event.stopPropagation();
    invoke("plugin:window|is_maximized")
      .then((maximized) => maximized || invoke("plugin:window|start_resize_dragging", { value: edge }))
      .catch(() => {});
  }, true);
})();
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::process::{Command, Stdio};

    /// Runs `body` with `SCRIPT` loaded on a fake window of 1000x700 pixels whose `invoke` answers `maximized` and
    /// records its calls; `body` reaches `mouse(type, x, y, button)` and `calls`, and prints what it checked as JSON.
    /// None when `node` is not installed (CI has it, for the web app).
    fn run(maximized: bool, body: &str) -> Option<String> {
        let program = format!(
            r#"
const listeners = {{}};
const calls = [];
const root = {{ style: {{ cursor: "" }} }};
const window = {{
  innerWidth: 1000, innerHeight: 700,
  addEventListener: (type, fn, capture) => {{ listeners[type] = {{ fn, capture }}; }},
  __TAURI_INTERNALS__: {{ invoke: (command, args) => {{ calls.push([command, args]); return Promise.resolve({maximized} && command.endsWith("is_maximized")); }} }},
}};
const document = {{ documentElement: root }};
{SCRIPT}
let stopped = 0;
const mouse = (type, x, y, button = 0) =>
  listeners[type].fn({{ clientX: x, clientY: y, button, preventDefault() {{ stopped++; }}, stopPropagation() {{}} }});
(async () => {{
  {body}
}})();
"#
        );
        let mut node = Command::new("node")
            .arg("-")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .ok()?;
        node.stdin.take()?.write_all(program.as_bytes()).ok()?;
        let out = node.wait_with_output().ok()?;
        assert!(out.status.success(), "the script failed");
        Some(String::from_utf8(out.stdout).unwrap().trim().to_string())
    }

    #[test]
    fn the_edge_is_the_width_the_rust_side_names() {
        assert!(SCRIPT.contains(&format!("const EDGE = {EDGE};")));
    }

    #[test]
    fn a_press_at_each_edge_and_corner_starts_that_resize() {
        let Some(out) = run(
            false,
            r#"
            const seen = {};
            for (const [name, x, y] of [
              ["West", 2, 350], ["East", 997, 350], ["North", 500, 2], ["South", 500, 697],
              ["NorthWest", 1, 1], ["NorthEast", 998, 1], ["SouthWest", 1, 698], ["SouthEast", 998, 698],
            ]) {
              calls.length = 0;
              mouse("mousedown", x, y);
              await new Promise((r) => setTimeout(r, 0));
              seen[name] = calls.map((c) => c[1].value ?? c[0].split("|")[1]).join(",");
            }
            console.log(JSON.stringify(seen));
            "#,
        ) else {
            return;
        };
        let seen: serde_json::Value = serde_json::from_str(&out).unwrap();
        for name in [
            "West",
            "East",
            "North",
            "South",
            "NorthWest",
            "NorthEast",
            "SouthWest",
            "SouthEast",
        ] {
            assert_eq!(seen[name], format!("is_maximized,{name}"), "{name}");
        }
    }

    #[test]
    fn a_press_inside_the_page_or_with_another_button_does_nothing() {
        let Some(out) = run(
            false,
            r#"
            mouse("mousedown", 500, 350);
            mouse("mousedown", 6, 350);
            mouse("mousedown", 993, 350);
            mouse("mousedown", 2, 350, 2);
            await new Promise((r) => setTimeout(r, 0));
            console.log(JSON.stringify({ calls: calls.length, stopped }));
            "#,
        ) else {
            return;
        };
        assert_eq!(out, r#"{"calls":0,"stopped":0}"#);
    }

    #[test]
    fn a_maximized_window_is_not_resized() {
        let Some(out) = run(
            true,
            r#"
            mouse("mousedown", 2, 350);
            await new Promise((r) => setTimeout(r, 0));
            console.log(JSON.stringify(calls.map((c) => c[0])));
            "#,
        ) else {
            return;
        };
        assert_eq!(out, r#"["plugin:window|is_maximized"]"#);
    }

    #[test]
    fn the_pointer_at_an_edge_shows_the_resize_cursor_and_leaving_clears_it() {
        let Some(out) = run(
            false,
            r#"
            const cursors = [];
            for (const [x, y] of [[2, 350], [3, 350], [1, 1], [998, 698], [500, 2], [500, 350]]) {
              mouse("mousemove", x, y);
              cursors.push(root.style.cursor);
            }
            console.log(JSON.stringify(cursors));
            "#,
        ) else {
            return;
        };
        assert_eq!(
            out,
            r#"["ew-resize","ew-resize","nwse-resize","nwse-resize","ns-resize",""]"#
        );
    }
}
