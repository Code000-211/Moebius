/** Types of what a person decides in the inbox beyond a card: the issues of a grilling. They follow docs/reference/desktop-api.md, `POST /v1/grillings/{id}/confirm`. */

/** One issue a grilling proposes, or that the person made of it; `blocked_by` are indexes into the list it is in. */
export interface GrillingIssue {
  title: string;
  body: string;
  blocked_by: number[];
}
