/**
 * One column of the board with its cards and their stacks of sub-issues, and one card as a draggable thing in it. What a
 * card shows is `BoardCardContent.tsx`; `BoardPage.tsx` lays the columns out and decides what a drop does.
 *
 * A column and a card are memoised. A column works out for each card what the card shows that depends on the board (its
 * status, its change, whether it is the next to start) and hands the card those, with callbacks that keep their identity,
 * so a card draws again only when its own data, one of those or whether it is chosen or open changes: a read of the
 * snapshot that changes one card, or opening another, no longer draws every card of the board.
 */
import { type MouseEvent, memo, type ReactNode, useState } from "react";
import { useNavigate } from "react-router";
import { type ListRange, Virtuoso } from "react-virtuoso";
import type { Board, Card, InboxItem } from "../../api/types/board";
import { ContextMenu, useContextMenu } from "../../ui/components/ContextMenu";
import {
  KanbanBoard,
  KanbanCard,
  KanbanCards,
  KanbanHeader,
  useDraggedCardId,
} from "../../ui/components/KanbanBoard";
import { Empty } from "../../ui/page/Empty";
import { SelectBox } from "../../ui/page/SelectBox";
import { scriptPath } from "../pages";
import { CardContent, type CardMenuActions } from "./BoardCardContent";
import { ColumnAdd } from "./ColumnAdd";
import { cardKey } from "./cardKey";
import { type CardStatus, cardStatus, inboxOf, type Stack, waitingWords } from "./model";

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

/**
 * What chooses cards: the choice and the right click of the page's `useBulk`, by what a card needs of it. The object keeps
 * its identity until the choice changes.
 */
export interface ColumnBulk {
  has: (id: number) => boolean;
  anyChosen: boolean;
  toggle: (id: number, range?: boolean) => void;
  onContextMenu: (card: Card) => (event: MouseEvent) => void;
}

const ANY_COLUMN = "A card arrives when the column's script moves one in.";

export const BoardColumn = memo(function BoardColumn({
  board,
  projectId,
  inbox,
  name,
  cards,
  stacks,
  opened,
  inspectedTopId,
  onToggleStack,
  busy,
  inspectedId,
  onSelect,
  menu,
  bulk,
  adding,
  onAdding,
  onCreated,
}: {
  board: Board;
  /** The project whose board this is: the column's script is the project's. */
  projectId: number;
  inbox: InboxItem[];
  name: string;
  cards: Card[];
  /** The stack of each card of the board that has sub-issues, by card id. */
  stacks: Map<number, Stack>;
  /** The stacks a person opened, by the id of the card at the top. */
  opened: ReadonlySet<number>;
  /** The card at the top of the stack the dock shows a card of: that stack is open while the dock is. */
  inspectedTopId: number | undefined;
  onToggleStack: (card: Card) => void;
  /** While a decision is on its way, when no card can be moved. */
  busy: boolean;
  inspectedId: number | undefined;
  onSelect: (card: Card) => void;
  menu: CardMenuActions;
  bulk: ColumnBulk;
  /** The line for a new card is open in this column. */
  adding: boolean;
  /** Opens or closes the line for a new card in the column of this name. */
  onAdding: (column: string, open: boolean) => void;
  onCreated: (card: Card, column: string) => void;
}) {
  const shared = { busy, onSelect, menu };
  /** What a card of this column shows that depends on the board. */
  const derived = (card: Card) => {
    const item = inboxOf(inbox, card);
    return { status: cardStatus(card, item, board), waiting: waitingWords(item?.kind) };
  };
  /** A card of the column with the stack of its sub-issues under it when that is open: one row of the column. */
  const renderCard = (card: Card) => {
    const stack = stacks.get(card.id);
    const open = stack !== undefined && (opened.has(card.id) || inspectedTopId === card.id);
    return (
      <div
        data-testid={stack ? `stack-${card.number}` : undefined}
        className="flex min-w-0 flex-col gap-3"
      >
        <BoardCard
          {...shared}
          {...derived(card)}
          card={card}
          selected={inspectedId === card.id}
          column={name}
          bulk={bulk}
          chosen={bulk.has(card.id)}
          stack={stack}
          open={open}
          onToggleStack={onToggleStack}
        />
        {open && stack.shown.length > 0 && (
          // The sub-issues lie under their parent, in a stack of their own: each is a card that moves, blocks and
          // deletes like any other,
          <div className="ml-3 flex min-w-0 flex-col gap-3 border-l-2 border-hairline-strong pl-3">
            {stack.shown.map((child) => (
              <BoardCard
                key={child.id}
                {...shared}
                {...derived(child)}
                card={child}
                selected={inspectedId === child.id}
                column={child.column}
              />
            ))}
          </div>
        )}
      </div>
    );
  };
  return (
    <KanbanBoard name={name}>
      <ColumnHead
        name={name}
        projectId={projectId}
        repository={board.repository.id}
        onAdding={onAdding}
      />
      <ColumnLine
        repository={board.repository.id}
        name={name}
        adding={adding}
        onAdding={onAdding}
        onCreated={onCreated}
      />
      {cards.length === 0 ? (
        <KanbanCards className="min-h-9">
          {/* The column is already the surface under these words; a card of its own inside it is a second one, and one
              that reads as a card which has lost its words. */}
          <Empty plain centered body={COLUMN_EMPTY[name] ?? ANY_COLUMN} />
        </KanbanCards>
      ) : (
        <ColumnCards cards={cards} renderCard={renderCard} />
      )}
    </KanbanBoard>
  );
});

/** The air under the last card, which the lanes row takes back from the page. */
function ColumnFoot({ context }: { context?: ColumnContext }) {
  return (
    <div className="h-4">
      {context?.pinned && (
        // The card being dragged stays in the page while its row is scrolled out of the window: the drag library needs
        // its element. It is out of sight and out of reach, and the drag overlay is what the person sees.
        <div
          inert
          aria-hidden
          className="pointer-events-none absolute top-0 -left-[9999px] w-60 opacity-0"
        >
          {context.pinned}
        </div>
      )}
    </div>
  );
}

interface ColumnContext {
  pinned: ReactNode;
}

/** Only what is in view of the column's cards is drawn, rows of different heights (a card, or a card with its open stack). */
const COLUMN_COMPONENTS = { Footer: ColumnFoot };

function ColumnCards({
  cards,
  renderCard,
}: {
  cards: Card[];
  renderCard: (card: Card) => ReactNode;
}) {
  const dragged = useDraggedCardId();
  // None until the list says what it drew: until then no card is taken to be out of it.
  const [range, setRange] = useState<ListRange | null>(null);
  const at = dragged === null ? -1 : cards.findIndex((card) => String(card.id) === dragged);
  const pinned =
    at >= 0 && range !== null && (at < range.startIndex || at > range.endIndex)
      ? renderCard(cards[at] as Card)
      : null;
  return (
    <Virtuoso
      data={cards}
      computeItemKey={(_, card) => card.id}
      increaseViewportBy={400}
      rangeChanged={setRange}
      context={{ pinned }}
      components={COLUMN_COMPONENTS}
      className="min-h-9 flex-1"
      style={{ height: "auto" }}
      itemContent={(_, card) => <div className="pb-2">{renderCard(card)}</div>}
    />
  );
}

/**
 * The head of a column: its name alone. Its context menu (right click, the menu key, or Enter while it has the focus)
 * offers "Add a card", the one way to add to a column that is not the header's New issue button, and "Edit the
 * script", which opens the column's script in the project's settings (Columns). Draws again only when the name changes.
 */
const ColumnHead = memo(function ColumnHead({
  name,
  projectId,
  repository,
  onAdding,
}: {
  name: string;
  projectId: number;
  repository: number;
  onAdding: (column: string, open: boolean) => void;
}) {
  const menu = useContextMenu();
  const navigate = useNavigate();
  return (
    <>
      <KanbanHeader
        name={name}
        focusable
        onContextMenu={menu.open}
        onActivate={(box) =>
          menu.open({
            preventDefault: () => {},
            clientX: box.left,
            clientY: box.bottom,
          } as MouseEvent)
        }
      />
      <ContextMenu
        point={menu.point}
        onClose={menu.close}
        title={name}
        commands={[
          { id: "add", label: "Add a card", run: () => onAdding(name, true) },
          {
            id: "script",
            label: "Edit the script",
            run: () => navigate(scriptPath(projectId, name, repository)),
          },
        ]}
      />
    </>
  );
});

/** The line for a new card, under the name while it is open. */
const ColumnLine = memo(function ColumnLine({
  repository,
  name,
  adding,
  onAdding,
  onCreated,
}: {
  repository: number;
  name: string;
  adding: boolean;
  onAdding: (column: string, open: boolean) => void;
  onCreated: (card: Card, column: string) => void;
}) {
  return (
    <ColumnAdd
      repository={repository}
      column={name}
      open={adding}
      onClose={() => onAdding(name, false)}
      onCreated={onCreated}
    />
  );
});

type BoardCardProps = {
  card: Card;
  /** What the card is doing: worked out by the column from the board. */
  status: CardStatus;
  /** What the card waits for when it needs a person, in words. */
  waiting: string;
  /** What chooses the card: left out for a sub-issue in a stack, which is not chosen with the cards of the column. */
  bulk?: ColumnBulk;
  /** Whether the card is chosen; only a card the column can choose says. */
  chosen?: boolean;
  /** The column the card is in: the one it is drawn in, or its own when it lies in the stack of its parent. */
  column: string;
  busy: boolean;
  /** The dock shows this card. */
  selected: boolean;
  onSelect: (card: Card) => void;
  menu: CardMenuActions;
  stack?: Stack;
  open?: boolean;
  onToggleStack?: (card: Card) => void;
};

/** Whether a stack reads the same: the page makes a new one at every read of the board, with the same cards in it. */
function sameStack(a: Stack | undefined, b: Stack | undefined): boolean {
  if (a === b) return true;
  if (!a || !b) return false;
  return (
    a.done === b.done &&
    a.total === b.total &&
    a.shown.length === b.shown.length &&
    a.shown.every((card, at) => card === b.shown[at])
  );
}

/**
 * Whether a card draws the same. Every prop is compared by identity, but for the stack, which is rebuilt with the same
 * content whenever the board is read again, and the choice, of which the card takes only whether it is in it.
 */
function sameCard(was: BoardCardProps, now: BoardCardProps): boolean {
  for (const key of Object.keys(now) as (keyof BoardCardProps)[]) {
    if (key === "stack" || key === "bulk") continue;
    if (!Object.is(was[key], now[key])) {
      return false;
    }
  }
  if (!sameStack(was.stack, now.stack)) return false;
  // The box shows on every card once any is chosen; its handlers do not change what a card draws.
  return (
    was.bulk?.anyChosen === now.bulk?.anyChosen &&
    (was.bulk === undefined) === (now.bulk === undefined)
  );
}

/** One card of a column, or of the stack under its parent. */
const BoardCard = memo(function BoardCard({
  card,
  status,
  waiting,
  column,
  busy,
  selected,
  onSelect,
  menu,
  bulk,
  chosen = false,
  stack,
  open = false,
  onToggleStack,
}: BoardCardProps) {
  return (
    <KanbanCard
      id={String(card.id)}
      name={`${cardKey(card)} ${card.title}`}
      testId={`card-${card.number}`}
      column={column}
      dragDisabled={busy}
      isSelected={selected}
      onOpen={() => onSelect(card)}
      onContextMenu={bulk?.onContextMenu(card)}
    >
      <CardContent
        card={card}
        status={status}
        waiting={waiting}
        onOpen={() => onSelect(card)}
        menu={menu}
        select={
          bulk && (
            <SelectBox
              label={`Choose #${card.number}`}
              checked={chosen}
              anyChosen={bulk.anyChosen}
              onToggle={(range) => bulk.toggle(card.id, range)}
            />
          )
        }
        stack={stack}
        open={open}
        onToggleStack={onToggleStack && (() => onToggleStack(card))}
      />
    </KanbanCard>
  );
}, sameCard);
