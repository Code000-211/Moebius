/**
 * The state of a place with nothing in it yet: what belongs here and how to begin. It replaces a blank panel or a row
 * of machinery, and carries an action only when there is one to take. On the canvas it is a card of its own; in a card
 * it is `plain`, because one surface is enough to read a sentence on.
 */
import type { ReactNode } from "react";
import { cn } from "../lib/cn";

export function Empty({
  title,
  body,
  icon,
  action,
  plain = false,
  className,
}: {
  /** What belongs here, said plainly: "No agent yet". */
  title: string;
  /** How to begin, or what the absence means. */
  body?: ReactNode;
  /**
   * The shape of the state, beside its name — a check for a place with nothing waiting in it. It leans on the title
   * rather than carrying it, so it is hidden from a screen reader and never needs words of its own.
   */
  icon?: ReactNode;
  /** The first thing to do, when there is one. */
  action?: ReactNode;
  /** Flat, for the empty state that sits inside a card: the card is already the surface under it. */
  plain?: boolean;
  /** For a card that sits inside another card, where the padding of a full one is too much. */
  className?: string;
}) {
  return (
    <section
      className={cn(
        "flex flex-col items-start gap-3",
        !plain && "rounded-card bg-surface p-6 shadow-card",
        className,
      )}
    >
      <div className="flex items-center gap-3">
        {icon && (
          <span aria-hidden="true" className="flex shrink-0">
            {icon}
          </span>
        )}
        <h2 className="m-0 text-lg font-semibold tracking-tight text-text">{title}</h2>
      </div>
      {body && <p className="m-0 max-w-prose text-base text-secondary">{body}</p>}
      {action}
    </section>
  );
}
