//! The global shortcut of the app, tested without a window: which key combinations are accepted and how they are
//! written ([`normalize`]), where the chosen one is kept across restarts ([`Store`]), and what the quick window asks
//! the main window to open ([`open_script`]). The Tauri side, the registration of the shortcut and the small window
//! over everything, is the program's `quick_window` module.
//!
//! The shortcut is a person's choice and holds no secret, so it is kept in a file of its own in the app's
//! configuration directory; a file that is missing or no longer holds a shortcut the app accepts gives the default.

use std::path::{Path, PathBuf};
use std::str::FromStr;

use global_hotkey::hotkey::HotKey;
use serde::{Deserialize, Serialize};

/// The shortcut until a person changes it.
pub const DEFAULT_SHORTCUT: &str = "Ctrl+Shift+Space";

/// The path of the web app's page that is the quick window.
pub const PAGE: &str = "/quick";

/// The name of the event the main window's page gets to open a path of the web app; the web app listens for it in
/// `web/src/desktop/bridge.ts`.
pub const OPEN_EVENT: &str = "moebius:open";

/// The name of the event the quick window's page gets each time the window is shown again.
pub const SHOWN_EVENT: &str = "moebius:quick-shown";

/// Why a shortcut or a path is refused.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum QuickError {
    #[error("a shortcut needs a key, such as Space or K, with Ctrl, Alt, Shift or Super held")]
    NoKey,
    #[error(
        "a shortcut that opens a window over everything needs Ctrl, Alt or Super held, or it would take over typing"
    )]
    NoModifier,
    #[error("a shortcut has one key, not several")]
    TwoKeys,
    #[error("`{0}` is not a key the shortcut can use")]
    UnknownKey(String),
    #[error("`{0}` is not a page of the web app")]
    NotAPath(String),
    #[error("cannot keep the shortcut: {0}")]
    Write(String),
}

/// `shortcut` in its one written form, modifiers first as `Ctrl+Alt+Shift+Super` and then the key (`Ctrl+Shift+Space`),
/// or why it is refused. Names are case-insensitive; `Control`, `Option`, `Meta`, `Cmd`, `Win` and `CommandOrControl`
/// are the modifiers under other names.
pub fn normalize(shortcut: &str) -> Result<String, QuickError> {
    let (mut ctrl, mut alt, mut shift, mut sup) = (false, false, false, false);
    let mut key: Option<String> = None;
    for part in shortcut.split('+').map(str::trim) {
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" | "commandorcontrol" | "cmdorctrl" => ctrl = true,
            "alt" | "option" => alt = true,
            "shift" => shift = true,
            "super" | "meta" | "cmd" | "command" | "win" => sup = true,
            "" => {}
            _ => {
                if key.is_some() {
                    return Err(QuickError::TwoKeys);
                }
                key = Some(key_name(part)?);
            }
        }
    }
    let key = key.ok_or(QuickError::NoKey)?;
    // Shift alone only changes which character is typed, so it does not make a key safe to take over.
    if !(ctrl || alt || sup) {
        return Err(QuickError::NoModifier);
    }
    let mut written = String::new();
    for (held, name) in [
        (ctrl, "Ctrl"),
        (alt, "Alt"),
        (shift, "Shift"),
        (sup, "Super"),
    ] {
        if held {
            written.push_str(name);
            written.push('+');
        }
    }
    written.push_str(&key);
    // The registration parses what is written here, so what it cannot parse is refused now.
    HotKey::from_str(&written).map_err(|_| QuickError::UnknownKey(key))?;
    Ok(written)
}

/// The written name of a key: a letter or digit in capitals, `F1` to `F24`, or one of the named keys.
fn key_name(key: &str) -> Result<String, QuickError> {
    const NAMED: [&str; 17] = [
        "Space",
        "Enter",
        "Tab",
        "Backspace",
        "Delete",
        "Insert",
        "Home",
        "End",
        "PageUp",
        "PageDown",
        "ArrowUp",
        "ArrowDown",
        "ArrowLeft",
        "ArrowRight",
        "Escape",
        "Minus",
        "Equal",
    ];
    let upper = key.to_ascii_uppercase();
    if upper.len() == 1 && upper.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Ok(upper);
    }
    if let Some(n) = upper.strip_prefix('F').and_then(|n| n.parse::<u8>().ok())
        && (1..=24).contains(&n)
    {
        return Ok(format!("F{n}"));
    }
    NAMED
        .iter()
        .find(|name| name.eq_ignore_ascii_case(key))
        .map(|name| (*name).to_string())
        .ok_or_else(|| QuickError::UnknownKey(key.to_string()))
}

/// The file that keeps the chosen shortcut.
#[derive(Serialize, Deserialize, Default)]
struct Saved {
    #[serde(default)]
    shortcut: String,
}

/// Where the chosen shortcut is kept: `quick.json` in the app's configuration directory.
pub struct Store {
    file: PathBuf,
}

impl Store {
    pub fn new(dir: &Path) -> Self {
        Self {
            file: dir.join("quick.json"),
        }
    }

    /// The kept shortcut, or the default when none is kept or the file holds one the app does not accept.
    pub fn load(&self) -> String {
        std::fs::read_to_string(&self.file)
            .ok()
            .and_then(|text| serde_json::from_str::<Saved>(&text).ok())
            .and_then(|saved| normalize(&saved.shortcut).ok())
            .unwrap_or_else(|| DEFAULT_SHORTCUT.to_string())
    }

    /// Keeps `shortcut` (already [`normalize`]d) for the next start; the file is replaced whole, so a start that
    /// comes in the middle of it finds the old one or the new one.
    pub fn save(&self, shortcut: &str) -> Result<(), QuickError> {
        let write = || -> std::io::Result<()> {
            if let Some(dir) = self.file.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let text = serde_json::to_string(&Saved {
                shortcut: shortcut.to_string(),
            })?;
            let temporary = self.file.with_extension("json.tmp");
            std::fs::write(&temporary, text)?;
            std::fs::rename(&temporary, &self.file)
        };
        write().map_err(|e| QuickError::Write(e.to_string()))
    }
}

/// Whether `path` is a page of the web app: it starts with one `/` and holds only printable ASCII, which a header can
/// carry as it is.
pub fn is_local_path(path: &str) -> bool {
    path.starts_with('/')
        && !path.starts_with("//")
        && !path.starts_with("/\\")
        && path.bytes().all(|b| (0x20..0x7f).contains(&b))
}

/// The script that makes the page of the main window open `path` (`/chat/5`): it sends the page [`OPEN_EVENT`] with
/// the path, which the web app answers with its router, so the page keeps what it holds.
pub fn open_script(path: &str) -> Result<String, QuickError> {
    if !is_local_path(path) {
        return Err(QuickError::NotAPath(path.to_string()));
    }
    let detail = serde_json::to_string(path).map_err(|e| QuickError::Write(e.to_string()))?;
    Ok(format!(
        "window.dispatchEvent(new CustomEvent({}, {{ detail: {detail} }}));",
        serde_json::to_string(OPEN_EVENT).expect("a string is JSON")
    ))
}

/// The script that tells the quick window's page it was shown again, to start empty with the cursor in place.
pub fn shown_script() -> String {
    format!(
        "window.dispatchEvent(new Event({}));",
        serde_json::to_string(SHOWN_EVENT).expect("a string is JSON")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_is_accepted_as_written() {
        assert_eq!(normalize(DEFAULT_SHORTCUT).unwrap(), DEFAULT_SHORTCUT);
    }

    #[test]
    fn names_and_order_are_made_one() {
        assert_eq!(
            normalize("shift + control + space").unwrap(),
            "Ctrl+Shift+Space"
        );
        assert_eq!(normalize("Cmd+Option+k").unwrap(), "Alt+Super+K");
        assert_eq!(normalize("CommandOrControl+F12").unwrap(), "Ctrl+F12");
        assert_eq!(normalize("Ctrl+Alt+ArrowUp").unwrap(), "Ctrl+Alt+ArrowUp");
        assert_eq!(normalize("ctrl+1").unwrap(), "Ctrl+1");
    }

    #[test]
    fn every_key_the_app_writes_is_one_the_registration_parses() {
        for key in [
            "Space",
            "Enter",
            "Tab",
            "Backspace",
            "Delete",
            "Insert",
            "Home",
            "End",
            "PageUp",
            "PageDown",
            "ArrowUp",
            "ArrowDown",
            "ArrowLeft",
            "ArrowRight",
            "Escape",
            "Minus",
            "Equal",
            "A",
            "Z",
            "0",
            "9",
            "F1",
            "F24",
        ] {
            let written =
                normalize(&format!("Ctrl+{key}")).unwrap_or_else(|e| panic!("{key}: {e}"));
            assert!(HotKey::from_str(&written).is_ok(), "{written}");
        }
    }

    #[test]
    fn a_shortcut_needs_one_key_and_a_modifier_that_is_not_only_shift() {
        assert_eq!(normalize(""), Err(QuickError::NoKey));
        assert_eq!(normalize("Ctrl+Shift"), Err(QuickError::NoKey));
        assert_eq!(normalize("Space"), Err(QuickError::NoModifier));
        assert_eq!(normalize("Shift+K"), Err(QuickError::NoModifier));
        assert_eq!(normalize("Ctrl+K+L"), Err(QuickError::TwoKeys));
        assert_eq!(
            normalize("Ctrl+Banana"),
            Err(QuickError::UnknownKey("Banana".into()))
        );
        assert_eq!(
            normalize("Ctrl+F25"),
            Err(QuickError::UnknownKey("F25".into()))
        );
    }

    #[test]
    fn a_kept_shortcut_comes_back_and_a_missing_or_broken_one_gives_the_default() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(&dir.path().join("config"));
        assert_eq!(store.load(), DEFAULT_SHORTCUT);
        store.save("Ctrl+Alt+K").unwrap();
        assert_eq!(Store::new(&dir.path().join("config")).load(), "Ctrl+Alt+K");
        std::fs::write(dir.path().join("config/quick.json"), "not json").unwrap();
        assert_eq!(store.load(), DEFAULT_SHORTCUT);
        std::fs::write(dir.path().join("config/quick.json"), r#"{"shortcut":"K"}"#).unwrap();
        assert_eq!(store.load(), DEFAULT_SHORTCUT);
    }

    #[test]
    fn only_pages_of_the_web_app_are_opened() {
        assert!(is_local_path("/chat/5"));
        assert!(is_local_path("/board/2?card=7"));
        for bad in [
            "",
            "chat/5",
            "//evil.example/x",
            "/\\evil.example",
            "https://evil.example",
            "/a\nb",
            "/caf\u{e9}",
        ] {
            assert!(!is_local_path(bad), "{bad:?}");
            assert_eq!(open_script(bad), Err(QuickError::NotAPath(bad.to_string())));
        }
    }

    #[test]
    fn the_open_script_carries_the_path_as_a_string_of_its_own() {
        let script = open_script("/board/2?card=7&x=\"</script>").unwrap();
        assert!(script.contains(OPEN_EVENT));
        assert!(script.contains(r#""/board/2?card=7&x=\"</script>""#));
    }

    #[test]
    fn the_web_app_listens_for_the_events_this_names() {
        let bridge = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/src/desktop/bridge.ts"),
        )
        .unwrap();
        assert!(bridge.contains(&format!("\"{OPEN_EVENT}\"")));
        assert!(bridge.contains(&format!("\"{SHOWN_EVENT}\"")));
        assert!(bridge.contains(&format!("\"{DEFAULT_SHORTCUT}\"")));
    }
}
