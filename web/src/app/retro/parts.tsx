/**
 * The piece of the Retro area that has no shared counterpart: a diff that wraps instead of scrolling. The shared code
 * block scrolls sideways, which hides the end of a diff line; `Diff` wraps every line so all of it is in view.
 */
import { cn } from "../../ui/lib/cn";

/** A diff, every line on a line of its own, with added and removed lines tinted as well as marked. */
export function Diff({ text }: { text: string }) {
  return (
    <pre className="m-0 overflow-hidden rounded-control bg-code-bg text-code-text">
      {text.split("\n").map((line, index) => (
        <span
          // biome-ignore lint/suspicious/noArrayIndexKey: a diff line is only known by where it sits
          key={index}
          className={cn(
            "block px-4 py-1 font-mono text-sm leading-relaxed break-words whitespace-pre-wrap",
            line.startsWith("+") && "bg-diff-added",
            line.startsWith("-") && "bg-diff-removed",
          )}
        >
          {line === "" ? " " : line}
        </span>
      ))}
    </pre>
  );
}
