/**
 * The people a card or an inbox item is given to, as avatars: a round mark with the first letter of the person's
 * handle, in the neutral tones of the theme (a person is never told apart by colour alone: the handle is the avatar's
 * name for a screen reader and its tooltip). A stack shows the first few and counts the rest.
 */
import { cn } from "../lib/cn";

/** How many avatars a stack shows before it counts the rest. */
const SHOWN = 3;

/** One person's avatar: the letter of their handle in a round mark. */
export function Avatar({ handle, className }: { handle: string; className?: string }) {
  return (
    <span
      title={`@${handle}`}
      aria-hidden="true"
      className={cn(
        "inline-flex size-5 shrink-0 items-center justify-center rounded-full border border-hairline bg-sunken text-[10px] font-semibold uppercase leading-none text-text",
        className,
      )}
    >
      {handle.charAt(0)}
    </span>
  );
}

/** The avatars of `handles`, overlapping, with the count of those not shown; nothing for no one. */
export function AvatarStack({ handles, className }: { handles: string[]; className?: string }) {
  if (handles.length === 0) return null;
  const shown = handles.slice(0, SHOWN);
  const more = handles.length - shown.length;
  return (
    <span
      role="img"
      aria-label={`Given to ${handles.map((h) => `@${h}`).join(", ")}`}
      className={cn("inline-flex shrink-0 items-center", className)}
    >
      {shown.map((handle, index) => (
        <Avatar key={handle} handle={handle} className={cn(index > 0 && "-ml-1.5")} />
      ))}
      {more > 0 && (
        <span aria-hidden="true" className="ml-1 text-xs text-secondary">
          {`+${more}`}
        </span>
      )}
    </span>
  );
}
