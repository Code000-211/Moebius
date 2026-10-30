//! The bytes a shell expects for the keys pressed in a terminal pane.

use iced::keyboard::key::Named;
use iced::keyboard::{Key, Modifiers};

/// The bytes a shell expects for a key press, or none when the key means nothing to it. `text` is what the key
/// produced as text; `app_cursor` is whether the program has asked for the arrow keys in application mode.
pub fn key_bytes(
    key: &Key,
    modifiers: Modifiers,
    text: Option<&str>,
    app_cursor: bool,
) -> Option<Vec<u8>> {
    let arrow = |letter: char| match app_cursor {
        true => format!("\x1bO{letter}"),
        false => format!("\x1b[{letter}"),
    };
    let named = match key {
        Key::Named(Named::Enter) => Some("\r".to_string()),
        Key::Named(Named::Backspace) => Some("\x7f".to_string()),
        Key::Named(Named::Tab) => Some(if modifiers.shift() { "\x1b[Z" } else { "\t" }.to_string()),
        Key::Named(Named::Escape) => Some("\x1b".to_string()),
        Key::Named(Named::Space) if !modifiers.control() => Some(" ".to_string()),
        Key::Named(Named::ArrowUp) => Some(arrow('A')),
        Key::Named(Named::ArrowDown) => Some(arrow('B')),
        Key::Named(Named::ArrowRight) => Some(arrow('C')),
        Key::Named(Named::ArrowLeft) => Some(arrow('D')),
        Key::Named(Named::Home) => Some(arrow('H')),
        Key::Named(Named::End) => Some(arrow('F')),
        Key::Named(Named::Delete) => Some("\x1b[3~".to_string()),
        Key::Named(Named::PageUp) => Some("\x1b[5~".to_string()),
        Key::Named(Named::PageDown) => Some("\x1b[6~".to_string()),
        _ => None,
    };
    if let Some(named) = named {
        return Some(named.into_bytes());
    }
    // Ctrl+Shift+C and Ctrl+Shift+V copy and paste in the pane.
    if modifiers.control()
        && modifiers.shift()
        && let Key::Character(c) = key
        && matches!(c.to_lowercase().as_str(), "c" | "v")
    {
        return None;
    }
    // Ctrl and a letter is the control character: Ctrl+C is 3, Ctrl+D is 4.
    if modifiers.control()
        && let Key::Character(c) = key
        && let Some(letter) = c.chars().next().filter(char::is_ascii_alphabetic)
    {
        return Some(vec![letter.to_ascii_lowercase() as u8 - b'a' + 1]);
    }
    if modifiers.control() || modifiers.logo() {
        return None;
    }
    let text = text.filter(|text| !text.is_empty() && !text.chars().any(char::is_control))?;
    // Alt sends the key with Escape before it.
    Some(
        match modifiers.alt() {
            true => format!("\x1b{text}"),
            false => text.to_string(),
        }
        .into_bytes(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pressed(key: Key, modifiers: Modifiers, text: Option<&str>) -> Option<Vec<u8>> {
        key_bytes(&key, modifiers, text, false)
    }

    fn character(c: &str) -> Key {
        Key::Character(c.into())
    }

    #[test]
    fn key_presses_are_the_bytes_a_shell_expects() {
        let none = Modifiers::empty();
        assert_eq!(pressed(character("a"), none, Some("a")).unwrap(), b"a");
        assert_eq!(
            pressed(character("A"), Modifiers::SHIFT, Some("A")).unwrap(),
            b"A"
        );
        assert_eq!(
            pressed(Key::Named(Named::Enter), none, Some("\r")).unwrap(),
            b"\r"
        );
        assert_eq!(
            pressed(Key::Named(Named::Backspace), none, None).unwrap(),
            b"\x7f"
        );
        assert_eq!(pressed(Key::Named(Named::Tab), none, None).unwrap(), b"\t");
        assert_eq!(
            pressed(Key::Named(Named::Escape), none, None).unwrap(),
            b"\x1b"
        );
        assert_eq!(
            pressed(Key::Named(Named::Space), none, Some(" ")).unwrap(),
            b" "
        );
        assert_eq!(
            pressed(Key::Named(Named::ArrowUp), none, None).unwrap(),
            b"\x1b[A"
        );
        assert_eq!(
            pressed(Key::Named(Named::ArrowLeft), none, None).unwrap(),
            b"\x1b[D"
        );
        assert_eq!(
            pressed(character("c"), Modifiers::CTRL, Some("c")).unwrap(),
            [3]
        );
        assert_eq!(pressed(character("d"), Modifiers::CTRL, None).unwrap(), [4]);
        // With Shift they copy and paste, in the pane.
        let both = Modifiers::CTRL | Modifiers::SHIFT;
        assert_eq!(pressed(character("C"), both, None), None);
        assert_eq!(pressed(character("V"), both, None), None);
        assert_eq!(
            pressed(character("x"), Modifiers::ALT, Some("x")).unwrap(),
            b"\x1bx"
        );
        assert_eq!(
            pressed(character("é"), none, Some("é")).unwrap(),
            "é".as_bytes()
        );
        assert_eq!(
            pressed(Key::Named(Named::Shift), Modifiers::SHIFT, None),
            None
        );
        assert_eq!(pressed(character("c"), Modifiers::LOGO, Some("c")), None);
        // A program that asked for application mode gets the arrows that way.
        let up = key_bytes(&Key::Named(Named::ArrowUp), none, None, true);
        assert_eq!(up.unwrap(), b"\x1bOA");
    }
}
