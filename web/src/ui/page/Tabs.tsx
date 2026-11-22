/**
 * The switch between the views of one thing — a run's events, changes and terminal. Each is a word 26px high in the
 * muted colour; the chosen one is indigo, semibold, with a 2px underline of the accent that sits on the bottom rule of
 * what holds the row. No pill, no fill. A tab may carry a count after its word (not drawn at zero), small and in the accent. The row is a tablist with the focus on one
 * tab: only the chosen one is in the tab order, and an arrow key moves the choice and the focus with it, so Tab steps
 * out of the row rather than through it. `TabPanel` is the view a tab chooses; it names the tab that shows it, and the
 * chosen tab names it in turn. A page takes one id from `useId` and gives it to both, under `idBase`.
 *
 * In a one-row `PageHeader` that is short of room the tabs that do not fit move, from the right, into a last tab
 * "More" with a caret: a menu whose items choose the tab like the tabs do. When the chosen tab is one of those, "More"
 * wears its word and the underline, so the row still says where the page is. Every tab stays reachable.
 */
import { CaretDownIcon } from "@phosphor-icons/react";
import { type ReactNode, useContext, useEffect, useRef } from "react";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "../components/DropdownMenu";
import { cn } from "../lib/cn";
import { HeaderFitContext } from "./HeaderFit";

const TAB =
  "relative inline-flex h-[26px] cursor-pointer items-center gap-1 rounded-control bg-transparent px-1 text-base transition-colors duration-(--motion-fast) ease-out";
const CHOSEN =
  "font-semibold text-accent-text after:absolute after:inset-x-0 after:-bottom-px after:h-0.5 after:bg-accent";
const QUIET = "text-muted hover:bg-hover hover:text-text";

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
  const fit = useContext(HeaderFitContext);
  const setTabCount = fit?.setTabCount;
  useEffect(() => {
    setTabCount?.(tabs.length);
  }, [setTabCount, tabs.length]);
  const hidden = Math.min(fit?.hiddenTabs ?? 0, Math.max(tabs.length - 1, 0));
  const shown = tabs.slice(0, tabs.length - hidden);
  const more = tabs.slice(tabs.length - hidden);
  const inMore = more.find((each) => each.value === value);
  return (
    <div
      role="tablist"
      aria-label={label}
      ref={row}
      className={cn("flex flex-wrap items-end gap-4", className)}
      onKeyDown={(event) => {
        if (event.key !== "ArrowRight" && event.key !== "ArrowLeft") return;
        // The menu of "More" is drawn elsewhere but its keys bubble here; they are the menu's.
        if (!row.current?.contains(event.target as Node)) return;
        const at = tabs.findIndex((each) => each.value === value);
        const next =
          event.key === "ArrowRight"
            ? (at + 1) % tabs.length
            : (at - 1 + tabs.length) % tabs.length;
        event.preventDefault();
        onChange((tabs[next] as { value: T }).value);
        // The focus follows the choice, which is what keeps the arrows moving the tabs a person is on.
        row.current
          ?.querySelectorAll<HTMLButtonElement>("[role='tab']")
          [Math.min(next, shown.length)]?.focus();
      }}
    >
      {shown.map((each) => {
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
            className={cn(TAB, chosen ? CHOSEN : QUIET)}
          >
            {each.label}
            {each.count !== undefined && each.count > 0 && (
              <span className="text-xs font-normal tabular-nums text-accent-text">
                {each.count}
              </span>
            )}
          </button>
        );
      })}
      {more.length > 0 && (
        <DropdownMenu>
          <DropdownMenuTrigger
            role="tab"
            data-testid="tabs-more"
            // Named as the chosen tab while it is in here, so its panel still has a tab to be named by.
            id={inMore ? tabId(idBase, inMore.value) : undefined}
            aria-controls={inMore ? panelId(idBase, inMore.value) : undefined}
            aria-selected={inMore !== undefined}
            tabIndex={inMore ? 0 : -1}
            className={cn(TAB, inMore ? CHOSEN : QUIET)}
          >
            {inMore ? inMore.label : "More"}
            <CaretDownIcon className="size-3" weight="bold" aria-hidden="true" />
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" aria-label="More tabs">
            {more.map((each) => (
              <DropdownMenuItem
                key={each.value}
                className={cn(each.value === value && "font-semibold")}
                onSelect={() => onChange(each.value)}
              >
                {each.label}
                {each.count !== undefined && each.count > 0 && (
                  <span className="text-xs font-normal tabular-nums text-accent-text">
                    {each.count}
                  </span>
                )}
              </DropdownMenuItem>
            ))}
          </DropdownMenuContent>
        </DropdownMenu>
      )}
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
