/**
 * The pieces the Experiments page shares.
 *
 * `Box` of `src/ui/page` is the card, and it names its own region with its heading, so a card needs no wrapper of its
 * own; `Region` is for what is not a card — a refusal that stands where a card would have been — which a screen
 * reader otherwise passes over without knowing what it is looking at. A word that carries a state is the shared tag of
 * `src/ui/chat`, which already sets it at caption size on the hue of that state.
 */
import type { ReactNode } from "react";

/** The name of the part of the page a loose piece of content holds, so its landmarks can be listed and found. */
export function Region({ label, children }: { label: string; children: ReactNode }) {
  return <section aria-label={label}>{children}</section>;
}
