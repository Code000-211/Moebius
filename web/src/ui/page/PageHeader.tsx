/**
 * The head of a page. Every page of the app starts with `PageHeader`: one line of 48px with the page's title, then its
 * tabs, and on the right its actions — quiet text buttons and at most one primary. It sticks to the top of the panel
 * over a hairline rule, which is the rule the chosen tab's underline sits on. There is no sentence under the title and
 * no breadcrumb: the sidebar already says where a page lives. A page reached by drilling down can carry the way back
 * on the left of its name, which is where it is looked for: a button, or a short trail of links such as a workflow's
 * `Project › Workflows ›`. A `badge` after the name says one small fact about the thing, such as what starts a workflow.
 *
 * At the right, in this order on every page: the page's own `actions` (quiet), then its `menu` (a gear or a `...`), then
 * the "Moebius" button that opens the agent's panel (`app/shell/AgentButton.tsx`), and last the page's one `primary`
 * button. So the button that does the page's main thing is always the rightmost and Moebius never moves between pages.
 *
 * A one-row header (`oneRow`) never reaches past its right padding, which in the desktop app is the room of the window
 * buttons. It measures itself (a `ResizeObserver` on the header) and, when its content does not fit, gives way in
 * order: the primary button drops its label and keeps its icon (`HeaderPrimary`, the label stays as its name and
 * tooltip), then the tabs that do not fit move from the right into a last tab "More" (`Tabs`), the chosen tab among
 * them shown there. The title keeps a few letters, cut inside its own box so it never draws over the tabs, and is the last thing cut. The rules are in `fitRules.ts`.
 *
 * In a narrow window (under 1000px) the sidebar is hidden and a button at the left of the header brings it back over
 * the page.
 *
 * In the desktop app the window buttons sit at the right of the header: the shell reserves their room in
 * `--window-buttons` (nothing by default) and the empty stretch of the header drags the window. `Body` is the sentence
 * inside a box, the same measure and colour, so a page reads the same at either level.
 *
 * ```tsx
 * <PageHeader title="Fleet" tabs={<Tabs … />} actions={<Button size="sm">Pause all</Button>} />
 * ```
 */
import {
  createContext,
  type ReactNode,
  useContext,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { AgentButton } from "../../app/shell/AgentButton";
import { useSidebarToggle } from "../../app/shell/SidebarToggle";
import { IconButton } from "../components/IconButton";
import { SidebarIcon } from "../icons";
import { cn } from "../lib/cn";
import { deepestLevel, overflows, partsOfLevel } from "./fitRules";
import { HeaderFitContext } from "./HeaderFit";

/**
 * True inside the content of a Settings area. There a page's title is the area's title, the one every area has: in
 * the same place and the same size as Appearance's, without the sticky bar across the panel, so the title, its rows
 * and their edges share one column.
 */
export const InSettingsArea = createContext(false);

export function PageHeader({
  title,
  tabs,
  actions,
  menu,
  primary,
  /** The way back, for a page that is a thing inside another thing. */
  back,
  /** A small fact about the thing, such as what starts a workflow, right after its name. */
  badge,
  /** Keeps the header to one row of 48px however narrow it gets: the tabs, not the buttons, give way. */
  oneRow = false,
}: {
  title: string;
  /** The tabs of the page, after its title. A page with nothing to choose between has none. */
  tabs?: ReactNode;
  /** The page's own quiet controls. */
  actions?: ReactNode;
  /** The gear or the `...` menu of the page, after its actions. */
  menu?: ReactNode;
  /** The page's one primary button, after the Moebius button. */
  primary?: ReactNode;
  back?: ReactNode;
  badge?: ReactNode;
  oneRow?: boolean;
}) {
  const sidebar = useSidebarToggle();
  const inArea = useContext(InSettingsArea);
  const header = useRef<HTMLElement>(null);
  const [tabCount, setTabCount] = useState(0);
  // A fresh object on every restart, so measuring runs again even when the level was already 0.
  const [fitting, setFitting] = useState({ level: 0 });
  const level = fitting.level;
  const last = deepestLevel(tabCount);
  // Measure after every layout, before the paint: while the content reaches into the right padding, go one level
  // deeper. A change of width (or of the tabs) starts again from the full header, so it grows back when there is room.
  useLayoutEffect(() => {
    const node = header.current;
    if (!oneRow || !node || level >= last) return;
    const end = node.lastElementChild?.getBoundingClientRect().right ?? 0;
    const limit =
      node.getBoundingClientRect().right - Number.parseFloat(getComputedStyle(node).paddingRight);
    if (overflows(end, limit)) setFitting({ level: level + 1 });
  });
  // biome-ignore lint/correctness/useExhaustiveDependencies: the tabs' count is what starts the measuring again
  useLayoutEffect(() => {
    const node = header.current;
    if (!oneRow || !node) return;
    let width = node.getBoundingClientRect().width;
    const watch = new ResizeObserver(([entry]) => {
      const next = entry?.borderBoxSize?.[0]?.inlineSize ?? node.getBoundingClientRect().width;
      if (Math.abs(next - width) < 0.5) return;
      width = next;
      setFitting({ level: 0 });
    });
    watch.observe(node);
    setFitting({ level: 0 });
    return () => watch.disconnect();
  }, [oneRow, tabCount, title]);
  const fit = useMemo(() => ({ ...partsOfLevel(level, tabCount), setTabCount }), [level, tabCount]);
  if (inArea) {
    return (
      <div className="mb-3 flex min-h-10 flex-wrap items-center gap-x-4 gap-y-1">
        <h1 className="m-0 min-w-0 truncate text-xl font-semibold tracking-tight text-text">
          {title}
        </h1>
        {tabs && <div className="flex min-w-0">{tabs}</div>}
        {(actions || menu || primary) && (
          <div className="ml-auto flex min-w-0 max-w-full flex-wrap items-center gap-2">
            {actions}
            {menu}
            {primary}
          </div>
        )}
      </div>
    );
  }
  return (
    <HeaderFitContext.Provider value={oneRow ? fit : null}>
      <header
        ref={header}
        data-page-header
        data-tauri-drag-region
        style={{ containerType: "inline-size", containerName: "header" }}
        className={cn(
          "sticky -top-4 z-10 -mx-4 -mt-4 mb-6 flex min-h-12 shrink-0 items-center gap-x-4 gap-y-1 py-1 border-b border-hairline bg-panel pl-4 pr-[calc(1rem+var(--window-buttons))]",
          oneRow ? "h-12 flex-nowrap" : "flex-wrap",
        )}
      >
        <div
          className={cn(
            "flex min-w-0 items-center gap-1",
            // One row: a few letters of the title stay, and what is longer is cut inside this box, so the title never
            // draws over the tabs; what does not fit after that collapses.
            oneRow && "min-w-20 overflow-hidden",
          )}
        >
          {sidebar?.narrow && (
            <IconButton
              icon={SidebarIcon}
              iconWeight="regular"
              iconClassName="size-5"
              aria-label="Show the sidebar"
              title="Show the sidebar"
              className="-ml-2"
              onClick={() => sidebar.setOpen(true)}
            />
          )}
          {back}
          <h1 className="m-0 min-w-0 truncate text-md font-semibold tracking-tight text-text">
            {title}
          </h1>
          {badge}
        </div>
        {tabs && (
          <div
            className={cn(
              "-mb-1 flex self-stretch [&>[role=tablist]]:items-stretch [&_[role=tab]]:h-full [&_[role=tab]]:pb-1",
              // One row: the tabs keep theirs and the title gives way (it is cut with an ellipsis), so a tab never
              // wraps below the header.
              oneRow
                ? "shrink-0 [&>[role=tablist]]:flex-nowrap [&>[role=tablist]]:gap-3"
                : "min-w-0",
            )}
          >
            {tabs}
          </div>
        )}
        <div
          className={cn(
            "ml-auto flex max-w-full items-center gap-2",
            oneRow ? "shrink-0 flex-nowrap" : "min-w-0 flex-wrap",
          )}
        >
          {actions}
          {menu}
          <AgentButton />
          {primary}
        </div>
      </header>
    </HeaderFitContext.Provider>
  );
}

/** A sentence of a box. */
export function Body({ children }: { children: ReactNode }) {
  return <p className="m-0 max-w-prose text-md text-text">{children}</p>;
}
