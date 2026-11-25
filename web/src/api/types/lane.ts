/** The test lane (ADR 0035): the lanes of the server, a repository's "Test lane" settings and a card's report of its jobs. */

/** A lane's job: one change at one pinned head. */
export interface LaneJob {
  id: number;
  repository_id: number;
  card_id: number;
  change_id: number;
  lane: string;
  /** The commit measured, and the merge base it was measured against. */
  head: string;
  base: string;
  /** Whether the change is a pull request someone else opened. */
  external: boolean;
  state: "queued" | "running" | "measured" | "passed" | "failed" | "errored" | "cancelled";
  failed_step: string | null;
  summary: string;
  report: LaneReport | null;
  /** The step a running job is at. */
  step: string | null;
  /** Whether what came of it was handed on. */
  delivered: boolean;
  queued_at: number;
  started_at: number | null;
  ended_at: number | null;
}

/** What a job reports: the machine, the steps, every run of the measurement, why it failed or errored. */
export interface LaneReport {
  machine?: {
    lane: string;
    where: string;
    gpus: string | null;
    power_cap: string | null;
    rocm: string | null;
    driver: string | null;
    image: string | null;
    isolation: LaneIsolation;
    contained: boolean;
  };
  steps?: Array<{ name: string; state: "passed" | "failed" | "skipped"; seconds: number }>;
  measurements?: Array<{
    step: string;
    runs: number;
    parity: string;
    rows: Array<{
      name: string;
      unit: string;
      before: number[];
      after: number[];
      median_before: number | null;
      median_after: number | null;
      change_percent: number | null;
    }>;
  }>;
  failure?: { step: string; why: string; output: string };
  error?: string;
}

export type LaneIsolation = "always" | "external_only" | "off";

/** A lane of the configuration, with its queue. */
export interface Lane {
  name: string;
  runner: string | null;
  isolation: LaneIsolation;
  /** What the owner is warned of while the lane is not isolated everywhere; null for `always`. */
  warning: string | null;
  runtime: string;
  image: string | null;
  gpus: string | null;
  power_cap: string | null;
  rocm: string | null;
  driver: string | null;
  devices: string[];
  timeout_seconds: number;
  queued: number;
  running: LaneJob | null;
  /** Set while the lane has stopped itself after jobs that failed for its own reasons. */
  paused: { since: number; reason: string } | null;
}

/** `GET /v1/repositories/{id}/lane`. */
export interface RepositoryLane {
  lane: string | null;
  current: Lane | null;
  lanes: Lane[];
  definition: { present: boolean; problem: string | null; steps: string[]; measures: boolean };
  jobs: LaneJob[];
}

/** `GET /v1/cards/{id}/lane`. */
export interface CardLane {
  lane: string | null;
  jobs: LaneJob[];
  /** The head of the card's change now: a job of another head is out of date. */
  head: string | null;
  /** Why no job is queued for `head`, when none is. */
  waiting: string | null;
}
