/** What a person keeps in the sidebar (`GET /v1/places`). */

export type PlaceKind = "project" | "view" | "card" | "page";

/** A project, view, card or page, as a person pinned it or opened it. */
export interface Place {
  id: number;
  kind: PlaceKind;
  repository_id: number;
  /** A card's id, a view's or a page's slug; empty for a project. */
  key: string;
  title: string;
}

/** The pins in the order the person put them, and the places they opened lately, newest first. */
export interface Places {
  favourites: Place[];
  recent: Place[];
}

/** The body of a pin or of a place opened; a card needs only its `key`, a project only its `repository_id`. */
export interface PlaceRef {
  kind: PlaceKind;
  repository_id?: number;
  key?: string;
  title?: string;
}
