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
/** The `?kind=` of the tab of the notices of cards given to the person. */
export const ASSIGNED = "assigned";
/** The `?kind=` of the tab of the notices of cards the person follows. */
export const FOLLOWING = "following";

/** The tabs of notices, in order, with the kind of notice each holds. */
export const NOTICE_TABS = [
  { value: MENTIONS, label: "Mentions", kind: "mentioned" },
  { value: ASSIGNED, label: "Assigned", kind: "assigned" },
  { value: FOLLOWING, label: "Following", kind: "changed" },
] as const;

/** Whether `kind` (a `?kind=`) is the tab of a kind of notice. */
export const isNoticeTab = (kind: string | null): boolean =>
  NOTICE_TABS.some((tab) => tab.value === kind);

/** The notices a tab shows: only those of its own kind. */
export function inTab(notices: readonly Notice[], tab: string): Notice[] {
  const kind = NOTICE_TABS.find((each) => each.value === tab)?.kind;
  return notices.filter((notice) => notice.kind === kind);
}

/** How many notices `GET /v1/notices` answers at most. */
export const CAP = 200;

/** A number of rows as a tab says it: "200+" when the server's list was cut at its cap. */
export function rows(count: number, capped: boolean): string {
  return capped ? `${count}+` : String(count);
}

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
