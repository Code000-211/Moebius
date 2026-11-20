/**
 * The board of a repository: its columns of the default workflow with their cards (dragged only where a person may
 * move them), the filters, the inspector dock and a shell in the repository's clone, in a sheet over the board. The
 * head is two lines: the title with the layout switch, Terminal and New issue (the form of a new issue, which the shell
 * holds and `C` opens too), and the filter bar under it. A column ends in "+ Add", which makes a card in place
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
import { useMemo, useState } from "react";
import { Link, useNavigate, useParams, useSearchParams } from "react-router";
import { snapshot } from "../../api/calls/board";
import { addView, listViews } from "../../api/calls/views";
import { sayError } from "../../api/errorText";
import { keys } from "../../api/keys";
import type { Card } from "../../api/types/board";
import { Button } from "../../ui/components/Button";
import { type DropResult, KanbanCardFace, KanbanProvider } from "../../ui/components/KanbanBoard";
import { FilterBar } from "../../ui/filters/FilterBar";
import { displayQuery, readDisplay } from "../../ui/filters/model";
import { cn } from "../../ui/lib/cn";
import { Empty } from "../../ui/page/Empty";
import { Select } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";
import { PageHeader } from "../../ui/page/PageHeader";
import { usePropertiesShown } from "../../ui/page/Properties";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { ScrollFades } from "../../ui/page/ScrollFades";
import { useSheetSlot } from "../../ui/page/SheetSlot";
import { SkeletonBoard } from "../../ui/page/Skeleton";
import { boardPath } from "../pages";
import { boardFiltersOf, viewOfBoard } from "../views/model";
import { CardContent } from "./BoardCardContent";
import { BoardColumn } from "./BoardColumn";
import { LayoutSwitch, LeftoverNotice } from "./BoardHead";
import { useBulk } from "./bulk";
import { BlockDialog, DeleteDialog, ParentDialog } from "./CardDialogs";
import { CardFocus } from "./CardFocus";
import { cardKey } from "./cardKey";
import { Dock } from "./Dock";
import { type Decision, useDecisions } from "./decisions";
import { ListLayout } from "./ListLayout";
import {
  cardStatus,
  dropRule,
  type Filters,
  groupByColumn,
  inboxOf,
  inspectedCard,
  mergeStoppedOf,
  NO_FILTERS,
  rootOf,
} from "./model";
import { useNewIssue } from "./NewIssueHost";
import { RepositoryTerminal } from "./RepositoryTerminal";
import { DISPLAY_CHOICES, filterFields, listGroups } from "./view";

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
  const { decide, pending, notice, setNotice } = useDecisions();
  const queryClient = useQueryClient();
  const newIssue = useNewIssue();
  // The column whose line for a new card is open.
  const [adding, setAdding] = useState<string | null>(null);
  // The card a menu asked to move below another, until the question is answered.
  const [moving, setMoving] = useState<Card | null>(null);
  // The stacks a person opened, by the id of the card at the top.
  const [opened, setOpened] = useState<ReadonlySet<number>>(new Set());
  const [terminalOpen, setTerminalOpen] = useState(false);
  const propertiesShown = usePropertiesShown();
  const sheetShown = useSheetSlot().active !== null;
  // The card a send-back needs a reason for, not a count: a count stays raised once it is raised, so every card
  // opened afterwards found the reason form waiting for it.
  const [rejectFor, setRejectFor] = useState<number | null>(null);
  // The card a menu asked to block or to delete, until the question is answered.
  const [blocking, setBlocking] = useState<Card | null>(null);
  // The cards that are blocked with it, for the one reason, when several were chosen.
  const [blockingMore, setBlockingMore] = useState<Card[]>([]);
  const [deleting, setDeleting] = useState<Card | null>(null);

  const board = data.data?.boards.find((b) => String(b.repository.id) === id);
  const inbox = data.data?.inbox ?? [];
  const filters = useMemo<Filters>(
    () => ({
      column: params.get("column") ?? "",
      agent: params.get("agent") ?? "",
      status: params.get("status") ?? "",
      label: params.get("label") ?? "",
      priority: params.get("priority") ?? "",
      due: params.get("due") ?? "",
    }),
    [params],
  );
  const display = useMemo(() => readDisplay(params, DISPLAY_CHOICES), [params]);
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
    onNotice: setNotice,
    onBlock: (cards) => {
      setBlocking(cards[0] ?? null);
      setBlockingMore(cards.slice(1));
    },
  });

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
  const setQuery = (changes: Record<string, string>) =>
    setParams(
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
  const saveView = () => {
    // The view is kept on the server, for this person; Views shares it.
    void (async () => {
      try {
        const count = (await listViews()).length;
        const fields = viewOfBoard(
          `View ${count + 1}`,
          board.repository.id,
          filters,
          display.layout,
          display.sort,
        );
        const made = await addView(fields);
        void queryClient.invalidateQueries({ queryKey: keys.views });
        setNotice({
          tone: "ok",
          text: `Saved this view as "${made.name}"`,
        });
      } catch (error) {
        setNotice({ tone: "error", text: sayError(error, "The view could not be saved.") });
      }
    })();
  };
  const inspected = inspectedCard(board, inbox, params.get("card"));
  const openView = views.data?.find((v) => String(v.id) === params.get("view"));
  const asked = openView && boardFiltersOf(openView);
  const viewChanged =
    asked != null &&
    (asked.repository !== board.repository.id ||
      JSON.stringify(asked.filters) !== JSON.stringify(filters));
  const openForm = (parent: number | null) =>
    newIssue({
      repository: board.repository.id,
      parent,
      onCreated: (card) => setNotice({ tone: "ok", text: `Created "${card.title}"` }),
    });
  const menu = {
    onBlock: setBlocking,
    onUnblock: (card: Card) => decide({ kind: "unblock", card: card.id, number: card.number }),
    onStop: (card: Card) => {
      if (card.run) decide({ kind: "stop", run: card.run.id, number: card.number });
    },
    onDelete: setDeleting,
    onAddSubIssue: (card: Card) => openForm(card.number),
    onMoveParent: setMoving,
  };
  // A stack is open when a person opened it, and while the dock shows one of its sub-issues: closing it there would hide
  // the card that is being read.
  const inspectedTop = inspected ? rootOf(board, inspected) : undefined;
  const stackOpen = (card: Card) => opened.has(card.id) || inspectedTop?.id === card.id;
  const toggleStack = (card: Card) =>
    setOpened((now) => {
      const next = new Set(now);
      if (stackOpen(card)) next.delete(card.id);
      else next.add(card.id);
      return next;
    });
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
      setQuery({ card: String(card.id) });
      decide(decision);
    } else if (drop.kind === "reject") {
      // Sending a change back needs a reason: the dock asks for it, and only for this card.
      setQuery({ card: String(card.id) });
      setRejectFor(card.id);
    } else if (drop.kind === "refused") {
      setNotice({ tone: "error", text: `Cannot move #${number} to ${to}: ${drop.reason}` });
    }
  }

  /** The card under the pointer while it is dragged. */
  function renderOverlay(cardId: string) {
    const card = board?.cards.find((c) => String(c.id) === cardId);
    if (!board || !card) return null;
    const status = cardStatus(card, inboxOf(inbox, card), board);
    return (
      <KanbanCardFace
        name={`${cardKey(card)} ${card.title}`}
        isLive={status === "live"}
        className="cursor-grabbing"
      >
        <CardContent board={board} inbox={inbox} card={card} cards={board.cards} hidden={hidden} />
      </KanbanCardFace>
    );
  }

  return (
    <>
      {/* A card open takes a column of the page for its drawer, and the head of the page is part of the page: its
          actions stand left of the drawer rather than under it. */}
      <div className={inspected ? "lg:pr-96" : undefined}>
        <PageHeader
          title={board.repository.name}
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
      </div>
      {/* The dock stands beside the whole of the board, not beside its lanes: it belongs to the page, and starting it
          under the filters spent the room it asks for and left the filters standing alone over nothing. */}
      <div className="flex min-w-0 flex-col gap-4 lg:flex-row lg:items-start">
        <div className="flex min-w-0 flex-1 flex-col">
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
            leading={
              <Select
                aria-label="Repository"
                value={String(board.repository.id)}
                options={data.data.boards.map((b): [string, string] => [
                  String(b.repository.id),
                  b.repository.name,
                ])}
                onChange={(value) => navigate(boardPath(Number(value)))}
                className="w-auto min-w-36"
              />
            }
            fields={filterFields(board, filters)}
            values={{ ...filters }}
            onFilter={(key, value) => setQuery({ [key]: value })}
            onClear={() => setQuery({ ...NO_FILTERS })}
            display={{
              choices: DISPLAY_CHOICES,
              value: display,
              onChange: (next) => setQuery(displayQuery(next)),
            }}
            onSaveView={saveView}
          />
          <LeftoverNotice repository={board.repository.id} />
          {notice && (
            <Notice
              tone={notice.tone}
              className="mb-4"
              action={
                <Button variant="ghost" size="sm" onClick={() => setNotice(null)}>
                  Dismiss
                </Button>
              }
            >
              {notice.text}
            </Notice>
          )}
          {display.layout === "list" ? (
            <ListLayout
              board={board}
              inbox={inbox}
              groups={rows}
              hidden={hidden}
              inspectedId={inspected?.id}
              onSelect={(card) => setQuery({ card: String(card.id) })}
              select={bulk.boxFor}
              onContextMenu={bulk.onContextMenu}
            />
          ) : (
            <ScrollFades className="flex-1 pb-3">
              <KanbanProvider onDragEnd={onDragEnd} renderOverlay={renderOverlay}>
                {groups.map(({ column, cards, total, stacks }) => (
                  <BoardColumn
                    key={column}
                    board={board}
                    inbox={inbox}
                    name={column}
                    cards={cards}
                    total={total}
                    stacks={stacks}
                    stackOpen={stackOpen}
                    onToggleStack={toggleStack}
                    busy={pending}
                    inspectedId={inspected?.id}
                    onSelect={(card) => setQuery({ card: String(card.id) })}
                    menu={menu}
                    bulk={bulk}
                    hidden={hidden}
                    adding={adding === column}
                    onAdding={(open) => setAdding(open ? column : null)}
                    onCreated={(card, where) =>
                      setNotice({
                        tone: "ok",
                        text:
                          where === "Backlog"
                            ? `Created "${card.title}"`
                            : `Created "${card.title}" in the Backlog`,
                      })
                    }
                  />
                ))}
              </KanbanProvider>
            </ScrollFades>
          )}
        </div>
        {inspected && (
          // The column holds the drawer's width so the board keeps the room it has; the drawer itself runs the whole
          // height of the page's surface and is flush to its top, right and bottom edges, inside its rounded corners —
          // it is the page's own card inspector rather than a sheet laid over it — and scrolls its sections rather
          // than the page scrolling them. It lies above the slim right column it covers, under the sheets over it;
          // while the column stands unfolded with the card's properties, the drawer stands beside it instead.
          <div className="w-full shrink-0 lg:w-96">
            <div
              className={cn(
                "lg:fixed lg:inset-y-1.5 lg:z-10 lg:w-96 lg:overflow-hidden",
                // A sheet (the Moebius agent) lies over the column and a strip of the page, so the drawer stands
                // beside it: the board gives way, and the card's text is not under the sheet.
                sheetShown
                  ? "lg:right-[calc(var(--sheet-width)+0.375rem)] lg:rounded-r-sheet"
                  : propertiesShown
                    ? "lg:right-[calc(var(--column-width)+0.375rem)]"
                    : "lg:right-1.5 lg:rounded-r-sheet",
              )}
            >
              <Dock
                key={inspected.id}
                board={board}
                card={inspected}
                item={inboxOf(inbox, inspected)}
                stopped={mergeStoppedOf(inbox, inspected)}
                boards={data.data?.boards}
                busy={pending}
                askFor={rejectFor}
                onAskAnswered={() => setRejectFor(null)}
                onDecide={decide}
                menu={menu}
                onOpenCard={(card) => setQuery({ card: String(card.id) })}
                onClose={() => {
                  setQuery({ card: "none" });
                  setRejectFor(null);
                }}
              />
            </div>
          </div>
        )}
      </div>
      {inspected && (
        <CardFocus
          card={inspected}
          item={inboxOf(inbox, inspected)}
          decide={decide}
          onNotice={setNotice}
          onBlock={setBlocking}
          onAddSubIssue={(card) => openForm(card.number)}
          onReject={(card) => setRejectFor(card.id)}
        />
      )}
      <RepositoryTerminal
        repository={board.repository.id}
        open={terminalOpen}
        onOpenChange={setTerminalOpen}
      />
      <BlockDialog
        card={blocking}
        more={blockingMore}
        onClose={() => {
          setBlocking(null);
          setBlockingMore([]);
        }}
        onDone={(text) => {
          setNotice({ tone: "ok", text });
          bulk.choice.clear();
        }}
      />
      {bulk.surface}
      <DeleteDialog
        card={deleting}
        repository={board.repository}
        onClose={() => setDeleting(null)}
        onDone={(card, text) => {
          // A card's id can be given again once it is gone, so a dock that was open on it is closed for good.
          if (params.get("card") === String(card.id)) setQuery({ card: "none" });
          setNotice({ tone: "ok", text });
        }}
      />
      <ParentDialog
        board={board}
        card={moving}
        onClose={() => setMoving(null)}
        onDone={(text) => setNotice({ tone: "ok", text })}
      />
    </>
  );
}
