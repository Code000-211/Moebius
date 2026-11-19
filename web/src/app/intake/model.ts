/** What the Intake page says about requests and who may decide them; the rules themselves are the server's. */
import type { IntakeRequest } from "../../api/types/intake";

/** What a request's status is shown as. */
export function statusWords(request: IntakeRequest): string {
  switch (request.status) {
    case "grilling":
      return "Being turned into an issue";
    case "drafted":
      return "Waiting for a member";
    case "accepted":
      return "Accepted";
    case "declined":
      return `Declined: ${request.reason}`;
  }
}

/** Whether a member may still decide the request. */
export function open(request: IntakeRequest): boolean {
  return request.status === "grilling" || request.status === "drafted";
}

/** Whether a role decides requests: a member or an owner; a viewer only writes and reads their own. */
export function decides(role: string | undefined): boolean {
  return role === "owner" || role === "member";
}

/** The requests still to decide first, each group newest first as the server lists them. */
export function ordered(requests: readonly IntakeRequest[]): IntakeRequest[] {
  return [...requests.filter(open), ...requests.filter((request) => !open(request))];
}
