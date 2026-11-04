/**
 * The calls of the Models and Plugins pages: what the server uses, the models its upstreams serve, the OAuth logins,
 * the plugins, and the keys of `moebius.toml` (`crates/moebius-desktop/src/client/calls/server.rs`). An API key goes to
 * `storeSecret` once and is never read back.
 */
import { del, get, post, put } from "../http";
import type { AvailableModels } from "../types/chat";
import type {
  Login,
  Models,
  Plugins,
  Provider,
  SavedSettings,
  ServerSettings,
} from "../types/models";

export function models(): Promise<Models> {
  return get<Models>("/v1/models");
}

/** The models the upstreams serve; `refresh` asks them again, `decision` asks the decision model's endpoint too. */
export function availableModels(refresh: boolean, decision: boolean): Promise<AvailableModels> {
  return get<AvailableModels>(`/v1/models/available?refresh=${refresh}&decision=${decision}`);
}

export function providers(): Promise<Provider[]> {
  return get<Provider[]>("/v1/models/providers");
}

/** Starts the OAuth login of the upstream at `place`; the person has confirmed the warning of its terms. */
export function startLogin(place: number): Promise<Login> {
  return post<Login>(`/v1/models/upstreams/${place}/login`, { accept_terms: true });
}

/** Where the login of the upstream at `place` stands. */
export function login(place: number): Promise<Login> {
  return get<Login>(`/v1/models/upstreams/${place}/login`);
}

/** Finishes the waiting login with the code, or the address the browser ended on. */
export function pasteLogin(place: number, pasted: string): Promise<Login> {
  return post<Login>(`/v1/models/upstreams/${place}/login/code`, { pasted });
}

export function plugins(): Promise<Plugins> {
  return get<Plugins>("/v1/plugins");
}

export function serverSettings(): Promise<ServerSettings> {
  return get<ServerSettings>("/v1/settings");
}

/** Changes `moebius.toml`: the dotted keys to set to a value and those to remove. */
export function saveSettings(
  set: Record<string, unknown>,
  remove: string[],
): Promise<SavedSettings> {
  return put<SavedSettings>("/v1/settings", { set, remove });
}

/** Stores an API key under `name` on the server only. */
export async function storeSecret(name: string, key: string): Promise<void> {
  await put<unknown>(`/v1/secrets/${encodeURIComponent(name)}`, { value: key });
}

/** Removes the key stored under `name`. */
export async function deleteSecret(name: string): Promise<void> {
  await del<unknown>(`/v1/secrets/${encodeURIComponent(name)}`);
}

/** Asks the server to restart, which it answers before it does. */
export async function restartServer(): Promise<void> {
  await post<unknown>("/v1/server/restart", {});
}
