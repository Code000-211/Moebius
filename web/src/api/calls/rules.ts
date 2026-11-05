/** The calls of the Rules page: the rules with what they refused, and saving a rules file. */
import { get, put } from "../http";
import type { RulesView, SavedRules } from "../types/rules";

export function rules(): Promise<RulesView> {
  return get<RulesView>("/v1/rules");
}

/** Replaces the server's rules in prose with `text`, the whole file; the server checks and commits it. */
export function saveRules(text: string): Promise<SavedRules> {
  return put<SavedRules>("/v1/rules", { text });
}

/** Writes the `rules.md` of a repository's project, a commit in the harness configuration repository that the server checks first. */
export async function saveRepositoryRules(repository: number, text: string): Promise<void> {
  await put<unknown>(`/v1/repositories/${repository}/settings/rules.md`, { text });
}
