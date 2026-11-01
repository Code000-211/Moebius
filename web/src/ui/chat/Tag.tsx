/** A small tag with a tone: the status of a chat, the place it works in. */
import { cn } from "../lib/cn";

export type TagTone = "teal" | "amber" | "red" | "plain";

const TONES: Record<TagTone, string> = {
  teal: "bg-teal-fill text-teal",
  amber: "bg-amber-fill text-amber",
  red: "bg-red-fill text-red",
  plain: "bg-card text-normal",
};

export function Tag({
  tone = "plain",
  className,
  children,
}: {
  tone?: TagTone;
  className?: string;
  children: React.ReactNode;
}) {
  return (
    <span
      className={cn(
        "inline-flex max-w-full items-center truncate rounded-sm px-base py-half font-mono text-xs",
        TONES[tone],
        className,
      )}
    >
      {children}
    </span>
  );
}
