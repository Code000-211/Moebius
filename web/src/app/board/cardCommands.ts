/**
 * The commands of a card in focus, for the palette: assign it to the person (or take that back), block or unblock it,
 * stop its run, close or reopen the issue, add a sub-issue, and move it to a column where the board lets a person
 * move it (approve, merge, send back, retry; `dropRule` decides, the same rule a drag follows). Each one names the
 * single key that does it on the page. The page says what each action does; this only says which are offered.
 */
import type { Card, InboxItem } from "../../api/types/board";
import type { PaletteCommand } from "../shell/palette/model";
import { COLUMNS, dropRule, isHeld, isLive } from "./model";

/** What the page does for a card; each is its own way of asking, answering and reading the board again. */
export interface CardActions {
  claim: (card: Card) => void;
  release: (card: Card) => void;
  close: (card: Card) => void;
  reopen: (card: Card) => void;
  block: (card: Card) => void;
  unblock: (card: Card) => void;
  stop: (card: Card) => void;
  addSubIssue: (card: Card) => void;
  approve: (card: Card) => void;
  retry: (card: Card) => void;
  merge: (card: Card, change: number) => void;
  /** Opens the dock at the form that asks why a change goes back. */
  reject: (card: Card, change: number) => void;
}

export function cardCommands(
  card: Card,
  item: InboxItem | undefined,
  act: CardActions,
): PaletteCommand[] {
  const commands: PaletteCommand[] = [
    card.claimed_by
      ? { id: "unassign", label: "Unassign", key: "a", run: () => act.release(card) }
      : { id: "assign", label: "Assign to me", key: "a", run: () => act.claim(card) },
    isHeld(card)
      ? { id: "unblock", label: "Unblock", key: "b", run: () => act.unblock(card) }
      : { id: "block", label: "Block…", key: "b", opensDialog: true, run: () => act.block(card) },
  ];
  if (isLive(card)) {
    commands.push({ id: "stop", label: "Stop run", key: "x", run: () => act.stop(card) });
  }
  commands.push(
    card.closed
      ? { id: "reopen", label: "Reopen issue", key: "c", run: () => act.reopen(card) }
      : { id: "close", label: "Close issue", key: "c", run: () => act.close(card) },
    {
      id: "sub-issue",
      label: "Add a sub-issue…",
      key: "s",
      opensDialog: true,
      run: () => act.addSubIssue(card),
    },
  );
  const columns: string[] = [...COLUMNS];
  for (const column of columns) {
    const drop = dropRule(card, column, item);
    const label = `Move to ${column}`;
    const id = `move-${column}`;
    if (drop.kind === "approve") commands.push({ id, label, run: () => act.approve(card) });
    else if (drop.kind === "retry") commands.push({ id, label, run: () => act.retry(card) });
    else if (drop.kind === "merge")
      commands.push({ id, label, run: () => act.merge(card, drop.change) });
    else if (drop.kind === "reject")
      commands.push({
        id,
        label: `Send back to ${column}`,
        run: () => act.reject(card, drop.change),
      });
  }
  return commands;
}
