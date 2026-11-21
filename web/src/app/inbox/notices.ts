/**
 * The Mentions tab of the Inbox (ADR 0018): what concerns the person signed in, as the server keeps it for them
 * (`GET /v1/notices`). Here are the words each kind of notice is shown with, how many are unread, and where choosing
 * one leads. The read mark is the server's and the person's own, unlike the dots of the other items, which a browser
 * keeps (`read.ts`).
 */
import type { Snapshot } from "../../api/types/board";
import type { Notice } from "../../api/types/notices";
import { repositoryBoardPath } from "../pages";

/** The values of the tabs' `?kind=`: the notices of what was given to the person or names them, and of what they follow. */
export const MENTIONS = "mentions";
export const FOLLOWING = "following";

/** Whether `kind` (a `?kind=`) is a tab of notices; the older `assigned` address leads to Mentions. */
export const isNoticeTab = (kind: string | null): boolean =>
  kind === MENTIONS || kind === FOLLOWING || kind === "assigned";

/** Whether a notice belongs under Following (a change on a card the person follows) rather than Mentions. */
export const isFollowing = (notice: Notice): boolean => notice.kind === "changed";

/**
 * How many things wait for the person: the open items of the inbox and the unread notices that were given to them
 * or name them. A notice of a card they follow only tells, so it is not counted. The sidebar's badge and the page's
 * headline both say this number.
 */
export function waiting(items: number, notices: readonly Notice[]): number {
  return items + notices.filter((notice) => !notice.read && notice.kind !== "changed").length;
}

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

/**
 * Where choosing the notice leads: the card on its board, or the inbox item it is about, or nowhere (null) when the
 * card is gone. The snapshot says which board a card is on.
 */
export function pathOf(snapshot: Snapshot, notice: Notice): string | null {
  if (notice.card_id !== null) {
    const board = snapshot.boards.find((each) =>
      each.cards.some((card) => card.id === notice.card_id),
    );
    if (board) return repositoryBoardPath(board.repository, notice.card_id);
  }
  if (
    notice.inbox_item_id !== null &&
    snapshot.inbox.some((item) => item.id === notice.inbox_item_id)
  ) {
    return `/inbox?item=${notice.inbox_item_id}`;
  }
  return null;
}
