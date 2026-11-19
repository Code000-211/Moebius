/** The calls about what a person pins and opens: `/v1/places`, `/v1/favourites` and `/v1/recent`. */
import { del, get, post, put } from "../http";
import type { PlaceRef, Places } from "../types/places";

/** The person's pins and recent places; the server forgets the ones they can no longer see. */
export function places(): Promise<Places> {
  return get<Places>("/v1/places");
}

/** Pins a place at the end of the list. */
export function pin(place: PlaceRef): Promise<Places> {
  return post<Places>("/v1/favourites", place);
}

/** Puts the pins in the order of `ids`. */
export function reorderPins(ids: number[]): Promise<Places> {
  return put<Places>("/v1/favourites/order", { ids });
}

export function unpin(id: number): Promise<Places> {
  return del<Places>(`/v1/favourites/${id}`);
}

/** Notes that a place was opened. */
export function seen(place: PlaceRef): Promise<Places> {
  return post<Places>("/v1/recent", place);
}

export function forgetRecent(id: number): Promise<Places> {
  return del<Places>(`/v1/recent/${id}`);
}
