/** What a person can do with an item of the inbox, and the request each is. The words are what the item says while the request is on its way. */
import { approve, merge, reject, retry } from "../../api/calls/board";
import {
  confirmGrilling,
  dismissGrilling,
  dismissProposal,
  dismissTakeover,
  mergeProposal,
  mergeTakeover,
} from "../../api/calls/inbox";
import type { GrillingIssue } from "../../api/types/inbox";

export type Action =
  | { kind: "merge"; change: number }
  | { kind: "reject"; change: number; reason: string }
  | { kind: "approve"; card: number }
  | { kind: "retry"; card: number }
  | { kind: "mergeProposal"; proposal: number }
  | { kind: "dismissProposal"; proposal: number }
  | { kind: "mergeTakeover"; takeover: number }
  | { kind: "dismissTakeover"; takeover: number }
  | { kind: "confirmGrilling"; grilling: number; issues: GrillingIssue[] }
  | { kind: "dismissGrilling"; grilling: number };

const WORKING: Record<Action["kind"], string> = {
  merge: "Merging…",
  reject: "Sending back…",
  approve: "Approving…",
  retry: "Retrying…",
  mergeProposal: "Merging…",
  dismissProposal: "Dismissing…",
  mergeTakeover: "Merging…",
  dismissTakeover: "Dismissing…",
  confirmGrilling: "Making the cards…",
  dismissGrilling: "Dismissing…",
};

/** What the item says while the action's request is on its way. */
export const working = (action: Action): string => WORKING[action.kind];

export async function perform(action: Action): Promise<void> {
  switch (action.kind) {
    case "merge":
      await merge(action.change);
      return;
    case "reject":
      await reject(action.change, action.reason);
      return;
    case "approve":
      await approve(action.card);
      return;
    case "retry":
      await retry(action.card);
      return;
    case "mergeProposal":
      return mergeProposal(action.proposal);
    case "dismissProposal":
      return dismissProposal(action.proposal);
    case "mergeTakeover":
      return mergeTakeover(action.takeover);
    case "dismissTakeover":
      return dismissTakeover(action.takeover);
    case "confirmGrilling":
      return confirmGrilling(action.grilling, action.issues);
    case "dismissGrilling":
      return dismissGrilling(action.grilling);
  }
}
