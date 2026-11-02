/**
 * The list of things on the left of a page and the detail of the one chosen beside it. `ListCard` is the row of that
 * list — a title, tags, a line, and whether it is the chosen one; `ListDetail` puts the two side by side, stacks them
 * on a narrow screen, gives each column its own scroll, and lays the rows out as a list a screen reader can count, so
 * a page needs no markup of its own for that; `Table` is the rows of a detail that is a list of records. These are the
 * pieces the Agents, Skills, MCP, Automations, Retro and Experiments pages share.
 */
import { Children, Fragment, isValidElement, type ReactElement, type ReactNode } from "react";
import { Tag, type TagTone } from "../chat/Tag";
import { cn } from "../lib/cn";
import { Empty } from "./Empty";

/** A card of a list on the left of a page: a title, tags, a line, and whether it is the chosen one. */
export function ListCard({
  title,
  tags,
  line,
  alert,
  selected,
  onClick,
}: {
  title: string;
  tags?: Array<{ label: string; tone: TagTone }>;
  line: string;
  /** The line is a failure. */
  alert?: boolean;
  selected: boolean;
  /** Off for a list nothing can be chosen from yet. */
  onClick?: () => void;
}) {
  return (
    <button
      type="button"
      data-testid="list-card"
      aria-current={selected ? "true" : undefined}
      disabled={!onClick}
      onClick={onClick}
      className={cn(
        "flex min-h-10 w-full cursor-pointer flex-col gap-2 rounded-control p-3 text-left",
        "transition-colors duration-(--motion-routine) ease-out",
        selected
          ? "bg-selected"
          : "bg-surface hover:bg-hover disabled:cursor-default disabled:hover:bg-surface",
      )}
    >
      <span className="flex min-w-0 items-start justify-between gap-3">
        <span className="min-w-0 break-words text-base font-semibold text-text">{title}</span>
        {tags && tags.length > 0 && (
          <span className="flex shrink-0 gap-2">
            {tags.map((tag) => (
              <Tag key={tag.label} tone={tag.tone}>
                {tag.label}
              </Tag>
            ))}
          </span>
        )}
      </span>
      {line && (
        // Four lines at most, the last one fading: the line tells one row from the next, and the whole of it is a
        // click away in the detail beside the list.
        <span
          className={cn(
            "clamp-4 fade-last-4 break-words text-sm",
            alert ? "text-error-text" : "text-secondary",
          )}
        >
          {line}
        </span>
      )}
    </button>
  );
}

/** The children of what a page handed over, in order, with the fragments opened so the rows inside one are rows too. */
function childrenOf(node: ReactNode): ReactNode[] {
  const list: ReactNode[] = [];
  Children.forEach(node, (child) => {
    if (child === null || child === undefined || typeof child === "boolean") return;
    if (isValidElement(child) && child.type === Fragment) {
      list.push(...childrenOf((child.props as { children?: ReactNode }).children));
      return;
    }
    list.push(child);
  });
  return list;
}

const isCard = (child: ReactNode): child is ReactElement =>
  isValidElement(child) && child.type === ListCard;

const isEmpty = (child: ReactNode): boolean => isValidElement(child) && child.type === Empty;

/**
 * The rows of the left column as a real list: every `ListCard` sits in an `<li>` of one `<ul role="list">`, so the
 * person hearing the page read learns how many rows there are instead of running through buttons. `role="list"` is
 * spelled out because the preflight of Tailwind takes the bullets off, which is what makes WebKit drop the list from
 * the accessibility tree. Anything else the page hands over — the line that counts the rows, a loader, a notice —
 * keeps its place above them.
 */
function Rows({ children }: { children: ReactNode }) {
  const cards: ReactElement[] = [];
  const other: ReactNode[] = [];
  for (const child of childrenOf(children)) {
    if (isCard(child)) cards.push(child);
    else other.push(child);
  }
  return (
    <>
      {other}
      {cards.length > 0 && (
        // biome-ignore lint/a11y/noRedundantRoles: preflight sets list-style: none, and WebKit then drops a list with no marker from the accessibility tree entirely
        <ul role="list" className="m-0 flex flex-col gap-2 p-0">
          {cards.map((card) => (
            <li key={card.key ?? undefined} className="flex flex-col">
              {card}
            </li>
          ))}
        </ul>
      )}
    </>
  );
}

/**
 * A list with the detail of the chosen item beside it; on a narrow screen they stack.
 *
 * A list with nothing in it is not worth a column of its own: an empty array — what `rows.map(...)` gives when there
 * are no rows — and an empty state handed over instead of rows both leave the column out, so the detail, which is then
 * the empty state with the first row in it, takes the width the page has. A loader is not nothing: the rows are on
 * their way, and the column keeps its place until they arrive.
 *
 * Each column has the height the page has and scrolls on its own, so reading a long detail does not carry the list
 * away with it. The caller fills the page for that — `flex h-full min-h-0 flex-col` round the header and this — and
 * stacked columns go back to scrolling with the page.
 *
 * A column is `relative`, because what it scrolls is its own: an absolutely placed thing inside it — the hidden hint
 * of a field, for one — otherwise stands where its static position falls inside the scrolled content but belongs to
 * the page, and stretches the document past the window, which is a second scrollbar under the one the column already
 * has. A column also takes back the page's bottom padding (`-mb-6` against the 24 `main` keeps), so its last row ends
 * at the edge of the window rather than a band of nothing above it.
 */
export function ListDetail({ list, detail }: { list: ReactNode; detail: ReactNode }) {
  const nothingToList = childrenOf(list).every((child) => !isCard(child) && isEmpty(child));
  return (
    <div
      className={cn(
        "flex min-h-0 flex-1 flex-col gap-6",
        !nothingToList && "lg:flex-row lg:items-stretch",
      )}
    >
      {!nothingToList && (
        <div
          data-testid="list-column"
          className="relative flex w-full shrink-0 flex-col gap-2 lg:-mb-6 lg:min-h-0 lg:w-72 lg:overflow-y-auto"
        >
          <Rows>{list}</Rows>
        </div>
      )}
      <div
        data-testid="detail-column"
        className="relative flex min-w-0 flex-1 flex-col gap-6 lg:-mb-6 lg:min-h-0 lg:overflow-y-auto"
      >
        {detail}
      </div>
    </div>
  );
}

/** A table with a head row; the cells are the caller's. */
export function Table({ head, children }: { head: string[]; children: ReactNode }) {
  return (
    <div className="overflow-x-auto">
      <table className="w-full border-collapse text-left text-base">
        <thead>
          <tr className="border-b border-hairline">
            {head.map((name) => (
              <th key={name} scope="col" className="px-3 py-2 text-sm font-semibold text-secondary">
                {name}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>{children}</tbody>
      </table>
    </div>
  );
}

/** The classes of a cell of `Table`. */
export const CELL = "border-b border-hairline px-3 py-3 align-top text-text";

/**
 * The row a list beside a detail opens on. The address decides when it names one; otherwise the first row is chosen, so
 * a page with something in it opens on something real instead of on "Nothing chosen". The empty state is left for a
 * list that is genuinely empty.
 *
 * A page passes no names for a list it cannot open yet — a read-only list, a list that has not been read — and gets no
 * choice back.
 */
export function chosenOrFirst(
  chosen: string | null,
  creating: boolean,
  names: readonly string[],
): string | null {
  if (chosen !== null) return chosen;
  if (creating) return null;
  return names[0] ?? null;
}
