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
 * (`ui/filters`); `view.ts` says what the board offers it and `ListLayout.tsx` draws the list layout.
 */
import {
  CaretDownIcon,
  CaretRightIcon,
  KanbanIcon,
  ListBulletsIcon,
  PlusIcon,
  TerminalWindowIcon,
} from "@phosphor-icons/react";
import { useQuery } from "@tanstack/react-query";
import { type SyntheticEvent, useMemo, useState } from "react";
import { Link, useNavigate, useParams, useSearchParams } from "react-router";
import { snapshot } from "../../api/calls/board";
import { readSettings } from "../../api/calls/projects";
import { keys } from "../../api/keys";
import type { Board, Card, InboxItem } from "../../api/types/board";
import { Button } from "../../ui/components/Button";
import {
  type DropResult,
  KanbanBoard,
  KanbanCard,
  KanbanCardFace,
  KanbanCards,
  KanbanHeader,
  KanbanProvider,
} from "../../ui/components/KanbanBoard";
import { KanbanCardContent } from "../../ui/components/KanbanCardContent";
import { FilterBar } from "../../ui/filters/FilterBar";
import { displayQuery, parseViews, readDisplay, viewQuery, withView } from "../../ui/filters/model";
import { cn } from "../../ui/lib/cn";
import { Empty } from "../../ui/page/Empty";
import { Select } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";
import { PageHeader } from "../../ui/page/PageHeader";
import { usePropertiesShown } from "../../ui/page/Properties";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { ScrollFades } from "../../ui/page/ScrollFades";
import { SkeletonBoard } from "../../ui/page/Skeleton";
import { boardPath } from "../pages";
import { readText, writeText } from "../storage";
import { BlockDialog, DeleteDialog, ParentDialog } from "./CardDialogs";
import { CardFocus } from "./CardFocus";
import { CardMenu } from "./CardMenu";
import { ColumnAdd, ColumnAddButton } from "./ColumnAdd";
import { ColumnMark } from "./ColumnMark";
import { Dock } from "./Dock";
import { type Decision, useDecisions } from "./decisions";
import { ListLayout } from "./ListLayout";
import {
  cardBadges,
  cardDetail,
  cardStatus,
  changeOf,
  dropRule,
  type Filters,
  groupByColumn,
  inboxOf,
  inspectedCard,
  issueUrl,
  NO_FILTERS,
  nextCard,
  pullRequestOf,
  rootOf,
  type Stack,
  stackWords,
} from "./model";
import { useNewIssue } from "./NewIssueHost";
import { RepositoryTerminal } from "./RepositoryTerminal";
import { DISPLAY_CHOICES, filterFields, listGroups, VIEW_KEYS } from "./view";

/**
 * What a column holds when it is empty: one sentence for how a card comes to it. There is no title, because the
 * column's header is already the name of the place, and a heading repeating it is a second label on the same thing.
 */
const COLUMN_EMPTY: Record<string, string> = {
  Backlog: "Issues you create wait here until they are approved.",
  "Not started": "Approved issues wait here until an agent takes one.",
  "In progress": "A card arrives here when an agent starts a run on it.",
  "In review": "A card arrives here when an agent has a change to review.",
  "Ready to merge": "A card arrives here when its change passes its checks.",
  Done: "A card arrives here once its change is merged.",
};

/** What the menus of the cards of a board do. */
type CardMenuActions = {
  onBlock: (card: Card) => void;
  onUnblock: (card: Card) => void;
  onStop: (card: Card) => void;
  onDelete: (card: Card) => void;
  onAddSubIssue: (card: Card) => void;
  onMoveParent: (card: Card) => void;
};

const ANY_COLUMN = "A card arrives when the column's script moves one in.";

/**
 * Said on the board of a repository whose default branch still has a `.moebius/` folder: nothing in it is read any
 * more, so its rules, checks and scripts do not apply until they are moved into the project's settings.
 */
function LeftoverNotice({ repository }: { repository: number }) {
  const settings = useQuery({
    queryKey: keys.projectSettings(repository),
    queryFn: () => readSettings(repository),
  });
  const leftover = settings.data?.leftover ?? [];
  if (leftover.length === 0) return null;
  return (
    <Notice
      tone="error"
      className="mb-4"
      action={
        <Link className="underline" to={`/projects/${repository}`}>
          Project settings
        </Link>
      }
    >
      <span className="font-semibold">This repository still has a .moebius folder. </span>
      Nothing in it is read any more, so its rules, checks and scripts do not apply (
      {leftover.join(", ")}). Move them into the project's settings.
    </Notice>
  );
}

/** The two layouts as two buttons in the head of the page; Display holds the same choice among the rest. */
function LayoutSwitch({
  layout,
  onChange,
}: {
  layout: "board" | "list";
  onChange: (layout: "board" | "list") => void;
}) {
  const choices = [
    ["board", "Board", KanbanIcon],
    ["list", "List", ListBulletsIcon],
  ] as const;
  return (
    <fieldset aria-label="Layout" className="m-0 flex min-w-0 items-center gap-0.5 border-0 p-0">
      {choices.map(([value, label, Icon]) => (
        <Button
          key={value}
          variant={layout === value ? "secondary" : "ghost"}
          size="sm"
          aria-pressed={layout === value}
          aria-label={`${label} layout`}
          title={`${label} layout`}
          onClick={() => onChange(value)}
        >
          <Icon className="size-4" aria-hidden="true" />
        </Button>
      ))}
    </fieldset>
  );
}

export function BoardPage() {
  const { id } = useParams();
  const navigate = useNavigate();
  const [params, setParams] = useSearchParams();
  const data = useQuery({ queryKey: keys.snapshot, queryFn: () => snapshot() });
  const { decide, pending, notice, setNotice } = useDecisions();
  const newIssue = useNewIssue();
  // The column whose line for a new card is open.
  const [adding, setAdding] = useState<string | null>(null);
  // The card a menu asked to move below another, until the question is answered.
  const [moving, setMoving] = useState<Card | null>(null);
  // The stacks a person opened, by the id of the card at the top.
  const [opened, setOpened] = useState<ReadonlySet<number>>(new Set());
  const [terminalOpen, setTerminalOpen] = useState(false);
  const propertiesShown = usePropertiesShown();
  // The card a send-back needs a reason for, not a count: a count stays raised once it is raised, so every card
  // opened afterwards found the reason form waiting for it.
  const [rejectFor, setRejectFor] = useState<number | null>(null);
  // The card a menu asked to block or to delete, until the question is answered.
  const [blocking, setBlocking] = useState<Card | null>(null);
  const [deleting, setDeleting] = useState<Card | null>(null);

  const board = data.data?.boards.find((b) => String(b.repository.id) === id);
  const inbox = data.data?.inbox ?? [];
  const filters = useMemo<Filters>(
    () => ({
      column: params.get("column") ?? "",
      agent: params.get("agent") ?? "",
      status: params.get("status") ?? "",
      label: params.get("label") ?? "",
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
    const key = `moebius.views.${board.repository.id}`;
    const views = withView(parseViews(readText(key)), viewQuery(params, VIEW_KEYS));
    writeText(key, JSON.stringify(views));
    setNotice({ tone: "ok", text: `Saved this view as "${views[views.length - 1]?.name}"` });
  };
  const inspected = inspectedCard(board, inbox, params.get("card"));
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
        name={`#${card.number} ${card.title}`}
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
                layout={display.layout}
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
                propertiesShown
                  ? "lg:right-[calc(var(--column-width)+0.375rem)]"
                  : "lg:right-1.5 lg:rounded-r-sheet",
              )}
            >
              <Dock
                key={inspected.id}
                board={board}
                card={inspected}
                item={inboxOf(inbox, inspected)}
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
        onClose={() => setBlocking(null)}
        onDone={(text) => setNotice({ tone: "ok", text })}
      />
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

function BoardColumn({
  board,
  inbox,
  name,
  cards,
  total,
  stacks,
  stackOpen,
  onToggleStack,
  busy,
  inspectedId,
  onSelect,
  menu,
  hidden,
  adding,
  onAdding,
  onCreated,
}: {
  board: Board;
  inbox: InboxItem[];
  name: string;
  cards: Card[];
  total: number;
  /** The stack of each card of the board that has sub-issues, by card id. */
  stacks: Map<number, Stack>;
  stackOpen: (card: Card) => boolean;
  onToggleStack: (card: Card) => void;
  /** While a decision is on its way, when no card can be moved. */
  busy: boolean;
  inspectedId: number | undefined;
  onSelect: (card: Card) => void;
  menu: CardMenuActions;
  /** The fields of a card Display switched off. */
  hidden: ReadonlySet<string>;
  /** The line for a new card is open in this column. */
  adding: boolean;
  onAdding: (open: boolean) => void;
  onCreated: (card: Card, column: string) => void;
}) {
  return (
    <KanbanBoard name={name}>
      <KanbanHeader
        name={name}
        count={cards.length}
        total={total}
        mark={<ColumnMark name={name} />}
        actions={<ColumnAddButton column={name} onClick={() => onAdding(true)} />}
      />
      <KanbanCards className="min-h-9">
        {cards.map((card) => {
          const stack = stacks.get(card.id);
          const open = stack !== undefined && stackOpen(card);
          return (
            <div
              key={card.id}
              data-testid={stack ? `stack-${card.number}` : undefined}
              className="flex min-w-0 flex-col gap-3"
            >
              <BoardCard
                board={board}
                inbox={inbox}
                card={card}
                column={name}
                cards={cards}
                busy={busy}
                inspectedId={inspectedId}
                onSelect={onSelect}
                menu={menu}
                stack={stack}
                open={open}
                onToggleStack={() => onToggleStack(card)}
                hidden={hidden}
              />
              {open && stack.shown.length > 0 && (
                // The sub-issues lie under their parent, in a stack of their own: each is a card that moves, blocks and
                // deletes like any other, and says which column it is in, since it is not in this one.
                <div className="ml-3 flex min-w-0 flex-col gap-3 border-l-2 border-hairline-strong pl-3">
                  {stack.shown.map((child) => (
                    <BoardCard
                      key={child.id}
                      board={board}
                      inbox={inbox}
                      card={child}
                      column={child.column}
                      cards={board.cards}
                      busy={busy}
                      inspectedId={inspectedId}
                      onSelect={onSelect}
                      menu={menu}
                      inStack
                      hidden={hidden}
                    />
                  ))}
                </div>
              )}
            </div>
          );
        })}
        {/* The column is already the surface under these words; a card of its own inside it is a second one, and one
            that reads as a card which has lost its words. */}
        {cards.length === 0 && <Empty plain centered body={COLUMN_EMPTY[name] ?? ANY_COLUMN} />}
      </KanbanCards>
      <ColumnAdd
        repository={board.repository.id}
        column={name}
        open={adding}
        onOpen={() => onAdding(true)}
        onClose={() => onAdding(false)}
        onCreated={onCreated}
      />
    </KanbanBoard>
  );
}

/** One card of a column, or of the stack under its parent. */
function BoardCard({
  board,
  inbox,
  card,
  column,
  cards,
  busy,
  inspectedId,
  onSelect,
  menu,
  stack,
  open = false,
  onToggleStack,
  inStack = false,
  hidden,
}: {
  board: Board;
  inbox: InboxItem[];
  card: Card;
  hidden: ReadonlySet<string>;
  /** The column the card is in: the one it is drawn in, or its own when it lies in the stack of its parent. */
  column: string;
  cards: Card[];
  busy: boolean;
  inspectedId: number | undefined;
  onSelect: (card: Card) => void;
  menu: CardMenuActions;
  stack?: Stack;
  open?: boolean;
  onToggleStack?: () => void;
  inStack?: boolean;
}) {
  const status = cardStatus(card, inboxOf(inbox, card), board);
  return (
    <KanbanCard
      id={String(card.id)}
      name={`#${card.number} ${card.title}`}
      testId={`card-${card.number}`}
      column={column}
      dragDisabled={busy}
      isSelected={inspectedId === card.id}
      isLive={status === "live"}
      onOpen={() => onSelect(card)}
    >
      <CardContent
        board={board}
        inbox={inbox}
        card={card}
        cards={cards}
        onOpen={() => onSelect(card)}
        menu={menu}
        stack={stack}
        open={open}
        onToggleStack={onToggleStack}
        inStack={inStack}
        hidden={hidden}
      />
    </KanbanCard>
  );
}

/** What a card shows, on the board and on the drag overlay. */
function CardContent({
  board,
  inbox,
  card,
  cards,
  onOpen,
  menu,
  stack,
  open = false,
  onToggleStack,
  inStack = false,
  hidden,
}: {
  board: Board;
  inbox: InboxItem[];
  card: Card;
  cards: Card[];
  /** The fields Display switched off. */
  hidden: ReadonlySet<string>;
  /** Opens the card; left out on the drag overlay, where the card is only a picture of itself. */
  onOpen?: () => void;
  /** What the card's menu does; left out on the drag overlay, with the menu itself. */
  menu?: CardMenuActions;
  /** The sub-issues of the card, when it has any: how many are done, and the control that opens them. */
  stack?: Stack;
  open?: boolean;
  onToggleStack?: () => void;
  /** The card lies in the stack of its parent, in a column that is not the one it is drawn in: it says which. */
  inStack?: boolean;
}) {
  const status = cardStatus(card, inboxOf(inbox, card), board);
  const change = changeOf(board, card);
  const coding = card.column === "In progress" || card.column === "In review";
  const next = card.column === "Not started" ? nextCard(cards, board.cards) : undefined;
  const detail = cardDetail(card, board.round_limit);
  return (
    <KanbanCardContent
      displayId={`#${card.number}`}
      issueUrl={issueUrl(board.repository, card)}
      title={card.title}
      preview={hidden.has("description") ? null : card.body}
      detail={
        hidden.has("detail")
          ? null
          : inStack
            ? [card.column, detail].filter(Boolean).join(" · ")
            : detail
      }
      labels={hidden.has("labels") ? [] : card.labels}
      claimedBy={hidden.has("claim") ? null : card.claimed_by}
      stack={
        stack && onToggleStack ? (
          <StackToggle card={card} stack={stack} open={open} onToggle={onToggleStack} />
        ) : stack ? (
          <p className="m-0 text-sm text-secondary">{stackWords(stack)}</p>
        ) : undefined
      }
      badges={cardBadges(card, status, next === card.id)}
      pullRequest={hidden.has("pullRequest") ? null : pullRequestOf(board.repository, change)}
      rounds={
        coding && !hidden.has("rounds") ? { done: card.rounds, limit: board.round_limit } : null
      }
      muted={card.column === "Done" || card.closed}
      onOpen={onOpen}
      menu={menu && <CardMenu card={card} {...menu} />}
    />
  );
}

/** How many sub-issues of a card are done, and the control that opens the stack under it or shuts it. */
function StackToggle({
  card,
  stack,
  open,
  onToggle,
}: {
  card: Card;
  stack: Stack;
  open: boolean;
  onToggle: () => void;
}) {
  const Caret = open ? CaretDownIcon : CaretRightIcon;
  // The card is dragged by its whole body and opened by a click on it; this control is neither.
  const keep = (event: SyntheticEvent) => event.stopPropagation();
  return (
    <button
      type="button"
      aria-expanded={open}
      aria-label={`Sub-issues of #${card.number}`}
      onPointerDown={keep}
      onKeyDown={keep}
      onClick={(event) => {
        event.stopPropagation();
        onToggle();
      }}
      className="-mx-2 flex min-h-8 cursor-pointer appearance-none items-center gap-2 rounded-control bg-transparent px-2 text-left text-sm text-secondary transition-colors duration-(--motion-routine) ease-out hover:bg-hover hover:text-text"
    >
      <Caret className="size-4 shrink-0" weight="bold" aria-hidden="true" />
      {stackWords(stack)}
    </button>
  );
}
