/** The answers of the Improve pages: the Retro, its proposals and lessons, the research, the decision model and the exports. */

export type RetroStatus = "reported" | "analysing" | "proposed" | "failed";

/** A Retro as `GET /v1/retros` lists it. */
export interface RetroSummary {
  /** The local day, `YYYY-MM-DD`. */
  date: string;
  status: RetroStatus;
  errors: number;
  proposals: number;
  at?: number;
}

export interface ReportError {
  kind: string;
  run: number | null;
  card: number | null;
  agent: string | null;
  text: string;
}

/** A proposal tried on one card. */
export interface Trial {
  card_id: number | null;
  /** Whether the card is one of the regression set. */
  regression?: boolean;
  outcome: "positive" | "negative" | "skipped" | "failed";
  reason: string;
}

export type ProposalStatus =
  | "open"
  | "trying"
  | "positive"
  | "negative"
  | "untried"
  | "merged"
  | "dismissed";

/** A change the Retro proposes: to the harness configuration, to `moebius.toml` or to a repository's `.moebius/`. */
export interface Proposal {
  id: number;
  kind?: "harness" | "settings" | "repository";
  /** The day of the Retro that made it, `YYYY-MM-DD` — the day the Retro page opens on to read it again. */
  retro_date: string;
  branch: string;
  title: string;
  body?: string;
  agents?: string[];
  cards?: number[];
  status: ProposalStatus;
  trials?: Trial[];
  /** What a `settings` or `repository` proposal changes, as a unified diff. */
  diff?: string;
}

/** A Retro with its report and proposals (`GET /v1/retros/{date}`). */
export interface RetroView {
  date: string;
  status: RetroStatus;
  warnings?: string[];
  report: { errors: ReportError[] };
  proposals?: Proposal[];
}

export interface Lesson {
  id: number;
  repository_id: number;
  kind: "failure" | "rejection" | "success";
  /** The kind of task the lesson is about. */
  situation: string;
  /** What went wrong, or what worked. */
  lesson: string;
  /** What to do. */
  advice: string;
  cards?: number[];
  status: "active" | "retired";
  retired_reason?: string | null;
  retrievals?: number;
}

/** One lever of a research, tried on a branch of its own. */
export interface Experiment {
  id: number;
  branch: string;
  title: string;
  /** What the measure printed on the branch; null when the measurement failed. */
  value: number | null;
  checks?: boolean | null;
  kept: boolean;
  reason?: string;
  change_id?: number | null;
}

export type ResearchStatus =
  | "draft"
  | "confirmed"
  | "running"
  | "done"
  | "spent"
  | "stopped"
  | "failed";

export type Better = "lower" | "higher";

/** What a research's budget counts. */
export type BudgetKind = "experiments" | "tokens";

/** The limit of a research: a number of experiments or of tokens. */
export interface Budget {
  kind: BudgetKind;
  amount: number;
}

export interface Research {
  id: number;
  repository_id: number;
  goal: string;
  measure: string;
  better: Better;
  budget: Budget;
  /** What has been used of the budget so far, whichever its kind is. */
  used?: { experiments: number; tokens: number };
  /** For a spent research, the limit it reached. */
  limit_reached?: BudgetKind | null;
  status: ResearchStatus;
  baseline: number | null;
  reason?: string;
  /** The tokens its runs have used. */
  tokens?: number;
  experiments?: Experiment[];
}

/** What a brief sends (`POST /v1/research`, `PUT /v1/research/{id}`). */
export interface Brief {
  goal: string;
  measure: string;
  better: Better;
  budget: Budget;
}

/** What the server holds of one kind of judgement. */
export interface KindCounts {
  kind: string;
  count: number;
  with_outcome: number;
  by_person: number;
  held_out: number;
}

/** A training-set export a person started. */
export interface Export {
  id: number;
  at: number;
  kinds: KindCounts[];
}

export interface Takeover {
  id: number;
  kind: string;
  /** The agreement when it was proposed, between 0 and 1. */
  agreement: number;
  count: number;
  status: "open" | "merged" | "dismissed" | "released";
}

export interface ShadowedKind {
  kind: string;
  shadows: number;
  agreement: number | null;
  failed: number;
  dropped: number;
  /** The model decides the kind now. */
  routed: boolean;
  /** `localize` only: the cards whose change is known, and the recall of the model and of the recorded hints. */
  measured?: number | null;
  recall?: number | null;
  baseline_recall?: number | null;
}

/** `GET /v1/decision-model`. */
export interface Shadowing {
  configured: boolean;
  model: string | null;
  /** The agreement a takeover needs. */
  agreement: number | null;
  min_judgements: number | null;
  kinds: ShadowedKind[];
  takeovers: Takeover[];
}
