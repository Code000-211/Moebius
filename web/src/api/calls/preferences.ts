/** The calls about a person's preferences: `/v1/preferences`. */
import { get, put } from "../http";
import type { Preferences } from "../types/preferences";

/** The preferences of the person asking; every one is off for a person who chose nothing. */
export function preferences(): Promise<Preferences> {
  return get<Preferences>("/v1/preferences");
}

/** Replaces the preferences and answers with them as the server kept them. */
export function savePreferences(next: Preferences): Promise<Preferences> {
  return put<Preferences>("/v1/preferences", next);
}
