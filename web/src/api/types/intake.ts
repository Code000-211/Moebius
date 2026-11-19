/** Types of the calls about intake; they follow docs/reference/desktop-api.md (Intake). */

/** A request to a project: what a person wrote, and the draft issue a grilling made of it. */
export interface IntakeRequest {
  id: number;
  project_id: number;
  /** The handle of the person who wrote it, without the `@`. */
  requester: string;
  /** In their own words. */
  text: string;
  /** The draft issue; empty while the request is `grilling`. */
  title: string;
  body: string;
  /** `grilling` (a chat works on it), `drafted` (a member may edit, accept or decline it), `accepted` or `declined`. */
  status: "grilling" | "drafted" | "accepted" | "declined";
  run_id: number | null;
  card_id: number | null;
  /** Why a declined request was declined. */
  reason: string;
  created_at: number;
  decided_at: number | null;
}
