/**
 * The keys of the TanStack Query cache, one per read of the API. `live/invalidation.ts` maps the server's events to
 * these, so a new read gets its key here and a line there.
 */
export const keys = {
  session: ["session"] as const,
  health: ["health"] as const,
  server: ["server"] as const,
  /** The people of the organisation, and the access tokens of one person or of the person asking. */
  people: ["people"] as const,
  tokensOf: (person: number) => ["people", "tokens", person] as const,
  ownTokens: ["own-tokens"] as const,
  start: ["start"] as const,
  /** The usage of a window of days; a finished run makes all of them stale. */
  usage: ["usage"] as const,
  usageDays: (days: number, repository: number | null, agent: string) =>
    ["usage", days, repository, agent] as const,
  /** The analytics of a window with its filters; a card's move, a cycle's change and a finished run make them stale. */
  analytics: ["analytics"] as const,
  analyticsDays: (days: number, repository: number | null, agent: string) =>
    ["analytics", days, repository, agent] as const,
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
  /** The settings of a repository's project; `project_settings_changed` makes all of them stale. */
  projectSettingsAll: ["project-settings"] as const,
  /** What the plugins ask of one repository, with its values. */
  pluginProjectSettings: (repository: number) => ["plugin-project-settings", repository] as const,
  projectSettings: (repository: number) => ["project-settings", repository] as const,
  snapshot: ["snapshot"] as const,
  /** The projects as their tiles show them; what changes the snapshot changes their open cards and repositories. */
  projects: ["projects"] as const,
  /** Everything read from the wiki starts with this, so `wiki_changed` makes all of it stale. */
  wiki: ["wiki"] as const,
  wikiPages: ["wiki", "pages"] as const,
  wikiStatus: ["wiki", "status"] as const,
  wikiPage: (path: string, at: string | null) => ["wiki", "page", path, at] as const,
  wikiHistory: (path: string) => ["wiki", "history", path] as const,
  wikiSearch: (query: string) => ["wiki", "search", query] as const,
  wikiLinks: (path: string) => ["wiki", "links", path] as const,
  wikiCardPages: (key: string) => ["wiki", "card", key] as const,
  /** The milestones with their progress, and one with its cards; what changes a card or a milestone makes them stale. */
  milestones: ["milestones"] as const,
  milestone: (id: number) => ["milestones", id] as const,
  /** The milestones one card is in. */
  cardMilestones: (card: number) => ["card", card, "milestones"] as const,
  /** The saved views of the person, and the cards of one. */
  views: ["views"] as const,
  viewCards: (id: number) => ["views", id, "cards"] as const,
  /** A project's cycles with their progress, one cycle with its burn-up and cards, and the open cycle of a card. */
  cycles: ["cycles"] as const,
  projectCycles: (project: number) => ["cycles", "project", project] as const,
  cycle: (id: number) => ["cycles", id] as const,
  cardCycle: (card: number) => ["card", card, "cycle"] as const,
  /** The kinds of issues with their fields. */
  issueKinds: ["issue-kinds"] as const,
  /** The issue templates of the kinds and the page templates of the wiki. */
  templates: ["templates"] as const,
  /** The cards of every project, as the Issues page asks for them: all of them, or the ones a query picks. */
  issues: ["issues"] as const,
  issuesFor: (query: string) => ["issues", query] as const,
  /** Everything read about one card (its history and its runs) starts with this, so an event can make it all stale. */
  cards: ["card"] as const,
  history: (card: number) => ["card", card, "history"] as const,
  cardRuns: (card: number) => ["card", card, "runs"] as const,
  comments: (card: number) => ["card", card, "comments"] as const,
  /** Who follows a card; under `cards`, so the events that make a card stale make this one stale. */
  followers: (card: number) => ["card", card, "followers"] as const,
  /** The requests to one project; `intake_changed` makes them stale. */
  requests: (project: number) => ["requests", project] as const,
  /** The notices of the person signed in, which `notice_added` and `notice_read` make stale. */
  notices: ["notices"] as const,
  /** The scripts of a project's columns; `script_changed` makes all of them stale. */
  scripts: ["script"] as const,
  projectScripts: (project: number) => ["script", project] as const,
  script: (project: number, column: string) => ["script", project, column] as const,
  chats: ["chats"] as const,
  /** The person's pins and recent places. */
  places: ["places"] as const,
  /** What the person chose for themselves in the app. */
  preferences: ["preferences"] as const,
  /** The person's logins on the forges. */
  logins: ["logins"] as const,
  agents: ["agents"] as const,
  models: ["models"] as const,
  skills: ["skills"] as const,
  /** Everything read about runs starts with this: `run_started` and `run_ended` make it stale. */
  runs: ["run"] as const,
  /** The runs that have ended, for the Fleet's Finished tab; under `runs`, so a run that starts or ends makes it stale. */
  finishedRuns: ["run", "finished"] as const,
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
  /** What the plugins ask of the whole server, with its values. */
  appSettings: ["plugins", "app-settings"] as const,
  /** Everything the package sheet and the Plugins page read about packages: `package_*` events make it stale. */
  packages: ["packages"] as const,
  /** The prefixes of the lists under `packages`, for reading them again without what a package opened in the sheet read. */
  packagesSearch: ["packages", "search"] as const,
  packagesInfo: ["packages", "info"] as const,
  piSearchAll: ["packages", "pi", "search"] as const,
  packageSearch: (query: string) => ["packages", "search", query] as const,
  packagesInstalled: ["packages", "installed"] as const,
  packageInfo: (name: string) => ["packages", "info", name] as const,
  piSearch: (query: string, type: string | null) =>
    ["packages", "pi", "search", query, type] as const,
  piInstalled: ["packages", "pi", "installed"] as const,
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
  /** The kinds of the judgements of one repository and one agent, either of which may be empty. */
  kindsOf: (repository: number | null, agent: string) =>
    ["decision-model", "kinds", repository, agent] as const,
  exports: ["decision-model", "exports"] as const,
  /** The Workflows tab: every list of workflows, every definition, and a run with its nodes. */
  workflows: ["workflows"] as const,
  workflowDefinitions: ["workflow"] as const,
  workflowRun: (id: number) => ["workflow-run", id] as const,
  /** Every list of runs. */
  allWorkflowRuns: ["workflow-runs"] as const,
  /** One project's workflows. */
  projectWorkflows: (project: number) => ["workflows", "project", project] as const,
  projectWorkflow: (project: number, name: string) =>
    ["workflow", "project", project, name] as const,
  /** The runs of all projects' workflows that Fleet lists; under `allWorkflowRuns`, so a workflow event refreshes it. */
  fleetWorkflowRuns: ["workflow-runs", { fleet: true }] as const,
  /** The runs of one project, of one of its workflows when `workflow` is given. */
  projectWorkflowRuns: (project: number, workflow?: string) =>
    ["workflow-runs", "project", project, workflow ?? null] as const,
};
