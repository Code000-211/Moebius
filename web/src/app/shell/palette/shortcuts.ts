/**
 * Every keyboard shortcut of the web app, in one place: the page that lists them (`shortcuts/ShortcutsPage.tsx`), the
 * keys the commands of a card in focus carry (`board/cardCommands.ts`) and the keys of the commands the palette always
 * offers (`CommandBar.tsx`) are all read from here, so a key is never on the page and not in the palette, or the other
 * way round. A test holds the card commands to this list.
 */

/** One row of the page: the keys pressed together and what they do. */
export interface Shortcut {
  /** `Mod` is `Ctrl` on Linux and Windows and `⌘` on a Mac; `keyName` writes it for the platform. */
  keys: string[];
  label: string;
}

export interface ShortcutGroup {
  title: string;
  note: string;
  shortcuts: readonly Shortcut[];
}

/** The keys of the commands of a card in focus, by command id. */
export const CARD_KEYS = {
  assign: "a",
  unassign: "a",
  block: "b",
  unblock: "b",
  stop: "x",
  close: "d",
  reopen: "d",
  "sub-issue": "s",
  "move-Not started": "n",
  "move-In progress": "i",
  "move-Done": "m",
} as const;

/** The key of the card command `id`, if it has one. */
export function cardKey(id: string): string | undefined {
  return (CARD_KEYS as Record<string, string>)[id];
}

/** The keys of the commands the palette offers wherever a person is. */
export const NEW_ISSUE_KEY = "c";
export const SHORTCUTS_KEY = "?";

export const SHORTCUT_GROUPS: readonly ShortcutGroup[] = [
  {
    title: "Anywhere",
    note: "These work on any page, except while a field, a menu or a dialog has the keys.",
    shortcuts: [
      { keys: ["Mod", "K"], label: "Search: open the palette (Escape closes it)" },
      { keys: [NEW_ISSUE_KEY.toUpperCase()], label: "New issue" },
      { keys: [SHORTCUTS_KEY], label: "Keyboard shortcuts: open this page" },
    ],
  },
  {
    title: "The card open in the dock",
    note: "Each key does its command only where the card allows it; the palette lists the ones that are offered.",
    shortcuts: [
      { keys: ["A"], label: "Assign to me, or unassign" },
      { keys: ["B"], label: "Block (asks why), or unblock" },
      { keys: ["X"], label: "Stop the run" },
      { keys: ["D"], label: "Close the issue, or reopen it" },
      { keys: ["S"], label: "Add a sub-issue" },
      { keys: ["N"], label: "Move to Not started: approve the issue" },
      { keys: ["I"], label: "Move to In progress: retry a stuck card, or send a change back" },
      { keys: ["M"], label: "Move to Done: merge the change" },
    ],
  },
  {
    title: "Moving a card on the board",
    note: "With a card focused on the board.",
    shortcuts: [
      { keys: ["Enter"], label: "Open the card in the dock" },
      { keys: ["Space"], label: "Lift the card, then drop it" },
      { keys: ["←", "→"], label: "Move the lifted card to another column" },
      { keys: ["Escape"], label: "Put the lifted card back" },
    ],
  },
];

/** `Mod` written for this platform: `⌘` on a Mac, `Ctrl` elsewhere. */
export function keyName(key: string): string {
  if (key !== "Mod") return key;
  return typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.platform)
    ? "⌘"
    : "Ctrl";
}
