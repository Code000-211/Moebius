/** The calls about saved views: filters with a layout and a sort, for a person or for everyone. */
import { del, get, patch, post } from "../http";
import type { View, ViewCards, ViewFields } from "../types/views";

/** The person's own views and the shared ones. */
export function listViews(): Promise<View[]> {
  return get<View[]>("/v1/views");
}

export function readView(id: number): Promise<View> {
  return get<View>(`/v1/views/${id}`);
}

/** The view with the cards it shows this person, in its order. */
export function viewCards(id: number): Promise<ViewCards> {
  return get<ViewCards>(`/v1/views/${id}/cards`);
}

export function addView(fields: ViewFields): Promise<View> {
  return post<View>("/v1/views", fields);
}

/** Sets the fields given and keeps the others, the filter included. */
export function editView(id: number, fields: Partial<ViewFields>): Promise<View> {
  return patch<View>(`/v1/views/${id}`, fields);
}

export function deleteView(id: number): Promise<View> {
  return del<View>(`/v1/views/${id}`);
}
