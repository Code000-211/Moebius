/**
 * Types of the Models and Plugins pages: `GET /v1/models`, the OAuth logins, the providers built in, the plugins the
 * host loaded and `GET /v1/settings` (docs/reference/desktop-api.md). The models the upstreams serve
 * (`AvailableModels`) are in `chat.ts`.
 */
import type { Agent } from "./chat";

export interface UpstreamOAuth {
  flow: "device" | "code";
  logged_in: boolean;
  /** What the person confirms before a login starts. */
  warning: string;
  preset?: string | null;
  /** What the upstream needs that the router does not speak yet. */
  needs?: string | null;
}

/** An upstream the server runs with; never a key, only whether it has one. */
export interface Upstream {
  protocol: string;
  base_url: string;
  api_key_set: boolean;
  oauth?: UpstreamOAuth | null;
}

export interface Teacher {
  model: string;
  protocol: string;
  timeout_seconds: number;
}

export interface DecisionModelEndpoint {
  url: string;
  model: string;
  timeout_seconds: number;
  api_key_set: boolean;
}

export interface Models {
  upstreams: Upstream[];
  agents: Agent[];
  teacher: Teacher | null;
  decision_model: DecisionModelEndpoint | null;
}

/** Where an OAuth login stands; no token is ever in it. */
export interface Login {
  state: "logged_out" | "waiting" | "logged_in" | "failed";
  flow: string;
  user_code: string | null;
  verification_uri: string | null;
  verification_uri_complete: string | null;
  authorize_url: string | null;
  message: string;
}

/** A provider built in, which an upstream's `oauth.preset` names. */
export interface Provider {
  name: string;
  title: string;
  protocol: string;
  base_url: string;
  needs?: string | null;
}

export interface PluginProcess {
  program: string;
  state: "starting" | "running" | "restarting" | "stopped";
  restarts: number;
  error: string | null;
  log: string;
}

export interface Plugin {
  name: string;
  version: string;
  kinds: string[];
  decides: string[];
  process: PluginProcess | null;
  /** False for a plugin that is turned off: listed from the manifest it last sent, decides nothing. */
  enabled: boolean;
}

/** What `PUT /v1/plugins/{plugin}/enabled` answers. */
export interface PluginEnabled {
  name: string;
  enabled: boolean;
  /** False when the plugin was in that state already. */
  changed: boolean;
  /** True when runs start unwrapped from now on: it was the last launcher that was on. */
  unwrapped_runs: boolean;
  note: string | null;
}

export interface Plugins {
  time_limit_seconds: number;
  plugins: Plugin[];
}

/** One key of `moebius.toml`: what the file says, or the default when `default` is true. */
export interface Setting {
  value: unknown;
  default: boolean;
}

/** `GET /v1/settings`. */
export interface ServerSettings {
  file: string;
  values: Record<string, Setting>;
  /** The names of the stored API keys, never the keys. */
  secrets: string[];
  restart_needed: boolean;
  restart_keys: string[];
}

/** What `PUT /v1/settings` and `POST /v1/harness/setup` answer. */
export interface SavedSettings {
  restart_needed: boolean;
  restart_keys: string[];
}
