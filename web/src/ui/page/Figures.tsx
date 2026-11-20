/**
 * A row of figures at the head of a page: each a value over its label, set in tabular numerals, with no box, no
 * outline and no caption round it. A fact that does not fit the label goes in `hint`, which is the figure's tooltip, so
 * nothing on the screen says it twice. The Analytics and Usage pages lead with one.
 *
 * ```tsx
 * <Figures items={[{ label: "Cards done", value: "42" }]} />
 * ```
 */
import { cn } from "../lib/cn";

export interface Figure {
  label: string;
  value: string;
  /** What else is true of the figure, shown as its tooltip. */
  hint?: string;
}

export function Figures({ items, className }: { items: Figure[]; className?: string }) {
  return (
    // biome-ignore lint/a11y/noRedundantRoles: preflight sets list-style: none, and WebKit then drops a list with no marker from the accessibility tree entirely
    <ul role="list" className={cn("m-0 flex list-none flex-wrap gap-x-10 gap-y-4 p-0", className)}>
      {items.map((item) => (
        <li
          key={item.label}
          data-testid={item.label}
          title={item.hint}
          className="flex min-w-24 flex-col gap-1"
        >
          <span
            data-figure
            className="text-xl font-semibold leading-none tabular-nums text-text [font-variant-numeric:tabular-nums]"
          >
            {item.value}
          </span>
          <span className="text-sm text-muted">{item.label}</span>
        </li>
      ))}
    </ul>
  );
}
