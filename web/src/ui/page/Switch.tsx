// One thing switched on or off. Two buttons that say "On" and "Off" ask to be read as a choice between two actions;
// a switch says the same in one control, whose knob is where the state is whether or not the colour reaches anyone.
import { cn } from "../lib/cn";

/**
 * A switch, at the height of every other control, with the word it stands on beside it.
 *
 * The state is the knob's end, then the word, then `aria-checked` — never the colour of the track alone. The word
 * beside it is not repeated in the accessible name, which names what is being switched rather than its state, so a
 * screen reader hears the control once.
 */
export function Switch({
  checked,
  onChange,
  label,
  on = "On",
  off = "Off",
  disabled = false,
  className,
}: {
  /** Whether it is on. */
  checked: boolean;
  /** What to do with the other state, when the knob is moved or the control pressed. */
  onChange: (checked: boolean) => void;
  /** What is being switched on and off, as a screen reader is told it. */
  label: string;
  /** The word that stands for on. */
  on?: string;
  /** The word that stands for off. */
  off?: string;
  /** Whether it cannot be moved now, such as while a change is being saved. */
  disabled?: boolean;
  className?: string;
}) {
  return (
    <span className="inline-flex items-center gap-2">
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
            "inline-flex h-5 w-9 items-center rounded-full px-0.5 transition-colors duration-(--motion-routine) ease-out",
            checked ? "bg-status-done" : "bg-track",
          )}
        >
          <span
            className={cn(
              "size-4 shrink-0 rounded-full bg-surface transition-transform duration-(--motion-routine) ease-out",
              checked ? "translate-x-4" : "translate-x-0",
            )}
          />
        </span>
      </button>
      <span className="text-sm text-secondary">{checked ? on : off}</span>
    </span>
  );
}
