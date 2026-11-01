//! Page-wide keyboard shortcuts, and the checks every one of them makes before it acts.

/**
 * True while a modal sheet is over the page. The page underneath still hears its own shortcut while focus is trapped
 * in the sheet, so the panel it opens would land behind the scrim where nobody can see it. A sheet that is not modal —
 * the floating chat, which leaves the page working — does not count.
 */
export function modalIsOpen(): boolean {
  return document.querySelector('[role="dialog"][aria-modal="true"]') !== null;
}

/**
 * Whether `event` is `Ctrl` on Linux and Windows or `⌘` on a Mac, plus `key`, and nothing else. `Shift` makes a
 * different shortcut, and a key a control has already taken comes back marked as handled.
 */
export function isShortcut(
  event: KeyboardEvent,
  key: string,
  { shift = false }: { shift?: boolean } = {},
): boolean {
  if (event.defaultPrevented || event.altKey || event.shiftKey !== shift) return false;
  return (event.ctrlKey || event.metaKey) && event.key.toLowerCase() === key;
}
