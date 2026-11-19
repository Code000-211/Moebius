/** Types of the calls about saved views; they follow docs/reference/desktop-api.md. */
import type { Card } from "./board";
import type { Cycle } from "./cycles";

/** What a view asks of a card: every part that is set must match, any value of a list matches. */
export interface ViewFilter {
  projects: number[];
  repositories: number[];
  labels: string[];
  /** Handles. */
  assignees: string[];
  /** `urgent`, `high`, `medium`, `low` or `none`. */
  priorities: string[];
  cycles: number[];
  milestones: number[];
  columns: string[];
  authors: string[];
  text: string;
}

export type ViewLayout = "list" | "board" | "calendar" | "timeline";

export interface View {
  id: number;
  name: string;
  owner_id: number;
  shared: boolean;
  filter: ViewFilter;
  layout: ViewLayout;
  /** `created`, `updated`, `priority`, `due`, `title` or `estimate`. */
  sort: string;
  descending: boolean;
  created_at: number;
  /** Whether the person who asked may change or delete it: its owner, or an owner of the organisation. */
  can_change: boolean;
}

/** What a person gives a view. */
export interface ViewFields {
  name: string;
  shared: boolean;
  filter: Partial<ViewFilter>;
  layout: ViewLayout;
  sort: string;
  descending: boolean;
}

/** A card of a view; `cycle` is its open cycle, filled in for the timeline layout only. */
export type ViewCard = Card & { cycle: Cycle | null };

export interface ViewCards {
  view: View;
  cards: ViewCard[];
}
