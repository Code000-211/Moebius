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
  /** Everything read about one card (its history and its runs) starts with this, so an event can make it all stale. */
  cards: ["card"] as const,
  history: (card: number) => ["card", card, "history"] as const,
  cardRuns: (card: number) => ["card", card, "runs"] as const,
  /** The script of a column; `script_changed` makes all of them stale. */
  scripts: ["script"] as const,
  script: (repository: number, column: string) => ["script", repository, column] as const,
  chats: ["chats"] as const,
  agents: ["agents"] as const,
  models: ["models"] as const,
  skills: ["skills"] as const,
  /** Everything read about runs starts with this: `run_started` and `run_ended` make it stale. */
  runs: ["run"] as const,
  run: (id: number) => ["run", id] as const,
  /** What a run or a change changed; an event that moves a change or ends a run makes all of them stale. */
  diffs: ["diff"] as const,
  runDiff: (id: number) => ["diff", "run", id] as const,
  changeDiff: (id: number) => ["diff", "change", id] as const,
  ports: ["ports"] as const,
};
