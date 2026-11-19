/** The calls about milestones, goals of the organisation that hold cards of any projects. */
import { del, get, patch, post, put } from "../http";
import type {
  Milestone,
  MilestoneDetail,
  MilestoneFields,
  MilestoneView,
} from "../types/milestones";

/** Every milestone with its progress, the nearest target date first; the server counts only the cards the person may see. */
export function listMilestones(): Promise<MilestoneView[]> {
  return get<MilestoneView[]>("/v1/milestones");
}

/** One milestone with the cards the person may see. */
export function readMilestone(id: number): Promise<MilestoneDetail> {
  return get<MilestoneDetail>(`/v1/milestones/${id}`);
}

export function addMilestone(fields: MilestoneFields): Promise<Milestone> {
  return post<Milestone>("/v1/milestones", fields);
}

export function editMilestone(id: number, fields: MilestoneFields): Promise<Milestone> {
  return patch<Milestone>(`/v1/milestones/${id}`, fields);
}

/** Deletes a milestone; the cards it held stay. */
export function deleteMilestone(id: number): Promise<Milestone> {
  return del<Milestone>(`/v1/milestones/${id}`);
}

/** The milestones a card is in, the nearest target date first. */
export function cardMilestones(card: number): Promise<Milestone[]> {
  return get<Milestone[]>(`/v1/cards/${card}/milestones`);
}

/** Replaces the milestones a card is in (ids). */
export function setCardMilestones(card: number, milestones: number[]): Promise<Milestone[]> {
  return put<Milestone[]>(`/v1/cards/${card}/milestones`, { milestones });
}
