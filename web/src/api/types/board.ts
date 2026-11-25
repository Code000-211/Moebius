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
  /** Whether the server keeps `ready_label` on the repository's issues that are approved and have no open blocker. */
  mark_ready?: boolean;
  /** The label of ready issues; "Ready for agent" until it is changed. */
  ready_label?: string;
  /** Which issues the repository's agents take: every approved one, or only those labelled and not taken. */
  agents_take?: AgentsTake;
  /** Whether people may take, release and finish issues with `/take`, `/release` and `/done` in a comment on the issue. */
  commands?: boolean;
  /** How many open issues a login may hold that it took with `/take`; 3 until it is changed. */
  command_limit?: number;
  /** Whether the server writes the result of every command as a comment on the issue; on until it is turned off. */
  answer_on_forge?: boolean;
  /** Whether the server merges a followed pull request the Reviewer approved and whose checks are green; on until turned off. */
  merge_reviewed?: boolean;
  /** Whether a change merges only with an accepted measurement or a `no-measurement` label; off until turned on. */
  require_measurement?: boolean;
  /** The owner's own path globs that never merge without a person, besides the ones the server always locks. */
  locked_paths?: string[];
  /** How many changes the server merges into the repository in a day; 10 until it is changed. */
  /** The files of the default branch that hold the repository's review rules; `["REVIEW.md"]` until it is changed. */
  review_brief?: string[];
  /** The label an approved change gets while its measurement is missing; `ready-for-testing` until it is changed. */
  testing_label?: string;
  /** The forge logins whose `## Measurement` comments count. */
  measurers?: string[];
  /** Why the repository is frozen (nothing is reviewed, merged or started), and since when; none when it is not. */
  frozen?: Frozen | null;
}

/** A frozen repository: `manual` by a person, `label` by `factory:freeze` on the forge, `automatic` by a breaker. */
export interface Frozen {
  reason: string;
  source: "manual" | "label" | "automatic";
  /** When it froze, in seconds since 1970. */
  since: number;
}

/** `all` approved issues, or `labelled` only those that carry the ready label and nobody else took. */
export type AgentsTake = "all" | "labelled";

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
  /** The name of the repository the change is in; absent from a server that has one change per card. */
  repository?: string;
  state: string;
  checks: "pending" | "success" | "failure" | "none";
  review_verdict: "approve" | "approve_code" | "changes" | null;
  /** The forge login of the person who opened the pull request the card follows; absent for Moebius's own change. */
  outside_author?: string;
  /** The latest round of the Reviewer on that pull request (ADR 0029); absent until the author asked for one. */
  external_review?: ExternalReview;
  /** The latest measurement a registered measurer posted on that pull request (ADR 0033); absent while there is none. */
  measurement?: Measurement;
  /** True while the pull request is a draft (ADR 0034); absent otherwise. */
  draft?: boolean;
  /** What the pull request's body says, read by the repository's pull request template (ADR 0034); absent until it was read. */
  pull?: PullFields;
}

/** A pull request body read by its template: the sections by the slug of their heading, and what it lacks. */
export interface PullFields {
  fixes: number[];
  fields: Record<string, string>;
  tests: { done: boolean; text: string }[];
  measurement: { state: string; reason: string } | null;
  /** The required parts of the template the body lacks, as the template writes them. */
  missing: string[];
}

/** A measurement a registered measurer posted as a comment on the issue, as the card window shows it. */
export interface Measurement {
  /** The forge login of the measurer. */
  measurer: string;
  /** The commit it measured, and the merge base it was measured against. */
  head: string;
  before: string;
  /** `submitted` waits for the Reviewer, `accepted` and `insufficient` are its answers, `invalid` means the head moved. */
  state: "submitted" | "accepted" | "insufficient" | "invalid";
  /** Whether it measured the pull request as it is now. */
  current: boolean;
  /** Whether a measurement is accepted for the pull request as it is now, whoever accepted it. */
  accepted: boolean;
  at: number;
}

/** A comment of a review on a line of the pull request's diff, in the file as the pull request leaves it. */
export interface ReviewComment {
  path: string;
  line: number;
  body: string;
}

/** `wanted` waits for the checks, `running` is being read, `approved` waits for the checks to merge, `posted` and `merged` are on the forge, `stale` and `failed` came to nothing. */
export type ExternalReviewState =
  | "wanted"
  | "starting"
  | "running"
  | "approved"
  | "posting"
  | "posted"
  | "awaiting_measurement"
  | "merged"
  | "stale"
  | "failed";

/** A round of the Reviewer on a pull request someone else opened, as the card window shows it. */
export interface ExternalReview {
  /** `author` for a round the author or the green checks asked for, `measurement` for one a measurer's comment started. */
  kind: "author" | "measurement";
  round: number;
  round_limit: number;
  state: ExternalReviewState;
  verdict: "approve" | "approve_code" | "changes" | null;
  summary: string;
  comments: ReviewComment[];
  /** Where the review is on the forge, once it is written there. */
  url: string | null;
  /** The commit the round read. */
  head: string;
  /** Whether the round read the pull request as it is now. */
  current: boolean;
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
  /** The built-in column the card's column counts as (a custom column has one); left out by an older server. */
  column_class?: string;
  created_at: number;
  note: string | null;
  retried_after: number | null;
  rounds: number;
  tokens: number;
  run: CardRun | null;
  blocked_by: number[];
  refusals: number;
  /** The card's open change in its own issue's repository. */
  change: CardChange | null;
  /** Every open change of the card, one per repository, in the order they are merged. */
  changes?: CardChange[];
  /** The repositories the card works in, its own issue's first. */
  repositories?: string[];
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
  /** The forge login, without the `@`, of someone on another server who took the issue; this server leaves it alone. */
  taken_by?: string | null;
  /** Why this server could not assign its person on the forge when they took the issue. */
  assign_problem?: string | null;
  /** `taken_by` took the issue with `/take` in a comment, so that this server assigned them; the owner may release it. */
  taken_by_command?: boolean;
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
  /** The forge login of the person who opened the pull request when someone on another server did. */
  outside_author?: string | null;
  /** The column the card was in when it began to follow that pull request. */
  taken_from?: string | null;
}

/** One column of a project, left to right: its name and the built-in column it counts as (its own name for a built-in). */
export interface ColumnDef {
  name: string;
  counts_as: string;
  /** Whether the project's `kanban.toml` added it to the six built-in ones. */
  custom?: boolean;
  /** The file of the column's script, such as `ready.luau`. */
  script?: string;
}

export interface Board {
  repository: Repository;
  /** The columns of the project whose board this is; left out by an older server, which has the six built-in ones. */
  columns?: ColumnDef[];
  /** The columns of each project that has cards on this board, by project id; the page of a project takes its own. */
  project_columns?: Record<string, ColumnDef[]>;
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
  /** Who paused all agents and when, while they are paused; left out otherwise. */
  paused?: { by: string; at: number };
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
