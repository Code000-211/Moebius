/**
 * The one virtualised list that Inbox, My issues, the project lists and Fleet share: rows and the group rows between
 * them in a single flat list, of which only the rows in view (and a screenful around them) are in the DOM.
 *
 * A caller hands over `items` in the order of the page: `{ type: "group", key, name }` or `{ type: "row", key, data }`,
 * and draws a row from its data with `renderRow`. The list is a `role="list"` of `role="listitem"`s, each with
 * `aria-posinset` and `aria-setsize` counted over the whole list, so a screen reader hears "3 of 2000" although only
 * a few are drawn.
 *
 * The selection is kept by key, not by place in the DOM: `selectedKey` is the row with the edge, and the arrow keys
 * move it to the next row (group rows are skipped), Home and End to the first and the last, Enter opens it
 * (`onOpen`). With Shift an arrow, or a click, chooses the range from the anchor to the row, worked out from the
 * ordered keys and not from what is drawn, so it reaches rows that are not on the screen (`onSelectRange`). The list
 * scrolls the row it moves to into view.
 *
 * While `loading`, skeleton rows are drawn; with no items and not loading, `empty` is drawn in the list's place.
 *
 * ```tsx
 * <VirtualList
 *   label="Inbox"
 *   items={items}
 *   selectedKey={open}
 *   onSelect={setOpen}
 *   onOpen={openCard}
 *   renderRow={(issue, { at, selected, onClick }) => (
 *     <ListRow at={at} selected={selected} onClick={onClick} status={issue.state} id={issue.id} title={issue.title} />
 *   )}
 *   empty={<Empty title="Nothing needs you" />}
 * />
 * ```
 *
 * `initialItemCount` is for tests: a virtual list draws nothing in a DOM that has no size, so a test sets it, or
 * mocks the viewport (`virtualListTest.ts`).
 */
import {
  type KeyboardEvent,
  type MouseEvent,
  type ReactNode,
  type Ref,
  useCallback,
  useImperativeHandle,
  useMemo,
  useRef,
} from "react";
import { Virtuoso, type VirtuosoHandle } from "react-virtuoso";
import { GroupRow, type RowPosition } from "./ListRow";
import { SkeletonList } from "./Skeleton";

export type ListItem<T> =
  | { type: "group"; key: string; name: string }
  | { type: "row"; key: string; data: T };

/** What `renderRow` is given besides the row's data. */
export interface RowState {
  key: string;
  /** Where the row stands in the whole list, for `ListRow`'s `at`. */
  at: RowPosition;
  selected: boolean;
  /** A click on the row: selects it, or with Shift chooses the range to it. */
  onClick: (event: MouseEvent<HTMLElement>) => void;
}

export interface VirtualListHandle {
  /** Scrolls to the item with this key, when the list has it. */
  scrollToKey: (key: string) => void;
}

/** How many skeleton rows a loading list draws: enough to fill a tall window. */
const SKELETON_ROWS = 12;

export function VirtualList<T>({
  items,
  renderRow,
  label,
  selectedKey = null,
  onSelect,
  onSelectRange,
  onOpen,
  loading = false,
  loadingMessage,
  empty,
  initialItemCount,
  handle,
  className,
}: {
  items: ReadonlyArray<ListItem<T>>;
  renderRow: (data: T, state: RowState) => ReactNode;
  /** The list's accessible name. */
  label: string;
  selectedKey?: string | null;
  onSelect?: (key: string) => void;
  /** The keys from the anchor to the row, in page order, when Shift is held. */
  onSelectRange?: (keys: string[]) => void;
  /** Enter on the selected row. */
  onOpen?: (key: string) => void;
  loading?: boolean;
  loadingMessage?: string;
  /** Drawn in place of the list when there is nothing in it and it is not loading. */
  empty?: ReactNode;
  initialItemCount?: number;
  handle?: Ref<VirtualListHandle>;
  className?: string;
}) {
  const virtuoso = useRef<VirtuosoHandle>(null);
  const anchor = useRef<string | null>(null);
  const indexOf = useMemo(() => new Map(items.map((item, index) => [item.key, index])), [items]);
  const rowKeys = useMemo(
    () => items.filter((item) => item.type === "row").map((item) => item.key),
    [items],
  );

  const scrollToKey = useCallback(
    (key: string) => {
      const index = indexOf.get(key);
      if (index !== undefined) virtuoso.current?.scrollIntoView({ index, behavior: "auto" });
    },
    [indexOf],
  );
  useImperativeHandle(handle, () => ({ scrollToKey }), [scrollToKey]);

  /** Moves the choice to `key`, or, with `range`, chooses from the anchor to it. */
  const choose = useCallback(
    (key: string, range: boolean) => {
      if (range && onSelectRange) {
        const from = rowKeys.indexOf(anchor.current ?? selectedKey ?? key);
        const to = rowKeys.indexOf(key);
        if (from >= 0 && to >= 0) {
          onSelectRange(rowKeys.slice(Math.min(from, to), Math.max(from, to) + 1));
          scrollToKey(key);
          return;
        }
      }
      anchor.current = key;
      onSelect?.(key);
      scrollToKey(key);
    },
    [onSelect, onSelectRange, rowKeys, scrollToKey, selectedKey],
  );

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.target !== event.currentTarget || rowKeys.length === 0) return;
    const at = selectedKey === null ? -1 : rowKeys.indexOf(selectedKey);
    let next: number | null = null;
    if (event.key === "ArrowDown") next = Math.min(rowKeys.length - 1, at + 1);
    else if (event.key === "ArrowUp") next = Math.max(0, at < 0 ? 0 : at - 1);
    else if (event.key === "Home") next = 0;
    else if (event.key === "End") next = rowKeys.length - 1;
    else if (event.key === "Enter" && selectedKey !== null) {
      event.preventDefault();
      onOpen?.(selectedKey);
      return;
    }
    if (next === null) return;
    event.preventDefault();
    const key = rowKeys[next];
    if (key !== undefined) choose(key, event.shiftKey);
  };

  if (loading) return <SkeletonList rows={SKELETON_ROWS} message={loadingMessage} />;
  if (items.length === 0) return <>{empty}</>;

  const size = items.length;
  return (
    <Virtuoso
      ref={virtuoso}
      data={items as ListItem<T>[]}
      computeItemKey={(_, item) => item.key}
      initialItemCount={initialItemCount}
      increaseViewportBy={360}
      role="list"
      aria-label={label}
      tabIndex={0}
      onKeyDown={onKeyDown}
      data-testid="virtual-list"
      className={className ?? "min-h-0 flex-1 outline-none"}
      itemContent={(index, item) => {
        const at = { index: index + 1, size };
        if (item.type === "group") return <GroupRow name={item.name} at={at} />;
        return renderRow(item.data, {
          key: item.key,
          at,
          selected: item.key === selectedKey,
          onClick: (event) => choose(item.key, event.shiftKey),
        });
      }}
    />
  );
}
