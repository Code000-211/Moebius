/** Types of the snapshot and what it holds; they follow docs/reference/desktop-api.md, "Types" and `GET /v1/snapshot`. */

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

export interface Card {
  id: number;
  repository_id: number;
  number: number;
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
