/** The calls of the Automations page: listing, saving and deleting them, firing one now and reading its firings. */
import { del, get, post, put } from "../http";
import type { Automation, AutomationSettings, Firing } from "../types/automations";

export function automations(): Promise<Automation[]> {
  return get<Automation[]>("/v1/automations");
}

/** Adds an automation, or replaces the automation `id`. */
export function saveAutomation(
  id: number | null,
  settings: AutomationSettings,
): Promise<Automation> {
  return id === null
    ? post<Automation>("/v1/automations", settings)
    : put<Automation>(`/v1/automations/${id}`, settings);
}

/** Deletes an automation; the chats it started stay. */
export async function deleteAutomation(id: number): Promise<void> {
  await del<unknown>(`/v1/automations/${id}`);
}

/** Fires an automation now, whether it is on or not. */
export function runAutomation(id: number): Promise<Firing> {
  return post<Firing>(`/v1/automations/${id}/run`, {});
}

/** The firings of an automation, newest first. */
export function firings(id: number): Promise<Firing[]> {
  return get<Firing[]>(`/v1/automations/${id}/firings`);
}
