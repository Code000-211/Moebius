/** What concerns the person asking, and following cards: `/v1/notices` and `/v1/cards/{id}/follow`. */
import { get, post, put } from "../http";
import type { Notice } from "../types/notices";

/** The person's notices, newest first. */
export function notices(): Promise<Notice[]> {
  return get<Notice[]>("/v1/notices");
}

/** Marks one of the person's notices read, for them alone. */
export function readNotice(id: number): Promise<Notice> {
  return post<Notice>(`/v1/notices/${id}/read`, {});
}

/** The handles of the people who follow the card. */
export function followers(card: number): Promise<string[]> {
  return get<string[]>(`/v1/cards/${card}/followers`);
}

/** Follows the card, or stops; answers who follows it now. */
export function follow(card: number, following: boolean): Promise<string[]> {
  return put<string[]>(`/v1/cards/${card}/follow`, { following });
}
