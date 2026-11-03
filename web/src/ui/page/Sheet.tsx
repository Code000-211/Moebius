/**
 * The floating sheet the shell keeps over the pages: a borderless card at the right of the window that leaves the page
 * recognisable behind it, with a header (a title, a line under it, the sheet's own controls, and the control that
 * minimises it). The floating chat and the package sheet are both made of these pieces, so they open, minimise and
 * close the same way: Escape or the minimise control hides the sheet and puts the focus back on the button that opened
 * it, and the sheet's state stays where it was until it is opened again. The sheets share one slot at the right of the
 * window (`SheetSlot.tsx`), with the round buttons in a row under it, so a button is never under a sheet.
 */
import { MinusIcon } from "@phosphor-icons/react";
import { type ReactNode, type RefObject, useEffect, useRef } from "react";
import { Button } from "../components/Button";
import { cn } from "../lib/cn";
import { useSheetSlot } from "./SheetSlot";

/**
 * Hides the sheet on Escape wherever the focus is on the page, and, once it is hidden, puts the focus back on the
 * button that opened it so a keyboard does not lose its place; a sheet that was minimised because the other one took
 * the slot leaves the focus where it is. Returns the ref to give that button.
 */
export function useSheetBehaviour(
  shown: boolean,
  hide: () => void,
): RefObject<HTMLButtonElement | null> {
  const opener = useRef<HTMLButtonElement>(null);
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
      if (slot.current === null) opener.current?.focus();
    }
  }, [shown]);
  return opener;
}

/** The round button a hidden sheet leaves at the bottom right; `className` places it. */
export function SheetOpener({
  label,
  title,
  icon,
  onOpen,
  openerRef,
  className,
}: {
  label: string;
  title: string;
  icon: ReactNode;
  onOpen: () => void;
  openerRef: RefObject<HTMLButtonElement | null>;
  className?: string;
}) {
  return (
    <Button
      type="button"
      variant="primary"
      ref={openerRef}
      aria-label={label}
      title={title}
      onClick={onOpen}
      className={cn("fixed bottom-6 z-sheet size-12 rounded-full p-0", className)}
    >
      {icon}
    </Button>
  );
}

/** The sheet itself: the header, and the body below it, which `className` and `children` lay out. */
export function SheetFrame({
  label,
  title,
  tag,
  line,
  actions,
  minimiseLabel,
  onMinimise,
  className,
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
  /** The sheet's own controls, before the one that minimises it. */
  actions?: ReactNode;
  minimiseLabel: string;
  onMinimise: () => void;
  /** Where the sheet sits and how wide it is. */
  className: string;
  /** Minimised, but kept: the sheet's state stays where it was while it is out of sight. */
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
        "fixed z-sheet flex max-w-full flex-col gap-3 rounded-card bg-surface p-3 shadow-raised",
        className,
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
          aria-label={minimiseLabel}
          title={minimiseLabel}
          onClick={onMinimise}
        >
          <MinusIcon aria-hidden="true" className="size-5" weight="bold" />
        </Button>
      </header>
      {children}
    </section>
  );
}
