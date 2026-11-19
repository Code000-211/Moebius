/** What concerns a person: a line of the Inbox's Mentions tab (`GET /v1/notices`). */
export interface Notice {
  id: number;
  /** `assigned` (given to them), `mentioned` (in a comment) or `changed` (a card they follow). */
  kind: "assigned" | "mentioned" | "changed";
  /** The handle of whoever caused it, without the `@`, or null when that is not known. */
  by: string | null;
  card_id: number | null;
  inbox_item_id: number | null;
  /** The title of the card or the inbox item. */
  title: string;
  /** The start of the comment, or what changed. */
  body: string;
  at: number;
  read: boolean;
}
