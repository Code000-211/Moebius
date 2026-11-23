/**
 * The people a card or an inbox item is given to, as avatars: a round mark with the first letter of the person's
 * handle in white on one of the three avatar colours of the standard (`--avatar-1` to `--avatar-3`), chosen from the
 * handle so a person keeps theirs everywhere (a person is never told apart by colour alone: the handle is the avatar's
 * name for a screen reader and its tooltip). Moebius and its agents (`agent:<name>`, or `Moebius`) are the dark indigo
 * rounded square (`--avatar-agent`) instead of a circle. A stack shows the first few and counts the rest.
 */
import { cn } from "../lib/cn";

/** How many avatars a stack shows before it counts the rest. */
const SHOWN = 3;

const PEOPLE = ["bg-avatar-1", "bg-avatar-2", "bg-avatar-3"] as const;

/** Whether the handle is Moebius or one of its agents rather than a person. */
export const isAgent = (handle: string): boolean =>
  handle === "Moebius" || handle.startsWith("agent:");

/** The colour of a person's avatar: the same handle always gets the same one. */
function fill(handle: string): string {
  let sum = 0;
  for (const letter of handle.toLowerCase()) sum += letter.codePointAt(0) ?? 0;
  return PEOPLE[sum % PEOPLE.length] ?? PEOPLE[0];
}

/** One avatar: the letter of a person's handle in a round mark, or Moebius's own square mark. */
export function Avatar({ handle, className }: { handle: string; className?: string }) {
  const agent = isAgent(handle);
  const name = agent ? handle.replace(/^agent:/, "") : handle;
  return (
    <span
      title={agent ? name : `@${handle}`}
      aria-hidden="true"
      className={cn(
        "inline-flex size-5 shrink-0 items-center justify-center text-[10px] font-semibold uppercase leading-none text-white",
        agent ? "rounded-control bg-avatar-agent" : cn("rounded-full", fill(handle)),
        className,
      )}
    >
      {name.charAt(0)}
    </span>
  );
}

/** The avatars of `handles`, overlapping, with the count of those not shown; nothing for no one. */
export function AvatarStack({
  handles,
  className,
  label = "Given to",
}: {
  handles: string[];
  className?: string;
  /** What the avatars are of, for a screen reader: "Given to" the people of the card, "Taken by" someone on another server. */
  label?: string;
}) {
  if (handles.length === 0) return null;
  const shown = handles.slice(0, SHOWN);
  const more = handles.length - shown.length;
  return (
    <span
      role="img"
      aria-label={`${label} ${handles.map((h) => `@${h}`).join(", ")}`}
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
