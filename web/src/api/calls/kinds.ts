/**
 * The calls about kinds of issues: defining them (an owner), the kinds a project offers, a card's kind and the values of
 * its fields, and the cards of every project filtered and sorted by them.
 */
import { del, get, post, put } from "../http";
import type { Card } from "../types/board";
import type { FieldValue, Kind, KindBody } from "../types/kinds";
import type { Project } from "../types/projects";

export function listKinds(): Promise<Kind[]> {
  return get<Kind[]>("/v1/kinds");
}

export function addKind(body: KindBody): Promise<Kind> {
  return post<Kind>("/v1/kinds", body);
}

export function updateKind(id: number, body: KindBody): Promise<Kind> {
  return put<Kind>(`/v1/kinds/${id}`, body);
}

export function deleteKind(id: number): Promise<unknown> {
  return del<unknown>(`/v1/kinds/${id}`);
}

/** Replaces the kinds a project offers, by name. */
export function setProjectKinds(project: number, kinds: string[]): Promise<Project> {
  return put<Project>(`/v1/projects/${project}/kinds`, { kinds });
}

/** Gives a card a kind its project offers, or none. */
export function setCardKind(card: number, kind: string | null): Promise<Card> {
  return put<Card>(`/v1/cards/${card}/kind`, { kind });
}

/** Sets values of fields of the card's kind; `null` clears one. */
export function setCardFields(
  card: number,
  fields: Record<string, FieldValue | null>,
): Promise<Card> {
  return put<Card>(`/v1/cards/${card}/fields`, { fields });
}

/** The cards of every project that match `query` (the parameters of `GET /v1/issues`). */
export function listIssues(query: URLSearchParams): Promise<Card[]> {
  const text = query.toString();
  return get<Card[]>(text === "" ? "/v1/issues" : `/v1/issues?${text}`);
}
