/** The calls of the test lane (ADR 0035). */
import { get, post, put } from "../http";
import type { CardLane, Lane, RepositoryLane } from "../types/lane";

export function readRepositoryLane(repository: number): Promise<RepositoryLane> {
  return get<RepositoryLane>(`/v1/repositories/${repository}/lane`);
}

/** Assigns the lane a repository uses, or none (a person who is an owner). */
export function editRepositoryLane(
  repository: number,
  lane: string | null,
): Promise<RepositoryLane> {
  return put<RepositoryLane>(`/v1/repositories/${repository}/lane`, { lane });
}

/** A lane that stopped itself is put back to work (a person who is an owner). */
export function resumeLane(name: string): Promise<Lane[]> {
  return post<Lane[]>(`/v1/lanes/${encodeURIComponent(name)}/resume`, {});
}

export function readCardLane(card: number): Promise<CardLane> {
  return get<CardLane>(`/v1/cards/${card}/lane`);
}
