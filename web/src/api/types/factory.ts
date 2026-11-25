/** The software factory (ADR 0031): a repository's stale settings, a project's backlog, and the daily digest an inbox item holds. */

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
