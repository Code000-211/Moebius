/** Types of the calls about cycles; they follow docs/reference/desktop-api.md. */

/** A cycle of a project: a time box with a first and a last day. */
export interface Cycle {
  id: number;
  project_id: number;
  name: string;
  /** The first day, `YYYY-MM-DD`. */
  starts_on: string;
  /** The last day, `YYYY-MM-DD`. */
  ends_on: string;
  /** When it was closed, in Unix seconds; null while it is open. */
  closed_at: number | null;
  created_at: number;
}

/** A cycle as the project's list answers it: with how far its cards are. */
export interface CycleView extends Cycle {
  cards: number;
  done: number;
  /** The points of the cards that have an estimate. */
  points: number;
  points_done: number;
}

/** A card of a cycle. */
export interface CycleCard {
  id: number;
  repository_id: number;
  number: number;
  title: string;
  column: string;
  done: boolean;
  estimate: number | null;
  added_at: number;
  done_at: number | null;
  by_agent: boolean;
  tokens: number;
}

/** One day of the burn-up: the points planned and done by the end of that day. */
export interface BurnUpDay {
  day: string;
  scope: number;
  done: number;
}

export interface CycleFigures {
  /** The done cards an agent ran code for. */
  by_agents: number;
  tokens: number;
  /** Tokens per done point; null while no point is done. */
  tokens_per_point: number | null;
}

/** `GET /v1/cycles/{id}`: the view with the cards in place of their count, its figures and its burn-up. */
export interface CycleDetail extends Omit<CycleView, "cards"> {
  figures: CycleFigures;
  burn_up: BurnUpDay[];
  cards: CycleCard[];
}

/** The body of `POST /v1/projects/{id}/cycles` and `PATCH /v1/cycles/{id}`. */
export interface CycleFields {
  name: string;
  starts_on: string;
  ends_on: string;
}

/** What closing does with the cards that are not done: to the open cycle `to` (default: the next), or out. */
export type Carry = { carry: "next"; to?: number } | { carry: "out" };

/** The answer of closing a cycle. */
export interface Closed {
  cycle: Cycle;
  carried_to: Cycle | null;
  carried: number[];
}
