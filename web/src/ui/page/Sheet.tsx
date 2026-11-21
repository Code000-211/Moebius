/**
 * The sheet the shell lays over the page's right column: a solid card the whole height of the page's surface, wider
 * than the column, with a header (a title, a line under it, the sheet's own controls, and the control that closes it).
 * The package sheet is made of these pieces: Escape or the close control hides the sheet and puts the focus back on
 * the button that opened it, when one did, and the sheet's state stays where it was until it is opened again. It has
 * the place (`SheetSlot.tsx`) to itself; the Moebius agent has a panel of its own.
 */
import { XIcon } from "@phosphor-icons/react";
import { type ReactNode, type RefObject, useEffect, useRef } from "react";
import { Button } from "../components/Button";
import { cn } from "../lib/cn";
import { useSheetSlot } from "./SheetSlot";

/**
 * Hides the sheet on Escape wherever the focus is on the page, and, once it is hidden, puts the focus back on
 * `opener`, the button that opened it, so a keyboard does not lose its place. A sheet hidden because the other one
 * took its place leaves the focus where it is, and so does one opened from a page, which names no button.
 */
export function useSheetBehaviour(
  shown: boolean,
  hide: () => void,
  opener?: RefObject<HTMLButtonElement | null>,
): void {
  const openedOnce = useRef(false);
  // Read when the sheet goes away: another sheet that took the slot has the focus, not this sheet's button.
  const { active } = useSheetSlot();
  const slot = useRef(active);
  slot.current = active;
  useEffect(() => {
    if (!shown) return;
    const onKeyDown = (event: KeyboardEvent) => {
      // A control that took the key itself — a menu or a dialog over this sheet — closes first, and the sheet stays.
      if (event.key === "Escape" && !event.defaultPrevented) hide();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [shown, hide]);
  useEffect(() => {
    if (shown) openedOnce.current = true;
    else if (openedOnce.current) {
      openedOnce.current = false;
      if (slot.current === null) opener?.current?.focus();
    }
  }, [shown, opener]);
}

/**
 * The sheet itself: the header, and the body below it. It lies over the right column of the page's surface, flush
 * with its top, right and bottom and as wide as `--sheet-width`, so it covers the column and a strip of the page beside
 * it; the page keeps its width under it.
 */
export function SheetFrame({
  label,
  title,
  tag,
  line,
  actions,
  closeLabel,
  onClose,
  hidden = false,
  children,
}: {
  /** The name of the dialog for a person hearing the page. */
  label: string;
  title: string;
  /** A state beside the title. */
  tag?: ReactNode;
  /** The line under the title: where the sheet is pointed. */
  line?: ReactNode;
  /** The sheet's own controls, before the one that closes it. */
  actions?: ReactNode;
  closeLabel: string;
  onClose: () => void;
  /** Hidden, but kept: the sheet's state stays where it was while it is out of sight. */
  hidden?: boolean;
  children: ReactNode;
}) {
  return (
    <section
      role="dialog"
      aria-label={label}
      // The attribute, not a class: preflight hides it, and a person hearing the page does not find a sheet that is away.
      hidden={hidden}
      className={cn(
        "absolute inset-y-0 right-0 z-sheet flex w-(--sheet-width) max-w-full flex-col gap-3 rounded-l-window bg-panel p-3 shadow-menu",
        "animate-in fade-in-0 slide-in-from-right-4 duration-(--motion-fast) ease-(--motion-ease)",
      )}
    >
      <header className="flex items-center gap-2">
        <div className="min-w-0 flex-1 px-2">
          <div className="flex flex-wrap items-center gap-2">
            <h2 className="m-0 min-w-0 break-words text-base font-semibold text-text">{title}</h2>
            {tag}
          </div>
          {line && <p className="m-0 min-w-0 break-words text-xs text-secondary">{line}</p>}
        </div>
        {actions}
        <Button
          type="button"
          variant="ghost"
          size="icon"
          aria-label={closeLabel}
          title={closeLabel}
          onClick={onClose}
        >
          <XIcon aria-hidden="true" className="size-4" weight="bold" />
        </Button>
      </header>
      {children}
    </section>
  );
}
