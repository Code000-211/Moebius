// One thing switched on or off. Two buttons that say "On" and "Off" ask to be read as a choice between two actions;
// a switch says the same in one control, whose knob is where the state is whether or not the colour reaches anyone.
// It carries no word of its own: a screen reader is told the state by `role="switch"` and `aria-checked`.
import { cn } from "../lib/cn";

/**
 * A switch, at the height of every other control.
 *
 * The state is the knob's end and `aria-checked` — never the colour of the track alone. There is no "On" or "Off"
 * beside it: the accessible name says what is being switched, and the role and `aria-checked` make a screen reader
 * announce the state ("on", "off") after it.
 */
export function Switch({
  checked,
  onChange,
  label,
  disabled = false,
  className,
}: {
  /** Whether it is on. */
  checked: boolean;
  /** What to do with the other state, when the knob is moved or the control pressed. */
  onChange: (checked: boolean) => void;
  /** What is being switched on and off, as a screen reader is told it. */
  label: string;
  /** Whether it cannot be moved now, such as while a change is being saved. */
  disabled?: boolean;
  className?: string;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className={cn(
        "inline-flex min-h-7 w-9 shrink-0 cursor-pointer items-center rounded-control disabled:cursor-not-allowed disabled:opacity-45",
        className,
      )}
    >
      <span
        aria-hidden="true"
        className={cn(
          "inline-flex h-5 w-9 items-center rounded-full px-0.5 ring-1 ring-inset ring-hairline transition-colors duration-(--motion-routine) ease-out",
          checked ? "bg-accent-fill" : "bg-track",
        )}
      >
        <span
          className={cn(
            "size-4 shrink-0 rounded-full bg-panel transition-transform duration-(--motion-routine) ease-out",
            checked ? "translate-x-4" : "translate-x-0",
          )}
        />
      </span>
    </button>
  );
}
