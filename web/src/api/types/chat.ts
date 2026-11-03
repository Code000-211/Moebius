/** Types of the chats and their run events; they follow docs/reference/desktop-api.md, "Types" and the chat routes. */
import type { RunStatus } from "./board";

/** Where a chat was started: the Chat page, an automation, or the floating window (`assistant`). */
export type ChatOrigin = "chat" | "automation" | "assistant";

/** A chat as `GET /v1/chats` lists it. */
export interface Chat {
  /** The id of its run. */
  id: number;
  agent: string;
  status: RunStatus;
  repository_id: number | null;
  harness_repository: boolean;
  started_at: number;
  ended_at: number | null;
  /** The first line of the first message, at most 80 characters; empty before the first message. */
  title: string;
  /** The `seq` of the last `agent_settled` or `moebius_error`; 0 without one. */
  answered: number;
  /** How far a person has read it; the chat is unread while `answered` is greater. */
  seen: number;
  changed_files: number;
  /** True while the chat waits and the agent's last answer ends in a question. */
  asks: boolean;
  origin: ChatOrigin;
  /** The model the chat was given in place of its agent's; null while it uses its agent's. */
  model: string | null;
  thinking: string | null;
}

/** The body of `POST /v1/chats`; every field is optional. */
export interface NewChat {
  agent?: string;
  message?: string;
  model?: string;
  thinking?: string;
  repository?: number;
  harness?: boolean;
  continues?: number;
  context?: string;
  origin?: ChatOrigin;
}

/** One stored event of a run, as `GET /v1/runs/{id}/events` has it. */
export interface RunEvent {
  seq: number;
  kind: string;
  data: Record<string, unknown>;
  /** Unix seconds. */
  at: number;
}

/** An agent a chat can be with, as `GET /v1/agents` lists it. */
export interface Agent {
  name: string;
  /** Empty for a built-in agent that has no model of its own or of the Coder. */
  model: string;
  protocol: "openai" | "anthropic";
  thinking?: string;
  token_budget?: number;
  harness: "pi-bolt" | "claude-code";
  built_in: boolean;
  error?: string;
}

export interface AvailableModel {
  id: string;
  protocol: string;
  upstream: string;
  /** The reasoning level names its endpoint takes, easiest first; absent when its upstream does not say. */
  levels?: string[];
}

export interface AvailableClass {
  name: string;
  models: string[];
  protocols: string[];
}

/** What `GET /v1/models/available` says. */
export interface AvailableModels {
  models: AvailableModel[];
  errors: Array<{ upstream: string; message: string }>;
  classes: AvailableClass[];
  decision_models: string[];
}

export interface Skill {
  name: string;
  description: string;
  built_in: boolean;
}

/** The reasoning levels Pi-Bolt takes. */
export const THINKING_LEVELS = ["off", "minimal", "low", "medium", "high", "xhigh", "max"] as const;

/**
 * The reasoning level names `model` was heard taking: its own when the upstream lists them, the union of the models of
 * the class it names, or nothing when neither is known.
 */
export function listedLevels(
  available: AvailableModels | undefined,
  model: string,
): string[] | undefined {
  if (!available || model === "") return undefined;
  const ofItsOwn = available.models
    .filter((each) => each.id === model)
    .flatMap((each) => each.levels ?? []);
  if (ofItsOwn.length > 0) return [...new Set(ofItsOwn)];
  const named = available.classes.find((each) => each.name === model);
  if (!named) return undefined;
  const ofItsModels = available.models
    .filter((each) => named.models.includes(each.id))
    .flatMap((each) => each.levels ?? []);
  return ofItsModels.length > 0 ? [...new Set(ofItsModels)] : undefined;
}

/**
 * The levels to offer for a model whose endpoint lists `levels`: the ones it names itself, and the ones it has a place
 * in its list for, hardest last. A model whose upstream says nothing is offered every level, which is what it is sent.
 */
export function offeredLevels(levels: string[] | undefined): string[] {
  if (!levels || levels.length === 0) return [...THINKING_LEVELS];
  return THINKING_LEVELS.filter((level, place) => levels.includes(level) || place < levels.length);
}
