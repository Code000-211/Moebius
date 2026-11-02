/**
 * The pieces the Retro page and its lessons box share: a name for a piece of content that is not a card, and a diff
 * that wraps instead of scrolling.
 *
 * `Box` of `src/ui/page` is the card, and it names its own region with its heading, so a card needs no wrapper of its
 * own; `Region` is for what is not a card — a refusal that stands where a card would have been. A word that carries a
 * state is the shared tag of `src/ui/chat`, which already sets it at caption size on the hue of that state. The shared
 * code block scrolls sideways, which hides the end of a diff line; `Diff` wraps every line so all of it is in view.
 */
import type { ReactNode } from "react";
import { cn } from "../../ui/lib/cn";

/** The name of the part of the page a loose piece of content holds, so its landmarks can be listed and found. */
export function Region({ label, children }: { label: string; children: ReactNode }) {
  return <section aria-label={label}>{children}</section>;
}

/** A diff, every line on a line of its own, with added and removed lines tinted as well as marked. */
export function Diff({ text }: { text: string }) {
  return (
    <pre className="m-0 overflow-hidden rounded-control bg-code text-text">
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
