/** Types of the calls about milestones; they follow docs/reference/desktop-api.md. */

/** A milestone as `POST` and `PATCH` answer it and a card lists its own: the record without its progress. */
export interface Milestone {
  id: number;
  name: string;
  /** The day the goal is due, `YYYY-MM-DD`. */
  target_date: string;
  description: string;
  created_at: number;
}

/** A project's share of a milestone, counting only the cards the asker may see. */
export interface MilestoneProject {
  id: number;
  name: string;
  key: string;
  cards: number;
  done: number;
}

/** A milestone as `GET /v1/milestones` answers it (`MilestoneView`): its progress counts the cards the asker may see. */
export interface MilestoneView extends Milestone {
  cards: number;
  done: number;
  /** The visible projects it holds cards of. */
  projects: MilestoneProject[];
}

/** A card of a milestone. */
export interface MilestoneCard {
  id: number;
  project_id: number;
  repository_id: number;
  number: number;
  title: string;
  column: string;
  done: boolean;
}

/** `GET /v1/milestones/{id}` (`MilestoneDetail`): the view, with the cards in place of their count. */
export interface MilestoneDetail extends Omit<MilestoneView, "cards"> {
  cards: MilestoneCard[];
  done: number;
}

/** The body of `POST /v1/milestones` and `PATCH /v1/milestones/{id}`. */
export interface MilestoneFields {
  name: string;
  target_date: string;
  description: string;
}
