/**
 * Reading the loosely typed `data` of a run event: an object or a string, or an empty one when it is not. The parts of
 * the conversation's fold share these.
 */
export type Json = Record<string, unknown>;

export const object = (value: unknown): Json =>
  typeof value === "object" && value !== null && !Array.isArray(value) ? (value as Json) : {};
export const string = (value: unknown): string => (typeof value === "string" ? value : "");
