/**
 * The box that chooses a row or a card. It shows when the pointer is over its row, when it has the focus, and on every
 * row while anything is chosen, so a page of quiet rows has no boxes until they are wanted. It is a real checkbox: Tab
 * reaches it and Space toggles it, and a click with Shift held chooses the range. It keeps its events to itself,
 * because the row it sits in opens on a click and a card is picked up by a pointer or a key.
 *
 * The row says `group` and the box sits where the row's `group-hover` reaches it.
 */
import type { SyntheticEvent } from "react";
import { cn } from "../lib/cn";
import { keepKey } from "../lib/shortcuts";

const keep = (event: SyntheticEvent) => event.stopPropagation();

export function SelectBox({
  label,
  checked,
  anyChosen,
  onToggle,
  className,
}: {
  /** What the box chooses, as the screen reader says it: `Choose #12`. */
  label: string;
  checked: boolean;
  /** Something is chosen on the page: every box shows. */
  anyChosen: boolean;
  onToggle: (range: boolean) => void;
  className?: string;
}) {
  return (
    <input
      type="checkbox"
      aria-label={label}
      checked={checked}
      onPointerDown={keep}
      onKeyDown={(event) => {
        // Escape lets go of the choice, which listens at the window: it is the one key this box does not keep.
        if (event.key !== "Escape") keepKey(event);
      }}
      onClick={(event) => {
        event.stopPropagation();
        onToggle(event.shiftKey);
      }}
      onChange={() => {}}
      className={cn(
        "size-4 shrink-0 cursor-pointer accent-(--accent) transition-opacity duration-(--motion-fast) ease-out focus-visible:opacity-100 group-hover:opacity-100",
        checked || anyChosen ? "opacity-100" : "opacity-0",
        className,
      )}
    />
  );
}
