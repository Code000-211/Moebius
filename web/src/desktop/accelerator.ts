/**
 * Writes a key press as the shortcut the desktop app registers (`Ctrl+Shift+Space`), for the small window where a
 * person records a new shortcut. The names are the ones `normalize` in `crates/moebius-desktop/src/quick.rs` accepts;
 * the app has the last word on what it accepts.
 */

/** The keys held for themselves; they are not the key of a shortcut. */
const MODIFIER_KEYS = new Set(["Control", "Shift", "Alt", "Meta", "AltGraph", "OS"]);

/** The name of the key `code` (a `KeyboardEvent.code`), or null for a key a shortcut cannot use. */
function keyName(code: string): string | null {
  const letter = /^Key([A-Z])$/.exec(code);
  if (letter) return letter[1] ?? null;
  const digit = /^Digit([0-9])$/.exec(code);
  if (digit) return digit[1] ?? null;
  if (/^F([1-9]|1[0-9]|2[0-4])$/.test(code)) return code;
  const named: Record<string, string> = {
    Space: "Space",
    Enter: "Enter",
    Tab: "Tab",
    Backspace: "Backspace",
    Delete: "Delete",
    Insert: "Insert",
    Home: "Home",
    End: "End",
    PageUp: "PageUp",
    PageDown: "PageDown",
    ArrowUp: "ArrowUp",
    ArrowDown: "ArrowDown",
    ArrowLeft: "ArrowLeft",
    ArrowRight: "ArrowRight",
    Minus: "Minus",
    Equal: "Equal",
  };
  return named[code] ?? null;
}

/**
 * The shortcut a key press makes, `Ctrl+Alt+Shift+Super` first and then the key, or null while only modifiers are
 * held or the key cannot be used.
 */
export function acceleratorOf(
  event: Pick<KeyboardEvent, "code" | "key" | "ctrlKey" | "altKey" | "shiftKey" | "metaKey">,
): string | null {
  if (MODIFIER_KEYS.has(event.key)) return null;
  const key = keyName(event.code);
  if (key === null) return null;
  const held = [
    event.ctrlKey && "Ctrl",
    event.altKey && "Alt",
    event.shiftKey && "Shift",
    event.metaKey && "Super",
  ].filter((name): name is string => typeof name === "string");
  return [...held, key].join("+");
}
