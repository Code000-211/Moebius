/** `GET /v1/analytics`; see docs/reference/desktop-api.md. */
import type { BurnUpDay } from "./cycles";

export interface AnalyticsTotals {
  /** The cards done in the window. */
  done: number;
  points_done: number;
  tokens: number;
  /** The tokens over the points done, rounded down; null while no point is done. */
  tokens_per_point: number | null;
  /** The cards that are not done and not archived. */
  open: number;
}

export interface ThroughputDay {
  day: string;
  done: number;
  points: number;
}

/** How long something took over a number of cards, in seconds. */
export interface Spread {
  cards: number;
  median_seconds: number;
  mean_seconds: number;
  p90_seconds: number;
}

export interface ColumnTime {
  column: string;
  cards: number;
  total_seconds: number;
  mean_seconds: number;
}

export interface ProjectFigures {
  id: number;
  name: string;
  key: string;
  done: number;
  points_done: number;
  tokens: number;
  tokens_per_point: number | null;
}

/** The burn-up of a cycle (with its project) or of a milestone. */
export interface BurnUp {
  id: number;
  name: string;
  project: string | null;
  starts_on: string;
  ends_on: string;
  cards: number;
  done: number;
  points: number;
  points_done: number;
  days: BurnUpDay[];
}

export interface Analytics {
  today: string;
  days: number;
  totals: AnalyticsTotals;
  throughput: ThroughputDay[];
  cycle_time: Spread | null;
  lead_time: Spread | null;
  columns: ColumnTime[];
  projects: ProjectFigures[];
  cycles: BurnUp[];
  milestones: BurnUp[];
}
