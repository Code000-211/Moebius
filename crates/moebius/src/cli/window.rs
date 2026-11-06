//! The web app as a window of its own: `moebius open` and `moebius connect <host>` start a Chromium-based browser in
//! app mode (no tabs, no address bar) on the address that signs in once, with a profile folder of their own so that
//! the window keeps its own cookie and never joins a browser the person is already using. Where there is no such
//! browser, the address goes to the system's default one.
//!
//! This module finds the browser, builds its arguments and starts it; the login code comes from the caller.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::Deserialize;

use super::client::{Client, Request};

/// The programs looked for on `PATH`, in this order, when no browser is named.
const BROWSERS: [&str; 7] = [
    "chromium",
    "chromium-browser",
    "google-chrome",
    "google-chrome-stable",
    "microsoft-edge",
    "msedge",
    "brave-browser",
];

/// Where browsers are installed when they are not on `PATH`: Edge and Chrome on Windows, the application bundles on
/// macOS.
pub fn install_paths() -> Vec<PathBuf> {
    let mut found = Vec::new();
    if cfg!(windows) {
        for variable in ["ProgramFiles", "ProgramFiles(x86)", "LocalAppData"] {
            if let Some(base) = std::env::var_os(variable) {
                let base = PathBuf::from(base);
                found.push(base.join(r"Microsoft\Edge\Application\msedge.exe"));
                found.push(base.join(r"Google\Chrome\Application\chrome.exe"));
            }
        }
    }
    if cfg!(target_os = "macos") {
        for bundle in [
            "Google Chrome.app/Contents/MacOS/Google Chrome",
            "Chromium.app/Contents/MacOS/Chromium",
            "Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
            "Brave Browser.app/Contents/MacOS/Brave Browser",
        ] {
            found.push(Path::new("/Applications").join(bundle));
        }
    }
    found
}

/// The browser to start: `explicit` (`--browser`, `MOEBIUS_BROWSER`) if given, which must exist; else the first of
/// [`BROWSERS`] on `path`; else the first of `installed` that exists. `None` when there is none.
pub fn find_browser(
    explicit: Option<&Path>,
    path: &OsStr,
    installed: &[PathBuf],
) -> Result<Option<PathBuf>, String> {
    if let Some(explicit) = explicit {
        // A bare name is looked for on `PATH`, a path is taken as it is.
        let found = if explicit.components().count() == 1 {
            crate::find_in_path(&explicit.to_string_lossy(), path)
        } else {
            explicit.is_file().then(|| explicit.to_path_buf())
        };
        return found
            .map(Some)
            .ok_or_else(|| format!("the browser {} does not exist", explicit.display()));
    }
    Ok(BROWSERS
        .iter()
        .find_map(|name| crate::find_in_path(name, path))
        .or_else(|| installed.iter().find(|p| p.is_file()).cloned()))
}

/// The arguments that open `url` as an app window with its own profile in `profile`.
pub fn app_args(url: &str, profile: &Path) -> Vec<String> {
    vec![
        format!("--app={url}"),
        format!("--user-data-dir={}", profile.display()),
        "--no-first-run".into(),
        "--no-default-browser-check".into(),
    ]
}

/// The folder the window's browser keeps its profile (and so its cookie) in: next to the command line's settings.
pub fn profile_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("moebius").join("window"))
}

/// The program and arguments that open `url` in the system's default browser.
pub fn default_opener(url: &str) -> (String, Vec<String>) {
    if cfg!(windows) {
        // The empty string is the window title `start` would otherwise take the address for.
        (
            "cmd".into(),
            ["/c", "start", "", url].map(String::from).into(),
        )
    } else if cfg!(target_os = "macos") {
        ("open".into(), vec![url.into()])
    } else {
        ("xdg-open".into(), vec![url.into()])
    }
}

/// How the address was opened.
#[derive(Debug, PartialEq)]
pub enum Opened {
    /// In app mode by this browser.
    Window(PathBuf),
    /// By the system's default browser, in a tab, because none of the browsers was found.
    DefaultBrowser,
}

/// Starts `program` with `args` without waiting for it: a browser keeps running after this command ends.
pub fn launch(program: &Path, args: &[String]) -> Result<(), String> {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(drop)
        .map_err(|e| format!("cannot start {}: {e}", program.display()))
}

/// Opens `url`: as an app window of `browser` (or the one found), else in the default browser.
pub fn open(url: &str, browser: Option<&Path>) -> Result<Opened, String> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    match find_browser(browser, &path, &install_paths())? {
        Some(program) => {
            let profile = profile_dir()
                .ok_or("there is no config directory to keep the window's profile in")?;
            std::fs::create_dir_all(&profile)
                .map_err(|e| format!("cannot make {}: {e}", profile.display()))?;
            launch(&program, &app_args(url, &profile))?;
            Ok(Opened::Window(program))
        }
        None => {
            let (program, args) = default_opener(url);
            launch(Path::new(&program), &args)?;
            Ok(Opened::DefaultBrowser)
        }
    }
}

/// The sentence that says how the window was opened. It never holds the address: the code in it is a credential.
pub fn describe(opened: &Opened) -> String {
    match opened {
        Opened::Window(program) => {
            format!("Opened the web app in a window of {}.", program.display())
        }
        Opened::DefaultBrowser => {
            "No Chromium-based browser was found (name one with --browser or \
            MOEBIUS_BROWSER), so the web app is opening in your default browser, in a tab."
                .to_string()
        }
    }
}

/// A new one-time login address from the server of `client`.
pub fn login_url(client: &Client) -> Result<String, String> {
    #[derive(Deserialize)]
    struct Code {
        url: String,
    }
    let text = client.send(&Request::post("/v1/session/codes", serde_json::json!({})))?;
    serde_json::from_str::<Code>(&text)
        .map(|code| code.url)
        .map_err(|e| format!("the answer has no url: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A directory of fake programs: files with the execute bit, which is what the lookup asks for (Unix only,
    /// because a Windows program is chosen by its extension).
    #[cfg(unix)]
    fn programs(names: &[&str]) -> tempfile::TempDir {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        for name in names {
            let file = dir.path().join(name);
            std::fs::write(&file, "#!/bin/sh\n").unwrap();
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        dir
    }

    // The lookup needs files with an execute bit, which only Unix has.
    #[cfg(unix)]
    #[test]
    fn the_first_browser_in_the_list_is_picked_whatever_the_order_on_path() {
        let first = programs(&["brave-browser", "google-chrome"]);
        let second = programs(&["chromium-browser", "msedge"]);
        let path = std::env::join_paths([first.path(), second.path()]).unwrap();
        let found = find_browser(None, &path, &[]).unwrap().unwrap();
        // The order of the list decides, not the order of the directories.
        assert_eq!(found, second.path().join("chromium-browser"));
        let only = programs(&["brave-browser"]);
        assert_eq!(
            find_browser(None, only.path().as_os_str(), &[]).unwrap(),
            Some(only.path().join("brave-browser"))
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_named_browser_wins_and_must_exist() {
        let dir = programs(&["chromium", "mine"]);
        let path = dir.path().as_os_str();
        let mine = dir.path().join("mine");
        assert_eq!(find_browser(Some(&mine), path, &[]).unwrap(), Some(mine));
        // A bare name is looked for on PATH.
        assert_eq!(
            find_browser(Some(Path::new("mine")), path, &[]).unwrap(),
            Some(dir.path().join("mine"))
        );
        let error = find_browser(Some(&dir.path().join("gone")), path, &[]).unwrap_err();
        assert!(error.contains("does not exist"), "{error}");
    }

    #[test]
    fn an_installed_browser_is_the_last_resort_and_none_means_the_default_browser() {
        let empty = tempfile::tempdir().unwrap();
        let installed = empty.path().join("Edge");
        std::fs::write(&installed, "").unwrap();
        let missing = empty.path().join("Chrome");
        assert_eq!(
            find_browser(
                None,
                empty.path().as_os_str(),
                &[missing.clone(), installed.clone()]
            )
            .unwrap(),
            Some(installed)
        );
        assert_eq!(
            find_browser(None, empty.path().as_os_str(), &[missing]).unwrap(),
            None
        );
    }

    #[test]
    fn the_window_opens_the_address_in_app_mode_with_a_profile_of_its_own() {
        let args = app_args("http://127.0.0.1:7171/?login=abc", Path::new("/p/window"));
        assert_eq!(args[0], "--app=http://127.0.0.1:7171/?login=abc");
        assert_eq!(args[1], "--user-data-dir=/p/window");
        assert!(args.contains(&"--no-first-run".to_string()));
    }

    #[test]
    fn the_default_browser_is_asked_with_the_address_alone() {
        let (program, args) = default_opener("http://x/?login=a");
        assert!(["xdg-open", "open", "cmd"].contains(&program.as_str()));
        assert_eq!(args.last().unwrap(), "http://x/?login=a");
    }

    #[test]
    fn what_is_said_never_holds_the_address() {
        let said = describe(&Opened::Window(PathBuf::from("/usr/bin/chromium")));
        assert!(said.contains("/usr/bin/chromium") && !said.contains("login"));
        assert!(describe(&Opened::DefaultBrowser).contains("default browser"));
    }

    // A fake browser is a shell script started by its `#!` line, which Windows does not run.
    #[cfg(unix)]
    #[test]
    fn launch_starts_the_program_with_the_arguments() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let record = dir.path().join("args");
        let fake = dir.path().join("fake");
        std::fs::write(
            &fake,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\n",
                record.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        launch(&fake, &["--app=u".into(), "two".into()]).unwrap();
        let started = std::time::Instant::now();
        let text = loop {
            if let Ok(text) = std::fs::read_to_string(&record)
                && text.ends_with("two\n")
            {
                break text;
            }
            assert!(started.elapsed().as_secs() < 60, "the fake never ran");
            std::thread::sleep(std::time::Duration::from_millis(20));
        };
        assert_eq!(text, "--app=u\ntwo\n");
        assert!(launch(&dir.path().join("none"), &[]).is_err());
    }
}
