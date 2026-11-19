/** The calls about intake: a project's requests, which a grilling drafts into issues that members accept or decline. */
import { get, post, put } from "../http";
import type { IntakeRequest } from "../types/intake";
import type { Project } from "../types/projects";

/** Turns a project's intake on or off (an owner). */
export function setIntake(project: number, enabled: boolean): Promise<Project> {
  return put<Project>(`/v1/projects/${project}/intake`, { enabled });
}

/** The requests the person may read, the newest first: their own for a viewer, everyone's for a member. */
export function listRequests(project: number): Promise<IntakeRequest[]> {
  return get<IntakeRequest[]>(`/v1/projects/${project}/requests`);
}

/** Writes a request to a project that has intake. */
export function submitRequest(project: number, text: string): Promise<IntakeRequest> {
  return post<IntakeRequest>(`/v1/projects/${project}/requests`, { text });
}

/** Edits the draft issue of a request (a member). */
export function editRequest(id: number, title: string, body: string): Promise<IntakeRequest> {
  return put<IntakeRequest>(`/v1/requests/${id}`, { title, body });
}

/** Makes a card of the draft, as it is or as edited (a member). */
export function acceptRequest(
  id: number,
  edited?: { title: string; body: string },
): Promise<IntakeRequest> {
  return post<IntakeRequest>(`/v1/requests/${id}/accept`, edited ?? {});
}

/** Declines a request; the requester is told the reason (a member). */
export function declineRequest(id: number, reason: string): Promise<IntakeRequest> {
  return post<IntakeRequest>(`/v1/requests/${id}/decline`, { reason });
}
