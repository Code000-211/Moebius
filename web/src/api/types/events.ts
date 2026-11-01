/** One message of the event WebSocket `GET /v1/events`: `{"kind", "data"}`, see "Events" in the desktop API reference. */
export interface ServerEvent {
  kind: string;
  data: Record<string, unknown>;
}

/** The kinds of the reference that the web app knows about; any other kind is passed on and ignored. */
export type EventKind =
  | "repository_added"
  | "repository_updated"
  | "repository_removed"
  | "file_changed"
  | "script_changed"
  | "card_added"
  | "card_updated"
  | "card_moved"
  | "card_waiting"
  | "change_offered"
  | "change_updated"
  | "change_merged"
  | "change_closed"
  | "attempt_judged"
  | "attempts_settled"
  | "run_started"
  | "run_event"
  | "run_ended"
  | "board_error"
  | "forge_error"
  | "harness_error"
  | "retro_done"
  | "retro_error"
  | "replayed"
  | "trial_done"
  | "proposal_status"
  | "takeover_proposed"
  | "takeover_status"
  | "research_status"
  | "experiment_done"
  | "automation_fired"
  | "grilling_ended"
  | "port_opened"
  | "port_closed"
  | "lagged";
