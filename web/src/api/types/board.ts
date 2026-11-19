/** Types of the snapshot and what it holds; they follow docs/reference/desktop-api.md, "Types" and `GET /v1/snapshot`. */

/** How soon a card should be done. */
export type Priority = "urgent" | "high" | "medium" | "low" | "none";

export type Forge = "none" | "github" | "gitlab" | "forgejo";

export interface Repository {
  id: number;
  name: string;
  path: string;
  forge: Forge;
  default_branch: string;
  github: string | null;
  trusted: string[];
  project?: string | null;
  /** The project whose own files this repository holds; null for every repository a person added. */
  files_of?: number | null;
  /** The project the repository's new issues land in, made with it. */
  home_project_id: number;
}

export type RunStatus = "starting" | "running" | "waiting" | "succeeded" | "failed" | "stopped";

/** The latest run of a card, as the snapshot shows it. */
export interface CardRun {
  id: number;
  agent: string;
  purpose: string;
  status: RunStatus;
}

export interface CardChange {
  id: number;
  state: string;
  checks: "pending" | "success" | "failure" | "none";
  review_verdict: "approve" | "changes" | null;
}

/** Why and by whom a card is held (blocked), and since when. */
export interface Held {
  reason: string | null;
  /** `person`, or `agent:Moebius` for the Moebius agent. */
  by: string;
  at: number;
}

export interface Card {
  id: number;
  repository_id: number;
  /** The project whose board the card is on; its column scripts are the ones the card runs. */
  project_id: number;
  /** The number of the card's issue in its repository: the forge's number for an issue of a forge. */
  number: number;
  /** The number the card has in its project, which names it with the project's key (`WEB-24`). */
  project_number?: number;
  /** The key of the card's project, such as `WEB`. */
  project_key?: string;
  title: string;
  body: string;
  author: string;
  column: string;
  created_at: number;
  note: string | null;
  retried_after: number | null;
  rounds: number;
  tokens: number;
  run: CardRun | null;
  blocked_by: number[];
  refusals: number;
  change: CardChange | null;
  /** True for a card made from a confirmed grilling, which needs no approval. */
  confirmed?: boolean;
  /** The latest group of attempts of a card that was tried several times at once; absent without one. */
  attempts?: CardAttempt[];
  /** Set while a person or the Moebius agent holds the card: the board runs nothing for it. */
  held?: Held | null;
  /** The issue number of the issue this one names as its parent. */
  parent: number | null;
  /**
   * Whether the card counts as a sub-issue of its parent: a person can have vouched for it. One that does not (an
   * outsider's issue nobody approved) is shown as a card of its own and holds nothing back, whatever it names.
   */
  counts_as_sub_issue: boolean;
  /** The issue is closed, or its card is in Done. */
  closed: boolean;
  labels: string[];
  /** Who claimed the issue: `person`, `agent:<name>` or a name a session gave. */
  claimed_by: string | null;
  /** The handles, without the `@`, of the people the card is given to, in the order the people were added. */
  assignees: string[];
  /** How soon the card should be done: the card's `priority:` label, `none` without one. */
  priority: Priority;
  /** The day the card is due as `YYYY-MM-DD`: the card's `due:` label, null without one. */
  due: string | null;
  /** The name of the card's kind of issue, none while it has none. Always sent by the server. */
  kind?: string | null;
  /** The values set for the fields of its kind, by field name. Always sent by the server. */
  fields?: Record<string, string | number>;
  /** The card's size in points: its `estimate:` label, null without one. */
  estimate: number | null;
}

/** A comment on an issue, written here or read from the forge. */
export interface Comment {
  id: number;
  card_id: number;
  /** A forge login, `person` or `agent:<name>`. */
  author: string;
  body: string;
  at: number;
}

export interface CardAttempt {
  run: number;
  model: string | null;
  status: RunStatus;
  result: "passed" | "failed" | null;
  output: string;
  picked: boolean;
  reason: string | null;
}

export interface Change {
  id: number;
  repository_id: number | null;
  harness_repository: boolean;
  card_id: number | null;
  run_id?: number | null;
  branch: string;
  state: "open" | "merged" | "closed";
  pull_request: number | null;
  checks: "pending" | "success" | "failure" | null;
  review: string | null;
  head: string | null;
  published_at: number | null;
  checks_head?: string | null;
  failed_checks?: string | null;
  check_output?: string | null;
}

export interface Board {
  repository: Repository;
  cards: Card[];
  round_limit: number;
  /** The reason each waiting card waits, by card id. */
  waiting: Record<string, string>;
  changes: Change[];
}

export interface InboxItem {
  id: number;
  kind: string;
  card_id: number | null;
  title: string;
  data: Record<string, unknown>;
  resolved_at: number | null;
  at: number;
  /** The handles of the people the item is given to. */
  assignees: string[];
}

/** `GET /v1/snapshot`. */
export interface Snapshot {
  boards: Board[];
  harness_changes: Change[];
  inbox: InboxItem[];
}

/** How many runs work on the cards of a board. */
export function liveRuns(board: Board): number {
  return board.cards.filter(
    (card) => card.run?.status === "starting" || card.run?.status === "running",
  ).length;
}
