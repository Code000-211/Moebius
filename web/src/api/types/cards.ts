/** Types of the reads about one card and its column scripts; they follow docs/reference/desktop-api.md, "Types". */
import type { RunStatus } from "./board";

/** One entry of `GET /v1/cards/{id}/history`; `data` depends on `kind` (see the reference). */
export interface HistoryEntry {
  at: number;
  kind: string;
  data: Record<string, unknown>;
}

/** A run of `GET /v1/cards/{id}/runs`. */
export interface Run {
  id: number;
  card_id: number | null;
  agent: string;
  harness: string;
  purpose: string;
  status: RunStatus;
  worktree: string | null;
  started_at: number;
  ended_at: number | null;
  summary: string | null;
  repository_id: number | null;
  harness_repository: boolean;
  origin: string;
  runner: string | null;
  /** The name a person gave this chat; null while it is untitled. Only a chat is ever named. */
  title: string | null;
}

/** `GET /v1/repositories/{id}/scripts/{column}`. */
export interface ScriptDetail {
  column: string;
  /** Where the repository keeps its own script for the column. */
  path: string;
  source: "repository" | "built-in";
  text: string;
}
