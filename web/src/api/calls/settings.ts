/** The calls about `moebius.toml` while the server runs, its restart, the webhooks of the forges and the harness repository. */
import { get, post, put } from "../http";
import type { SavedSettings, ServerSettings, SettingsChange, Webhook } from "../types/settings";

export function settings(): Promise<ServerSettings> {
  return get<ServerSettings>("/v1/settings");
}

/** Sets and removes keys of the file; the server checks the result as `moebius serve` would read it. */
export function saveSettings(change: SettingsChange): Promise<SavedSettings> {
  return put<SavedSettings>("/v1/settings", change);
}

/** Asks the server to restart; it answers at once and goes away a moment later. */
export async function restartServer(): Promise<void> {
  await post<unknown>("/v1/server/restart", {});
}

/** The webhooks of the forges that are set up; a run token is refused. */
export function webhooks(): Promise<Webhook[]> {
  return get<Webhook[]>("/v1/webhooks");
}

/** Makes a new secret for a forge's webhook; the one before stops working. */
export function makeWebhookSecret(forge: string): Promise<Webhook> {
  return post<Webhook>(`/v1/webhooks/${encodeURIComponent(forge)}/secret`, {});
}

/** Moves the agents and rules of `moebius.toml` into a new harness configuration repository. */
export function setUpHarness(): Promise<SavedSettings> {
  return post<SavedSettings>("/v1/harness/setup", {});
}
