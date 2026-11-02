/**
 * The keys of the TanStack Query cache, one per read of the API. `live/invalidation.ts` maps the server's events to
 * these, so a new read gets its key here and a line there.
 */
export const keys = {
  session: ["session"] as const,
  health: ["health"] as const,
  server: ["server"] as const,
  start: ["start"] as const,
  /** The usage of a window of days; a finished run makes all of them stale. */
  usage: ["usage"] as const,
  usageDays: (days: number) => ["usage", days] as const,
  settings: ["settings"] as const,
  webhooks: ["webhooks"] as const,
  /** What a repository's files page has read: directories, one file, the paths for "Go to file", a search. */
  files: (repository: number) => ["files", repository] as const,
  tree: (repository: number, directory: string) =>
    ["files", repository, "tree", directory] as const,
  file: (repository: number, path: string) => ["files", repository, "file", path] as const,
  filePaths: (repository: number) => ["files", repository, "paths"] as const,
  fileSearch: (repository: number, query: string, regex: boolean) =>
    ["files", repository, "search", query, regex] as const,
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
  /** The pages of the harness configuration and of the setup: lists, and the item being edited. */
  runners: ["runners"] as const,
  agent: (name: string) => ["agent", name] as const,
  skill: (name: string) => ["skill", name] as const,
  skillFile: (name: string, path: string) => ["skill", name, "file", path] as const,
  mcpServers: ["mcp-servers"] as const,
  mcpServer: (name: string) => ["mcp-server", name] as const,
  /** What the server uses (`GET /v1/models`), what its upstreams serve, the providers built in and the plugins. */
  modelsInUse: ["models-in-use"] as const,
  availableDecision: ["models", "decision"] as const,
  providers: ["providers"] as const,
  plugins: ["plugins"] as const,
  rules: ["rules"] as const,
  automations: ["automations"] as const,
  firings: (automation: number) => ["automations", "firings", automation] as const,
  retros: ["retros"] as const,
  retro: (date: string) => ["retros", date] as const,
  proposals: ["proposals"] as const,
  lessons: ["lessons"] as const,
  research: ["research"] as const,
  shadowing: ["decision-model", "shadowing"] as const,
  kinds: ["decision-model", "kinds"] as const,
  exports: ["decision-model", "exports"] as const,
};
