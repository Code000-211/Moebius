/** The calls about cycles, the time boxes of one project. */
import { del, get, patch, post, put } from "../http";
import type { Carry, Closed, Cycle, CycleDetail, CycleFields, CycleView } from "../types/cycles";

/** The project's cycles with their progress, the latest start first. */
export function listCycles(project: number): Promise<CycleView[]> {
  return get<CycleView[]>(`/v1/projects/${project}/cycles`);
}

export function readCycle(id: number): Promise<CycleDetail> {
  return get<CycleDetail>(`/v1/cycles/${id}`);
}

export function addCycle(project: number, fields: CycleFields): Promise<Cycle> {
  return post<Cycle>(`/v1/projects/${project}/cycles`, fields);
}

export function editCycle(id: number, fields: CycleFields): Promise<Cycle> {
  return patch<Cycle>(`/v1/cycles/${id}`, fields);
}

/** Deletes a cycle; the cards it held stay. */
export function deleteCycle(id: number): Promise<Cycle> {
  return del<Cycle>(`/v1/cycles/${id}`);
}

/** Closes a cycle; `carry` says where the cards that are not done go. */
export function closeCycle(id: number, carry: Carry): Promise<Closed> {
  return post<Closed>(`/v1/cycles/${id}/close`, carry);
}

/** The open cycle a card is in, or null. */
export function cardCycle(card: number): Promise<Cycle | null> {
  return get<Cycle | null>(`/v1/cards/${card}/cycle`);
}

/** Puts a card into an open cycle of its project, or out of its cycle with null. */
export function setCardCycle(card: number, cycle: number | null): Promise<Cycle | null> {
  return put<Cycle | null>(`/v1/cards/${card}/cycle`, { cycle });
}
