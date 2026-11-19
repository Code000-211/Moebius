/**
 * The Mentions tab of the Inbox (ADR 0018): what concerns the person signed in, as the server keeps it for them
 * (`GET /v1/notices`). Here are the words each kind of notice is shown with, how many are unread, and where choosing
 * one leads. The read mark is the server's and the person's own, unlike the dots of the other items, which a browser
 * keeps (`read.ts`).
 */
import type { Snapshot } from "../../api/types/board";
import type { Notice } from "../../api/types/notices";
import { boardPath } from "../pages";

/** The value of the tab's `?kind=`. */
export const MENTIONS = "mentions";

/** How many notices are not read yet. */
export function unreadCount(notices: readonly Notice[]): number {
  return notices.filter((notice) => !notice.read).length;
}

/** What a notice says under its title: who did what, and the start of the comment or the change. */
export function said(notice: Notice): string {
  const by = notice.by === null ? "" : `@${notice.by}`;
  switch (notice.kind) {
    case "assigned":
      return "Given to you";
    case "mentioned":
      return `${by === "" ? "Someone" : by} mentioned you: ${notice.body}`;
    case "changed":
      return notice.body;
  }
}

/** The word on the row for what kind of notice it is. */
export function kindWord(notice: Notice): string {
  switch (notice.kind) {
    case "assigned":
      return "Assigned";
    case "mentioned":
      return "Mention";
    case "changed":
      return "Following";
  }
}

/**
 * Where choosing the notice leads: the card on its board, or the inbox item it is about, or nowhere (null) when the
 * card is gone. The snapshot says which board a card is on.
 */
export function pathOf(snapshot: Snapshot, notice: Notice): string | null {
  if (notice.card_id !== null) {
    const board = snapshot.boards.find((each) =>
      each.cards.some((card) => card.id === notice.card_id),
    );
    if (board) return `${boardPath(board.repository.id)}?card=${notice.card_id}`;
  }
  if (
    notice.inbox_item_id !== null &&
    snapshot.inbox.some((item) => item.id === notice.inbox_item_id)
  ) {
    return `/inbox?item=${notice.inbox_item_id}`;
  }
  return null;
}
