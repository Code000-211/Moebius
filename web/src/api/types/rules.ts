/** The Rules page's answers (`GET /v1/rules`, `PUT /v1/rules`): the patterns the guard refuses, the rules in prose and what each refused. */

/** A rule of a rules file: its `## name` and the text under it. */
export interface Rule {
  name: string;
  text: string;
}

/** A built-in pattern, always on and checked first. */
export interface Pattern {
  name: string;
  description: string;
}

/** The server's rules file as it is now. */
export interface ProseRules {
  /** `harness` (the harness configuration repository), `file` (the `rules_file` of the configuration) or none. */
  source: "harness" | "file" | null;
  /** True exactly when `source` is `harness`. */
  editable: boolean;
  text: string;
  rules: Rule[];
  /** Why the file cannot be used; null when it can. */
  error: string | null;
}

/** The `.moebius/rules.md` of a repository's default branch. */
export interface RepositoryRules {
  repository: string;
  repository_id: number;
  text: string;
  rules: Rule[];
  error: string | null;
}

export interface RulesView {
  patterns: Pattern[];
  prose: ProseRules;
  repositories: RepositoryRules[];
  /** For each rule name, how many calls it refused in the last 7 days; a rule without refusals is left out. */
  refusals: Record<string, number>;
}

/** What `PUT /v1/rules` answers: the page again, with the short id of the commit when the text changed. */
export type SavedRules = RulesView & { commit?: string };
