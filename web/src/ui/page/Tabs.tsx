/**
 * The switch between the views of one thing — a run's events, changes and terminal. Each is a
 * 40px pill, the chosen one is the selected fill rather than an underline. The row is a tablist with the focus on one
 * tab: only the chosen one is in the tab order, and an arrow key moves the choice and the focus with it, so Tab steps
 * out of the row rather than through it. `TabPanel` is the view a tab chooses; it names the tab that shows it, and the
 * chosen tab names it in turn. A page takes one id from `useId` and gives it to both, under `idBase`.
 */
import { type ReactNode, useRef } from "react";
import { cn } from "../lib/cn";

/** The id of the tab of `value`, and of the panel that holds it. */
const tabId = (base: string, value: string) => `${base}-tab-${value}`;
const panelId = (base: string, value: string) => `${base}-panel-${value}`;

export function Tabs<T extends string>({
  tabs,
  value,
  onChange,
  label,
  idBase,
  className,
}: {
  /** What may be shown, in the order they sit. */
  tabs: Array<{ value: T; label: string; count?: number }>;
  value: T;
  onChange: (value: T) => void;
  /** What the row of tabs is for, for the person hearing the page read aloud. */
  label: string;
  /** The id the `TabPanel`s of these views are named from, so the two know each other. */
  idBase: string;
  className?: string;
}) {
  const row = useRef<HTMLDivElement>(null);
  return (
    <div
      role="tablist"
      aria-label={label}
      ref={row}
      className={cn("flex flex-wrap gap-2", className)}
      onKeyDown={(event) => {
        if (event.key !== "ArrowRight" && event.key !== "ArrowLeft") return;
        const at = tabs.findIndex((each) => each.value === value);
        const next =
          event.key === "ArrowRight"
            ? (at + 1) % tabs.length
            : (at - 1 + tabs.length) % tabs.length;
        event.preventDefault();
        onChange((tabs[next] as { value: T }).value);
        // The focus follows the choice, which is what keeps the arrows moving the tabs a person is on.
        row.current?.querySelectorAll<HTMLButtonElement>("[role='tab']")[next]?.focus();
      }}
    >
      {tabs.map((each) => {
        const chosen = each.value === value;
        return (
          <button
            key={each.value}
            type="button"
            role="tab"
            id={tabId(idBase, each.value)}
            aria-selected={chosen}
            // Only the chosen tab is a stop of the tab order, and only it has a panel to name: the others are not in
            // the document until they are chosen.
            tabIndex={chosen ? 0 : -1}
            aria-controls={chosen ? panelId(idBase, each.value) : undefined}
            onClick={() => onChange(each.value)}
            className={cn(
              "inline-flex min-h-10 cursor-pointer items-center gap-2 rounded-control px-4 text-base transition-colors duration-(--motion-routine) ease-out",
              chosen
                ? "bg-selected font-semibold text-text"
                : "bg-sunken text-secondary hover:bg-hover hover:text-text",
            )}
          >
            {each.label}
            {each.count !== undefined && <span className="text-sm tabular-nums">{each.count}</span>}
          </button>
        );
      })}
    </div>
  );
}

/**
 * The view one tab of `Tabs` holds. It is named by that tab and named back to it, and takes focus so that the
 * keyboard can reach what is in it after leaving the row of tabs.
 */
export function TabPanel({
  idBase,
  value,
  className,
  testId,
  children,
}: {
  /** The same id the row of `Tabs` was given. */
  idBase: string;
  /** The view this panel holds, the name its tab carries. */
  value: string;
  className?: string;
  /** For the tests that watch one view of a page. */
  testId?: string;
  children: ReactNode;
}) {
  return (
    <div
      role="tabpanel"
      /* biome-ignore lint/a11y/noNoninteractiveTabindex: the panel is where Tab goes after the row of tabs, and
          what the keyboard scrolls when a view is long, so it takes focus itself */
      tabIndex={0}
      id={panelId(idBase, value)}
      aria-labelledby={tabId(idBase, value)}
      data-testid={testId}
      className={cn("min-w-0", className)}
    >
      {children}
    </div>
  );
}
