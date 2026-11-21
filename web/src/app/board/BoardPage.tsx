/**
 * The board of a repository: its columns of the default workflow with their cards (dragged only where a person may
 * move them), the filters and a shell in the repository's clone, in a sheet over the board. The
 * head is two lines: the title with the layout switch, Terminal and New issue (the form of a new issue, which the shell
 * holds and `C` opens too), and the filter bar under it. The card chosen opens as a window over the page (`CardWindowHost.tsx`). A column ends in "+ Add", which makes a card in place
 * (`ColumnAdd.tsx`). The
 * column of work in progress is the point of the page, so everything else is quiet: the header counts in one sentence,
 * the filters are chips, a column says what its cards are doing in a word, and an empty column says
 * what will arrive there. Everything comes from the snapshot, which the event stream keeps current
 * (`live/invalidation.ts`), so a card moves on this page when the server moves it. The chosen card and the filters are
 * in the address (`?card=`, `?column=`, `?agent=`, `?status=`, `?label=`), and so are Display's choices (`?layout=`,
 * `?group=`, `?sort=`, `?hide=`), so a reload and a link keep them. The filters are the chips of the shared bar
 * (`ui/filters`); `view.ts` says what the board offers it and `ListLayout.tsx` draws the list layout. The page itself decides what a drop does and holds the dialogs; `BoardHead.tsx`
 * has the layout switch and the notice for a leftover `.moebius` folder, `BoardColumn.tsx` a column and its cards, and
 * `BoardCardContent.tsx` what a card shows.
 */
import { BookmarkSimpleIcon, PlusIcon, TerminalWindowIcon } from "@phosphor-icons/react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback, useMemo, useRef, useState } from "react";
import { Link, useNavigate, useParams, useSearchParams } from "react-router";
import { snapshot } from "../../api/calls/board";
import { addView, listViews } from "../../api/calls/views";
import { sayError } from "../../api/errorText";
import { keys } from "../../api/keys";
import type { Card, InboxItem } from "../../api/types/board";
import { Button } from "../../ui/components/Button";
import { type DropResult, KanbanCardFace, KanbanProvider } from "../../ui/components/KanbanBoard";
import { FilterBar } from "../../ui/filters/FilterBar";
import { type Display, displayQuery, readDisplay } from "../../ui/filters/model";
import { cn } from "../../ui/lib/cn";
import { Empty } from "../../ui/page/Empty";
import { Select } from "../../ui/page/Field";
import { PageHeader } from "../../ui/page/PageHeader";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { SkeletonBoard } from "../../ui/page/Skeleton";
import { boardPath } from "../pages";
import { boardFiltersOf, viewOfBoard } from "../views/model";
import { CardContent } from "./BoardCardContent";
import { BoardColumn } from "./BoardColumn";
import { LayoutSwitch, LeftoverNotice } from "./BoardHead";
import { useBulk } from "./bulk";
import { BlockDialog, DeleteDialog, ParentDialog } from "./CardDialogs";
import { useOpenCard } from "./CardWindowHost";
import { cardKey } from "./cardKey";
import { type Decision, useDecisions } from "./decisions";
import { ListLayout } from "./ListLayout";
import {
  cardStatus,
  dropRule,
  type Filters,
  groupByColumn,
  inboxOf,
  NO_FILTERS,
  rootOf,
} from "./model";
import { useNewIssue } from "./NewIssueHost";
import { RepositoryTerminal } from "./RepositoryTerminal";
import { DISPLAY_CHOICES, filterFields, listGroups } from "./view";

/** No inbox read yet: one array for every render, so that what hangs on the inbox is not worked out again. */
const NO_INBOX: InboxItem[] = [];

/** `value` itself, or the one this returned before when it is equal to it: a new object with the same content is no change. */
function useSame<T>(value: T): T {
  const kept = useRef(value);
  if (kept.current !== value && JSON.stringify(kept.current) !== JSON.stringify(value)) {
    kept.current = value;
  }
  return kept.current;
}

export function BoardPage() {
  const { id } = useParams();
  const navigate = useNavigate();
  const [params, setParams] = useSearchParams();
  const data = useQuery({ queryKey: keys.snapshot, queryFn: () => snapshot() });
  // The saved view the address names, to say its name in the filter bar; a failed read only leaves the name out.
  const views = useQuery({
    queryKey: keys.views,
    queryFn: listViews,
    enabled: params.has("view"),
  });
  const { decide, pending, notify } = useDecisions();
  const queryClient = useQueryClient();
  const newIssue = useNewIssue();
  // The column whose line for a new card is open.
  const [adding, setAdding] = useState<string | null>(null);
  // The card a menu asked to move below another, until the question is answered.
  const [moving, setMoving] = useState<Card | null>(null);
  // The stacks a person opened, by the id of the card at the top.
  const [opened, setOpened] = useState<ReadonlySet<number>>(new Set());
  const [terminalOpen, setTerminalOpen] = useState(false);
  const openCard = useOpenCard();
  // The card a menu asked to block or to delete, until the question is answered.
  const [blocking, setBlocking] = useState<Card | null>(null);
  // The cards that are blocked with it, for the one reason, when several were chosen.
  const [blockingMore, setBlockingMore] = useState<Card[]>([]);
  const [deleting, setDeleting] = useState<Card | null>(null);

  const board = data.data?.boards.find((b) => String(b.repository.id) === id);
  const inbox = data.data?.inbox ?? NO_INBOX;
  // The filters and Display are read from the address, which also holds the open card: a new address with the same
  // filters is not a new filter, or opening a card would group the cards again and draw them all.
  const filters = useSame<Filters>({
    column: params.get("column") ?? "",
    agent: params.get("agent") ?? "",
    status: params.get("status") ?? "",
    label: params.get("label") ?? "",
    priority: params.get("priority") ?? "",
    due: params.get("due") ?? "",
  });
  const display = useSame(readDisplay(params, DISPLAY_CHOICES));
  const hidden = useMemo(() => new Set(display.hidden), [display]);
  // The cards are grouped once per read of the board and once per change of the filter, not once per render of the
  // dock or of a decision coming back.
  const groups = useMemo(
    () =>
      board && display.layout === "board" ? groupByColumn(board, inbox, filters, display.sort) : [],
    [board, inbox, filters, display.layout, display.sort],
  );
  const rows = useMemo(
    () =>
      board && display.layout === "list"
        ? listGroups(board, inbox, filters, display.group, display.sort)
        : [],
    [board, inbox, filters, display.layout, display.group, display.sort],
  );

  const choosable = useMemo(
    () =>
      display.layout === "list" ? rows.flatMap((g) => g.cards) : groups.flatMap((g) => g.cards),
    [display.layout, rows, groups],
  );
  const bulk = useBulk({
    board,
    inbox,
    order: useMemo(() => choosable.map((card) => card.id), [choosable]),
    onNotice: notify,
    onBlock: (cards) => {
      setBlocking(cards[0] ?? null);
      setBlockingMore(cards.slice(1));
    },
  });

  // The handlers the columns and their cards are given keep their identity from one render to the next: a card that
  // is memoised on them draws again only when its own data changes. They read the latest of what they need through
  // refs, or through a state setter's function form.
  const setParamsRef = useRef(setParams);
  setParamsRef.current = setParams;
  // The filters change the address in place; a card opens as the window over the page (`CardWindowHost.tsx`).
  const setQuery = useCallback((changes: Record<string, string>) => {
    setParamsRef.current(
      (current) => {
        const next = new URLSearchParams(current);
        for (const [key, value] of Object.entries(changes)) {
          if (value === "") next.delete(key);
          else next.set(key, value);
        }
        return next;
      },
      { replace: true },
    );
  }, []);
  // The columns' cards are memoised on this handler, so it keeps its identity while the address changes.
  const openCardRef = useRef(openCard);
  openCardRef.current = openCard;
  const onSelect = useCallback((card: Card) => openCardRef.current(card.id), []);
  const repositoryId = board?.repository.id;
  const openForm = useCallback(
    (parent: number | null) => {
      if (repositoryId === undefined) return;
      newIssue({
        repository: repositoryId,
        parent,
        onCreated: (card) => notify({ tone: "ok", text: `Created "${card.title}"` }),
      });
    },
    [newIssue, repositoryId, notify],
  );
  const menu = useMemo(
    () => ({
      onBlock: setBlocking,
      onUnblock: (card: Card) => decide({ kind: "unblock", card: card.id, number: card.number }),
      onStop: (card: Card) => {
        if (card.run) decide({ kind: "stop", run: card.run.id, number: card.number });
      },
      onDelete: setDeleting,
      onAddSubIssue: (card: Card) => openForm(card.number),
      onMoveParent: setMoving,
    }),
    [decide, openForm],
  );
  // The card the window shows, marked on the board behind it.
  const inspected = board?.cards.find((card) => String(card.id) === params.get("card"));
  // A stack is open when a person opened it, and while the window shows one of its sub-issues: closing it there would hide
  // the card that is being read.
  const inspectedTopId = board && inspected ? rootOf(board, inspected).id : undefined;
  const inspectedTopRef = useRef(inspectedTopId);
  inspectedTopRef.current = inspectedTopId;
  const onToggleStack = useCallback(
    (card: Card) =>
      setOpened((now) => {
        const next = new Set(now);
        if (now.has(card.id) || inspectedTopRef.current === card.id) next.delete(card.id);
        else next.add(card.id);
        return next;
      }),
    [],
  );
  const onAdding = useCallback(
    (column: string, open: boolean) => setAdding(open ? column : null),
    [],
  );
  const onCreated = useCallback(
    (card: Card, where: string) =>
      notify({
        tone: "ok",
        text:
          where === "Backlog"
            ? `Created "${card.title}"`
            : `Created "${card.title}" in the Backlog`,
      }),
    [notify],
  );
  const navigateRef = useRef(navigate);
  navigateRef.current = navigate;
  const onRepository = useCallback(
    (value: string) => navigateRef.current(boardPath(Number(value))),
    [],
  );
  const repositories = useSame(
    data.data?.boards.map((b): [string, string] => [String(b.repository.id), b.repository.name]) ??
      [],
  );
  const leading = useMemo(
    () => (
      <Select
        aria-label="Repository"
        value={repositoryId === undefined ? "" : String(repositoryId)}
        options={repositories}
        onChange={onRepository}
        className="w-auto min-w-36"
      />
    ),
    [repositoryId, repositories, onRepository],
  );
  const fields = useSame(board ? filterFields(board, filters) : []);
  const filterValues = useMemo<Record<string, string>>(() => ({ ...filters }), [filters]);
  const onFilter = useCallback(
    (key: string, value: string) => setQuery({ [key]: value }),
    [setQuery],
  );
  const onClear = useCallback(() => setQuery({ ...NO_FILTERS }), [setQuery]);
  const displayProps = useMemo(
    () => ({
      choices: DISPLAY_CHOICES,
      value: display,
      onChange: (next: Display) => setQuery(displayQuery(next)),
    }),
    [display, setQuery],
  );
  const saveView = useCallback(() => {
    if (repositoryId === undefined) return;
    // The view is kept on the server, for this person; Views shares it.
    void (async () => {
      try {
        const count = (await listViews()).length;
        const fields = viewOfBoard(
          `View ${count + 1}`,
          repositoryId,
          filters,
          display.layout,
          display.sort,
        );
        const made = await addView(fields);
        void queryClient.invalidateQueries({ queryKey: keys.views });
        notify({
          tone: "ok",
          text: `Saved this view as "${made.name}"`,
        });
      } catch (error) {
        notify({ tone: "error", text: sayError(error, "The view could not be saved.") });
      }
    })();
  }, [repositoryId, filters, display.layout, display.sort, queryClient, notify]);
  // The head of the page, drawn again only when its title, the layout or the handlers change: a read of the snapshot
  // leaves it as it is.
  const repositoryName = board?.repository.name;
  const header = useMemo(
    () => (
      <PageHeader
        title={repositoryName ?? ""}
        actions={
          <>
            <LayoutSwitch
              layout={display.layout === "list" ? "list" : "board"}
              onChange={(layout) => setQuery(displayQuery({ ...display, layout }))}
            />
            <Button variant="secondary" onClick={() => setTerminalOpen(true)}>
              <TerminalWindowIcon className="size-4" aria-hidden="true" />
              Terminal
            </Button>
            <Button onClick={() => openForm(null)}>
              <PlusIcon className="size-4" weight="bold" aria-hidden="true" />
              New issue
            </Button>
          </>
        }
      />
    ),
    [repositoryName, display, setQuery, openForm],
  );
  const clearChoice = bulk.choice.clear;
  const closeBlock = useCallback(() => {
    setBlocking(null);
    setBlockingMore([]);
  }, []);
  const doneBlock = useCallback(
    (text: string, undo: () => void) => {
      notify({ tone: "ok", text, undo });
      clearChoice();
    },
    [notify, clearChoice],
  );
  const closeDelete = useCallback(() => setDeleting(null), []);
  const cardParam = params.get("card");
  const doneDelete = useCallback(
    (card: Card, text: string) => {
      // A card's id can be given again once it is gone, so a window that was open on it is closed for good.
      if (cardParam === String(card.id)) setQuery({ card: "" });
      notify({ tone: "ok", text });
    },
    [cardParam, setQuery, notify],
  );
  const closeParent = useCallback(() => setMoving(null), []);
  const doneParent = useCallback((text: string) => notify({ tone: "ok", text }), [notify]);
  const { has, toggle } = bulk.choice;
  const { anyChosen, onContextMenu } = bulk;
  const columnBulk = useMemo(
    () => ({ has, anyChosen, toggle, onContextMenu }),
    [has, anyChosen, toggle, onContextMenu],
  );

  if (data.isPending)
    return (
      <>
        <PageHeader title="Board" />
        <SkeletonBoard message="Reading the board" />
      </>
    );
  // A read that failed is not a board that does not exist, and the page that says so would send the person off to
  // choose a project that is already chosen.
  if (data.isError) {
    return (
      <>
        <PageHeader title="Board" />
        <ReadFailed
          error={data.error}
          fallback="The board could not be read. Try again."
          busy={data.isFetching}
          onRetry={() => void data.refetch()}
        />
      </>
    );
  }
  if (!data.data || !board) {
    return (
      <>
        <PageHeader title="Board" />
        <Empty
          title="There is no such board"
          body="Pick a project in the sidebar to open its board."
        />
      </>
    );
  }
  const openView = views.data?.find((v) => String(v.id) === params.get("view"));
  const asked = openView && boardFiltersOf(openView);
  const viewChanged =
    asked != null &&
    (asked.repository !== board.repository.id ||
      JSON.stringify(asked.filters) !== JSON.stringify(filters));
  function onDragEnd(result: DropResult) {
    // A decision is on its way: the buttons are disabled for the same moment, and a drag is another way to ask for one.
    if (pending) return;
    if (!result.column) return;
    const card = board?.cards.find((c) => String(c.id) === result.cardId);
    if (!card) return;
    const to = result.column;
    const drop = dropRule(card, to, inboxOf(inbox, card));
    const number = card.number;
    let decision: Decision | null = null;
    if (drop.kind === "approve") decision = { kind: "approve", card: card.id, number };
    if (drop.kind === "retry") decision = { kind: "retry", card: card.id, number };
    if (drop.kind === "merge") decision = { kind: "merge", change: drop.change, number };
    if (decision) {
      openCard(card.id);
      decide(decision);
    } else if (drop.kind === "reject") {
      // Sending a change back needs a reason: the window asks for it, and only for this card.
      openCard(card.id, { askToReject: true });
    } else if (drop.kind === "refused") {
      notify({ tone: "error", text: `Cannot move #${number} to ${to}: ${drop.reason}` });
    }
  }

  /** The card under the pointer while it is dragged. */
  function renderOverlay(cardId: string) {
    const card = board?.cards.find((c) => String(c.id) === cardId);
    if (!board || !card) return null;
    const status = cardStatus(card, inboxOf(inbox, card), board);
    return (
      <KanbanCardFace name={`${cardKey(card)} ${card.title}`} className="cursor-grabbing">
        <CardContent card={card} status={status} />
      </KanbanCardFace>
    );
  }

  const boardLayout = display.layout !== "list";
  return (
    // The board fills the panel: the page does not scroll, the lanes row scrolls sideways and each column's cards scroll
    // up and down. The list layout is a page like the others and scrolls as one sheet.
    <div className={cn("flex min-w-0 flex-col", boardLayout && "h-full min-h-0")}>
      {header}
      <div className={cn("flex min-w-0 flex-col", boardLayout && "min-h-0 flex-1")}>
        <div className={cn("flex min-w-0 flex-1 flex-col", boardLayout && "min-h-0")}>
          {openView && (
            <p
              className="m-0 mb-2 flex flex-wrap items-center gap-x-3 text-sm text-secondary"
              data-testid="board-view"
            >
              <BookmarkSimpleIcon className="size-4" aria-hidden="true" />
              <span>
                View <strong className="font-semibold text-text">{openView.name}</strong>
                {viewChanged ? " (filters changed)" : ""}
              </span>
              <Link to={`/views/${openView.id}?manage`}>Manage view</Link>
              <button
                type="button"
                className="cursor-pointer border-0 bg-transparent p-0 text-sm text-secondary underline"
                onClick={() => setQuery({ view: "" })}
              >
                Close view
              </button>
            </p>
          )}
          <FilterBar
            label="Filter the cards"
            leading={leading}
            fields={fields}
            values={filterValues}
            onFilter={onFilter}
            onClear={onClear}
            display={displayProps}
            onSaveView={saveView}
          />
          <LeftoverNotice repository={board.repository.id} />
          {display.layout === "list" ? (
            <ListLayout
              board={board}
              inbox={inbox}
              groups={rows}
              hidden={hidden}
              inspectedId={inspected?.id}
              onSelect={onSelect}
              select={bulk.boxFor}
              onContextMenu={bulk.onContextMenu}
            />
          ) : (
            <div
              data-testid="board-lanes"
              className="-mx-6 -mb-6 min-h-0 flex-1 overflow-x-auto overflow-y-hidden px-6"
            >
              <KanbanProvider onDragEnd={onDragEnd} renderOverlay={renderOverlay}>
                {groups.map(({ column, cards, stacks }) => (
                  <BoardColumn
                    key={column}
                    board={board}
                    inbox={inbox}
                    name={column}
                    cards={cards}
                    stacks={stacks}
                    opened={opened}
                    inspectedTopId={inspectedTopId}
                    onToggleStack={onToggleStack}
                    busy={pending}
                    inspectedId={inspected?.id}
                    onSelect={onSelect}
                    menu={menu}
                    bulk={columnBulk}
                    adding={adding === column}
                    onAdding={onAdding}
                    onCreated={onCreated}
                  />
                ))}
              </KanbanProvider>
            </div>
          )}
        </div>
      </div>
      <RepositoryTerminal
        repository={board.repository.id}
        open={terminalOpen}
        onOpenChange={setTerminalOpen}
      />
      <BlockDialog card={blocking} more={blockingMore} onClose={closeBlock} onDone={doneBlock} />
      {bulk.surface}
      <DeleteDialog
        card={deleting}
        repository={board.repository}
        onClose={closeDelete}
        onDone={doneDelete}
      />
      <ParentDialog board={board} card={moving} onClose={closeParent} onDone={doneParent} />
    </div>
  );
}
