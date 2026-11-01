/**
 * The keys of the TanStack Query cache, one per read of the API. `live/invalidation.ts` maps the server's events to
 * these, so a new read gets its key here and a line there.
 */
export const keys = {
  session: ["session"] as const,
  health: ["health"] as const,
  server: ["server"] as const,
  start: ["start"] as const,
  snapshot: ["snapshot"] as const,
};
