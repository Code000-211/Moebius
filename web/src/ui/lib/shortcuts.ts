//! Page-wide keyboard shortcuts, and the checks every one of them makes before it acts.

/**
 * True while a modal sheet is over the page (the app's `DialogContent` says `aria-modal`; one that is closing does not count). The page underneath still hears its own shortcut while focus is trapped
 * in the sheet, so the panel it opens would land behind the scrim where nobody can see it. A sheet that is not modal —
 * the Moebius agent's sheet, which leaves the page working — does not count. A shortcut that opens a window over the card window (`C`, a new issue) asks with `exceptCardWindow`.
 */
export function modalIsOpen({
  exceptCardWindow = false,
}: {
  exceptCardWindow?: boolean;
} = {}): boolean {
  // The card window marks itself `data-card-window`; a shortcut that opens a window of its own over it may ignore it.
  const open = '[role="dialog"][aria-modal="true"][data-state="open"]';
  return (
    document.querySelector(exceptCardWindow ? `${open}:not([data-card-window])` : open) !== null
  );
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

/**
 * For the `onKeyDown` of a control that keeps its keys from the card or dock around it: stops the key unless it is a
 * `Ctrl` or `⌘` chord. A chord is a page shortcut (`Ctrl+K`, `Ctrl+J`) and no control means it, so it goes on up.
 */
export function keepKey(event: {
  ctrlKey: boolean;
  metaKey: boolean;
  stopPropagation: () => void;
}) {
  if (event.ctrlKey || event.metaKey) return;
  event.stopPropagation();
}

/** True when a key typed at `target` belongs to it (a field, a menu) and not to the page. */
export function fieldHasKeys(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target.isContentEditable) return true;
  return (
    target.closest(
      "input:not([type=checkbox]), textarea, select, [role=menu], [role=listbox], [role=combobox]",
    ) !== null
  );
}
