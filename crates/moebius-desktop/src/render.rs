//! How WebKitGTK renders the window on Linux. Where accelerated compositing has no working GPU path (a virtual machine,
//! a remote desktop, Xvfb with software GL, some NVIDIA and Wayland setups) its web process spends about twice the CPU
//! and memory on the same page and loads it slower, measured on the board at 0.14.0. [`choose`] says which environment
//! variable to set before the first webview exists: compositing is turned off when the machine has no DRI render node,
//! and left as WebKitGTK decides when it has one. A person overrides the choice with `MOEBIUS_RENDERING` (`gpu`,
//! `software` or `auto`), or by setting `WEBKIT_DISABLE_COMPOSITING_MODE` or `WEBKIT_DISABLE_DMABUF_RENDERER`
//! themselves, which this module never overwrites. Windows does not use it.

/// The variable that turns WebKitGTK's accelerated compositing off.
pub const DISABLE_COMPOSITING: &str = "WEBKIT_DISABLE_COMPOSITING_MODE";
/// WebKitGTK's own variable for the DMA-BUF renderer; set by a person, it keeps the app from choosing.
pub const DISABLE_DMABUF: &str = "WEBKIT_DISABLE_DMABUF_RENDERER";
/// The app's setting: `gpu` leaves WebKitGTK alone, `software` turns compositing off, `auto` (the default) looks for a
/// GPU.
pub const SETTING: &str = "MOEBIUS_RENDERING";

/// What the app does about the renderer.
#[derive(Debug, PartialEq, Eq)]
pub enum Choice {
    /// Set [`DISABLE_COMPOSITING`] to `1`.
    Software,
    /// Leave WebKitGTK as it is.
    Leave,
}

/// Decides from the environment (`var` reads a variable) and whether a DRI render node exists.
pub fn choose(var: impl Fn(&str) -> Option<String>, has_render_node: bool) -> Choice {
    if var(DISABLE_COMPOSITING).is_some() || var(DISABLE_DMABUF).is_some() {
        return Choice::Leave;
    }
    match var(SETTING).as_deref().map(str::trim) {
        Some("software") => Choice::Software,
        Some("gpu") => Choice::Leave,
        _ if has_render_node => Choice::Leave,
        _ => Choice::Software,
    }
}

/// Whether the machine has a GPU render node (`/dev/dri/renderD*`).
#[cfg(target_os = "linux")]
fn has_render_node() -> bool {
    std::fs::read_dir("/dev/dri").is_ok_and(|dir| {
        dir.flatten()
            .any(|entry| entry.file_name().to_string_lossy().starts_with("renderD"))
    })
}

/// Chooses the renderer for this process; call it first thing in `main`. WebKitGTK reads its variables when the first
/// webview is made, and the workspace forbids the `unsafe` that changing this process's own environment takes, so where
/// compositing is to go off the app starts itself again with the variable set, in place of the running process (the
/// new run finds the variable and leaves things as they are). It returns when nothing is to change or the restart
/// fails, and the window then runs with WebKitGTK's defaults. A no-op away from Linux.
pub fn apply() {
    #[cfg(target_os = "linux")]
    if choose(|name| std::env::var(name).ok(), has_render_node()) == Choice::Software
        && let Ok(program) = std::env::current_exe()
    {
        use std::os::unix::process::CommandExt;
        let error = std::process::Command::new(program)
            .args(std::env::args_os().skip(1))
            .env(DISABLE_COMPOSITING, "1")
            .exec();
        eprintln!("moebius-desktop: cannot restart with compositing off: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| {
            pairs
                .iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| v.to_string())
        }
    }

    #[test]
    fn no_render_node_turns_compositing_off() {
        assert_eq!(choose(env(&[]), false), Choice::Software);
    }

    #[test]
    fn a_render_node_leaves_webkit_alone() {
        assert_eq!(choose(env(&[]), true), Choice::Leave);
    }

    #[test]
    fn the_setting_overrides_detection() {
        assert_eq!(choose(env(&[(SETTING, "gpu")]), false), Choice::Leave);
        assert_eq!(
            choose(env(&[(SETTING, "software")]), true),
            Choice::Software
        );
        assert_eq!(choose(env(&[(SETTING, "auto")]), false), Choice::Software);
    }

    #[test]
    fn webkits_own_variables_are_never_overridden() {
        assert_eq!(
            choose(env(&[(DISABLE_COMPOSITING, "0")]), false),
            Choice::Leave
        );
        assert_eq!(
            choose(env(&[(DISABLE_DMABUF, "1"), (SETTING, "software")]), false),
            Choice::Leave
        );
    }
}
