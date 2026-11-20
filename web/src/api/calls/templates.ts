/**
 * The calls about templates, which live in the harness configuration repository: the issue template of each kind and
 * the page templates of the wiki; making a page from one; and the one-off import of a Jira project.
 */
import { del, get, post, put } from "../http";
import type {
  IssueTemplateBody,
  JiraImport,
  JiraReport,
  PageTemplate,
  Templates,
} from "../types/templates";
import type { WikiPageView } from "../types/wiki";

export function listTemplates(): Promise<Templates> {
  return get<Templates>("/v1/templates");
}

/** Sets the template of the kind `kind` (by name). */
export async function setIssueTemplate(kind: string, body: IssueTemplateBody): Promise<void> {
  await put<unknown>(`/v1/templates/issues/${encodeURIComponent(kind)}`, body);
}

export async function deleteIssueTemplate(kind: string): Promise<void> {
  await del<unknown>(`/v1/templates/issues/${encodeURIComponent(kind)}`);
}

/** Sets or makes the page template `name`. */
export async function setPageTemplate(template: PageTemplate): Promise<void> {
  await put<unknown>(`/v1/templates/pages/${encodeURIComponent(template.name)}`, {
    text: template.text,
  });
}

export async function deletePageTemplate(name: string): Promise<void> {
  await del<unknown>(`/v1/templates/pages/${encodeURIComponent(name)}`);
}

/** Makes the page `path` from the page template `template`. */
export function pageFromTemplate(template: string, path: string): Promise<WikiPageView> {
  return post<WikiPageView>("/v1/wiki/from-template", { template, path });
}

/** Imports a Jira project into the project `project` (an owner only; the token is a secret named in the body). */
export function importJira(project: number, body: JiraImport): Promise<JiraReport> {
  return post<JiraReport>(`/v1/projects/${project}/import/jira`, body);
}
