/**
 * The commands for several chosen cards, for the bar at the foot of the board and the right-click menu: assign them to
 * the person (or take that back), block or unblock them, stop their runs, and move them to a column, which is what a
 * drag to that column would do for each (`dropRule`: approve, retry, merge). Each command names how many of the cards
 * it reaches when that is not all of them, since a card that cannot go there is left as it is. The keys are the ones of
 * a single card (`cardKey`), so a person who knows them for one knows them for many. The page says what each does.
 */

import { columnNames, type HasColumns } from "../../api/columns";
import type { Card, InboxItem } from "../../api/types/board";
import type { SelectionCommand } from "../../ui/page/SelectionBar";
import { cardKey } from "../shell/palette/shortcuts";
import { dropRule, isHeld, isLive } from "./model";

/** What the page does for the cards a command reaches. */
export interface BulkActions {
  claim: (cards: Card[]) => void;
  release: (cards: Card[]) => void;
  /** Asks why; the page opens its dialog. */
  block: (cards: Card[]) => void;
  unblock: (cards: Card[]) => void;
  stop: (cards: Card[]) => void;
  /** Moves the cards to `column`: each card by the decision the drop there would make. */
  move: (cards: Card[], column: string) => void;
}

/** The chosen cards a drop on `column` decides for, which is a person's to decide. */
export function movable(
  cards: readonly Card[],
  column: string,
  itemOf: (card: Card) => InboxItem | undefined,
  columns?: HasColumns,
): Card[] {
  return cards.filter((card) => {
    const kind = dropRule(card, column, itemOf(card), columns).kind;
    return kind === "approve" || kind === "retry" || kind === "merge";
  });
}

function reach(label: string, reached: readonly Card[], all: readonly Card[]): string {
  return reached.length === all.length ? label : `${label} (${reached.length} of ${all.length})`;
}

export function bulkCommands(
  cards: readonly Card[],
  itemOf: (card: Card) => InboxItem | undefined,
  act: BulkActions,
  /** The columns of the project, custom ones included: a move command is offered for each. */
  columns?: HasColumns,
): SelectionCommand[] {
  if (cards.length === 0) return [];
  const commands: SelectionCommand[] = [];
  const free = cards.filter((card) => !card.claimed_by);
  if (free.length > 0) {
    commands.push({
      id: "assign",
      label: reach("Assign to me", free, cards),
      key: cardKey("assign"),
      run: () => act.claim(free),
    });
  } else {
    commands.push({
      id: "unassign",
      label: "Unassign",
      key: cardKey("unassign"),
      run: () => act.release([...cards]),
    });
  }
  const open = cards.filter((card) => !isHeld(card));
  const held = cards.filter(isHeld);
  if (open.length > 0) {
    commands.push({
      id: "block",
      label: reach("Block…", open, cards),
      key: cardKey("block"),
      run: () => act.block(open),
    });
  }
  if (held.length > 0) {
    commands.push({
      id: "unblock",
      label: reach("Unblock", held, cards),
      // One key cannot be two commands: with both on offer, Block keeps it.
      key: open.length === 0 ? cardKey("unblock") : undefined,
      run: () => act.unblock(held),
    });
  }
  const live = cards.filter(isLive);
  if (live.length > 0) {
    commands.push({
      id: "stop",
      label: reach("Stop run", live, cards),
      key: cardKey("stop"),
      run: () => act.stop(live),
    });
  }
  for (const column of columnNames(columns)) {
    const reached = movable(cards, column, itemOf, columns);
    if (reached.length === 0) continue;
    const id = `move-${column}`;
    commands.push({
      id,
      label: reach(`Move to ${column}`, reached, cards),
      key: cardKey(id),
      group: "Move",
      run: () => act.move(reached, column),
    });
  }
  return commands;
}
