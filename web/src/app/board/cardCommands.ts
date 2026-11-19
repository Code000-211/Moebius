/**
 * The commands of a card in focus, for the palette: assign it to the person (or take that back), block or unblock it,
 * stop its run, close or reopen the issue, add a sub-issue, and move it to a column where the board lets a person
 * move it (approve, merge, send back, retry; `dropRule` decides, the same rule a drag follows). Each one names the
 * single key that does it on the page. The page says what each action does; this only says which are offered. A person the card can be given to
 * has a command of their own, without a key.
 */
import type { Card, InboxItem } from "../../api/types/board";
import type { PaletteCommand } from "../shell/palette/model";
import { cardKey } from "../shell/palette/shortcuts";
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
  /** Gives the card to the person with `handle` (`on`), or takes it off them. */
  assignTo: (card: Card, handle: string, on: boolean) => void;
  /** Opens the dock at the form that asks why a change goes back. */
  reject: (card: Card, change: number) => void;
}

export function cardCommands(
  card: Card,
  item: InboxItem | undefined,
  act: CardActions,
  /** The people the card can be given to, as handles; one command for each, since the palette searches by name. */
  people: string[] = [],
): PaletteCommand[] {
  const commands: PaletteCommand[] = [
    card.claimed_by
      ? {
          id: "unassign",
          label: "Unassign",
          key: cardKey("unassign"),
          run: () => act.release(card),
        }
      : { id: "assign", label: "Assign to me", key: cardKey("assign"), run: () => act.claim(card) },
    isHeld(card)
      ? { id: "unblock", label: "Unblock", key: cardKey("unblock"), run: () => act.unblock(card) }
      : {
          id: "block",
          label: "Block…",
          key: cardKey("block"),
          opensDialog: true,
          run: () => act.block(card),
        },
  ];
  for (const handle of people) {
    const on = !card.assignees.includes(handle);
    commands.push({
      id: `${on ? "assignee" : "unassignee"}-${handle}`,
      label: on ? `Assign to @${handle}` : `Take off @${handle}`,
      run: () => act.assignTo(card, handle, on),
    });
  }
  if (isLive(card)) {
    commands.push({
      id: "stop",
      label: "Stop run",
      key: cardKey("stop"),
      run: () => act.stop(card),
    });
  }
  commands.push(
    card.closed
      ? { id: "reopen", label: "Reopen issue", key: cardKey("reopen"), run: () => act.reopen(card) }
      : { id: "close", label: "Close issue", key: cardKey("close"), run: () => act.close(card) },
    {
      id: "sub-issue",
      label: "Add a sub-issue…",
      key: cardKey("sub-issue"),
      opensDialog: true,
      run: () => act.addSubIssue(card),
    },
  );
  const columns: string[] = [...COLUMNS];
  for (const column of columns) {
    const drop = dropRule(card, column, item);
    const label = `Move to ${column}`;
    const id = `move-${column}`;
    if (drop.kind === "approve")
      commands.push({ id, label, key: cardKey(id), run: () => act.approve(card) });
    else if (drop.kind === "retry")
      commands.push({ id, label, key: cardKey(id), run: () => act.retry(card) });
    else if (drop.kind === "merge")
      commands.push({ id, label, key: cardKey(id), run: () => act.merge(card, drop.change) });
    else if (drop.kind === "reject")
      commands.push({
        id,
        label: `Send back to ${column}`,
        key: cardKey(id),
        run: () => act.reject(card, drop.change),
      });
  }
  return commands;
}
