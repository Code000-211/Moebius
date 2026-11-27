/** The software factory (ADR 0032): a repository's stale settings, a project's backlog, and the daily digest an inbox item holds. */

/** `GET /v1/repositories/{id}/upkeep`: days, 0 is off. */
export interface Upkeep {
  release_after_days: number;
  stale_after_days: number;
  close_after_days: number;
}

export const DEFAULT_UPKEEP: Upkeep = {
  release_after_days: 14,
  stale_after_days: 14,
  close_after_days: 30,
};

/** `GET /v1/repositories/{id}/flow` (ADR 0033): whether a followed pull request is reviewed by itself, and when one that names no issue is closed. */
export interface Flow {
  review_on_green: boolean;
  /** Minutes the checks must have been green, with no push, before the review starts. */
  green_wait_minutes: number;
  /** Days before a pull request that names no issue is closed; 0 never closes one. */
  unbound_close_days: number;
  /** Whether the server keeps one status label on every open issue that has a card. */
  status_labels: boolean;
  /** What a status label starts with. */
  status_prefix: string;
  /** Whether Moebius's own agents work on the forge as people do (ADR 0034): a draft pull request from the first round, the verdict as a review. */
  work_on_forge: boolean;
  /** How many rounds of the Reviewer a followed pull request gets before the owner is asked to look; 0 for no limit. */
  review_round_limit: number;
}

export const DEFAULT_FLOW: Flow = {
  review_on_green: false,
  green_wait_minutes: 5,
  unbound_close_days: 14,
  status_labels: false,
  status_prefix: "status:",
  work_on_forge: true,
  review_round_limit: 3,
};

/** One template in effect for a repository (ADR 0034). */
export interface TemplateInfo {
  /** `pull_request`, or the kind of a comment such as `done`. */
  name: string;
  source: "default" | "project";
  /** Where the repository keeps its copy. */
  path: string;
  /** What a comment of the kind starts with; empty for the pull request template. */
  first_line: string;
}

/** `GET /v1/repositories/{id}/templates`. */
export interface Templates {
  templates: TemplateInfo[];
  sections: { heading: string; required: boolean; hint: string }[];
  /** Why the project's pull request template is not used, or null. */
  problem: string | null;
}

/** `POST /v1/repositories/{id}/templates/sync`: the pull request that keeps the copies current, or null when they are. */
export interface TemplateSync {
  change: { id: number; pull_request: number | null } | null;
  message?: string;
}

/** `GET /v1/projects/{id}/backlog`, or null when the project has none. */
export interface Backlog {
  project_id: number;
  repository: string;
  path: string;
  pattern: string;
  closed_marker: string;
  cap: number;
  difficulty_label: string;
  lane_label: string;
}

/** What `PUT /v1/projects/{id}/backlog` takes; what is left out stays. */
export type BacklogEdit = Partial<Omit<Backlog, "project_id">>;

export interface Synced {
  id: string;
  number: number;
  title: string;
}

/** `POST /v1/projects/{id}/backlog/sync`. */
export interface SyncReport {
  project: string;
  repository: string;
  path: string;
  dry_run: boolean;
  items: number;
  created: Synced[];
  updated: Synced[];
  labelled: Synced[];
  closed: Synced[];
  deferred: string[];
  unchanged: number;
  warnings: string[];
}

/** A line of a digest's section: an issue or a pull request and what to say of it. */
export interface DigestLine {
  number: number | null;
  title: string;
  who: string | null;
  detail: string;
  measured: boolean;
  /** True when the measurement is the one the author committed with the change, approved by the Reviewer; absent otherwise. */
  by_author?: boolean;
}

/** What one repository did since the last digest. */
export interface DigestSection {
  id: number;
  name: string;
  merged: DigestLine[];
  refused: DigestLine[];
  reverts: DigestLine[];
  review_runs: number;
  rounds: DigestLine[];
  taken: DigestLine[];
  released: DigestLine[];
  upkeep: DigestLine[];
  frozen: DigestLine[];
  /** How many lines were left out of each list, by the list's name. */
  more: Record<string, number>;
}

/** What waits for a person, for one reason. */
export interface DigestNeeds {
  kind: string;
  reason: string;
  items: {
    id: number;
    title: string;
    card_id: number | null;
    number: number | null;
    repository: string | null;
  }[];
}

/** The `data` of an inbox item of the kind `daily_digest`. */
export interface Digest {
  day: string;
  since: number;
  until: number;
  repositories: DigestSection[];
  tokens: { spent: number; day_budget: number | null; today: number };
  needs_a_person: DigestNeeds[];
  text: string;
}
