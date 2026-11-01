/** The Automations page's answers (`GET /v1/automations`, `GET /v1/automations/{id}/firings`) and what a save sends. */

/** A firing of an automation: the chat it started, or why it started none. */
export interface Firing {
  id: number;
  status: "started" | "skipped" | "failed";
  run_id: number | null;
  /** The status of its chat now. */
  chat_status: string | null;
  /** Whether a person fired it ("Run now"). */
  manual: boolean;
  reason: string;
  at: number;
}

export interface Automation {
  id: number;
  name: string;
  /** A cron expression of five fields in the server's local time. */
  schedule: string;
  agent: string;
  prompt: string;
  repository_id: number | null;
  /** Whether its chats work in the harness configuration repository. */
  harness_repository: boolean;
  enabled: boolean;
  /** When it fires next, in Unix seconds; null while it is off. */
  next_firing: number | null;
  last_firing: Firing | null;
}

/** The body of `POST /v1/automations` and `PUT /v1/automations/{id}`. */
export interface AutomationSettings {
  name: string;
  schedule: string;
  agent: string;
  prompt: string;
  repository: number | null;
  harness: boolean;
  enabled: boolean;
}
